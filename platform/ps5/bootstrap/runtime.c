/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Project startup; stock CRT/patch.c excluded, kernel resolver explicit. */
#include "bootstrap.h"
#include "ps5_bt.h"
#include "syscall.h"
#include <stddef.h>
#ifdef PS5_BT_LOADER_KERNEL_RESOLVER
#include "kernel.h"
#endif

typedef long (*write_fn)(int, const void *, unsigned long);
typedef void (*exit_fn)(int);
typedef int (*notify_fn)(int, void *, size_t, int);

struct notify_request {
    unsigned char reserved[45];
    char message[3075];
};

/* Namespace metadata observation is separate from protected-store acceptance.
 * Its fixed private diagnostic log survives launches of the baseline tests.
 * No caller-supplied path or general log-path override is accepted. */
#ifdef PS5_BT_APPLICATION_RAW_OPENAT
#define PS5_DIAGNOSTIC_LOG_PATH "/data/ps5-bt-raw-openat.log"
#elif defined(PS5_BT_APPLICATION_OPEN_COMPARE)
#define PS5_DIAGNOSTIC_LOG_PATH "/data/ps5-bt-open-compare.log"
#elif defined(PS5_BT_APPLICATION_NAMESPACE)
#define PS5_DIAGNOSTIC_LOG_PATH "/data/ps5-bt-namespace.log"
#else
#define PS5_DIAGNOSTIC_LOG_PATH "/data/ps5-bt-hw1.log"
#endif
#define PS5_O_WRONLY 0x0001
#define PS5_O_CREAT 0x0200
#define PS5_O_TRUNC 0x0400
#define PS5_DIAGNOSTIC_LOG_MODE 0600
#define PS5_PERSISTENT_LOG_FAILED 126
#define PS5_NOTIFICATION_FAILED 127

static write_fn platform_write;
static exit_fn platform_exit;
static notify_fn platform_notify;
static int stream_log_failed;
static int persistent_log_failed;
static int notification_failed;
static int persistent_fd;
static int resolver_ready;
#ifdef PS5_BT_LOADER_KERNEL_RESOLVER
static int kernel_resolver_ready;
#endif

/* Some compatible loaders pass the internal implementation reached by the
 * public sceKernelDlsym veneer. Resolve the public entry exactly as the selected
 * SDK does, then give the SDK a local copy whose first field is canonical. A
 * getpid veneer returns a PID and leaves resolved untouched, so it passes
 * unchanged to the SDK. The loader is trusted to supply
 * a callable first field and valid argument storage. */
static void prepare_syscall_args(payload_args_t *args, payload_args_t *prepared)
{
    void *resolved = 0;
    *prepared = *args;
    int error = args->sys_dynlib_dlsym(0x1, "sceKernelDlsym", &resolved);
    if (error)
        error = args->sys_dynlib_dlsym(0x2001, "sceKernelDlsym", &resolved);
    if (!error && resolved) {
        prepared->sys_dynlib_dlsym = (int (*)(int, const char *, void *))(uintptr_t)resolved;
        return;
    }
    /* The loader contract is trusted. Libkernel text may be execute-only:
     * reading its opcode bytes can fault even when the callback is callable. */
}

/* The pinned loader places payloadout at byte 0x28 in this upstream structure. */
_Static_assert(sizeof(void *) == 8, "requires x86-64 pointers");
_Static_assert(sizeof(write_fn) == sizeof(void *), "requires equal code/data pointer sizes");
_Static_assert(sizeof(notify_fn) == sizeof(void *), "requires equal code/data pointer sizes");
_Static_assert(sizeof(struct notify_request) == 3120, "notification ABI");
_Static_assert(offsetof(payload_args_t, payloadout) == 0x28, "loader ABI");

extern int main(void);
extern unsigned char __bss_start[], __bss_end[];

/* Freestanding Rust core may lower byte operations to these standard symbols.
 * With -fno-builtin, the compiler must not turn these loops into self-calls. */
void *memset(void *destination, int value, size_t count)
{
    unsigned char *out = destination;
    for (size_t i = 0; i < count; ++i)
        out[i] = (unsigned char)value;
    return destination;
}

void *memcpy(void *destination, const void *source, size_t count)
{
    unsigned char *out = destination;
    const unsigned char *in = source;
    for (size_t i = 0; i < count; ++i)
        out[i] = in[i];
    return destination;
}

int memcmp(const void *first, const void *second, size_t count)
{
    const unsigned char *left = first, *right = second;
    for (size_t i = 0; i < count; ++i) {
        if (left[i] != right[i])
            return (int)left[i] - (int)right[i];
    }
    return 0;
}

static size_t append_text(char *buffer, size_t capacity, size_t used, const char *text)
{
    while (*text && used < capacity)
        buffer[used++] = *text++;
    return used;
}

static size_t append_status(char *buffer, size_t capacity, size_t used, int status)
{
    unsigned int value;
    char digits[10];
    size_t count = 0;
    if (status < 0) {
        if (used < capacity)
            buffer[used++] = '-';
        value = 0U - (unsigned int)status;
    } else {
        value = (unsigned int)status;
    }
    do {
        digits[count++] = (char)('0' + value % 10U);
        value /= 10U;
    } while (value && count < sizeof digits);
    while (count && used < capacity)
        buffer[used++] = digits[--count];
    return used;
}

static void persistent_log(const uint8_t *data, uint32_t len)
{
    if (!data || !len || persistent_fd < 0 || persistent_log_failed)
        return;
    if (__crt_syscall(SYS_write, persistent_fd, data, (unsigned long)len) != (long)len)
        persistent_log_failed = 1;
}

static void lifecycle_log(const uint8_t *data, uint32_t len)
{
    if (!data || !len)
        return;
    if (platform_write && !stream_log_failed && platform_write(2, data, len) != (long)len)
        stream_log_failed = 1;
    persistent_log(data, len);
}

static int open_persistent_log(void)
{
    persistent_fd =
        (int)__crt_syscall(SYS_open, PS5_DIAGNOSTIC_LOG_PATH,
                           PS5_O_WRONLY | PS5_O_CREAT | PS5_O_TRUNC, PS5_DIAGNOSTIC_LOG_MODE);
    if (persistent_fd < 0) {
        persistent_log_failed = 1;
        return -1;
    }
    return 0;
}

static int close_persistent_log(void)
{
    int result = 0;
    if (persistent_fd >= 0) {
        if (__crt_syscall(SYS_close, persistent_fd) < 0) {
            persistent_log_failed = 1;
            result = -1;
        }
        persistent_fd = -1;
    }
    return result;
}

static int send_notification(const char *message)
{
    static struct notify_request request;
    if (!platform_notify)
        return -1;
    memset(&request, 0, sizeof request);
    size_t length = 0;
    while (message[length] && length + 1 < sizeof request.message) {
        request.message[length] = message[length];
        ++length;
    }
    request.message[length] = '\0';
    return platform_notify(0, &request, sizeof request, 0);
}

static size_t format_status(char *buffer, size_t capacity, const char *prefix, int status)
{
    size_t used = append_text(buffer, capacity, 0, prefix);
    used = append_status(buffer, capacity, used, status);
    if (status == 2)
        used = append_text(buffer, capacity, used, " (expected Unsupported)");
    return used;
}

static void report_channel_state(void)
{
    static const uint8_t log_unavailable[] =
        "event=persistent_log_unavailable path=" PS5_DIAGNOSTIC_LOG_PATH "\n";
    static const uint8_t notify_unavailable[] = "event=notification_unavailable\n";
    if (persistent_log_failed)
        lifecycle_log(log_unavailable, (uint32_t)(sizeof log_unavailable - 1));
    if (!platform_notify)
        lifecycle_log(notify_unavailable, (uint32_t)(sizeof notify_unavailable - 1));
}

static void report_start(void)
{
    static const uint8_t started[] = "event=payload_start log=" PS5_DIAGNOSTIC_LOG_PATH "\n";
    static const char notice[] = "PS5-BT diagnostic started\nLog: " PS5_DIAGNOSTIC_LOG_PATH;
    lifecycle_log(started, (uint32_t)(sizeof started - 1));
    report_channel_state();
    if (send_notification(notice)) {
        static const uint8_t failed[] = "event=notification_failed phase=start\n";
        notification_failed = 1;
        lifecycle_log(failed, (uint32_t)(sizeof failed - 1));
    }
}

static int final_status_for(int primary_status, int bootstrap_failure)
{
    int status = primary_status;
    /* A loader may intentionally discard fd 2. A complete persistent log is
     * sufficient; preserve the stream warning without turning completion into
     * a startup failure when the durable channel is healthy. */
    if (!bootstrap_failure && stream_log_failed && persistent_log_failed)
        status = 124;
    if (!bootstrap_failure && persistent_log_failed)
        status = PS5_PERSISTENT_LOG_FAILED;
    if (!bootstrap_failure && notification_failed)
        status = PS5_NOTIFICATION_FAILED;
    return status;
}

static int report_finish(int primary_status)
{
    int bootstrap_failure =
        (primary_status >= 120 && primary_status <= 125) || primary_status == 128;
    if (stream_log_failed) {
        static const uint8_t warning[] = "event=stream_log_unavailable fallback=persistent\n";
        lifecycle_log(warning, sizeof warning - 1);
    }
    char line[128];
    size_t length = format_status(line, sizeof line, "event=payload_exit status=", primary_status);
    length = append_text(line, sizeof line, length, "\n");
    lifecycle_log((const uint8_t *)line, (uint32_t)length);

    int final_status = final_status_for(primary_status, bootstrap_failure);
    length = format_status(line, sizeof line, "event=payload_return status=", final_status);
    length = append_text(line, sizeof line, length, "\n");
    lifecycle_log((const uint8_t *)line, (uint32_t)length);
    final_status = final_status_for(primary_status, bootstrap_failure);

    if (close_persistent_log() && !bootstrap_failure)
        final_status = PS5_PERSISTENT_LOG_FAILED;

    char notice[128];
    length =
        format_status(notice, sizeof notice, "PS5-BT diagnostic finished\nStatus: ", final_status);
    if (length < sizeof notice)
        notice[length] = '\0';
    else
        notice[sizeof notice - 1] = '\0';
    if (send_notification(notice)) {
        static const uint8_t failed[] = "event=notification_failed phase=finish\n";
        notification_failed = 1;
        if (platform_write && !stream_log_failed)
            (void)platform_write(2, failed, sizeof failed - 1);
        if (!bootstrap_failure)
            final_status = PS5_NOTIFICATION_FAILED;
    }
    return final_status;
}

/* Fixed alternative libkernel handles follow the pinned SDK syscall startup.
 * Each syscall error is returned without privilege changes or module loading. */
int64_t ps5_bootstrap_lookup(const char *name, void **out)
{
    if (!out)
        return PS5_BT_INVALID;
    *out = 0;
    if (!name || !name[0])
        return PS5_BT_INVALID;
    if (!resolver_ready)
        return PS5_BT_IO;
    long error = __crt_syscall(SYS_sprx_dlsym, 0x1, name, out);
    if (error) {
        *out = 0;
        error = __crt_syscall(SYS_sprx_dlsym, 0x2001, name, out);
    }
    if (error) {
        *out = 0;
#ifdef PS5_BT_LOADER_KERNEL_RESOLVER
        if (kernel_resolver_ready) {
            unsigned long address = kernel_dynlib_dlsym(-1, 0x1, name);
            if (!address)
                address = kernel_dynlib_dlsym(-1, 0x2001, name);
            if (address) {
                *out = (void *)(uintptr_t)address;
                return 0;
            }
        }
#endif
        char line[128];
        size_t length = append_text(line, sizeof line, 0, "event=lookup_error symbol=");
        length = append_text(line, sizeof line, length, name);
        length = append_text(line, sizeof line, length, " code=");
        length = append_status(line, sizeof line, length, (int)error);
        length = append_text(line, sizeof line, length, "\n");
        lifecycle_log((const uint8_t *)line, (uint32_t)length);
    }
    return error ? error : *out ? 0 : PS5_BT_IO;
}

#if defined(PS5_BT_CAPTURE_PROBE) || defined(PS5_BT_CAPTURE_STREAM) || defined(PS5_BT_APPLICATION_CONTROL)
int64_t ps5_bootstrap_module_lookup(int32_t module, const char *name, void **out)
{
    if (!out)
        return PS5_BT_INVALID;
    *out = 0;
    if (module < 0 || !name || !name[0])
        return PS5_BT_INVALID;
    if (!resolver_ready)
        return PS5_BT_IO;
    long error = __crt_syscall(SYS_sprx_dlsym, module, name, out);
    if (error || !*out) {
        *out = 0;
#ifdef PS5_BT_LOADER_KERNEL_RESOLVER
        if (kernel_resolver_ready) {
            /* Fixed current process (-1), existing audited SDK NID resolver.
             * The caller supplies only its freshly loaded Avcap2 handle. */
            unsigned long address = kernel_dynlib_dlsym(-1, (unsigned int)module, name);
            if (address) {
                *out = (void *)(uintptr_t)address;
                return 0;
            }
        }
#endif
    }
    return error ? error : *out ? 0 : PS5_BT_IO;
}
#endif

void ps5_platform_log(const uint8_t *data, uint32_t len) { lifecycle_log(data, len); }

_Noreturn void ps5_platform_abort(void)
{
    /* Same process-termination policy as the desktop shim; never resume Rust. */
    if (platform_exit)
        platform_exit(101);
    __builtin_trap();
}

int ps5_bootstrap_entry(payload_args_t *args)
{
    int result = 120;
    /* elfldr's malloc-backed mirror does not zero the p_memsz file tails. */
    for (volatile unsigned char *p = __bss_start; p < __bss_end; ++p)
        *p = 0;
    /* The loader owns the validity/lifetime of args and payloadout. It must not
     * invoke this entry concurrently. The explicit kernel-resolver variant borrows
     * the loader's pipe/socket fields for SDK initialization; it never owns them. */
    if (!args || !args->sys_dynlib_dlsym || !args->payloadout)
        return result;
    platform_write = 0;
    platform_exit = 0;
    platform_notify = 0;
    stream_log_failed = 0;
    persistent_log_failed = 0;
    notification_failed = 0;
    persistent_fd = -1;
    resolver_ready = 0;
#ifdef PS5_BT_LOADER_KERNEL_RESOLVER
    kernel_resolver_ready = 0;
#endif
    void *symbol = 0;
    payload_args_t syscall_args;
    prepare_syscall_args(args, &syscall_args);
    if (__crt_syscall_init(&syscall_args)) {
        result = 121;
    } else {
        resolver_ready = 1;
        (void)open_persistent_log();
#ifdef PS5_BT_LOADER_KERNEL_RESOLVER
        static const uint8_t initializing[] = "event=loader_kernel_init_begin\n";
        lifecycle_log(initializing, sizeof initializing - 1);
        int kernel_error =
            (!args->rwpipe || !args->rwpair || !args->kpipe_addr || !args->kdata_base_addr)
                ? -14
                : __kernel_init(args);
        kernel_resolver_ready = kernel_error == 0;
        char init_line[96];
        size_t init_length =
            append_text(init_line, sizeof init_line, 0, "event=loader_kernel_init_end code=");
        init_length = append_status(init_line, sizeof init_line, init_length, kernel_error);
        init_length = append_text(init_line, sizeof init_line, init_length, "\n");
        lifecycle_log((const uint8_t *)init_line, (uint32_t)init_length);
        if (kernel_error) {
            result = 128;
            goto finish_run;
        }
#endif
        if (ps5_bootstrap_lookup("write", &symbol)) {
            result = 122;
        } else {
            /* x86-64 System V uses the same representation for these pointers. */
            platform_write = (write_fn)symbol;
            if (ps5_bootstrap_lookup("_exit", &symbol)) {
                result = 123;
            } else {
                platform_exit = (exit_fn)symbol;
                if (!ps5_bootstrap_lookup("sceKernelSendNotificationRequest", &symbol))
                    platform_notify = (notify_fn)symbol;
                report_start();
                result = main();
            }
        }
#ifdef PS5_BT_LOADER_KERNEL_RESOLVER
    finish_run:
#endif
        result = report_finish(result);
    }
    resolver_ready = 0;
#ifdef PS5_BT_LOADER_KERNEL_RESOLVER
    kernel_resolver_ready = 0;
#endif
    *args->payloadout = result;
    return result;
}

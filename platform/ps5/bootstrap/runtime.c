/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Minimal startup; deliberately excludes SDK crt.c, kernel.c and patch.c. */
#include "bootstrap.h"
#include "ps5_bt.h"
#include "syscall.h"
#include <stddef.h>

typedef long (*write_fn)(int, const void *, unsigned long);
typedef void (*exit_fn)(int);
typedef int (*notify_fn)(int, void *, size_t, int);

struct notify_request {
    unsigned char reserved[45];
    char message[3075];
};

#define PS5_DIAGNOSTIC_LOG_PATH "/data/ps5-bt-hw1.log"
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

/* The pinned SDK jumps to +0xa in the loader-supplied getpid veneer. Validate
 * the syscall instruction at that exact entry. The preceding veneer encoding
 * and following return/error path are firmware-owned; the syscall bridge enters
 * neither at an invented offset nor through a copied instruction sequence. The
 * trusted loader must supply a readable code pointer with at least 12 bytes. */
#define PS5_SYSCALL_ENTRY_OFFSET 0x0a
#define PS5_SYSCALL_ENTRY_SIZE 2
static int supported_syscall_entry(const unsigned char *code, size_t available)
{
    static const unsigned char expected[] = {0x0f, 0x05};
    if (!code || available < PS5_SYSCALL_ENTRY_OFFSET + sizeof expected)
        return 0;
    for (size_t i = 0; i < sizeof expected; ++i) {
        if (code[PS5_SYSCALL_ENTRY_OFFSET + i] != expected[i])
            return 0;
    }
    return 1;
}

/* Some compatible loaders pass the internal implementation reached by the
 * public sceKernelDlsym veneer. Resolve the public entry exactly as the selected
 * SDK does, then give the SDK a local copy whose first field is canonical. A
 * getpid veneer returns a PID and leaves resolved untouched, so it falls back
 * to the fixed syscall-entry validation below. The loader is trusted to supply
 * a callable first field and valid argument storage. */
static int prepare_syscall_args(payload_args_t *args, const unsigned char *code,
                                size_t available, payload_args_t *prepared)
{
    void *resolved = 0;
    *prepared = *args;
    int error = args->sys_dynlib_dlsym(0x1, "sceKernelDlsym", &resolved);
    if (error)
        error = args->sys_dynlib_dlsym(0x2001, "sceKernelDlsym", &resolved);
    if (!error && resolved) {
        prepared->sys_dynlib_dlsym =
            (int (*)(int, const char *, void *))(uintptr_t)resolved;
        return 1;
    }
    return supported_syscall_entry(code, available);
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
    if (platform_write && !stream_log_failed &&
        platform_write(2, data, len) != (long)len)
        stream_log_failed = 1;
    persistent_log(data, len);
}

static int open_persistent_log(void)
{
    persistent_fd = (int)__crt_syscall(SYS_open, PS5_DIAGNOSTIC_LOG_PATH,
                                       PS5_O_WRONLY | PS5_O_CREAT | PS5_O_TRUNC,
                                       PS5_DIAGNOSTIC_LOG_MODE);
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
    static const uint8_t started[] =
        "event=payload_start log=" PS5_DIAGNOSTIC_LOG_PATH "\n";
    static const char notice[] =
        "PS5-BT diagnostic started\nLog: " PS5_DIAGNOSTIC_LOG_PATH;
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
    if (!bootstrap_failure && stream_log_failed)
        status = 124;
    if (!bootstrap_failure && persistent_log_failed)
        status = PS5_PERSISTENT_LOG_FAILED;
    if (!bootstrap_failure && notification_failed)
        status = PS5_NOTIFICATION_FAILED;
    return status;
}

static int report_finish(int primary_status)
{
    int bootstrap_failure = primary_status >= 120 && primary_status <= 125;
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
    length = format_status(notice, sizeof notice, "PS5-BT diagnostic finished\nStatus: ",
                           final_status);
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
    if (error)
        *out = 0;
    return error ? error : *out ? 0 : PS5_BT_IO;
}

void ps5_platform_log(const uint8_t *data, uint32_t len)
{
    lifecycle_log(data, len);
}

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
     * invoke this entry concurrently. No kernel-R/W field is accessed here. */
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
    void *symbol = 0;
    payload_args_t syscall_args;
    int contract_ready =
        prepare_syscall_args(args, (const unsigned char *)(uintptr_t)args->sys_dynlib_dlsym,
                             PS5_SYSCALL_ENTRY_OFFSET + PS5_SYSCALL_ENTRY_SIZE,
                             &syscall_args);
    if (!contract_ready) {
        result = 125;
    } else if (__crt_syscall_init(&syscall_args)) {
        result = 121;
    } else {
        resolver_ready = 1;
        (void)open_persistent_log();
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
        result = report_finish(result);
    }
    resolver_ready = 0;
    *args->payloadout = result;
    return result;
}

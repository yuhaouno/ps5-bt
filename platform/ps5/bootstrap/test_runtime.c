/* SPDX-License-Identifier: GPL-3.0-or-later */
/* HOST ONLY: exercises startup errors with a synthetic syscall resolver.
 * The linker BSS interval is intentionally empty; this does not test a loader. */
#include <assert.h>
#include <stdarg.h>
#include <string.h>
unsigned char test_empty_bss[1];
#define __bss_start test_empty_bss
#define __bss_end test_empty_bss
#define main test_probe
#undef memset
#undef memcpy
#undef memcmp
#define memset bootstrap_memset
#define memcpy bootstrap_memcpy
#define memcmp bootstrap_memcmp
#include "runtime.c"
#undef main
#undef memset
#undef memcpy
#undef memcmp

enum test_mode {
    MODE_OK,
    MODE_SYSCALL_INIT_FAIL,
    MODE_WRITE_RESOLVE_ERROR,
    MODE_WRITE_RESOLVE_NULL,
    MODE_EXIT_RESOLVE_ERROR,
    MODE_EXIT_RESOLVE_NULL,
    MODE_FIRST_MODULE_ERROR,
    MODE_STREAM_SHORT,
    MODE_STREAM_ERROR,
    MODE_FILE_OPEN_FAIL,
    MODE_FILE_SHORT,
    MODE_FILE_CLOSE_FAIL,
    MODE_NOTIFY_RESOLVE_ERROR,
    MODE_NOTIFY_RESOLVE_NULL,
    MODE_NOTIFY_START_FAIL,
    MODE_NOTIFY_FINISH_FAIL,
};

static enum test_mode mode;
static int resolutions, writes, probes, opens, file_writes, closes, notifications;
static char file_log[4096];
static size_t file_log_length;
static const unsigned char fake_stub[] = {0x48, 0xc7, 0xc0, 0x14, 0,    0,
                                          0,    0x49, 0x89, 0xca, 0x0f, 0x05};
static const unsigned char unknown_stub[sizeof fake_stub] = {0};

static long fake_write(int fd, const void *data, unsigned long len)
{
    assert(fd == 2 && data && len);
    ++writes;
    return mode == MODE_STREAM_SHORT ? (long)len - 1
                                     : mode == MODE_STREAM_ERROR ? -1 : (long)len;
}

static void fake_exit(int status)
{
    (void)status;
    assert(!"unexpected exit");
}

static int fake_notify(int kind, void *data, size_t size, int flags)
{
    struct notify_request *request = data;
    assert(kind == 0 && request && size == sizeof *request && flags == 0);
    for (size_t i = 0; i < sizeof request->reserved; ++i)
        assert(request->reserved[i] == 0);
    assert(memchr(request->message, '\0', sizeof request->message));
    ++notifications;
    if (notifications == 1) {
        assert(!strcmp(request->message,
                       "PS5-BT diagnostic started\nLog: /data/ps5-bt-hw1.log"));
    } else {
        assert(strstr(request->message, "PS5-BT diagnostic finished\nStatus: "));
    }
    if (mode == MODE_NOTIFY_START_FAIL && notifications == 1)
        return -1;
    if (mode == MODE_NOTIFY_FINISH_FAIL && notifications == 2)
        return -1;
    return 0;
}

int __crt_syscall_init(payload_args_t *args)
{
    assert(args && (uintptr_t)args->sys_dynlib_dlsym == (uintptr_t)fake_stub);
    return mode == MODE_SYSCALL_INIT_FAIL ? -1 : 0;
}

long __crt_syscall(long number, ...)
{
    va_list args;
    va_start(args, number);
    if (number == SYS_open) {
        const char *path = va_arg(args, const char *);
        int flags = va_arg(args, int);
        int permissions = va_arg(args, int);
        va_end(args);
        assert(!strcmp(path, PS5_DIAGNOSTIC_LOG_PATH));
        assert(flags == (PS5_O_WRONLY | PS5_O_CREAT | PS5_O_TRUNC));
        assert(permissions == PS5_DIAGNOSTIC_LOG_MODE);
        ++opens;
        return mode == MODE_FILE_OPEN_FAIL ? -1 : 42;
    }
    if (number == SYS_write) {
        int fd = va_arg(args, int);
        const void *data = va_arg(args, const void *);
        unsigned long len = va_arg(args, unsigned long);
        va_end(args);
        assert(fd == 42 && data && len);
        ++file_writes;
        if (mode == MODE_FILE_SHORT)
            return (long)len - 1;
        assert(file_log_length + len < sizeof file_log);
        memcpy(file_log + file_log_length, data, len);
        file_log_length += len;
        file_log[file_log_length] = '\0';
        return (long)len;
    }
    if (number == SYS_close) {
        int fd = va_arg(args, int);
        va_end(args);
        assert(fd == 42);
        ++closes;
        return mode == MODE_FILE_CLOSE_FAIL ? -1 : 0;
    }
    assert(number == SYS_sprx_dlsym);
    int module = va_arg(args, int);
    const char *name = va_arg(args, const char *);
    void *out = va_arg(args, void *);
    va_end(args);
    assert(module == 1 || module == 0x2001);
    ++resolutions;
    if (mode == MODE_FIRST_MODULE_ERROR && module == 1)
        return 13;
    if (!strcmp(name, "write")) {
        if (mode == MODE_WRITE_RESOLVE_ERROR)
            return 13;
        if (mode != MODE_WRITE_RESOLVE_NULL)
            *(void **)out = (void *)fake_write;
    } else if (!strcmp(name, "_exit")) {
        if (mode == MODE_EXIT_RESOLVE_ERROR)
            return 13;
        if (mode != MODE_EXIT_RESOLVE_NULL)
            *(void **)out = (void *)fake_exit;
    } else {
        assert(!strcmp(name, "sceKernelSendNotificationRequest"));
        if (mode == MODE_NOTIFY_RESOLVE_ERROR)
            return 13;
        if (mode != MODE_NOTIFY_RESOLVE_NULL)
            *(void **)out = (void *)fake_notify;
    }
    return 0;
}

int test_probe(void)
{
    const uint8_t message[] = "synthetic";
    ++probes;
    ps5_platform_log(message, sizeof message - 1);
    ps5_platform_log(message, sizeof message - 1);
    return 2;
}

static void reset_test(enum test_mode selected)
{
    mode = selected;
    resolutions = writes = probes = opens = file_writes = closes = notifications = 0;
    file_log_length = 0;
    file_log[0] = '\0';
}

static int run(payload_args_t *args, enum test_mode selected)
{
    reset_test(selected);
    *args->payloadout = -999;
    int result = ps5_bootstrap_entry(args);
    assert(*args->payloadout == result);
    return result;
}

int main(void)
{
    int result = -999;
    assert(supported_stub_bytes(fake_stub, sizeof fake_stub));
    assert(!supported_stub_bytes(0, 0));
    for (size_t i = 0; i < sizeof fake_stub; ++i) {
        assert(!supported_stub_bytes(fake_stub, i));
        unsigned char changed[sizeof fake_stub];
        memcpy(changed, fake_stub, sizeof changed);
        changed[i] ^= 1;
        assert(!supported_stub_bytes(changed, sizeof changed));
    }
    payload_args_t args = {.sys_dynlib_dlsym =
                               (int (*)(int, const char *, void *))(uintptr_t)fake_stub,
                           .payloadout = &result};
    reset_test(MODE_OK);
    assert(ps5_bootstrap_entry(0) == 120);
    payload_args_t invalid = args;
    invalid.payloadout = 0;
    assert(ps5_bootstrap_entry(&invalid) == 120);
    invalid = args;
    invalid.sys_dynlib_dlsym = 0;
    assert(ps5_bootstrap_entry(&invalid) == 120);
    invalid = args;
    invalid.sys_dynlib_dlsym = (int (*)(int, const char *, void *))(uintptr_t)unknown_stub;
    assert(ps5_bootstrap_entry(&invalid) == 125 && result == 125);
    assert(resolutions == 0 && writes == 0 && probes == 0 && opens == 0);

    assert(run(&args, MODE_OK) == 2);
    assert(probes == 1 && opens == 1 && closes == 1 && notifications == 2);
    assert(strstr(file_log, "event=payload_start log=/data/ps5-bt-hw1.log\n"));
    assert(strstr(file_log, "syntheticsynthetic"));
    assert(strstr(file_log, "event=payload_exit status=2 (expected Unsupported)\n"));
    assert(strstr(file_log, "event=payload_return status=2 (expected Unsupported)\n"));

    assert(run(&args, MODE_SYSCALL_INIT_FAIL) == 121);
    assert(probes == 0 && opens == 0 && notifications == 0);
    assert(run(&args, MODE_WRITE_RESOLVE_ERROR) == 122);
    assert(probes == 0 && opens == 1 && closes == 1);
    assert(strstr(file_log, "event=payload_return status=122\n"));
    assert(run(&args, MODE_WRITE_RESOLVE_NULL) == 122);
    assert(run(&args, MODE_EXIT_RESOLVE_ERROR) == 123);
    assert(probes == 0 && opens == 1 && closes == 1);
    assert(run(&args, MODE_EXIT_RESOLVE_NULL) == 123);
    assert(run(&args, MODE_FIRST_MODULE_ERROR) == 2);
    assert(resolutions == 6 && probes == 1);
    assert(run(&args, MODE_STREAM_SHORT) == 124);
    assert(probes == 1 && writes == 1);
    assert(run(&args, MODE_STREAM_ERROR) == 124);
    assert(run(&args, MODE_FILE_OPEN_FAIL) == PS5_PERSISTENT_LOG_FAILED);
    assert(probes == 1 && notifications == 2 && closes == 0);
    assert(run(&args, MODE_FILE_SHORT) == PS5_PERSISTENT_LOG_FAILED);
    assert(run(&args, MODE_FILE_CLOSE_FAIL) == PS5_PERSISTENT_LOG_FAILED);
    assert(run(&args, MODE_NOTIFY_RESOLVE_ERROR) == PS5_NOTIFICATION_FAILED);
    assert(probes == 1 && notifications == 0);
    assert(strstr(file_log, "event=payload_return status=127\n"));
    assert(run(&args, MODE_NOTIFY_RESOLVE_NULL) == PS5_NOTIFICATION_FAILED);
    assert(run(&args, MODE_NOTIFY_START_FAIL) == PS5_NOTIFICATION_FAILED);
    assert(notifications == 2);
    assert(run(&args, MODE_NOTIFY_FINISH_FAIL) == PS5_NOTIFICATION_FAILED);
    unsigned char memory[5];
    assert(bootstrap_memset(memory, 0xab, 5) == memory);
    assert(memory[0] == 0xab && memory[4] == 0xab);
    assert(bootstrap_memcpy(memory, "abcde", 5) == memory);
    assert(bootstrap_memcmp(memory, "abcde", 5) == 0);
    assert(bootstrap_memcmp(memory, "abddd", 5) < 0);
    assert(bootstrap_memcmp(memory, "abbde", 5) > 0);
    void *symbol = (void *)1;
    assert(ps5_bootstrap_lookup("write", &symbol) == PS5_BT_IO && !symbol);
    symbol = (void *)1;
    assert(ps5_bootstrap_lookup(0, &symbol) == PS5_BT_INVALID && !symbol);
    assert(ps5_bootstrap_lookup("write", 0) == PS5_BT_INVALID);
    resolver_ready = 1;
    mode = MODE_WRITE_RESOLVE_ERROR;
    symbol = (void *)1;
    assert(ps5_bootstrap_lookup("write", &symbol) == 13 && !symbol);
    mode = MODE_WRITE_RESOLVE_NULL;
    assert(ps5_bootstrap_lookup("write", &symbol) == PS5_BT_IO && !symbol);
    return 0;
}

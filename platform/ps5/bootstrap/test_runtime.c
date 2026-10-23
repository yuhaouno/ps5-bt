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

static int mode, resolutions, writes, probes;
static const unsigned char fake_stub[] = {0x48, 0xc7, 0xc0, 0x14, 0,    0,
                                          0,    0x49, 0x89, 0xca, 0x0f, 0x05};
static const unsigned char unknown_stub[sizeof fake_stub] = {0};

static long fake_write(int fd, const void *data, unsigned long len)
{
    assert(fd == 2 && data && len);
    ++writes;
    return mode == 7 ? (long)len - 1 : mode == 8 ? -1 : (long)len;
}

static void fake_exit(int status)
{
    (void)status;
    assert(!"unexpected exit");
}

int __crt_syscall_init(payload_args_t *args)
{
    assert(args && (uintptr_t)args->sys_dynlib_dlsym == (uintptr_t)fake_stub);
    return mode == 1 ? -1 : 0;
}

long __crt_syscall(long number, ...)
{
    va_list args;
    va_start(args, number);
    int module = va_arg(args, int);
    const char *name = va_arg(args, const char *);
    void *out = va_arg(args, void *);
    va_end(args);
    assert(number == SYS_sprx_dlsym && (module == 1 || module == 0x2001));
    ++resolutions;
    if (mode == 6 && module == 1)
        return 13;
    if (!strcmp(name, "write")) {
        if (mode == 2)
            return 13;
        if (mode != 3)
            *(void **)out = (void *)fake_write;
    } else {
        assert(!strcmp(name, "_exit"));
        if (mode == 4)
            return 13;
        if (mode != 5)
            *(void **)out = (void *)fake_exit;
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

int main(void)
{
    int result;
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
    assert(resolutions == 0 && writes == 0 && probes == 0);
    const int expected[] = {2, 121, 122, 122, 123, 123, 2, 124, 124};
    for (mode = 0; mode < 9; ++mode) {
        resolutions = writes = probes = 0;
        result = -999;
        assert(ps5_bootstrap_entry(&args) == expected[mode]);
        assert(result == expected[mode]);
        assert(probes == (mode == 0 || mode >= 6));
        if (mode == 0 || mode == 6)
            assert(writes == 2);
        if (mode == 7 || mode == 8)
            assert(writes == 1);
        if (mode >= 1 && mode <= 5)
            assert(writes == 0);
        if (mode == 6)
            assert(resolutions == 4);
    }
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
    mode = 2;
    symbol = (void *)1;
    assert(ps5_bootstrap_lookup("write", &symbol) == 13 && !symbol);
    mode = 3;
    assert(ps5_bootstrap_lookup("write", &symbol) == PS5_BT_IO && !symbol);
    return 0;
}

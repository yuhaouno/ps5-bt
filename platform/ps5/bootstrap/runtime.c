/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Minimal startup; deliberately excludes SDK crt.c, kernel.c and patch.c. */
#include "bootstrap.h"
#include "ps5_bt.h"
#include "syscall.h"
#include <stddef.h>

typedef long (*write_fn)(int, const void *, unsigned long);
typedef void (*exit_fn)(int);
static write_fn platform_write;
static exit_fn platform_exit;
static int log_failed;
static int resolver_ready;

/* Strict accepted getpid instruction layout for the SDK's +0xa convention.
 * Actual firmware bytes are UNKNOWN; any other layout is rejected, not guessed.
 * Accept only the selected elfldr/websrv stub convention, before any callback.
 * The trusted loader must supply a readable code pointer with at least 12 bytes. */
static int supported_stub_bytes(const unsigned char *code, size_t available)
{
    static const unsigned char expected[] = {0x48, 0xc7, 0xc0, 0x14, 0,    0,
                                             0,    0x49, 0x89, 0xca, 0x0f, 0x05};
    if (!code || available < sizeof expected)
        return 0;
    for (size_t i = 0; i < sizeof expected; ++i) {
        if (code[i] != expected[i])
            return 0;
    }
    return 1;
}

/* The pinned loader places payloadout at byte 0x28 in this upstream structure. */
_Static_assert(sizeof(void *) == 8, "requires x86-64 pointers");
_Static_assert(sizeof(write_fn) == sizeof(void *), "requires equal code/data pointer sizes");
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
    if (!data || !len || log_failed)
        return;
    if (!platform_write || platform_write(2, data, len) != (long)len) {
        /* A short/failed write is terminal for logging; do not hide it by retry. */
        log_failed = 1;
    }
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
    log_failed = 0;
    resolver_ready = 0;
    void *symbol = 0;
    if (!supported_stub_bytes((const unsigned char *)(uintptr_t)args->sys_dynlib_dlsym, 12)) {
        result = 125;
    } else if (__crt_syscall_init(args)) {
        result = 121;
    } else {
        resolver_ready = 1;
        if (ps5_bootstrap_lookup("write", &symbol)) {
            result = 122;
        } else {
            /* x86-64 System V uses the same representation for these pointers. */
            platform_write = (write_fn)symbol;
            if (ps5_bootstrap_lookup("_exit", &symbol)) {
                result = 123;
            } else {
                platform_exit = (exit_fn)symbol;
                result = main();
                if (log_failed)
                    result = 124;
            }
        }
    }
    resolver_ready = 0;
    *args->payloadout = result;
    return result;
}

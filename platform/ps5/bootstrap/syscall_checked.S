/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Preserve all six syscall arguments and capture carry before C can change it.
 * The selected SDK bridge invokes the existing libkernel syscall entry.
 * FreeBSD raw errors are CF=1, rax=errno; expose them to C as -errno.
 * mmap alone retains MAP_FAILED (-1), as required by the selected SDK. */
.text
#ifdef __APPLE__
#define CHECKED ___wrap___crt_syscall
#define RAW ___real___crt_syscall
#else
#define CHECKED __wrap___crt_syscall
#define RAW __real___crt_syscall
.type CHECKED, @function
#endif
.global CHECKED
CHECKED:
    subq $24, %rsp
    movq %rdi, 8(%rsp)
    movq 32(%rsp), %rax
    movq %rax, (%rsp)
    call RAW
    jnc 1f
    cmpq $477, 8(%rsp)
    je 2f
    negq %rax
1:
    leaq 24(%rsp), %rsp
    ret
2:
    movq $-1, %rax
    jmp 1b
#ifndef __APPLE__
.size CHECKED, .-CHECKED
.section .note.GNU-stack,"",@progbits
#endif

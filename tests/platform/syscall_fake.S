/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Host-only stand-in: verify the wrapper preserves every argument and SP.
 * This never executes a syscall or uses a PS5 address. */
.text
#ifdef __APPLE__
#define RAW ___real___crt_syscall
#else
#define RAW __real___crt_syscall
#endif
.global RAW
RAW:
    cmpq $11, %rsi
    jne 9f
    cmpq $22, %rdx
    jne 9f
    cmpq $33, %rcx
    jne 9f
    cmpq $44, %r8
    jne 9f
    cmpq $55, %r9
    jne 9f
    cmpq $66, 8(%rsp)
    jne 9f
    movq %rsp, %rax
    andq $15, %rax
    cmpq $8, %rax
    jne 9f
    cmpq $1, %rdi
    je 1f
    cmpq $477, %rdi
    je 1f
    cmpq $2, %rdi
    je 2f
    movq 8(%rsp), %rax
    clc
    ret
1:
    movq $13, %rax
    stc
    ret
2:
    movq $-1, %rax
    clc
    ret
9:
    movq $999, %rax
    clc
    ret
#ifndef __APPLE__
.section .note.GNU-stack,"",@progbits
#endif

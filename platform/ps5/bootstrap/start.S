/* SPDX-License-Identifier: GPL-3.0-or-later */
/* The audited loader supplies rdi and a return address, but not stack alignment. */
.text
.global _start
.type _start, @function
_start:
    push %rbp
    mov %rsp, %rbp
    and $-16, %rsp
    call ps5_bootstrap_entry
    mov %rbp, %rsp
    pop %rbp
    ret
.size _start, .-_start
.section .note.GNU-stack,"",@progbits

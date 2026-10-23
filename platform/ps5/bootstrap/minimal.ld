/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Every load starts on a PS5 16 KiB page, even without unwind/data sections. */
OUTPUT_FORMAT("elf64-x86-64")
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)
PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
    dynamic PT_DYNAMIC FLAGS(6);
    stack PT_GNU_STACK FLAGS(6);
}
SECTIONS {
    . = 0;
    .text : { *(.text .text.*) } :text
    . = ALIGN(0x4000);
    .rodata : { *(.rodata .rodata.*) } :rodata
    .dynsym : { *(.dynsym) } :rodata
    .dynstr : { *(.dynstr) } :rodata
    .gnu.hash : { *(.gnu.hash) } :rodata
    .hash : { *(.hash) } :rodata
    .rela.dyn : { *(.rela.dyn) } :rodata
    .eh_frame_hdr : { *(.eh_frame_hdr) } :rodata
    .eh_frame : { *(.eh_frame) } :rodata
    . = ALIGN(0x4000);
    .data : { *(.data .data.*) *(.data.rel.ro .data.rel.ro.*) *(.got .got.*) } :data
    .dynamic : { *(.dynamic) } :data :dynamic
    . = ALIGN(8);
    .bss : {
        __bss_start = .;
        *(.bss .bss.*) *(COMMON)
        __bss_end = .;
    } :data
    /DISCARD/ : { *(.comment) *(.note.GNU-stack) }
}

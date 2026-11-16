/* Flat image at 1 MiB. QEMU's PVH loader reads the PT_NOTE segment to find the 32-bit entry point. */
ENTRY(_start32)

PHDRS
{
    text   PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data   PT_LOAD FLAGS(6);
    note   PT_NOTE FLAGS(4);
}

SECTIONS
{
    . = 1M;
    .text : { *(.text.boot) *(.text .text.*) } :text

    . = ALIGN(4K);
    .rodata : { *(.rodata .rodata.*) } :rodata
    .note : { KEEP(*(.note.Xen)) } :rodata :note

    . = ALIGN(4K);
    .data : { *(.data .data.*) } :data
    .bss : { *(.bss .bss.*) *(COMMON) } :data

    /DISCARD/ : { *(.eh_frame .eh_frame_hdr .comment) }
}

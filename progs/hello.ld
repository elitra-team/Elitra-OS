SECTIONS {
    . = 0x00800000;
    .text : { *(.text) }
    .data : { *(.data) }
    .bss  : { *(.bss) }
}

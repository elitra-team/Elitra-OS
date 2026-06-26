OUTPUT_FORMAT(elf64-x86-64)

PHDRS {
    text PT_LOAD FLAGS(5);
    data PT_LOAD FLAGS(6);
}

SECTIONS {
    . = 0x00800000;

    .text : {
        *(.text)
        *(.text.*)
    } :text

    .rodata : ALIGN(0x1000) {
        *(.rodata)
        *(.rodata.*)
        *(.eh_frame)
        *(.gcc_except_table.*)
    } :text

    .data : ALIGN(0x1000) {
        *(.data)
        *(.data.*)
        *(.got)
        *(.got.*)
        *(.got.plt)
    } :data

    .bss : {
        *(COMMON)
        *(.bss)
        *(.bss.*)
    } :data
}

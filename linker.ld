ENTRY(_start)

PHDRS
{
    text    PT_LOAD FLAGS(5);
    data    PT_LOAD FLAGS(6);
    note    PT_NOTE FLAGS(4);
}

SECTIONS
{
    . = 1M;

    .multiboot : {
        *(.multiboot)
    } :text

    .text : {
        *(.text)
        *(.text.*)
    } :text

    .rodata : {
        *(.rodata)
        *(.rodata.*)
    } :text

    .note : {
        *(.note)
        *(.note.*)
    } :text :note

    .data : {
        *(.data)
        *(.data.*)
    } :data

    .init_array : {
        __init_array_start = .;
        *(.init_array)
        *(.init_array.*)
        __init_array_end = .;
    } :data

    .fini_array : {
        __fini_array_start = .;
        *(.fini_array)
        *(.fini_array.*)
        __fini_array_end = .;
    } :data

    .got : {
        *(.got)
        *(.got.*)
    } :data

    .got.plt : {
        *(.got.plt)
        *(.got.plt.*)
    } :data

    .eh_frame : {
        *(.eh_frame)
        *(.eh_frame.*)
    } :data

    .bss : {
        *(COMMON)
        *(.bss)
        *(.bss.*)
    } :data

    _kernel_end = .;
}

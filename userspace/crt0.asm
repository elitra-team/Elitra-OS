global _start
extern rust_main

section .text
bits 64
_start:
    ; rax = argc, rbx = argv (set in ISR frame by kernel)
    mov rdi, rax
    mov rsi, rbx
    call rust_main
    mov eax, 1
    int 0x80

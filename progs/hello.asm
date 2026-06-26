global _start
section .text
_start:
    mov eax, 0          ; sys_write
    mov ebx, msg
    int 0x80

    mov eax, 1          ; sys_exit
    int 0x80

section .data
msg: db "Hello from ELF!", 10

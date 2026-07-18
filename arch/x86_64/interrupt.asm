; 64-bit ISR stubs for exceptions 0-31
; Exceptions with error code: 8, 10, 11, 12, 13, 14, 17, 21, 30

bits 64

%macro ISR_NOERR 1
global isr%1
isr%1:
    cli
    push 0              ; dummy error code
    push %1             ; interrupt number
    jmp isr_common_stub
%endmacro

%macro ISR_ERR 1
global isr%1
isr%1:
    cli
    push %1             ; interrupt number (error code already on stack)
    jmp isr_common_stub
%endmacro

ISR_NOERR 0
ISR_NOERR 1
ISR_NOERR 2
ISR_NOERR 3
ISR_NOERR 4
ISR_NOERR 5
ISR_NOERR 6
ISR_NOERR 7
ISR_ERR   8
ISR_NOERR 9
ISR_ERR   10
ISR_ERR   11
ISR_ERR   12
ISR_ERR   13
ISR_ERR   14
ISR_NOERR 15
ISR_NOERR 16
ISR_ERR   17
ISR_NOERR 18
ISR_NOERR 19
ISR_NOERR 20
ISR_ERR   21
ISR_NOERR 22
ISR_NOERR 23
ISR_NOERR 24
ISR_NOERR 25
ISR_NOERR 26
ISR_NOERR 27
ISR_NOERR 28
ISR_NOERR 29
ISR_ERR   30
ISR_NOERR 31

; IRQ stubs (remapped to interrupts 32-47)
%macro IRQ 2
global irq%1
irq%1:
    cli
    push 0              ; dummy error code
    push %2             ; interrupt number
    jmp irq_common_stub
%endmacro

IRQ 0, 32
IRQ 1, 33
IRQ 2, 34
IRQ 3, 35
IRQ 4, 36
IRQ 5, 37
IRQ 6, 38
IRQ 7, 39
IRQ 8, 40
IRQ 9, 41
IRQ 10, 42
IRQ 11, 43
IRQ 12, 44
IRQ 13, 45
IRQ 14, 46
IRQ 15, 47

; Reschedule IPI (vector 0x40) — used for SMP cross-CPU scheduling
global isr_reschedule
isr_reschedule:
    cli
    push 0              ; dummy error code
    push 0x40           ; interrupt number
    jmp isr_common_stub

; e1000 network card interrupt (vector 0x41 = 65)
global isr_e1000
isr_e1000:
    cli
    push 0              ; dummy error code
    push 0x41           ; interrupt number
    jmp isr_common_stub

; Common ISR handler
extern isr_handler
isr_common_stub:
    cld
    push rax
    push rcx
    push rdx
    push rbx
    push rsp
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    sub rsp, 8            ; align stack to 16 bytes (23 pushes = 184 ≡ 8 mod 16)
    mov rdi, rsp
    add rdi, 8            ; skip alignment padding to pass actual frame pointer
    call isr_handler
    add rsp, 8            ; remove alignment padding
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rsp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    add rsp, 16          ; skip int_no and err_code
    iretq

; Common IRQ handler
extern irq_handler
irq_common_stub:
    cld
    push rax
    push rcx
    push rdx
    push rbx
    push rsp
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    sub rsp, 8            ; align stack to 16 bytes
    mov rdi, rsp
    add rdi, 8            ; skip alignment padding to pass actual frame pointer
    call irq_handler
    add rsp, 8            ; remove alignment padding
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rsp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    add rsp, 16
    iretq

; GDT flush
global gdt_flush
gdt_flush:
    lgdt [rdi]
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    push 0x08
    push .flush
    retfq
.flush:
    ret

; IDT flush
global idt_flush
idt_flush:
    lidt [rdi]
    ret

; Syscall entry via MSR (syscall instruction)
; rcx = return rip, r11 = saved rflags
extern syscall_handler_c

section .data
align 8
syscall_scratch_rsp: dq 0
syscall_scratch_r15: dq 0

section .text
global syscall_entry
syscall_entry:
    swapgs
    mov [rel syscall_scratch_r15], r15
    mov [rel syscall_scratch_rsp], rsp
    mov rsp, [gs:0]              ; per-CPU kernel stack (gs -> PerCpuData, offset 0 = kernel_stack)
    cld
    ; Build full pt_regs frame
    push 0x23                   ; ss (user data segment)
    push qword [rel syscall_scratch_rsp]  ; user rsp
    push r11                    ; rflags
    push 0x1B                   ; cs (user code segment)
    push rcx                    ; rip
    push 0                      ; err_code
    push 0x80                   ; int_no
    push rax
    push rcx
    push rdx
    push rbx
    push qword [rel syscall_scratch_rsp]  ; rsp slot (user rsp for C handler access)
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push qword [rel syscall_scratch_r15]  ; r15 slot (user's real r15)
    sub rsp, 8                 ; align stack to 16 bytes
    mov rdi, rsp
    add rdi, 8                 ; skip alignment padding to pass actual frame pointer
    call syscall_handler_c
    add rsp, 8                 ; remove alignment padding
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    add rsp, 8                 ; skip rsp slot (don't clobber restored r15)
    pop rbx
    pop rdx
    pop rcx
    pop rax
    add rsp, 16                 ; skip int_no and err_code
    ; Stack now: [rip, cs, rflags, user_rsp, ss]
    swapgs
    iretq

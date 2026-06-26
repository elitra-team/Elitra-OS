; 64-bit task context switch
; Calling convention: System V AMD64 (rdi, rsi, rdx, rcx, r8, r9)

bits 64

; Swap RSP (cooperative yield helper)
; Save/restore callee-saved regs per SysV AMD64 ABI
; void context_switch(uint64_t *old_rsp, uint64_t new_rsp)
global context_switch
context_switch:
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp            ; *old_rsp = current rsp (points to saved r15)
    mov rsp, rsi              ; rsp = new_rsp
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    ret

; Resume a task from its saved ISR frame
; TaskContext { uint64_t rsp; uint64_t cr3; }
global task_resume
task_resume:
    mov rax, [rdi + 8]       ; cr3
    test rax, rax
    jz .no_cr3
    mov cr3, rax
.no_cr3:
    mov rsp, [rdi]           ; rsp = ctx->rsp (points to r15 in ISR frame)
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
    add rsp, 8               ; skip saved rsp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    add rsp, 16              ; skip int_no, err_code
    iretq

; Voluntarily yield CPU (build ISR frame and call yield_handler_c)
global yield_task
extern yield_handler_c
yield_task:
    cli
    pop rax                  ; return address
    ; Build full ISR frame matching isr_common_stub layout
    push 0                   ; ss (dummy)
    push 0                   ; user_rsp (dummy)
    push rax                 ; rflags (will fix up)
    push 0x08                ; cs (kernel code)
    push rax                 ; rip
    push 0                   ; err_code
    push 0                   ; int_no
    push rax                 ; rax
    push rcx                 ; rcx
    push rdx                 ; rdx
    push rbx                 ; rbx
    push rsp                 ; rsp (current)
    push rbp                 ; rbp
    push rsi                 ; rsi
    push rdi                 ; rdi
    push r8                  ; r8
    push r9                  ; r9
    push r10                 ; r10
    push r11                 ; r11
    push r12                 ; r12
    push r13                 ; r13
    push r14                 ; r14
    push r15                 ; r15
    ; Fix up rflags
    mov qword [rsp + 160], 0x202  ; fix rflags in the frame (offset 20*8 from rsp)
    mov rdi, rsp
    call yield_handler_c
    ; If yield_handler returns (no other task), clean up
    add rsp, 184             ; pop entire frame (23 qwords)
    sti
    ret

; Syscall entry via int 0x80
global syscall_stub
extern syscall_handler_c
syscall_stub:
    cli
    push 0                   ; dummy error code
    push 0x80                ; int_no
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
    mov rdi, rsp
    call syscall_handler_c
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

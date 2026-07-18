; AP (Application Processor) trampoline
; Compiled as flat binary, will be loaded at physical 0x8000 by BSP
; APs start in 16-bit real mode after INIT-SIPI-SIPI

org 0x8000

bits 16
global ap_trampoline_start
ap_trampoline_start:

    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00

    ; Enable A20
    in al, 0x92
    or al, 2
    out 0x92, al

    ; Load 32-bit GDT
    lgdt [gdt32_ptr]

    ; Enter protected mode
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:.pm32

bits 32
.pm32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov esp, 0x8000

    ; Load PML4 from communication block
    mov eax, [comm_cr3]
    mov cr3, eax

    ; Enable PAE + PGE + SMEP/SMAP from communication block
    mov eax, [comm_cr4]
    or eax, (1 << 5) | (1 << 7)
    mov cr4, eax

    ; Enable long mode (LME) in EFER
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 8
    wrmsr

    ; Enable paging
    mov eax, cr0
    or eax, 1 << 31
    mov cr0, eax
    jmp 0x08:.lm64

bits 64
.lm64:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax

    ; Set up per-CPU kernel stack
    mov rsp, [comm_stack]

    ; Store AP ack
    mov eax, [comm_apic_id]
    mov [comm_ack], eax

    ; Call Rust AP entry point
    mov rdi, [comm_percpu_ptr]
    mov rax, [comm_entry]
    call rax

.hang:
    cli
    hlt
    jmp .hang

; ─── 32-bit GDT ────────────────────────────────────────────────
align 16
gdt32:
    dq 0                    ; null
    dq 0x00209A0000000000   ; 64-bit kernel code (0x08)
    dq 0x0000920000000000   ; 64-bit kernel data (0x10)
gdt32_end:

gdt32_ptr:
    dw gdt32_end - gdt32 - 1
    dd gdt32

; ─── Communication block ───────────────────────────────────────
; All offsets relative to 0x8000
; BSP writes before SIPI, AP reads after startup
comm_apic_id:   dd 0        ; 0x8040 — target APIC ID
comm_ack:       dd 0        ; 0x8044 — AP writes its ID here
comm_cr3:       dq 0        ; 0x8048 — PML4 physical address
comm_cr4:       dq 0        ; 0x8050 — CR4 value (BSP sets SMEP/SMAP)
comm_stack:     dq 0        ; 0x8058 — per-AP kernel stack (top)
comm_percpu_ptr:dq 0        ; 0x8060 — pointer to PerCpuData
comm_entry:     dq 0        ; 0x8068 — Rust ap_main() function pointer

ap_trampoline_end:

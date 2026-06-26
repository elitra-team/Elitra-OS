#include "isr.hpp"
#include "idt.hpp"
#include "port.hpp"
#include "vga.hpp"
#include "lib.hpp"

using namespace arch::x86;

ISRHandler ISR::handlers[256];

static const char *exception_messages[] = {
    "Division By Zero",
    "Debug",
    "Non Maskable Interrupt",
    "Breakpoint",
    "Overflow",
    "BOUND Range Exceeded",
    "Invalid Opcode",
    "Device Not Available",
    "Double Fault",
    "Coprocessor Segment Overrun",
    "Invalid TSS",
    "Segment Not Present",
    "Stack-Segment Fault",
    "General Protection Fault",
    "Page Fault",
    "Reserved",
    "x87 FPU Floating-Point Error",
    "Alignment Check",
    "Machine Check",
    "SIMD Floating-Point Exception",
    "Virtualization Exception",
    "Control Protection Exception",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Hypervisor Injection Exception",
    "VMM Communication Exception",
    "Security Exception",
    "Reserved"
};

extern "C" {
extern void isr0(void);  extern void isr1(void);  extern void isr2(void);  extern void isr3(void);
extern void isr4(void);  extern void isr5(void);  extern void isr6(void);  extern void isr7(void);
extern void isr8(void);  extern void isr9(void);  extern void isr10(void); extern void isr11(void);
extern void isr12(void); extern void isr13(void); extern void isr14(void); extern void isr15(void);
extern void isr16(void); extern void isr17(void); extern void isr18(void); extern void isr19(void);
extern void isr20(void); extern void isr21(void); extern void isr22(void); extern void isr23(void);
extern void isr24(void); extern void isr25(void); extern void isr26(void); extern void isr27(void);
extern void isr28(void); extern void isr29(void); extern void isr30(void); extern void isr31(void);
}

void ISR::install() {
    lib::memset(handlers, 0, sizeof(handlers));

    IDT::set_gate(0,  reinterpret_cast<uint64_t>(isr0),  0x08, 0x8E);
    IDT::set_gate(1,  reinterpret_cast<uint64_t>(isr1),  0x08, 0x8E);
    IDT::set_gate(2,  reinterpret_cast<uint64_t>(isr2),  0x08, 0x8E);
    IDT::set_gate(3,  reinterpret_cast<uint64_t>(isr3),  0x08, 0x8E);
    IDT::set_gate(4,  reinterpret_cast<uint64_t>(isr4),  0x08, 0x8E);
    IDT::set_gate(5,  reinterpret_cast<uint64_t>(isr5),  0x08, 0x8E);
    IDT::set_gate(6,  reinterpret_cast<uint64_t>(isr6),  0x08, 0x8E);
    IDT::set_gate(7,  reinterpret_cast<uint64_t>(isr7),  0x08, 0x8E);
    IDT::set_gate(8,  reinterpret_cast<uint64_t>(isr8),  0x08, 0x8E);
    IDT::set_gate(9,  reinterpret_cast<uint64_t>(isr9),  0x08, 0x8E);
    IDT::set_gate(10, reinterpret_cast<uint64_t>(isr10), 0x08, 0x8E);
    IDT::set_gate(11, reinterpret_cast<uint64_t>(isr11), 0x08, 0x8E);
    IDT::set_gate(12, reinterpret_cast<uint64_t>(isr12), 0x08, 0x8E);
    IDT::set_gate(13, reinterpret_cast<uint64_t>(isr13), 0x08, 0x8E);
    IDT::set_gate(14, reinterpret_cast<uint64_t>(isr14), 0x08, 0x8E);
    IDT::set_gate(15, reinterpret_cast<uint64_t>(isr15), 0x08, 0x8E);
    IDT::set_gate(16, reinterpret_cast<uint64_t>(isr16), 0x08, 0x8E);
    IDT::set_gate(17, reinterpret_cast<uint64_t>(isr17), 0x08, 0x8E);
    IDT::set_gate(18, reinterpret_cast<uint64_t>(isr18), 0x08, 0x8E);
    IDT::set_gate(19, reinterpret_cast<uint64_t>(isr19), 0x08, 0x8E);
    IDT::set_gate(20, reinterpret_cast<uint64_t>(isr20), 0x08, 0x8E);
    IDT::set_gate(21, reinterpret_cast<uint64_t>(isr21), 0x08, 0x8E);
    IDT::set_gate(22, reinterpret_cast<uint64_t>(isr22), 0x08, 0x8E);
    IDT::set_gate(23, reinterpret_cast<uint64_t>(isr23), 0x08, 0x8E);
    IDT::set_gate(24, reinterpret_cast<uint64_t>(isr24), 0x08, 0x8E);
    IDT::set_gate(25, reinterpret_cast<uint64_t>(isr25), 0x08, 0x8E);
    IDT::set_gate(26, reinterpret_cast<uint64_t>(isr26), 0x08, 0x8E);
    IDT::set_gate(27, reinterpret_cast<uint64_t>(isr27), 0x08, 0x8E);
    IDT::set_gate(28, reinterpret_cast<uint64_t>(isr28), 0x08, 0x8E);
    IDT::set_gate(29, reinterpret_cast<uint64_t>(isr29), 0x08, 0x8E);
    IDT::set_gate(30, reinterpret_cast<uint64_t>(isr30), 0x08, 0x8E);
    IDT::set_gate(31, reinterpret_cast<uint64_t>(isr31), 0x08, 0x8E);
}

extern "C" void isr_handler(Registers *r) {
    ISR::handler(r);
}

void ISR::handler(Registers *r) {
    if (handlers[r->int_no]) {
        handlers[r->int_no](r);
        return;
    }

    drivers::VGA::printf("\n[PANIC] Unhandled exception: %s (int=0x%x, err=0x%x)\n",
                             exception_messages[r->int_no], r->int_no, r->err_code);
    drivers::VGA::printf("  RIP=0x%lx CS=0x%lx RFLAGS=0x%lx\n", r->rip, r->cs, r->rflags);
    drivers::VGA::printf("  RAX=0x%lx RBX=0x%lx RCX=0x%lx RDX=0x%lx\n",
                             r->rax, r->rbx, r->rcx, r->rdx);
    drivers::VGA::printf("  RSP=0x%lx RBP=0x%lx RSI=0x%lx RDI=0x%lx\n",
                             r->user_rsp, r->rbp, r->rsi, r->rdi);

    arch::x86::disable_interrupts();
    for (;;) {
        __asm__ volatile ("hlt");
    }
}

void ISR::register_handler(uint8_t int_no, ISRHandler handler) {
    handlers[int_no] = handler;
}

void ISR::unregister_handler(uint8_t int_no) {
    handlers[int_no] = nullptr;
}

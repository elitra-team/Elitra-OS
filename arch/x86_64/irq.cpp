#include "irq.hpp"
#include "idt.hpp"
#include "port.hpp"
#include "lib.hpp"

using namespace arch::x86;

ISRHandler IRQ::routines[16];

extern "C" {
extern void irq0(void);  extern void irq1(void);  extern void irq2(void);  extern void irq3(void);
extern void irq4(void);  extern void irq5(void);  extern void irq6(void);  extern void irq7(void);
extern void irq8(void);  extern void irq9(void);  extern void irq10(void); extern void irq11(void);
extern void irq12(void); extern void irq13(void); extern void irq14(void); extern void irq15(void);
}

void IRQ::install_handler(int irq, ISRHandler handler) {
    routines[irq] = handler;
}

void IRQ::uninstall_handler(int irq) {
    routines[irq] = nullptr;
}

void IRQ::install() {
    lib::memset(routines, 0, sizeof(routines));

    disable_interrupts();

    outb(0x20, 0x11);
    outb(0xA0, 0x11);
    outb(0x21, 0x20);
    outb(0xA1, 0x28);
    outb(0x21, 0x04);
    outb(0xA1, 0x02);
    outb(0x21, 0x01);
    outb(0xA1, 0x01);
    outb(0x21, 0x00);
    outb(0xA1, 0x00);

    IDT::set_gate(32, reinterpret_cast<uint64_t>(irq0),  0x08, 0x8E);
    IDT::set_gate(33, reinterpret_cast<uint64_t>(irq1),  0x08, 0x8E);
    IDT::set_gate(34, reinterpret_cast<uint64_t>(irq2),  0x08, 0x8E);
    IDT::set_gate(35, reinterpret_cast<uint64_t>(irq3),  0x08, 0x8E);
    IDT::set_gate(36, reinterpret_cast<uint64_t>(irq4),  0x08, 0x8E);
    IDT::set_gate(37, reinterpret_cast<uint64_t>(irq5),  0x08, 0x8E);
    IDT::set_gate(38, reinterpret_cast<uint64_t>(irq6),  0x08, 0x8E);
    IDT::set_gate(39, reinterpret_cast<uint64_t>(irq7),  0x08, 0x8E);
    IDT::set_gate(40, reinterpret_cast<uint64_t>(irq8),  0x08, 0x8E);
    IDT::set_gate(41, reinterpret_cast<uint64_t>(irq9),  0x08, 0x8E);
    IDT::set_gate(42, reinterpret_cast<uint64_t>(irq10), 0x08, 0x8E);
    IDT::set_gate(43, reinterpret_cast<uint64_t>(irq11), 0x08, 0x8E);
    IDT::set_gate(44, reinterpret_cast<uint64_t>(irq12), 0x08, 0x8E);
    IDT::set_gate(45, reinterpret_cast<uint64_t>(irq13), 0x08, 0x8E);
    IDT::set_gate(46, reinterpret_cast<uint64_t>(irq14), 0x08, 0x8E);
    IDT::set_gate(47, reinterpret_cast<uint64_t>(irq15), 0x08, 0x8E);
}

extern "C" void irq_handler(Registers *r) {
    IRQ::handler(r);
}

void IRQ::handler(Registers *r) {
    if (r->int_no >= 40)
        outb(0xA0, 0x20);
    outb(0x20, 0x20);

    int irq = r->int_no - 32;
    if (irq >= 0 && irq < 16 && routines[irq])
        routines[irq](r);
}

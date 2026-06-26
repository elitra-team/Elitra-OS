#include "idt.hpp"
#include "lib.hpp"

using namespace arch::x86;

IDT::Entry IDT::entries[256];
IDT::Ptr   IDT::ptr;

extern "C" void idt_flush(uint64_t);

void IDT::set_gate(uint8_t num, uint64_t base, uint16_t sel, uint8_t flags) {
    entries[num].base_low  = base & 0xFFFF;
    entries[num].base_mid  = (base >> 16) & 0xFFFF;
    entries[num].base_high = (base >> 32) & 0xFFFFFFFF;
    entries[num].sel       = sel;
    entries[num].ist       = 0;
    entries[num].flags     = flags;
    entries[num].reserved  = 0;
}

void IDT::install() {
    ptr.limit = sizeof(Entry) * 256 - 1;
    ptr.base  = reinterpret_cast<uint64_t>(&entries);

    lib::memset(&entries, 0, sizeof(Entry) * 256);

    idt_flush(reinterpret_cast<uint64_t>(&ptr));
}

#include "gdt.hpp"

using namespace arch::x86;

GDT::Entry GDT::entries[7];
GDT::Ptr   GDT::ptr;

extern "C" void gdt_flush(uint64_t);

void GDT::set_gate(int num, uint64_t base, uint32_t limit, uint8_t access, uint8_t gran) {
    entries[num].limit_low    = limit & 0xFFFF;
    entries[num].base_low     = base & 0xFFFF;
    entries[num].base_middle  = (base >> 16) & 0xFF;
    entries[num].base_high    = (base >> 24) & 0xFF;
    entries[num].granularity  = (limit >> 16) & 0x0F;
    entries[num].granularity |= gran & 0xF0;
    entries[num].access       = access;
}

void GDT::install() {
    ptr.limit = sizeof(Entry) * 7 - 1;
    ptr.base  = reinterpret_cast<uint64_t>(&entries);

    set_gate(0, 0, 0, 0, 0);
    set_gate(1, 0, 0xFFFFFFFF, 0x9A, 0x20);
    set_gate(2, 0, 0xFFFFFFFF, 0x92, 0x00);
    set_gate(3, 0, 0xFFFFFFFF, 0xFA, 0x20);
    set_gate(4, 0, 0xFFFFFFFF, 0xF2, 0x00);

    gdt_flush(reinterpret_cast<uint64_t>(&ptr));
}

#include "tss.hpp"
#include "gdt.hpp"
#include "vga.hpp"
#include "ns16550.hpp"
#include "lib.hpp"

using namespace arch::x86;

TSSEntry64 TSS::entry;

void TSS::init() {
    lib::memset(&entry, 0, sizeof(TSSEntry64));

    entry.rsp0 = 0;
    entry.iomap_base = sizeof(TSSEntry64);

    uint64_t base = reinterpret_cast<uint64_t>(&entry);
    uint32_t limit = sizeof(TSSEntry64) - 1;

    uint64_t *desc = reinterpret_cast<uint64_t *>(&GDT::entries[5]);
    desc[0] = (limit & 0xFFFF) |
              ((base & 0xFFFFFF) << 16) |
              (0x89ULL << 40) |
              (static_cast<uint64_t>((limit >> 16) & 0x0F) << 48) |
              ((base & 0xFF000000ULL) << 32);
    desc[1] = base >> 32;

    __asm__ volatile ("ltr %%ax" : : "a"(0x28));

    drivers::VGA::writestring_color("TSS installed\n",
        static_cast<uint8_t>(drivers::VGAColor::GREEN));
    drivers::NS16550::write("tss: installed\n");
}

void TSS::set_kernel_stack(uint64_t rsp0) {
    entry.rsp0 = rsp0;
}

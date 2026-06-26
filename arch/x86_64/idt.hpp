#ifndef ELITRA_IDT_HPP
#define ELITRA_IDT_HPP

#include <cstdint>

namespace arch::x86 {

class IDT {
public:
    static void install();
    static void set_gate(uint8_t num, uint64_t base, uint16_t sel, uint8_t flags);

private:
    struct Entry {
        uint16_t base_low;
        uint16_t sel;
        uint8_t  ist;
        uint8_t  flags;
        uint16_t base_mid;
        uint32_t base_high;
        uint32_t reserved;
    } __attribute__((packed));

    struct Ptr {
        uint16_t limit;
        uint64_t base;
    } __attribute__((packed));

    static Entry entries[256];
    static Ptr   ptr;
};

}

#endif

#ifndef ELITRA_GDT_HPP
#define ELITRA_GDT_HPP

#include <cstdint>

namespace arch::x86 {

class GDT {
public:
    static void install();
    static void set_gate(int num, uint64_t base, uint32_t limit, uint8_t access, uint8_t gran);

    static const int KERNEL_CS = 0x08;
    static const int KERNEL_DS = 0x10;
    static const int USER_CS   = 0x1B;
    static const int USER_DS   = 0x23;
    static const int TSS_SEL   = 0x28;

private:
    friend class TSS;
    struct Entry {
        uint16_t limit_low;
        uint16_t base_low;
        uint8_t  base_middle;
        uint8_t  access;
        uint8_t  granularity;
        uint8_t  base_high;
    } __attribute__((packed));

    struct Ptr {
        uint16_t limit;
        uint64_t base;
    } __attribute__((packed));

    static Entry entries[7];
    static Ptr   ptr;
};

}

#endif

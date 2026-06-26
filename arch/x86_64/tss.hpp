#ifndef ELITRA_TSS_HPP
#define ELITRA_TSS_HPP

#include <cstdint>

namespace arch::x86 {

struct TSSEntry64 {
    uint32_t reserved0;
    uint64_t rsp0;
    uint64_t rsp1;
    uint64_t rsp2;
    uint64_t reserved1;
    uint64_t ist1;
    uint64_t ist2;
    uint64_t ist3;
    uint64_t ist4;
    uint64_t ist5;
    uint64_t ist6;
    uint64_t ist7;
    uint64_t reserved2;
    uint16_t reserved3;
    uint16_t iomap_base;
} __attribute__((packed));

class TSS {
public:
    static void init();
    static void set_kernel_stack(uint64_t rsp0);

    static TSSEntry64 entry;
};

}

#endif

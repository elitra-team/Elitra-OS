#ifndef ELITRA_FPU_HPP
#define ELITRA_FPU_HPP

#include <cstdint>

namespace arch {
namespace x86 {

static inline bool has_fxsr() {
    uint32_t eax, edx;
    __asm__ volatile ("cpuid" : "=a"(eax), "=d"(edx) : "a"(1) : "ecx", "ebx");
    return edx & (1 << 24);
}

static inline void fpu_init() {
    uint64_t cr0, cr4;

    __asm__ volatile ("mov %%cr0, %0" : "=r"(cr0));
    cr0 &= ~(1 << 2);
    cr0 |= (1 << 1) | (1 << 5);
    __asm__ volatile ("mov %0, %%cr0" : : "r"(cr0) : "memory");

    if (has_fxsr()) {
        __asm__ volatile ("mov %%cr4, %0" : "=r"(cr4));
        cr4 |= (1 << 9) | (1 << 10);
        __asm__ volatile ("mov %0, %%cr4" : : "r"(cr4) : "memory");
    }

    __asm__ volatile ("fninit");
}

static inline void fpu_save(void *addr) {
    __asm__ volatile ("fxsave (%0)" : : "r"(addr) : "memory");
}

static inline void fpu_restore(void *addr) {
    __asm__ volatile ("fxrstor (%0)" : : "r"(addr) : "memory");
}

}
}

#endif

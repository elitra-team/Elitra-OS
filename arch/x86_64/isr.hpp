#ifndef ELITRA_ISR_HPP
#define ELITRA_ISR_HPP

#include <cstdint>

namespace arch::x86 {

struct Registers {
    uint64_t r15, r14, r13, r12, r11, r10, r9, r8;
    uint64_t rdi, rsi, rbp, rsp;
    uint64_t rbx, rdx, rcx, rax;
    uint64_t int_no, err_code;
    uint64_t rip, cs, rflags, user_rsp, ss;
};

using ISRHandler = void (*)(Registers *);

class ISR {
public:
    static void install();
    static void handler(Registers *r);
    static void register_handler(uint8_t int_no, ISRHandler handler);
    static void unregister_handler(uint8_t int_no);

private:
    static ISRHandler handlers[256];
};

}

extern "C" void isr_handler(arch::x86::Registers *r);

#endif

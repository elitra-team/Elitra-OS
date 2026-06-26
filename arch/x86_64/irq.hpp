#ifndef ELITRA_IRQ_HPP
#define ELITRA_IRQ_HPP

#include <cstdint>
#include "isr.hpp"

namespace arch::x86 {

class IRQ {
public:
    static void install();
    static void install_handler(int irq, ISRHandler handler);
    static void uninstall_handler(int irq);
    static void handler(Registers *r);

private:
    static ISRHandler routines[16];
};

}

extern "C" void irq_handler(arch::x86::Registers *r);

#endif

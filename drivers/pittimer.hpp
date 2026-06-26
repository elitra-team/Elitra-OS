#ifndef ELITRA_PITTIMER_HPP
#define ELITRA_PITTIMER_HPP

#include <cstdint>
#include "isr.hpp"

namespace drivers {

class PITTimer {
public:
    static void init(uint32_t frequency);
    static uint32_t get_ticks();
    static void sleep(uint32_t ms);

private:
    static volatile uint32_t tick_count;
    static uint32_t tick_ms;

    static void callback(arch::x86::Registers *r);
};

}

#endif

#include "pittimer.hpp"
#include "irq.hpp"
#include "port.hpp"
#include "task.hpp"

using namespace drivers;

volatile uint32_t PITTimer::tick_count = 0;
uint32_t PITTimer::tick_ms = 10;

void PITTimer::callback(arch::x86::Registers *r) {
    tick_count = tick_count + 1;
    kernel::Scheduler::preempt(r);
}

void PITTimer::init(uint32_t frequency) {
    arch::x86::IRQ::install_handler(0, callback);

    uint32_t divisor = 1193182 / frequency;
    tick_ms = 1000 / frequency;

    arch::x86::outb(0x43, 0x36);
    arch::x86::outb(0x40, static_cast<uint8_t>(divisor & 0xFF));
    arch::x86::outb(0x40, static_cast<uint8_t>((divisor >> 8) & 0xFF));
}

uint32_t PITTimer::get_ticks() {
    return tick_count;
}

void PITTimer::sleep(uint32_t ms) {
    uint32_t target = tick_count + (ms / tick_ms) + 1;
    while (tick_count < target) {
        __asm__ volatile ("pause");
    }
}

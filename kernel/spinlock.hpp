#ifndef ELITRA_SPINLOCK_HPP
#define ELITRA_SPINLOCK_HPP

#include <cstdint>
#include "port.hpp"

namespace kernel {

class Spinlock {
public:
    Spinlock() : lock_flag(0), saved_flags(0) {}

    void lock() {
        saved_flags = read_eflags();
        arch::x86::disable_interrupts();
        while (__atomic_test_and_set(&lock_flag, __ATOMIC_ACQUIRE))
            __asm__ volatile ("pause");
    }

    void unlock() {
        __atomic_clear(&lock_flag, __ATOMIC_RELEASE);
        write_eflags(saved_flags);
    }

private:
    volatile uint32_t lock_flag;
    uint64_t saved_flags;

    static uint64_t read_eflags() {
        uint64_t f;
        __asm__ volatile ("pushfq\n popq %0" : "=r"(f));
        return f;
    }

    static void write_eflags(uint64_t f) {
        __asm__ volatile ("pushq %0\n popfq" : : "r"(f) : "cc", "memory");
    }
};

class IrqLock {
public:
    IrqLock() { arch::x86::disable_interrupts(); }
    ~IrqLock() { arch::x86::enable_interrupts(); }

    IrqLock(const IrqLock &) = delete;
    IrqLock &operator=(const IrqLock &) = delete;
};

}

#endif

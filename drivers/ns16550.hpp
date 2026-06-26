#ifndef ELITRA_NS16550_HPP
#define ELITRA_NS16550_HPP

#include <cstdint>
#include <cstddef>

namespace drivers {

class NS16550 {
public:
    static void init();
    static void write(const char *data);
    static void write(const char *data, size_t len);
    static void printf(const char *fmt, ...);

private:
    static const uint16_t COM1 = 0x3F8;

    static bool is_transmit_empty();
    static void putchar(char c);
};

}

#endif

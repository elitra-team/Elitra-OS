#include "ns16550.hpp"
#include "port.hpp"
#include "lib.hpp"
#include <cstdarg>

using namespace drivers;

void NS16550::init() {
    using namespace arch::x86;
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x80);
    outb(COM1 + 0, 0x03);
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x03);
    outb(COM1 + 2, 0xC7);
    outb(COM1 + 4, 0x0B);
    outb(COM1 + 4, 0x1E);
    outb(COM1 + 0, 0xAE);
    if (inb(COM1 + 0) != 0xAE)
        return;
    outb(COM1 + 4, 0x0F);
}

bool NS16550::is_transmit_empty() {
    return arch::x86::inb(COM1 + 5) & 0x20;
}

void NS16550::putchar(char c) {
    while (!is_transmit_empty());
    arch::x86::outb(COM1, static_cast<unsigned char>(c));
    if (c == '\n')
        putchar('\r');
}

void NS16550::write(const char *data) {
    while (*data) {
        putchar(*data);
        data++;
    }
}

void NS16550::write(const char *data, size_t len) {
    for (size_t i = 0; i < len; i++)
        putchar(data[i]);
}

void NS16550::printf(const char *fmt, ...) {
    va_list args;
    va_start(args, fmt);

    for (const char *p = fmt; *p; p++) {
        if (*p != '%') {
            putchar(*p);
            continue;
        }
        p++;
        switch (*p) {
            case 'd':
            case 'i': {
                int num = va_arg(args, int);
                char buf[12];
                lib::itoa(num, buf, 10);
                write(buf);
                break;
            }
            case 'u': {
                uint32_t num = va_arg(args, uint32_t);
                char buf[12];
                lib::uitoa(num, buf, 10);
                write(buf);
                break;
            }
            case 'l': {
                p++;
                if (*p == 'l') p++;
                switch (*p) {
                    case 'x':
                    case 'X': {
                        uint64_t num = va_arg(args, uint64_t);
                        char buf[22];
                        lib::uitoa64(num, buf, 16);
                        write(buf);
                        break;
                    }
                    case 'u': {
                        uint64_t num = va_arg(args, uint64_t);
                        char buf[22];
                        lib::uitoa64(num, buf, 10);
                        write(buf);
                        break;
                    }
                    case 'd':
                    case 'i': {
                        int64_t num = static_cast<int64_t>(va_arg(args, int64_t));
                        char buf[22];
                        if (num < 0) {
                            putchar('-');
                            num = -num;
                        }
                        lib::uitoa64(static_cast<uint64_t>(num), buf, 10);
                        write(buf);
                        break;
                    }
                    default:
                        putchar('%');
                        putchar('l');
                        if (*(p-1) == 'l') putchar('l');
                        putchar(*p);
                        break;
                }
                break;
            }
            case 'x':
            case 'X': {
                uint32_t num = va_arg(args, uint32_t);
                char buf[12];
                lib::uitoa(num, buf, 16);
                write(buf);
                break;
            }
            case 's': {
                const char *s = va_arg(args, const char *);
                if (!s) s = "(null)";
                write(s);
                break;
            }
            case 'c': {
                char c = static_cast<char>(va_arg(args, int));
                putchar(c);
                break;
            }
            case '%':
                putchar('%');
                break;
            default:
                putchar('%');
                putchar(*p);
                break;
        }
    }

    va_end(args);
}

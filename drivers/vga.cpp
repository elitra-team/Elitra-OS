#include "vga.hpp"
#include "lib.hpp"
#include "port.hpp"

using namespace drivers;

uint16_t *const VGA::VGA_MEMORY = reinterpret_cast<uint16_t *>(0xB8000);

size_t VGA::row    = 0;
size_t VGA::col    = 0;
uint8_t VGA::color = 0;
uint16_t VGA::buffer[80 * 25];

uint8_t VGA::make_color(VGAColor fg, VGAColor bg) {
    return static_cast<uint8_t>(fg) | (static_cast<uint8_t>(bg) << 4);
}

uint16_t VGA::make_entry(unsigned char c, uint8_t color) {
    return static_cast<uint16_t>(c) | (static_cast<uint16_t>(color) << 8);
}

void VGA::init() {
    row   = 0;
    col   = 0;
    color = make_color(VGAColor::LIGHT_GREY, VGAColor::BLACK);
    clear();
}

void VGA::set_color(uint8_t c) {
    color = c;
}

void VGA::clear() {
    for (size_t y = 0; y < HEIGHT; y++) {
        for (size_t x = 0; x < WIDTH; x++) {
            size_t idx = y * WIDTH + x;
            buffer[idx] = make_entry(' ', color);
            VGA_MEMORY[idx] = make_entry(' ', color);
        }
    }
    row = 0;
    col = 0;
}

void VGA::set_pos(int x, int y) {
    if (x >= 0 && x < static_cast<int>(WIDTH))  col = x;
    if (y >= 0 && y < static_cast<int>(HEIGHT)) row = y;
}

void VGA::get_pos(int *x, int *y) {
    *x = col;
    *y = row;
}

void VGA::scroll() {
    for (size_t y = 0; y < HEIGHT - 1; y++) {
        for (size_t x = 0; x < WIDTH; x++) {
            size_t from = (y + 1) * WIDTH + x;
            size_t to   = y * WIDTH + x;
            buffer[to]  = buffer[from];
            VGA_MEMORY[to] = VGA_MEMORY[from];
        }
    }
    for (size_t x = 0; x < WIDTH; x++) {
        size_t idx = (HEIGHT - 1) * WIDTH + x;
        buffer[idx] = make_entry(' ', color);
        VGA_MEMORY[idx] = make_entry(' ', color);
    }
    row = HEIGHT - 1;
}

void VGA::putchar(char c) {
    if (c == '\n') {
        col = 0;
        row++;
        if (row >= HEIGHT) scroll();
        return;
    }
    if (c == '\r') {
        col = 0;
        return;
    }
    if (c == '\t') {
        do {
            putchar(' ');
        } while (col % 8 != 0);
        return;
    }
    if (c == '\b') {
        if (col > 0) {
            col--;
        } else if (row > 0) {
            row--;
            col = WIDTH - 1;
        }
        size_t idx = row * WIDTH + col;
        buffer[idx] = make_entry(' ', color);
        VGA_MEMORY[idx] = make_entry(' ', color);
        return;
    }
    size_t idx = row * WIDTH + col;
    buffer[idx] = make_entry(static_cast<unsigned char>(c), color);
    VGA_MEMORY[idx] = make_entry(static_cast<unsigned char>(c), color);
    col++;
    if (col >= WIDTH) {
        col = 0;
        row++;
        if (row >= HEIGHT) scroll();
    }
}

void VGA::write(const char *data, size_t size) {
    for (size_t i = 0; i < size; i++)
        putchar(data[i]);
}

void VGA::writestring(const char *data) {
    write(data, lib::strlen(data));
}

void VGA::writestring_color(const char *data, uint8_t c) {
    uint8_t old = color;
    color = c;
    writestring(data);
    color = old;
}

void VGA::printf(const char *fmt, ...) {
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
                writestring(buf);
                break;
            }
            case 'u': {
                uint32_t num = va_arg(args, uint32_t);
                char buf[12];
                lib::uitoa(num, buf, 10);
                writestring(buf);
                break;
            }
            case 'x':
            case 'X': {
                uint32_t num = va_arg(args, uint32_t);
                char buf[12];
                lib::uitoa(num, buf, 16);
                writestring(buf);
                break;
            }
            case 's': {
                const char *s = va_arg(args, const char *);
                if (!s) s = "(null)";
                writestring(s);
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

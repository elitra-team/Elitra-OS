#ifndef ELITRA_VGA_HPP
#define ELITRA_VGA_HPP

#include <cstdint>
#include <cstddef>
#include <cstdarg>

namespace drivers {

enum class VGAColor : uint8_t {
    BLACK         = 0,
    BLUE          = 1,
    GREEN         = 2,
    CYAN          = 3,
    RED           = 4,
    MAGENTA       = 5,
    BROWN         = 6,
    LIGHT_GREY    = 7,
    DARK_GREY     = 8,
    LIGHT_BLUE    = 9,
    LIGHT_GREEN   = 10,
    LIGHT_CYAN    = 11,
    LIGHT_RED     = 12,
    LIGHT_MAGENTA = 13,
    LIGHT_BROWN   = 14,
    WHITE         = 15,
};

class VGA {
public:
    static void init();
    static void set_color(uint8_t color);
    static void putchar(char c);
    static void write(const char *data, size_t size);
    static void writestring(const char *data);
    static void writestring_color(const char *data, uint8_t color);
    static void printf(const char *fmt, ...);
    static void clear();
    static void set_pos(int x, int y);
    static void get_pos(int *x, int *y);

private:
    static const size_t WIDTH  = 80;
    static const size_t HEIGHT = 25;
    static uint16_t *const VGA_MEMORY;

    static size_t row;
    static size_t col;
    static uint8_t color;
    static uint16_t buffer[80 * 25];

    static uint8_t make_color(VGAColor fg, VGAColor bg);
    static uint16_t make_entry(unsigned char c, uint8_t color);
    static void scroll();
};

}

#endif

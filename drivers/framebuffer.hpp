#ifndef ELITRA_FRAMEBUFFER_HPP
#define ELITRA_FRAMEBUFFER_HPP

#include <cstdint>

namespace drivers {
namespace framebuffer {

struct FBInfo {
    uint64_t phys_addr;
    uint32_t width;
    uint32_t height;
    uint32_t pitch;
    uint8_t  bpp;
    bool     present;
};

void init(uint64_t phys_addr, uint32_t width, uint32_t height,
          uint32_t pitch, uint8_t bpp);
const FBInfo &info();

void put_pixel(uint32_t x, uint32_t y, uint32_t color);
void fill_rect(uint32_t x, uint32_t y, uint32_t w, uint32_t h, uint32_t color);
void clear(uint32_t color);
void draw_char(uint32_t x, uint32_t y, char c, uint32_t fg, uint32_t bg);

static const uint32_t COLOR_BLACK   = 0x000000;
static const uint32_t COLOR_BLUE    = 0x0000AA;
static const uint32_t COLOR_GREEN   = 0x00AA00;
static const uint32_t COLOR_CYAN    = 0x00AAAA;
static const uint32_t COLOR_RED     = 0xAA0000;
static const uint32_t COLOR_MAGENTA = 0xAA00AA;
static const uint32_t COLOR_BROWN   = 0xAA5500;
static const uint32_t COLOR_WHITE   = 0xAAAAAA;
static const uint32_t COLOR_GRAY    = 0x555555;

} // namespace framebuffer
} // namespace drivers

#endif
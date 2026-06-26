#ifndef ELITRA_TERMINAL_HPP
#define ELITRA_TERMINAL_HPP

#include <cstdint>
#include <cstddef>
#include <cstdarg>
#include "vga.hpp"

namespace drivers {

class Terminal {
public:
    static const int WIDTH  = 80;
    static const int HEIGHT = 25;
    static const int MAX_VTS = 4;
    static const int SCROLL_LINES = 200;

    struct VTState {
        uint16_t buffer[WIDTH * HEIGHT];
        uint16_t scrollback[SCROLL_LINES * WIDTH];
        int scrollback_head;    // next write position in ring
        int scrollback_count;   // total lines written
        int scroll_offset;      // 0 = live, >0 = scrolled back
        int cursor_x, cursor_y;
        uint8_t color;
        bool active;
    };

    static void init();
    static void putchar(char c);
    static void write(const char *data, size_t size);
    static void writestring(const char *data);
    static void writestring_color(const char *data, uint8_t color);
    static void printf(const char *fmt, ...);
    static void set_color(uint8_t color);
    static void clear();
    static void set_pos(int x, int y);
    static void get_pos(int *x, int *y);
    static void switch_vt(int vt);
    static int  active_vt();
    static bool process_key(int key);  // returns true if key was handled as control
    static void readline(char *buf, int max);  // read line with terminal echo
    static void update_status_bar();

private:
    static VTState vts[MAX_VTS];
    static int active;

    static void flush_vt(int vt);

    static void save_vt(int vt);
    static void restore_vt(int vt);
    static void scroll_up(int vt);
    static void push_scrollback(int vt, const uint16_t *line);
    static void render_scrollback(int vt);
    static void restore_live(int vt);
    static void update_cursor();
};

}

#endif

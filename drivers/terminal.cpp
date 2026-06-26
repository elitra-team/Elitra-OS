#include "terminal.hpp"
#include "port.hpp"
#include "lib.hpp"
#include "ns16550.hpp"
#include "ps2keyboard.hpp"
#include "cmos_rtc.hpp"

using namespace drivers;

Terminal::VTState Terminal::vts[MAX_VTS];
int Terminal::active = 0;

static uint16_t *const VGA_MEM = reinterpret_cast<uint16_t *>(0xB8000);
static const int CONTENT_TOP = 1;

static uint8_t make_color(VGAColor fg, VGAColor bg) {
    return static_cast<uint8_t>(fg) | (static_cast<uint8_t>(bg) << 4);
}

static uint16_t make_entry(unsigned char c, uint8_t color) {
    return static_cast<uint16_t>(c) | (static_cast<uint16_t>(color) << 8);
}

static void draw_status_bar();

void Terminal::flush_vt(int vt) {
    if (vt == active && vts[vt].scroll_offset == 0) {
        lib::memcpy(VGA_MEM, vts[vt].buffer, WIDTH * HEIGHT * sizeof(uint16_t));
    }
}

void Terminal::init() {
    uint8_t def = make_color(VGAColor::LIGHT_GREY, VGAColor::BLACK);

    for (int i = 0; i < MAX_VTS; i++) {
        vts[i].scrollback_head = 0;
        vts[i].scrollback_count = 0;
        vts[i].scroll_offset = 0;
        vts[i].cursor_x = 0;
        vts[i].cursor_y = CONTENT_TOP;
        vts[i].color = def;
        vts[i].active = false;

        for (int j = 0; j < WIDTH * HEIGHT; j++)
            vts[i].buffer[j] = make_entry(' ', def);
        for (int j = 0; j < SCROLL_LINES * WIDTH; j++)
            vts[i].scrollback[j] = make_entry(' ', def);
    }

    vts[1].color = make_color(VGAColor::GREEN, VGAColor::BLACK);
    vts[2].color = make_color(VGAColor::CYAN, VGAColor::BLACK);
    vts[3].color = make_color(VGAColor::MAGENTA, VGAColor::BLACK);

    active = 0;
    vts[0].active = true;

    for (int i = 0; i < WIDTH * HEIGHT; i++)
        VGA_MEM[i] = vts[0].buffer[i];

    draw_status_bar();
    Terminal::update_cursor();

    drivers::NS16550::write("terminal: 4 VTs, F1-F4 switch, PgUp/PgDn scroll\n");
}

void Terminal::push_scrollback(int vt, const uint16_t *line) {
    int idx = vts[vt].scrollback_head * WIDTH;
    lib::memcpy(&vts[vt].scrollback[idx], line, WIDTH * sizeof(uint16_t));
    vts[vt].scrollback_head = (vts[vt].scrollback_head + 1) % SCROLL_LINES;
    if (vts[vt].scrollback_count < SCROLL_LINES)
        vts[vt].scrollback_count++;
}

void Terminal::scroll_up(int vt) {
    // Push the line at CONTENT_TOP (row 1) to scrollback
    push_scrollback(vt, &vts[vt].buffer[CONTENT_TOP * WIDTH]);

    // Shift rows up: row 2..HEIGHT-1 → row CONTENT_TOP..HEIGHT-2
    for (int y = CONTENT_TOP + 1; y < HEIGHT; y++) {
        lib::memcpy(&vts[vt].buffer[(y - 1) * WIDTH],
                    &vts[vt].buffer[y * WIDTH],
                    WIDTH * sizeof(uint16_t));
    }

    // Clear last content row (HEIGHT-1)
    for (int x = 0; x < WIDTH; x++)
        vts[vt].buffer[(HEIGHT - 1) * WIDTH + x] = make_entry(' ', vts[vt].color);

    // Restore status bar in buffer
    draw_status_bar();

    if (vt == active && vts[vt].scroll_offset == 0) {
        lib::memcpy(VGA_MEM, vts[vt].buffer, WIDTH * HEIGHT * sizeof(uint16_t));
    }
}

void Terminal::render_scrollback(int vt) {
    if (vts[vt].scroll_offset <= 0) return;

    int count = vts[vt].scrollback_count;
    int offset = vts[vt].scroll_offset;
    int start_line = count - offset;
    if (start_line < 0) start_line = 0;

    // Scrollback fills CONTENT_TOP..HEIGHT-1 on VGA
    for (int y = CONTENT_TOP; y < HEIGHT; y++) {
        int sb_line = start_line + (y - CONTENT_TOP);
        if (sb_line < count) {
            int sb_idx = ((vts[vt].scrollback_head - count + sb_line + SCROLL_LINES) % SCROLL_LINES) * WIDTH;
            lib::memcpy(&VGA_MEM[y * WIDTH], &vts[vt].scrollback[sb_idx], WIDTH * sizeof(uint16_t));
        } else {
            lib::memcpy(&VGA_MEM[y * WIDTH], &vts[vt].buffer[y * WIDTH], WIDTH * sizeof(uint16_t));
        }
    }
    draw_status_bar();
}

void Terminal::restore_live(int vt) {
    vts[vt].scroll_offset = 0;
    lib::memcpy(VGA_MEM, vts[vt].buffer, WIDTH * HEIGHT * sizeof(uint16_t));
    draw_status_bar();
}

void Terminal::putchar(char c) {
    int vt = active;
    VTState &s = vts[vt];

    if (s.scroll_offset > 0) {
        restore_live(vt);
    }

    if (c == '\n') {
        s.cursor_x = 0;
        s.cursor_y++;
        if (s.cursor_y >= HEIGHT) {
            scroll_up(vt);
            s.cursor_y = HEIGHT - 1;
        }
        update_cursor();
        return;
    }
    if (c == '\r') {
        s.cursor_x = 0;
        update_cursor();
        return;
    }
    if (c == '\t') {
        do { putchar(' '); } while (s.cursor_x % 8 != 0);
        return;
    }
    if (c == '\b') {
        if (s.cursor_x > 0) {
            s.cursor_x--;
        } else if (s.cursor_y > CONTENT_TOP) {
            s.cursor_y--;
            s.cursor_x = WIDTH - 1;
        }
        uint32_t idx = s.cursor_y * WIDTH + s.cursor_x;
        s.buffer[idx] = make_entry(' ', s.color);
        VGA_MEM[idx] = s.buffer[idx];
        update_cursor();
        return;
    }

    uint32_t idx = s.cursor_y * WIDTH + s.cursor_x;
    s.buffer[idx] = make_entry(static_cast<unsigned char>(c), s.color);
    VGA_MEM[idx] = s.buffer[idx];

    s.cursor_x++;
    if (s.cursor_x >= WIDTH) {
        s.cursor_x = 0;
        s.cursor_y++;
        if (s.cursor_y >= HEIGHT) {
            scroll_up(vt);
            s.cursor_y = HEIGHT - 1;
        }
    }
    update_cursor();
}

void Terminal::write(const char *data, size_t size) {
    for (size_t i = 0; i < size; i++)
        putchar(data[i]);
}

void Terminal::writestring(const char *data) {
    write(data, lib::strlen(data));
}

void Terminal::writestring_color(const char *data, uint8_t color) {
    uint8_t old = vts[active].color;
    vts[active].color = color;
    writestring(data);
    vts[active].color = old;
}

void Terminal::set_color(uint8_t color) {
    vts[active].color = color;
}

void Terminal::clear() {
    int vt = active;
    VTState &s = vts[vt];
    s.scroll_offset = 0;

    // Clear all rows
    uint16_t entry = make_entry(' ', s.color);
    for (int i = 0; i < WIDTH * HEIGHT; i++)
        s.buffer[i] = entry;

    s.cursor_x = 0;
    s.cursor_y = CONTENT_TOP;

    lib::memcpy(VGA_MEM, s.buffer, WIDTH * HEIGHT * sizeof(uint16_t));
    draw_status_bar();
    update_cursor();
}

void Terminal::set_pos(int x, int y) {
    if (x >= 0 && x < WIDTH)  vts[active].cursor_x = x;
    if (y >= CONTENT_TOP && y < HEIGHT) vts[active].cursor_y = y;
    update_cursor();
}

void Terminal::get_pos(int *x, int *y) {
    *x = vts[active].cursor_x;
    *y = vts[active].cursor_y;
}

void Terminal::update_cursor() {
    const VTState &s = vts[active];
    uint16_t pos = static_cast<uint16_t>(s.cursor_y * WIDTH + s.cursor_x);
    arch::x86::outb(0x3D4, 0x0F);
    arch::x86::outb(0x3D5, pos & 0xFF);
    arch::x86::outb(0x3D4, 0x0E);
    arch::x86::outb(0x3D5, (pos >> 8) & 0xFF);
}

static void draw_status_bar() {
    int n = Terminal::active_vt();
    uint8_t text_color = make_color(VGAColor::LIGHT_GREY, VGAColor::BLACK);
    uint8_t dim_color  = make_color(VGAColor::DARK_GREY, VGAColor::BLACK);

    // Draw underscore line across the top
    uint8_t line_color = make_color(VGAColor::DARK_GREY, VGAColor::BLACK);
    for (int x = 0; x < Terminal::WIDTH; x++)
        VGA_MEM[x] = make_entry('_', line_color);

    // Draw VT labels on the left (normal text, not reversed)
    for (int i = 0; i < Terminal::MAX_VTS; i++) {
        int x = i * 20;
        uint8_t c = (i == n) ? text_color : dim_color;
        VGA_MEM[x + 0] = make_entry((i == n) ? '[' : ' ', c);
        VGA_MEM[x + 1] = make_entry('F', c);
        VGA_MEM[x + 2] = make_entry('1' + i, c);
        VGA_MEM[x + 3] = make_entry((i == n) ? ']' : ' ', c);
    }

    // Draw date/time on the right side
    RTCInfo rtc = CMOSRTC::read_time();
    char time_str[20];
    time_str[0] = '0' + (rtc.hour / 10);
    time_str[1] = '0' + (rtc.hour % 10);
    time_str[2] = ':';
    time_str[3] = '0' + (rtc.minute / 10);
    time_str[4] = '0' + (rtc.minute % 10);
    time_str[5] = ':';
    time_str[6] = '0' + (rtc.second / 10);
    time_str[7] = '0' + (rtc.second % 10);
    time_str[8] = ' ';

    time_str[9]  = '0' + ((rtc.year / 1000) % 10);
    time_str[10] = '0' + ((rtc.year / 100) % 10);
    time_str[11] = '0' + ((rtc.year / 10) % 10);
    time_str[12] = '0' + (rtc.year % 10);
    time_str[13] = '-';
    time_str[14] = '0' + (rtc.month / 10);
    time_str[15] = '0' + (rtc.month % 10);
    time_str[16] = '-';
    time_str[17] = '0' + (rtc.day / 10);
    time_str[18] = '0' + (rtc.day % 10);

    int start_x = Terminal::WIDTH - 22;
    for (int j = 0; j < 19; j++)
        VGA_MEM[start_x + j] = make_entry(time_str[j], text_color);
}

void Terminal::switch_vt(int vt) {
    if (vt < 0 || vt >= MAX_VTS || vt == active) return;

    // Store current VGA into current VT's buffer
    lib::memcpy(vts[active].buffer, VGA_MEM, WIDTH * HEIGHT * sizeof(uint16_t));

    active = vt;
    VTState &s = vts[vt];

    if (s.scroll_offset > 0)
        render_scrollback(vt);
    else
        lib::memcpy(VGA_MEM, s.buffer, WIDTH * HEIGHT * sizeof(uint16_t));

    draw_status_bar();
    update_cursor();
}

int Terminal::active_vt() {
    return active;
}

void Terminal::update_status_bar() {
    draw_status_bar();
}

bool Terminal::process_key(int key) {
    if (key >= KEY_F1 && key <= KEY_F4) {
        switch_vt(key - KEY_F1);
        update_status_bar();
        return true;
    }

    int vt = active;
    VTState &s = vts[vt];

    if (key == KEY_PGUP) {
        if (s.scrollback_count > 0) {
            s.scroll_offset++;
            if (s.scroll_offset > s.scrollback_count)
                s.scroll_offset = s.scrollback_count;
            render_scrollback(vt);
        }
        return true;
    }

    if (key == KEY_PGDN) {
        if (s.scroll_offset > 0) {
            s.scroll_offset--;
            if (s.scroll_offset <= 0)
                restore_live(vt);
            else
                render_scrollback(vt);
        }
        return true;
    }

    if (key == KEY_HOME) {
        if (s.scrollback_count > 0 && s.scroll_offset != s.scrollback_count) {
            s.scroll_offset = s.scrollback_count;
            render_scrollback(vt);
        }
        return true;
    }

    if (key == KEY_END) {
        if (s.scroll_offset > 0) {
            s.scroll_offset = 0;
            restore_live(vt);
        }
        return true;
    }

    if (s.scroll_offset > 0) {
        s.scroll_offset = 0;
        restore_live(vt);
    }

    return false;
}

void Terminal::readline(char *buf, int max) {
    // Update status bar (shows current time) before showing prompt
    draw_status_bar();

    int pos = 0;
    while (pos < max - 1) {
        int k = PS2Keyboard::getkey();

        // Let terminal process control keys (VT switch, scroll)
        if (process_key(k))
            continue;

        // Handle special keys that should modify input
        if (k == KEY_UP || k == KEY_DOWN || k == KEY_LEFT || k == KEY_RIGHT)
            continue;  // ignore arrows for now

        // Handle character input
        if (k >= 0 && k < 256) {
            char c = static_cast<char>(k);
            if (c == '\n') {
                putchar('\n');
                buf[pos] = '\0';
                return;
            }
            if (c == '\b') {
                if (pos > 0) {
                    pos--;
                    putchar('\b');
                    putchar(' ');
                    putchar('\b');
                }
                continue;
            }
            buf[pos++] = c;
            putchar(c);
        }
    }
    buf[pos] = '\0';
}

void Terminal::printf(const char *fmt, ...) {
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

#ifndef ELITRA_PS2KEYBOARD_HPP
#define ELITRA_PS2KEYBOARD_HPP

#include <cstdint>
#include "isr.hpp"

namespace drivers {

enum KeyCode : int {
    KEY_NONE    = 0,
    KEY_F1      = 256,
    KEY_F2      = 257,
    KEY_F3      = 258,
    KEY_F4      = 259,
    KEY_F5      = 260,
    KEY_F6      = 261,
    KEY_F7      = 262,
    KEY_F8      = 263,
    KEY_F9      = 264,
    KEY_F10     = 265,
    KEY_F11     = 266,
    KEY_F12     = 267,
    KEY_PGUP    = 268,
    KEY_PGDN    = 269,
    KEY_UP      = 270,
    KEY_DOWN    = 271,
    KEY_LEFT    = 272,
    KEY_RIGHT   = 273,
    KEY_HOME    = 274,
    KEY_END     = 275,
};

class PS2Keyboard {
public:
    static void init();
    static char getchar();
    static int  getkey();
    static void readline(char *buf, int max);
    static bool data_available();

private:
    static const int BUF_SIZE   = 256;
    static const int KEYBUF_SIZE = 64;

    static volatile char buf[BUF_SIZE];
    static volatile int head;
    static volatile int tail;

    static volatile int keybuf[KEYBUF_SIZE];
    static volatile int keyhead;
    static volatile int keytail;

    static int shift_pressed;
    static int ctrl_pressed;
    static int alt_pressed;
    static int caps_lock;
    static int ext_scan;  // 0xE0 or 0xE1 prefix pending

    static const char scancode_ascii[128];
    static const char scancode_shift_ascii[128];

    static void callback(arch::x86::Registers *r);
};

}

#endif

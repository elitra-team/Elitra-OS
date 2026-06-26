#include "ps2keyboard.hpp"
#include "irq.hpp"
#include "port.hpp"
#include "vga.hpp"
#include "lib.hpp"

using namespace drivers;

volatile char PS2Keyboard::buf[BUF_SIZE];
volatile int PS2Keyboard::head = 0;
volatile int PS2Keyboard::tail = 0;

volatile int PS2Keyboard::keybuf[KEYBUF_SIZE];
volatile int PS2Keyboard::keyhead = 0;
volatile int PS2Keyboard::keytail = 0;

int PS2Keyboard::shift_pressed = 0;
int PS2Keyboard::ctrl_pressed = 0;
int PS2Keyboard::alt_pressed = 0;
int PS2Keyboard::caps_lock = 0;
int PS2Keyboard::ext_scan = 0;

const char PS2Keyboard::scancode_ascii[128] = {
    0,   0,   '1', '2', '3', '4', '5', '6', '7', '8',
    '9', '0', '-', '=', '\b','\t','q', 'w', 'e', 'r',
    't', 'y', 'u', 'i', 'o', 'p', '[', ']', '\n', 0,
    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', ';',
    '\'','`', 0,   '\\','z', 'x', 'c', 'v', 'b', 'n',
    'm', ',', '.', '/', 0,   '*', 0,   ' ', 0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0
};

const char PS2Keyboard::scancode_shift_ascii[128] = {
    0,   0,   '!', '@', '#', '$', '%', '^', '&', '*',
    '(', ')', '_', '+', '\b','\t','Q', 'W', 'E', 'R',
    'T', 'Y', 'U', 'I', 'O', 'P', '{', '}', '\n', 0,
    'A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', ':',
    '"', '~', 0,   '|', 'Z', 'X', 'C', 'V', 'B', 'N',
    'M', '<', '>', '?', 0,   '*', 0,   ' ', 0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0
};

void PS2Keyboard::callback(arch::x86::Registers *r) {
    (void)r;
    using namespace arch::x86;
    uint8_t scancode = inb(0x60);

    // Handle extended scancode prefixes
    if (scancode == 0xE0) {
        ext_scan = 0xE0;
        return;
    }
    if (scancode == 0xE1) {
        ext_scan = 0xE1;
        return;
    }

    int release = (scancode & 0x80) ? 1 : 0;
    scancode &= 0x7F;

    // Track modifier keys
    if (scancode == 0x2A || scancode == 0x36) {
        shift_pressed = !release;
        return;
    }
    if (scancode == 0x1D) {
        ctrl_pressed = !release;
        return;
    }
    if (scancode == 0x38) {
        alt_pressed = !release;
        return;
    }
    if (scancode == 0x3A && !release) {
        caps_lock = !caps_lock;
        return;
    }

    // Release: clear extended flag, done
    if (release) {
        ext_scan = 0;
        return;
    }

    int key = 0;

    if (ext_scan == 0xE0) {
        ext_scan = 0;
        switch (scancode) {
            case 0x48: key = KEY_UP;    break;
            case 0x50: key = KEY_DOWN;  break;
            case 0x4B: key = KEY_LEFT;  break;
            case 0x4D: key = KEY_RIGHT; break;
            case 0x49: key = KEY_PGUP;  break;
            case 0x51: key = KEY_PGDN;  break;
            case 0x47: key = KEY_HOME;  break;
            case 0x4F: key = KEY_END;   break;
        }
    } else {
        ext_scan = 0;
        // Function keys F1-F12
        if (scancode >= 0x3B && scancode <= 0x44) {
            key = KEY_F1 + (scancode - 0x3B);
        } else if (scancode >= 0x57 && scancode <= 0x58) {
            key = KEY_F11 + (scancode - 0x57);
        } else {
            // Regular ASCII
            char c = shift_pressed ? scancode_shift_ascii[scancode] : scancode_ascii[scancode];
            if (c >= 'a' && c <= 'z' && caps_lock)
                c -= 32;
            else if (c >= 'A' && c <= 'Z' && caps_lock)
                c += 32;
            if (c) {
                key = static_cast<int>(c);
                // Also push to legacy char buffer
                int next = (head + 1) % BUF_SIZE;
                if (next != tail) {
                    buf[head] = c;
                    head = next;
                }
            }
        }
    }

    if (key) {
        int next = (keyhead + 1) % KEYBUF_SIZE;
        if (next != keytail) {
            keybuf[keyhead] = key;
            keyhead = next;
        }
    }
}

void PS2Keyboard::init() {
    arch::x86::IRQ::install_handler(1, callback);
}

bool PS2Keyboard::data_available() {
    return head != tail;
}

char PS2Keyboard::getchar() {
    while (head == tail) {
        __asm__ volatile ("pause");
    }
    char c = buf[tail];
    tail = (tail + 1) % BUF_SIZE;
    return c;
}

int PS2Keyboard::getkey() {
    while (keyhead == keytail) {
        __asm__ volatile ("pause");
    }
    int k = keybuf[keytail];
    keytail = (keytail + 1) % KEYBUF_SIZE;
    return k;
}

void PS2Keyboard::readline(char *buf, int max) {
    int pos = 0;
    while (pos < max - 1) {
        char c = getchar();
        if (c == '\n') {
            VGA::putchar('\n');
            buf[pos] = '\0';
            return;
        }
        if (c == '\b') {
            if (pos > 0) {
                pos--;
                VGA::putchar('\b');
                VGA::putchar(' ');
                VGA::putchar('\b');
            }
            continue;
        }
        buf[pos++] = c;
        VGA::putchar(c);
    }
    buf[pos] = '\0';
}

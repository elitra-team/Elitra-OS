
use crate::scheduler::Registers;

const BUF_SIZE: usize = 256;
const KEYBUF_SIZE: usize = 64;

pub const KEY_NONE: i32 = 0;
pub const KEY_F1: i32 = 256;
pub const KEY_F2: i32 = 257;
pub const KEY_F3: i32 = 258;
pub const KEY_F4: i32 = 259;
pub const KEY_F5: i32 = 260;
pub const KEY_F6: i32 = 261;
pub const KEY_F7: i32 = 262;
pub const KEY_F8: i32 = 263;
pub const KEY_F9: i32 = 264;
pub const KEY_F10: i32 = 265;
pub const KEY_F11: i32 = 266;
pub const KEY_F12: i32 = 267;
pub const KEY_PGUP: i32 = 268;
pub const KEY_PGDN: i32 = 269;
pub const KEY_UP: i32 = 270;
pub const KEY_DOWN: i32 = 271;
pub const KEY_LEFT: i32 = 272;
pub const KEY_RIGHT: i32 = 273;
pub const KEY_HOME: i32 = 274;
pub const KEY_END: i32 = 275;

static mut BUF: [u8; BUF_SIZE] = [0u8; BUF_SIZE];
static mut HEAD: usize = 0;
static mut TAIL: usize = 0;

static mut KEYBUF: [i32; KEYBUF_SIZE] = [0i32; KEYBUF_SIZE];
static mut KEYHEAD: usize = 0;
static mut KEYTAIL: usize = 0;

static mut SHIFT_PRESSED: bool = false;
static mut CTRL_PRESSED: bool = false;
static mut ALT_PRESSED: bool = false;
static mut CAPS_LOCK: bool = false;
static mut EXT_SCAN: u8 = 0;

static SCANCODE_ASCII: [u8; 128] = [
    0,   0,   b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8',
    b'9', b'0', b'-', b'=', 8,   9,   b'q', b'w', b'e', b'r',
    b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', 10,  0,
    b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';',
    b'\'', b'`', 0,   b'\\', b'z', b'x', b'c', b'v', b'b', b'n',
    b'm', b',', b'.', b'/', 0,   b'*', 0,   b' ', 0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
];

static SCANCODE_SHIFT_ASCII: [u8; 128] = [
    0,   0,   b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*',
    b'(', b')', b'_', b'+', 8,   9,   b'Q', b'W', b'E', b'R',
    b'T', b'Y', b'U', b'I', b'O', b'P', b'{', b'}', 10,  0,
    b'A', b'S', b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':',
    b'"', b'~', 0,   b'|', b'Z', b'X', b'C', b'V', b'B', b'N',
    b'M', b'<', b'>', b'?', 0,   b'*', 0,   b' ', 0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
];

unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port);
    val
}

extern "C" {
    fn krust_irq_install_handler(irq: i32, handler: extern "C" fn(*mut Registers));
    fn krust_vga_putchar(c: u8);
}

fn push_key(key: i32) {
    unsafe {
        let next = (KEYHEAD + 1) % KEYBUF_SIZE;
        if next != KEYTAIL {
            KEYBUF[KEYHEAD] = key;
            KEYHEAD = next;
        }
    }
}

fn push_char(c: u8) {
    unsafe {
        let next = (HEAD + 1) % BUF_SIZE;
        if next != TAIL {
            BUF[HEAD] = c;
            HEAD = next;
        }
    }
}

extern "C" fn ps2keyboard_callback(_r: *mut Registers) {
    unsafe {
        let scancode = inb(0x60);

        if scancode == 0xE0 {
            EXT_SCAN = 0xE0;
            return;
        }
        if scancode == 0xE1 {
            EXT_SCAN = 0xE1;
            return;
        }

        let release = (scancode & 0x80) != 0;
        let sc = scancode & 0x7F;

        if sc == 0x2A || sc == 0x36 {
            SHIFT_PRESSED = !release;
            return;
        }
        if sc == 0x1D {
            CTRL_PRESSED = !release;
            return;
        }
        if sc == 0x38 {
            ALT_PRESSED = !release;
            return;
        }
        if sc == 0x3A && !release {
            CAPS_LOCK = !CAPS_LOCK;
            return;
        }

        if release {
            EXT_SCAN = 0;
            return;
        }

        let mut key: i32 = 0;

        if EXT_SCAN == 0xE0 {
            EXT_SCAN = 0;
            match sc {
                0x48 => key = KEY_UP,
                0x50 => key = KEY_DOWN,
                0x4B => key = KEY_LEFT,
                0x4D => key = KEY_RIGHT,
                0x49 => key = KEY_PGUP,
                0x51 => key = KEY_PGDN,
                0x47 => key = KEY_HOME,
                0x4F => key = KEY_END,
                _ => {}
            }
        } else {
            EXT_SCAN = 0;
            if sc >= 0x3B && sc <= 0x44 {
                key = KEY_F1 + (sc as i32 - 0x3B);
            } else if sc >= 0x57 && sc <= 0x58 {
                key = KEY_F11 + (sc as i32 - 0x57);
            } else {
                let mut c = if SHIFT_PRESSED {
                    SCANCODE_SHIFT_ASCII[sc as usize]
                } else {
                    SCANCODE_ASCII[sc as usize]
                };
                if c >= b'a' && c <= b'z' && CAPS_LOCK {
                    c -= 32;
                } else if c >= b'A' && c <= b'Z' && CAPS_LOCK {
                    c += 32;
                }
                if c != 0 {
                    key = c as i32;
                    push_char(c);
                }
            }
        }

        if key != 0 {
            push_key(key);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ps2kbd_init() {
    krust_irq_install_handler(1, ps2keyboard_callback);
}

#[no_mangle]
pub unsafe extern "C" fn krust_ps2kbd_getchar() -> u8 {
    while HEAD == TAIL {
        core::hint::spin_loop();
    }
    let c = BUF[TAIL];
    TAIL = (TAIL + 1) % BUF_SIZE;
    c
}

#[no_mangle]
pub unsafe extern "C" fn krust_ps2kbd_getkey() -> i32 {
    while KEYHEAD == KEYTAIL {
        core::hint::spin_loop();
    }
    let k = KEYBUF[KEYTAIL];
    KEYTAIL = (KEYTAIL + 1) % KEYBUF_SIZE;
    k
}

#[no_mangle]
pub unsafe extern "C" fn krust_ps2kbd_data_available() -> bool {
    HEAD != TAIL
}

use crate::ps2mouse::MousePacket;

pub static mut GUI_ACTIVE: bool = false;

static mut MOUSE_X: i32 = 0;
static mut MOUSE_Y: i32 = 0;
static mut MOUSE_INITIALIZED: bool = false;
static mut SCREEN_W: i32 = 0;
static mut SCREEN_H: i32 = 0;

unsafe fn draw_cursor_at(cx: i32, cy: i32) {
    let white: u32 = 0xFFFFFF;
    let black: u32 = 0x000000;

    let bits: [u8; 14] = [
        0b00000001,
        0b00000011,
        0b00000111,
        0b00001111,
        0b00011111,
        0b00111111,
        0b01111111,
        0b11111111,
        0b01111110,
        0b00110110,
        0b00110011,
        0b01100011,
        0b11000011,
        0b11000000,
    ];

    for row in 0..14i32 {
        let y = cy + row;
        if y < 0 { continue; }
        let bits_row = bits[row as usize];
        for col in 0..8i32 {
            let x = cx + col;
            if x < 0 { continue; }
            if (bits_row & (1 << (7 - col))) != 0 {
                if col == 0 || (bits_row & (1 << (8 - col))) == 0 {
                    crate::fb_console::fb_console_putpixel(x as u32, y as u32, black);
                } else {
                    crate::fb_console::fb_console_putpixel(x as u32, y as u32, white);
                }
            }
        }
    }
}

unsafe fn erase_cursor_at(cx: i32, cy: i32) {
    let bits: [u8; 14] = [
        0b00000001,
        0b00000011,
        0b00000111,
        0b00001111,
        0b00011111,
        0b00111111,
        0b01111111,
        0b11111111,
        0b01111110,
        0b00110110,
        0b00110011,
        0b01100011,
        0b11000011,
        0b11000000,
    ];

    for row in 0..14i32 {
        let y = cy + row;
        if y < 0 { continue; }
        let bits_row = bits[row as usize];
        for col in 0..8i32 {
            let x = cx + col;
            if x < 0 { continue; }
            if (bits_row & (1 << (7 - col))) != 0 {
                let bg = crate::fb_console::fb_console_get_bg_at(x as u32, y as u32);
                crate::fb_console::fb_console_putpixel(x as u32, y as u32, bg);
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn mouse_cursor_init() {
    SCREEN_W = crate::fb_console::fb_console_get_width() as i32;
    SCREEN_H = crate::fb_console::fb_console_get_height() as i32;
    MOUSE_X = SCREEN_W / 2;
    MOUSE_Y = SCREEN_H / 2;
    MOUSE_INITIALIZED = true;
}

#[no_mangle]
pub unsafe extern "C" fn mouse_cursor_update() {
    if !MOUSE_INITIALIZED { return; }
    if GUI_ACTIVE { return; }
    if !crate::ps2mouse::krust_ps2mouse_data_available() { return; }

    let mut pkt = MousePacket { flags: 0, dx: 0, dy: 0 };
    if !crate::ps2mouse::krust_ps2mouse_read_packet(&mut pkt as *mut _) { return; }

    erase_cursor_at(MOUSE_X, MOUSE_Y);

    MOUSE_X += pkt.dx as i32;
    MOUSE_Y -= pkt.dy as i32;

    if MOUSE_X < 0 { MOUSE_X = 0; }
    if MOUSE_Y < 0 { MOUSE_Y = 0; }
    if MOUSE_X >= SCREEN_W { MOUSE_X = SCREEN_W - 1; }
    if MOUSE_Y >= SCREEN_H { MOUSE_Y = SCREEN_H - 1; }

    draw_cursor_at(MOUSE_X, MOUSE_Y);
}

#[no_mangle]
pub unsafe extern "C" fn mouse_cursor_get_x() -> i32 { MOUSE_X }
#[no_mangle]
pub unsafe extern "C" fn mouse_cursor_get_y() -> i32 { MOUSE_Y }

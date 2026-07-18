use core::ptr;

const VGA_BUF: *mut u16 = 0xB8000 as *mut u16;
const WIDTH: usize = 80;
const HEIGHT: usize = 25;

static mut ROW: usize = 0;
static mut COL: usize = 0;
static mut COLOR: u8 = 0x07; // light gray on black

fn vga_index(row: usize, col: usize) -> usize {
    row * WIDTH + col
}

fn make_entry(c: u8, color: u8) -> u16 {
    (color as u16) << 8 | c as u16
}

unsafe fn scroll() {
    if ROW >= HEIGHT {
        for r in 1..HEIGHT {
            for c in 0..WIDTH {
                let src = VGA_BUF.add(vga_index(r, c));
                let dst = VGA_BUF.add(vga_index(r - 1, c));
                ptr::write_volatile(dst, ptr::read_volatile(src));
            }
        }
        for c in 0..WIDTH {
            ptr::write_volatile(VGA_BUF.add(vga_index(HEIGHT - 1, c)), make_entry(b' ', COLOR));
        }
        ROW = HEIGHT - 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_init() {
    for i in 0..(WIDTH * HEIGHT) {
        ptr::write_volatile(VGA_BUF.add(i), make_entry(b' ', 0x07));
    }
    ROW = 0;
    COL = 0;
    COLOR = 0x07;
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_clear() {
    for i in 0..(WIDTH * HEIGHT) {
        ptr::write_volatile(VGA_BUF.add(i), make_entry(b' ', COLOR));
    }
    ROW = 0;
    COL = 0;
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_set_color(fg: u8, bg: u8) {
    COLOR = (bg << 4) | fg;
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_putchar(c: u8) {
    match c {
        b'\n' => { ROW += 1; COL = 0; }
        b'\r' => { COL = 0; }
        b'\t' => { COL = (COL + 8) & !7; }
        0x08 => { if COL > 0 { COL -= 1; } }
        _ => {
            ptr::write_volatile(VGA_BUF.add(vga_index(ROW, COL)), make_entry(c, COLOR));
            COL += 1;
            if COL >= WIDTH { COL = 0; ROW += 1; }
        }
    }
    scroll();
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_write(data: *const u8, len: usize) {
    for i in 0..len {
        krust_vga_putchar(ptr::read_volatile(data.add(i)));
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_writestring(s: *const u8) {
    let mut i = 0;
    loop {
        let c = ptr::read_volatile(s.add(i));
        if c == 0 { break; }
        krust_vga_putchar(c);
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_get_cursor_row() -> usize { ROW }
#[no_mangle]
pub unsafe extern "C" fn krust_vga_get_cursor_col() -> usize { COL }

#[no_mangle]
pub unsafe extern "C" fn krust_vga_set_pos(x: usize, y: usize) {
    if x < WIDTH { COL = x; }
    if y < HEIGHT { ROW = y; }
}

#[no_mangle]
pub unsafe extern "C" fn krust_vga_writestring_color(s: *const u8, color: u8) {
    let old = COLOR;
    COLOR = color;
    krust_vga_writestring(s);
    COLOR = old;
}

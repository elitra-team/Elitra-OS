use core::ptr;

const COM1: u16 = 0x3F8;

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val);
}

unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port);
    val
}

fn is_transmit_empty() -> bool {
    unsafe { (inb(COM1 + 5) & 0x20) != 0 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ns16550_init() {
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x80);
    outb(COM1 + 0, 0x03);
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x03);
    outb(COM1 + 2, 0xC7);
    outb(COM1 + 4, 0x0B);
    outb(COM1 + 4, 0x1E);
    outb(COM1 + 0, 0xAE);
    if inb(COM1 + 0) != 0xAE { return; }
    outb(COM1 + 4, 0x0F);
}

#[no_mangle]
pub unsafe extern "C" fn krust_ns16550_putchar(c: u8) {
    while !is_transmit_empty() {}
    outb(COM1, c);
    if c == b'\n' {
        krust_ns16550_putchar(b'\r');
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ns16550_write_str(s: *const u8) {
    let mut p = s;
    loop {
        let c = ptr::read_volatile(p);
        if c == 0 { break; }
        krust_ns16550_putchar(c);
        p = p.add(1);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ns16550_write_buf(data: *const u8, len: usize) {
    for i in 0..len {
        krust_ns16550_putchar(ptr::read_volatile(data.add(i)));
    }
}

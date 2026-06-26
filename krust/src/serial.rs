use core::ptr;

const COM1: u16 = 0x3F8;

unsafe fn inb(port: u16) -> u8 {
    let result: u8;
    core::arch::asm!("in al, dx", out("al") result, in("dx") port, options(nostack, preserves_flags));
    result
}

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nostack, preserves_flags));
}

#[no_mangle]
pub unsafe extern "C" fn krust_serial_init() {
    outb(COM1 + 1, 0x00); // disable interrupts
    outb(COM1 + 3, 0x80); // enable DLAB
    outb(COM1 + 0, 0x01); // divisor low  (115200 baud)
    outb(COM1 + 1, 0x00); // divisor high
    outb(COM1 + 3, 0x03); // 8n1
    outb(COM1 + 2, 0xC7); // enable FIFO, clear, 14-byte threshold
    outb(COM1 + 4, 0x0B); // enable IRQ, RTS/DSR set
}

#[no_mangle]
pub unsafe extern "C" fn krust_serial_putchar(c: u8) {
    while (inb(COM1 + 5) & 0x20) == 0 {}
    outb(COM1, c);
}

#[no_mangle]
pub unsafe extern "C" fn krust_serial_write(data: *const u8, len: usize) {
    for i in 0..len {
        let c = ptr::read_volatile(data.add(i));
        if c == b'\n' { krust_serial_putchar(b'\r'); }
        krust_serial_putchar(c);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_serial_writestring(s: *const u8) {
    let mut i = 0;
    loop {
        let c = ptr::read_volatile(s.add(i));
        if c == 0 { break; }
        if c == b'\n' { krust_serial_putchar(b'\r'); }
        krust_serial_putchar(c);
        i += 1;
    }
}

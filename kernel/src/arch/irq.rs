
use crate::scheduler::Registers;

pub type ISRHandler = extern "C" fn(*mut Registers);

static mut ROUTINES: [Option<ISRHandler>; 16] = [None; 16];

extern "C" {
    fn irq0();  fn irq1();  fn irq2();  fn irq3();
    fn irq4();  fn irq5();  fn irq6();  fn irq7();
    fn irq8();  fn irq9();  fn irq10(); fn irq11();
    fn irq12(); fn irq13(); fn irq14(); fn irq15();
}

pub fn install() {
    unsafe {
        let stubs = [
            irq0, irq1, irq2, irq3, irq4, irq5, irq6, irq7,
            irq8, irq9, irq10, irq11, irq12, irq13, irq14, irq15,
        ];

        core::arch::asm!("cli");

        outb(0x20, 0x11);
        outb(0xA0, 0x11);
        outb(0x21, 0x20);
        outb(0xA1, 0x28);
        outb(0x21, 0x04);
        outb(0xA1, 0x02);
        outb(0x21, 0x01);
        outb(0xA1, 0x01);
        outb(0x21, 0x00);
        outb(0xA1, 0x00);

        for (i, &stub) in stubs.iter().enumerate() {
            crate::idt::set_gate(32 + i, stub as u64, 0x08, 0x8E);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn irq_handler(r: *mut Registers) {
    handler(r);
}

fn handler(r: *mut Registers) {
    unsafe {
        let int_no = (*r).int_no;
        if int_no >= 40 {
            outb(0xA0, 0x20);
        }
        outb(0x20, 0x20);

        let irq = ((*r).int_no as i32) - 32;
        if irq >= 0 && irq < 16 {
            if let Some(h) = ROUTINES[irq as usize] {
                h(r);
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_irq_install_handler(irq: i32, handler_fn: ISRHandler) {
    if irq >= 0 && irq < 16 {
        ROUTINES[irq as usize] = Some(handler_fn);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_irq_uninstall_handler(irq: i32) {
    if irq >= 0 && irq < 16 {
        ROUTINES[irq as usize] = None;
    }
}

pub unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val);
}

pub unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port);
    val
}

pub unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val);
}

pub unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    core::arch::asm!("in ax, dx", out("ax") val, in("dx") port);
    val
}

pub unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") val);
}

pub unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    core::arch::asm!("in eax, dx", out("eax") val, in("dx") port);
    val
}

pub unsafe fn enable_interrupts() {
    core::arch::asm!("sti");
}

pub unsafe fn disable_interrupts() {
    core::arch::asm!("cli");
}

pub unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
    );
}

#[no_mangle]
pub unsafe extern "C" fn krust_outw(port: u16, val: u16) {
    outw(port, val);
}

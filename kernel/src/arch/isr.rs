
use crate::scheduler::Registers;

pub type ISRHandler = extern "C" fn(*mut Registers);

static mut HANDLERS: [Option<ISRHandler>; 256] = [None; 256];

const EXCEPTION_MESSAGES: [&str; 32] = [
    "Division By Zero",
    "Debug",
    "Non Maskable Interrupt",
    "Breakpoint",
    "Overflow",
    "BOUND Range Exceeded",
    "Invalid Opcode",
    "Device Not Available",
    "Double Fault",
    "Coprocessor Segment Overrun",
    "Invalid TSS",
    "Segment Not Present",
    "Stack-Segment Fault",
    "General Protection Fault",
    "Page Fault",
    "Reserved",
    "x87 FPU Floating-Point Error",
    "Alignment Check",
    "Machine Check",
    "SIMD Floating-Point Exception",
    "Virtualization Exception",
    "Control Protection Exception",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Hypervisor Injection Exception",
    "VMM Communication Exception",
    "Security Exception",
    "Reserved",
];

extern "C" {
    fn isr0();  fn isr1();  fn isr2();  fn isr3();
    fn isr4();  fn isr5();  fn isr6();  fn isr7();
    fn isr8();  fn isr9();  fn isr10(); fn isr11();
    fn isr12(); fn isr13(); fn isr14(); fn isr15();
    fn isr16(); fn isr17(); fn isr18(); fn isr19();
    fn isr20(); fn isr21(); fn isr22(); fn isr23();
    fn isr24(); fn isr25(); fn isr26(); fn isr27();
    fn isr28(); fn isr29(); fn isr30(); fn isr31();
    fn isr_reschedule();
    fn isr_e1000();
}

pub fn install() {
    let stubs = [
        isr0, isr1, isr2, isr3, isr4, isr5, isr6, isr7,
        isr8, isr9, isr10, isr11, isr12, isr13, isr14, isr15,
        isr16, isr17, isr18, isr19, isr20, isr21, isr22, isr23,
        isr24, isr25, isr26, isr27, isr28, isr29, isr30, isr31,
    ];
    for (i, &stub) in stubs.iter().enumerate() {
        crate::idt::set_gate(i, stub as u64, 0x08, 0x8E);
    }
    // Vector 0x40: reschedule IPI for SMP
    crate::idt::set_gate(0x40, isr_reschedule as u64, 0x08, 0x8E);
    // Vector 0x41: e1000 network card interrupt
    crate::idt::set_gate(0x41, isr_e1000 as u64, 0x08, 0x8E);
}

#[no_mangle]
pub unsafe extern "C" fn isr_handler(r: *mut Registers) {
    handler(r);
}

unsafe fn puthex(val: u64) {
    let hex = b"0123456789ABCDEF";
    let mut buf = [0u8; 18];
    buf[0] = b'0';
    buf[1] = b'x';
    for i in (2..18).rev() {
        buf[i] = hex[(val & 0xF) as usize];
        let v = val >> 4;
        if v == 0 { break; }
    }
    crate::vga::krust_vga_write(buf.as_ptr(), 18);
}

fn handler(r: *mut Registers) {
    unsafe {
        let int_no = (*r).int_no as usize;

        if int_no < 256 {
            if let Some(h) = HANDLERS[int_no] {
                h(r);
                return;
            }
        }

        let msg = if int_no < 32 {
            EXCEPTION_MESSAGES[int_no]
        } else {
            "Unknown"
        };

        crate::vga::krust_vga_writestring(b"\n[PANIC] Unhandled exception: \0" as *const u8);
        crate::vga::krust_vga_writestring(msg.as_ptr());
        crate::vga::krust_vga_putchar(b' ');
        puthex((*r).int_no as u64);
        crate::vga::krust_vga_writestring(b" err=\0" as *const u8);
        puthex((*r).err_code as u64);
        crate::vga::krust_vga_putchar(b'\n');

        crate::vga::krust_vga_writestring(b"RIP=\0" as *const u8);
        puthex((*r).rip);
        crate::vga::krust_vga_writestring(b" CS=\0" as *const u8);
        puthex((*r).cs as u64);
        crate::vga::krust_vga_writestring(b" RFLAGS=\0" as *const u8);
        puthex((*r).rflags as u64);
        crate::vga::krust_vga_putchar(b'\n');

        crate::vga::krust_vga_writestring(b"RAX=\0" as *const u8);
        puthex((*r).rax);
        crate::vga::krust_vga_writestring(b" RBX=\0" as *const u8);
        puthex((*r).rbx);
        crate::vga::krust_vga_writestring(b" RCX=\0" as *const u8);
        puthex((*r).rcx);
        crate::vga::krust_vga_writestring(b" RDX=\0" as *const u8);
        puthex((*r).rdx);
        crate::vga::krust_vga_putchar(b'\n');
        crate::vga::krust_vga_writestring(b"RSP=\0" as *const u8);
        puthex((*r).user_rsp);
        crate::vga::krust_vga_writestring(b" RBP=\0" as *const u8);
        puthex((*r).rbp);
        crate::vga::krust_vga_writestring(b" RSI=\0" as *const u8);
        puthex((*r).rsi);
        crate::vga::krust_vga_writestring(b" RDI=\0" as *const u8);
        puthex((*r).rdi);
        crate::vga::krust_vga_putchar(b'\n');

        core::arch::asm!("cli");
        loop {
            core::arch::asm!("hlt");
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_isr_register_handler(vec: u8, handler_fn: ISRHandler) {
    HANDLERS[vec as usize] = Some(handler_fn);
}

#[no_mangle]
pub unsafe extern "C" fn krust_isr_unregister_handler(vec: u8) {
    HANDLERS[vec as usize] = None;
}

#![no_std]
#![allow(non_camel_case_types)]

pub const KERNEL_NAME: &str = "Elitra OS";
pub const KERNEL_VERSION: &str = "0.2.0";
pub const KERNEL_ARCH: &str = "x86-64";

pub mod arch;
pub mod mm;
pub mod drivers;
pub mod fs;
pub mod net;
pub mod power;
pub mod process;
pub mod ui;
pub mod util;
pub mod kernel_init;

// Re-exports for backward compatibility (crate::module paths)
pub use arch::{gdt, idt, irq, isr, smp, tss};
pub use mm::{cow, heap, paging, pmm, swap, vma};
pub use drivers::pci;
pub use drivers::display::{fb_console, framebuffer, vga};
pub use drivers::input::{ps2keyboard, ps2mouse};
pub use drivers::serial::{ns16550, serial};
pub use drivers::sound::hda;
pub use drivers::storage::{ahci, ata_pio, block, nvme, usb_storage, virtio_blk};
pub use drivers::usb::{ehci, usb};
pub use fs::{ext2, fat32, mount, procfs, vfs};
pub use power::{acpi, apic, apic_hw, cmos_rtc, hpet, ioapic, pittimer};
pub use process::{scheduler, syscalls};
pub use ui::{cli_art, gui, mouse_cursor, shell, terminal};
pub use util::{cpuid, elf, klib, rdrand, socket, spinlock};

use core::panic::PanicInfo;

unsafe fn serial_puthex(val: u64) {
    let hex = b"0123456789abcdef";
    crate::serial::krust_serial_putchar(b'0');
    crate::serial::krust_serial_putchar(b'x');
    for i in (0..16).rev() {
        let nibble = ((val >> (i * 4)) & 0xF) as usize;
        crate::serial::krust_serial_putchar(hex[nibble]);
    }
}

unsafe fn vga_puthex(val: u64) {
    let hex = b"0123456789abcdef";
    crate::vga::krust_vga_putchar(b'0');
    crate::vga::krust_vga_putchar(b'x');
    for i in (0..16).rev() {
        let nibble = ((val >> (i * 4)) & 0xF) as usize;
        crate::vga::krust_vga_putchar(hex[nibble]);
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        let cr2: u64;
        let cr3: u64;
        let rsp: u64;
        let rbp: u64;
        let rip: u64;
        core::arch::asm!("mov {cr2}, cr2", cr2 = out(reg) cr2);
        core::arch::asm!("mov {cr3}, cr3", cr3 = out(reg) cr3);
        core::arch::asm!("mov {rsp}, rsp", rsp = out(reg) rsp);
        core::arch::asm!("mov {rbp}, rbp", rbp = out(reg) rbp);
        rip = info.location().map_or(0, |l| l.file() as *const str as *const u8 as u64);

        crate::vga::krust_vga_set_color(0xF, 0x4); // white on red
        crate::vga::krust_vga_writestring(b"\n*** KERNEL PANIC ***\n\0" as *const u8);

        // Print panic message to VGA
        {
            crate::vga::krust_vga_writestring(b"Message: \0" as *const u8);
            // Format the message via the Display impl
            let mut buf = [0u8; 256];
            use core::fmt::Write;
            struct BufWriter<'a> {
                buf: &'a mut [u8],
                pos: usize,
            }
            impl<'a> core::fmt::Write for BufWriter<'a> {
                fn write_str(&mut self, s: &str) -> core::fmt::Result {
                    for b in s.bytes() {
                        if self.pos < self.buf.len() - 1 {
                            self.buf[self.pos] = b;
                            self.pos += 1;
                        }
                    }
                    Ok(())
                }
            }
            let mut writer = BufWriter { buf: &mut buf, pos: 0 };
            let _ = write!(writer, "{}", info.message());
            writer.buf[writer.pos] = 0;
            crate::vga::krust_vga_writestring(writer.buf.as_ptr());
            crate::vga::krust_vga_putchar(b'\n');
        }

        crate::vga::krust_vga_writestring(b"RIP: \0" as *const u8);
        vga_puthex(rip);
        crate::vga::krust_vga_writestring(b"\nRSP: \0" as *const u8);
        vga_puthex(rsp);
        crate::vga::krust_vga_writestring(b"\nRBP: \0" as *const u8);
        vga_puthex(rbp);
        crate::vga::krust_vga_writestring(b"\nCR2: \0" as *const u8);
        vga_puthex(cr2);
        crate::vga::krust_vga_writestring(b"\nCR3: \0" as *const u8);
        vga_puthex(cr3);
        crate::vga::krust_vga_writestring(b"\n\0" as *const u8);

        // Serial output
        crate::serial::krust_serial_writestring(b"\n*** KERNEL PANIC ***\n\0" as *const u8);
        if let Some(loc) = info.location() {
            crate::serial::krust_serial_writestring(b" at \0" as *const u8);
            crate::serial::krust_serial_writestring(loc.file().as_ptr() as *const u8);
            crate::serial::krust_serial_putchar(b':');
            let line = loc.line();
            let mut lbuf = [0u8; 12];
            let mut tmp = line;
            let mut i = 10;
            lbuf[11] = 0;
            if tmp == 0 { lbuf[i] = b'0'; i -= 1; }
            while tmp > 0 { lbuf[i] = b'0' + (tmp % 10) as u8; tmp /= 10; i -= 1; }
            crate::serial::krust_serial_writestring(lbuf.as_ptr().add(i + 1));
        }
        crate::serial::krust_serial_putchar(b'\n');
        crate::serial::krust_serial_writestring(b"RIP: \0" as *const u8);
        serial_puthex(rip);
        crate::serial::krust_serial_writestring(b"\nRSP: \0" as *const u8);
        serial_puthex(rsp);
        crate::serial::krust_serial_writestring(b"\nRBP: \0" as *const u8);
        serial_puthex(rbp);
        crate::serial::krust_serial_writestring(b"\nCR2: \0" as *const u8);
        serial_puthex(cr2);
        crate::serial::krust_serial_writestring(b"\nCR3: \0" as *const u8);
        serial_puthex(cr3);
        crate::serial::krust_serial_writestring(b"\n\0" as *const u8);
    }
    loop { core::hint::spin_loop() }
}

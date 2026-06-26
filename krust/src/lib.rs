#![no_std]
#![allow(non_camel_case_types)]

pub mod klib;
pub mod vga;
pub mod serial;
pub mod pmm;
pub mod fat32;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe {
        crate::vga::krust_vga_writestring(b"*** KERNEL PANIC ***\n\0" as *const u8);
    }
    loop { core::hint::spin_loop() }
}

// --- Types ---

pub type size_t = usize;
pub type uint8_t = u8;
pub type uint16_t = u16;
pub type uint32_t = u32;
pub type int32_t = i32;

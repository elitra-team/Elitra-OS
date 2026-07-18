use core::ptr;

pub const IOAPIC_ID: u32 = 0x00;
pub const IOAPIC_VER: u32 = 0x01;
pub const IOAPIC_ARB: u32 = 0x02;
pub const IOAPIC_REDRTBL: u32 = 0x10;

const IOAPIC_INT_DISABLE: u32 = 1 << 16;
const IOAPIC_INT_LEVEL: u32 = 1 << 15;
const IOAPIC_REMOTEIRR: u32 = 1 << 14;
const IOAPIC_DELIVS: u32 = 1 << 12;
const IOAPIC_DEST_PHYS: u32 = 0 << 11;
const IOAPIC_DEST_LOGICAL: u32 = 1 << 11;
const IOAPIC_DM_LOWEST: u32 = 1 << 8;
const IOAPIC_DM_FIXED: u32 = 0;

static mut IOAPIC_BASE: *mut u8 = core::ptr::null_mut();
static mut IOAPIC_VERSION: u8 = 0;
static mut IOAPIC_MAX_REDIRECT: u8 = 0;
static mut Available: bool = false;

unsafe fn ioapic_read(reg: u32) -> u32 {
    ptr::write_volatile(IOAPIC_BASE as *mut u32, reg);
    ptr::read_volatile(IOAPIC_BASE.add(0x10) as *const u32)
}

unsafe fn ioapic_write(reg: u32, val: u32) {
    ptr::write_volatile(IOAPIC_BASE as *mut u32, reg);
    ptr::write_volatile(IOAPIC_BASE.add(0x10) as *mut u32, val);
}

pub fn is_available() -> bool {
    unsafe { Available }
}

pub fn max_redirect() -> u8 {
    unsafe { IOAPIC_MAX_REDIRECT }
}

pub fn version() -> u8 {
    unsafe { IOAPIC_VERSION }
}

pub fn init(mmio_addr: u64) -> bool {
    unsafe {
        IOAPIC_BASE = crate::paging::krust_map_mmio(mmio_addr, 0x1000) as *mut u8;
        if IOAPIC_BASE.is_null() { return false; }

        crate::ns16550::krust_ns16550_write_str(b"ioapic: base mapped\n\0".as_ptr());

        let ver = ioapic_read(IOAPIC_VER);
        crate::ns16550::krust_ns16550_write_str(b"ioapic: ver read\n\0".as_ptr());
        IOAPIC_VERSION = ver as u8;
        IOAPIC_MAX_REDIRECT = ((ver >> 16) & 0xFF) as u8;

        crate::ns16550::krust_ns16550_write_str(b"ioapic: disabling\n\0".as_ptr());
        if IOAPIC_MAX_REDIRECT > 0 {
            for i in 0..=IOAPIC_MAX_REDIRECT {
                disable_redirect(i);
            }
        }
        crate::ns16550::krust_ns16550_write_str(b"ioapic: all disabled\n\0".as_ptr());

        Available = true;
        true
    }
}

pub fn set_redirect(irq: u8, vector: u8, apic_id: u8, level: bool, active_low: bool) {
    unsafe {
        let idx = IOAPIC_REDRTBL + (irq as u32) * 2;

        let low = vector as u32
            | IOAPIC_DEST_PHYS
            | if level { IOAPIC_INT_LEVEL } else { 0 }
            | if active_low { 1 << 13 } else { 0 };

        let high = (apic_id as u32) << 24;

        ioapic_write(idx + 1, high);
        ioapic_write(idx, low);
    }
}

pub fn set_redirect_logical(irq: u8, vector: u8, cpu_mask: u8, level: bool, active_low: bool) {
    unsafe {
        let idx = IOAPIC_REDRTBL + (irq as u32) * 2;

        let low = vector as u32
            | IOAPIC_DEST_LOGICAL
            | if level { IOAPIC_INT_LEVEL } else { 0 }
            | if active_low { 1 << 13 } else { 0 };

        let high = (cpu_mask as u32) << 24;

        ioapic_write(idx + 1, high);
        ioapic_write(idx, low);
    }
}

pub fn disable_redirect(irq: u8) {
    unsafe {
        let idx = IOAPIC_REDRTBL + (irq as u32) * 2;
        let low = ioapic_read(idx);
        ioapic_write(idx, low | IOAPIC_INT_DISABLE);
    }
}

pub fn enable_redirect(irq: u8) {
    unsafe {
        let idx = IOAPIC_REDRTBL + (irq as u32) * 2;
        let low = ioapic_read(idx);
        ioapic_write(idx, low & !IOAPIC_INT_DISABLE);
    }
}

pub fn read_redirect(irq: u8) -> (u32, u32) {
    unsafe {
        let idx = IOAPIC_REDRTBL + (irq as u32) * 2;
        (ioapic_read(idx), ioapic_read(idx + 1))
    }
}

pub fn mask_all() {
    let max = unsafe { IOAPIC_MAX_REDIRECT };
    for i in 0..=max {
        disable_redirect(i);
    }
}

pub fn unmask_irq(irq: u8) {
    enable_redirect(irq);
}

pub fn mask_irq(irq: u8) {
    disable_redirect(irq);
}

pub fn read_id() -> u32 {
    unsafe { ioapic_read(IOAPIC_ID) }
}

pub fn read_version() -> u32 {
    unsafe { ioapic_read(IOAPIC_VER) }
}

pub fn read_arb() -> u32 {
    unsafe { ioapic_read(IOAPIC_ARB) }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ioapic_init(mmio_addr: u64) -> bool {
    init(mmio_addr)
}

#[no_mangle]
pub unsafe extern "C" fn krust_ioapic_set_redirect(irq: u8, vector: u8, apic_id: u8, level: i32, active_low: i32) {
    set_redirect(irq, vector, apic_id, level != 0, active_low != 0);
}

#[no_mangle]
pub unsafe extern "C" fn krust_ioapic_disable_redirect(irq: u8) {
    disable_redirect(irq);
}

#[no_mangle]
pub unsafe extern "C" fn krust_ioapic_enable_redirect(irq: u8) {
    enable_redirect(irq);
}

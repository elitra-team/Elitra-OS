
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct IDTEntry {
    pub base_low: u16,
    pub sel: u16,
    pub ist: u8,
    pub flags: u8,
    pub base_mid: u16,
    pub base_high: u32,
    pub reserved: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct IDTPtr {
    pub limit: u16,
    pub base: u64,
}

static mut ENTRIES: [IDTEntry; 256] = [IDTEntry {
    base_low: 0,
    sel: 0,
    ist: 0,
    flags: 0,
    base_mid: 0,
    base_high: 0,
    reserved: 0,
}; 256];

static mut PTR: IDTPtr = IDTPtr { limit: 0, base: 0 };

extern "C" {
    fn idt_flush(addr: u64);
}

pub fn set_gate(num: usize, base: u64, sel: u16, flags: u8) {
    if num >= 256 {
        return;
    }
    unsafe {
        ENTRIES[num].base_low = (base & 0xFFFF) as u16;
        ENTRIES[num].base_mid = ((base >> 16) & 0xFFFF) as u16;
        ENTRIES[num].base_high = ((base >> 32) & 0xFFFFFFFF) as u32;
        ENTRIES[num].sel = sel;
        ENTRIES[num].ist = 0;
        ENTRIES[num].flags = flags;
        ENTRIES[num].reserved = 0;
    }
}

pub fn install() {
    unsafe {
        PTR.limit = (core::mem::size_of::<IDTEntry>() * 256 - 1) as u16;
        PTR.base = &ENTRIES as *const _ as u64;

        core::ptr::write_bytes(ENTRIES.as_mut_ptr() as *mut u8, 0, core::mem::size_of::<IDTEntry>() * 256);

        idt_flush(&PTR as *const _ as u64);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_idt_set_gate(num: u8, base: u64, sel: u16, flags: u8) {
    set_gate(num as usize, base, sel, flags);
}

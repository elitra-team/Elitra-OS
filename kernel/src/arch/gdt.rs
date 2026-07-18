
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct GDTEntry {
    pub limit_low: u16,
    pub base_low: u16,
    pub base_middle: u8,
    pub access: u8,
    pub granularity: u8,
    pub base_high: u8,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct GDTPtr {
    pub limit: u16,
    pub base: u64,
}

pub const KERNEL_CS: u16 = 0x08;
pub const KERNEL_DS: u16 = 0x10;
pub const USER_CS: u16 = 0x1B;
pub const USER_DS: u16 = 0x23;
pub const TSS_SEL: u16 = 0x28;

static mut ENTRIES: [GDTEntry; 7] = [GDTEntry {
    limit_low: 0,
    base_low: 0,
    base_middle: 0,
    access: 0,
    granularity: 0,
    base_high: 0,
}; 7];

static mut PTR: GDTPtr = GDTPtr { limit: 0, base: 0 };

extern "C" {
    fn gdt_flush(addr: u64);
}

pub fn set_gate(num: usize, base: u64, limit: u32, access: u8, gran: u8) {
    if num >= 7 {
        return;
    }
    unsafe {
        ENTRIES[num].limit_low = (limit & 0xFFFF) as u16;
        ENTRIES[num].base_low = (base & 0xFFFF) as u16;
        ENTRIES[num].base_middle = ((base >> 16) & 0xFF) as u8;
        ENTRIES[num].base_high = ((base >> 24) & 0xFF) as u8;
        ENTRIES[num].granularity = (((limit >> 16) & 0x0F) as u8) | (gran & 0xF0);
        ENTRIES[num].access = access;
    }
}

pub fn install() {
    unsafe {
        PTR.limit = (core::mem::size_of::<GDTEntry>() * 7 - 1) as u16;
        PTR.base = &ENTRIES as *const _ as u64;

        set_gate(0, 0, 0, 0, 0);
        set_gate(1, 0, 0xFFFFFFFF, 0x9A, 0x20);
        set_gate(2, 0, 0xFFFFFFFF, 0x92, 0x00);
        set_gate(3, 0, 0xFFFFFFFF, 0xFA, 0x20);
        set_gate(4, 0, 0xFFFFFFFF, 0xF2, 0x00);

        gdt_flush(&PTR as *const _ as u64);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_gdt_entries() -> *mut GDTEntry {
    ENTRIES.as_mut_ptr()
}


#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct TSSEntry64 {
    pub reserved0: u32,
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    pub reserved1: u64,
    pub ist1: u64,
    pub ist2: u64,
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    pub reserved2: u64,
    pub reserved3: u16,
    pub iomap_base: u16,
}

// TODO: On SMP, all CPUs share a single TSS. For ring3 interrupts,
// hardware uses TSS.rsp0 to find the kernel stack. Since we use
// syscall entry via gs:[0] instead, TSS.rsp0 is only relevant for
// hardware interrupts from user mode. Per-CPU TSS needed if we
// support user-mode interrupts on APs.
pub static mut ENTRY: TSSEntry64 = TSSEntry64 {
    reserved0: 0,
    rsp0: 0,
    rsp1: 0,
    rsp2: 0,
    reserved1: 0,
    ist1: 0,
    ist2: 0,
    ist3: 0,
    ist4: 0,
    ist5: 0,
    ist6: 0,
    ist7: 0,
    reserved2: 0,
    reserved3: 0,
    iomap_base: 0,
};

pub fn init() {
    unsafe {
        core::ptr::write_bytes(&mut ENTRY as *mut _ as *mut u8, 0, core::mem::size_of::<TSSEntry64>());

        ENTRY.rsp0 = 0;
        ENTRY.iomap_base = core::mem::size_of::<TSSEntry64>() as u16;

        let base = &ENTRY as *const _ as u64;
        let limit = core::mem::size_of::<TSSEntry64>() as u32 - 1;

        let entries = crate::gdt::krust_gdt_entries();
        let desc = entries.add(5) as *mut u64;
        *desc = (limit & 0xFFFF) as u64
            | ((base & 0xFFFFFF) << 16)
            | (0x89u64 << 40)
            | ((((limit >> 16) & 0x0F) as u64) << 48)
            | ((base & 0xFF000000u64) << 32);
        *desc.add(1) = base >> 32;

        core::arch::asm!("ltr ax", in("ax") 0x28u16);

        crate::vga::krust_vga_writestring_color(
            b"TSS installed\n\0" as *const u8,
            0x0A,
        );
        crate::ns16550::krust_ns16550_write_str(b"tss: installed\n\0" as *const u8);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_tss_set_kernel_stack(rsp: u64) {
    ENTRY.rsp0 = rsp;
}

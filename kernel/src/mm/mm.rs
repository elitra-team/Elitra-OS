#[repr(C, packed)]
struct MultibootInfo {
    flags: u32,
    mem_lower: u32,
    mem_upper: u32,
    boot_device: u32,
    cmdline: u32,
    mods_count: u32,
    mods_addr: u32,
    syms: [u32; 4],
    mmap_length: u32,
    mmap_addr: u32,
    drives_length: u32,
    drives_addr: u32,
    config_table: u32,
    boot_loader_name: u32,
    apm_table: u32,
    vbe_control_info: u32,
    vbe_mode_info: u32,
    vbe_mode: u16,
    vbe_interface_seg: u16,
    vbe_interface_off: u16,
    vbe_interface_len: u16,
    framebuffer_addr: u64,
    framebuffer_pitch: u32,
    framebuffer_width: u32,
    framebuffer_height: u32,
    framebuffer_bpp: u8,
    framebuffer_type: u8,
    color_info: [u8; 6],
}

extern "C" {
    static _kernel_end: u32;
    fn krust_ns16550_write_str(s: *const u8);
}

static mut PLACEMENT_ADDR: u64 = 0;
static mut TOTAL_MEMORY_KB: u32 = 0;
static mut USED_MEMORY_KB: u32 = 0;

#[no_mangle]
pub unsafe extern "C" fn krust_mm_init(magic: u32, addr: u32) -> u32 {
    PLACEMENT_ADDR = &_kernel_end as *const u32 as u64;

    let mem_upper_kb: u32 = if magic != 0x2BADB002 {
        32 * 1024 - 1024
    } else {
        let mbi = &*(addr as *const MultibootInfo);
        if mbi.flags & (1 << 0) != 0 { mbi.mem_upper } else { 32 * 1024 - 1024 }
    };

    krust_ns16550_write_str(b"step: mm detected memory\n\0" as *const u8);

    TOTAL_MEMORY_KB = mem_upper_kb + 1024;
    USED_MEMORY_KB = ((PLACEMENT_ADDR - 0x100000 + 1023) / 1024) as u32;

    krust_ns16550_write_str(b"step: mm calculated\n\0" as *const u8);

    crate::pmm::krust_mm_cpp_pmm_init(mem_upper_kb, PLACEMENT_ADDR);

    krust_ns16550_write_str(b"step: after pmm init\n\0" as *const u8);
    krust_ns16550_write_str(b"step: before display printf\n\0" as *const u8);

    TOTAL_MEMORY_KB
}

#[no_mangle]
pub unsafe extern "C" fn krust_mm_kmalloc(size: usize) -> *mut core::ffi::c_void {
    let addr = PLACEMENT_ADDR;
    PLACEMENT_ADDR += size as u64;
    PLACEMENT_ADDR = (PLACEMENT_ADDR + 3) & !3;
    USED_MEMORY_KB = ((PLACEMENT_ADDR - 0x100000 + 1023) / 1024) as u32;
    addr as *mut core::ffi::c_void
}

#[no_mangle]
pub unsafe extern "C" fn krust_mm_kmalloc_aligned(size: usize, align: u32) -> *mut core::ffi::c_void {
    let mut addr = PLACEMENT_ADDR;
    let align64 = align as u64;
    if addr & (align64 - 1) != 0 {
        addr = (addr + align64 - 1) & !(align64 - 1);
    }
    PLACEMENT_ADDR = addr + size as u64;
    USED_MEMORY_KB = ((PLACEMENT_ADDR - 0x100000 + 1023) / 1024) as u32;
    addr as *mut core::ffi::c_void
}

#[no_mangle]
pub unsafe extern "C" fn krust_mm_info(total_kb: *mut u32, free_kb: *mut u32) {
    *total_kb = TOTAL_MEMORY_KB;
    *free_kb = if USED_MEMORY_KB < TOTAL_MEMORY_KB {
        TOTAL_MEMORY_KB - USED_MEMORY_KB
    } else {
        0
    };
}

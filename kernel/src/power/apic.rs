
use core::arch::asm;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU8, Ordering};

pub struct PerCpu {
    pub cpu_id: u8,
    pub apic_id: u8,
    pub core_id: u8,
    pub apic_id_done: AtomicU8,
}

impl PerCpu {
    pub fn get_current() -> &'static mut Self {
        let cpu_id = get_cpu_id();
        unsafe { &mut (*PER_CPU_DATA.as_mut_ptr())[cpu_id as usize] }
    }
}

// Use MaybeUninit<[PerCpu; 256]> - avoids requiring PerCpu: Copy
static mut PER_CPU_DATA: MaybeUninit<[PerCpu; 256]> = MaybeUninit::uninit();

#[inline]
pub fn get_cpu_id() -> u8 {
    let id: u64;
    unsafe { asm!("mov {}, gs:[0]", out(reg) id); }
    id as u8
}

pub fn init_per_cpu() {
    let cpu_id = get_cpu_id() as usize;
    unsafe {
        (*PER_CPU_DATA.as_mut_ptr())[cpu_id] = PerCpu {
            cpu_id: cpu_id as u8,
            apic_id: 0,
            core_id: 0,
            apic_id_done: AtomicU8::new(0),
        };
    }
}

pub extern "C" fn smp_ap_entry() {
    let cpu_id = get_cpu_id();
    unsafe {
        asm!("xor %eax, %eax");
        asm!("mov %ds, %ax");
        asm!("mov %gs, %dx");
        asm!("mov %dx, %ss");
        let per_cpu = &mut (*PER_CPU_DATA.as_mut_ptr())[cpu_id as usize];
        per_cpu.apic_id = detect_apic_id();
        per_cpu.core_id = detect_core_id();
        per_cpu.apic_id_done.store(1, Ordering::Relaxed);
    }
}

pub fn start_aps() {
    let bsp_id = get_cpu_id();
    let cpu_count = get_cpu_count();
    unsafe { crate::serial::krust_serial_writestring(b"Starting APs...\n\0" as *const u8); }
    for apic_id in 0..cpu_count {
        let cpu_id = find_cpu_id_for_apic(apic_id);
        if cpu_id == 0 || cpu_id == bsp_id { continue; }
        send_ipi_to_cpu(apic_id, 0x00000100);
    }
    unsafe { crate::serial::krust_serial_writestring(b"All APs started\n\0" as *const u8); }
}

pub fn find_cpu_id_for_apic(apic_id: u8) -> u8 {
    for cpu_id in 0..=255 {
        unsafe {
            let per_cpu = &(*PER_CPU_DATA.as_ptr())[cpu_id as usize];
            if per_cpu.apic_id == apic_id {
                return cpu_id;
            }
        }
    }
    0
}

pub fn send_ipi_to_cpu(target_apic_id: u8, icr_value: u32) {
    let _cr8: u64;
    unsafe { asm!("mov {}, cr8", out(reg) _cr8); }
    let _ = target_apic_id;
    let _ = icr_value;
}

pub fn detect_apic_id() -> u8 {
    let mut id = 0u32;
    unsafe { asm!("cpuid", lateout("eax") id, in("eax") 1, in("ecx") 0); }
    ((id >> 24) & 0xFF) as u8
}

pub fn detect_core_id() -> u8 {
    let mut ecx = 0u32;
    unsafe { asm!("cpuid", lateout("ecx") ecx, in("eax") 1, in("ecx") 0); }
    (ecx & 0xFF) as u8
}

pub fn wait_for_aps() {
    let cpu_count = get_cpu_count();
    unsafe { crate::serial::krust_serial_writestring(b"Waiting for APs to come up...\n\0" as *const u8); }
    for apic_id in 0..cpu_count {
        if apic_id == get_cpu_id() { continue; }
        let cpu_id = find_cpu_id_for_apic(apic_id);
        while unsafe { (*PER_CPU_DATA.as_ptr())[cpu_id as usize].apic_id_done.load(Ordering::Relaxed) } == 0 {}
    }
    unsafe { crate::serial::krust_serial_writestring(b"All APs online!\n\0" as *const u8); }
}

pub fn get_cpu_count() -> u8 {
    unsafe { crate::apic_hw::krust_apic_get_cpu_count() }
}

#[no_mangle]
pub extern "C" fn krust_apic_init_per_cpu() {
    init_per_cpu();
}
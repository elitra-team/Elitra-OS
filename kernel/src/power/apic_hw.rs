use crate::scheduler::Registers;

// LAPIC register offsets
const LAPIC_ID: u32 = 0x0020;
const LAPIC_TPR: u32 = 0x0080;
const LAPIC_EOI: u32 = 0x00B0;
const LAPIC_SVR: u32 = 0x00F0;
const LAPIC_ICR_LO: u32 = 0x0300;
const LAPIC_ICR_HI: u32 = 0x0310;
const LAPIC_LVT_TIMER: u32 = 0x0320;
const LAPIC_LVT_THERMAL: u32 = 0x0330;
const LAPIC_LVT_PERF: u32 = 0x0340;
const LAPIC_LVT_LINT0: u32 = 0x0350;
const LAPIC_LVT_LINT1: u32 = 0x0360;
const LAPIC_LVT_ERROR: u32 = 0x0370;
const LAPIC_TIMER_INITCNT: u32 = 0x0380;
const LAPIC_TIMER_DIV: u32 = 0x03E0;

// LAPIC flags
const LAPIC_LVT_MASKED: u32 = 0x00010000;
const LAPIC_TIMER_PERIODIC: u32 = 0x00020000;
const LAPIC_SVR_ENABLE: u32 = 0x00000100;
const LAPIC_ICR_INIT: u32 = 0x00000500;
const LAPIC_ICR_STARTUP: u32 = 0x00000600;
const LAPIC_ICR_DELIVS: u32 = 0x00001000;
const LAPIC_ICR_BCAST: u32 = 0x00080000;

#[derive(Copy, Clone)]
#[repr(C, packed)]
struct PerCPU {
    cpu_id: u32,
    apic_id: u32,
    kernel_stack: u32,
    tss: u32,
}

static mut PERCPU_DATA: [PerCPU; 256] = [PerCPU { cpu_id: 0, apic_id: 0, kernel_stack: 0, tss: 0 }; 256];
static mut AP_READY_COUNT: u32 = 0;
static mut LAPIC_VADDR: *mut u64 = core::ptr::null_mut();
static mut ENABLED: bool = false;

extern "C" {
    fn krust_map_mmio(phys: u64, size: u64) -> u64;
    fn krust_pmm_alloc_frame() -> usize;
    fn krust_pittimer_sleep(ms: u32);
    fn krust_irq_install_handler(irq: i32, handler: extern "C" fn(*mut Registers));
    fn krust_vga_writestring_color(s: *const u8, color: u8);
}

unsafe fn lapic_write(reg: u32, value: u32) {
    if !LAPIC_VADDR.is_null() {
        core::ptr::write_volatile(LAPIC_VADDR.add((reg / 4) as usize), value as u64);
    }
}

unsafe fn lapic_read(reg: u32) -> u32 {
    if !LAPIC_VADDR.is_null() {
        core::ptr::read_volatile(LAPIC_VADDR.add((reg / 4) as usize)) as u32
    } else {
        0
    }
}

unsafe fn map_lapic() {
    let smp = &crate::acpi::SMP_INFO as *const crate::acpi::SMPInfo;
    let lapic_phys = (*smp).lapic_addr as u64;
    LAPIC_VADDR = krust_map_mmio(lapic_phys, 0x1000) as *mut u64;
    if LAPIC_VADDR.is_null() {
        krust_vga_writestring_color(b"APIC: failed to map LAPIC\n\0" as *const u8, 0x0C);
        return;
    }
}

unsafe fn lapic_init_hw() {
    lapic_write(LAPIC_SVR, LAPIC_SVR_ENABLE | 0xFF);
    lapic_write(LAPIC_TPR, 0);
    lapic_write(LAPIC_LVT_TIMER, LAPIC_LVT_MASKED);
    lapic_write(LAPIC_LVT_THERMAL, LAPIC_LVT_MASKED);
    lapic_write(LAPIC_LVT_PERF, LAPIC_LVT_MASKED);
    lapic_write(LAPIC_LVT_LINT0, LAPIC_LVT_MASKED);
    lapic_write(LAPIC_LVT_LINT1, LAPIC_LVT_MASKED);
    lapic_write(LAPIC_LVT_ERROR, LAPIC_LVT_MASKED);
    lapic_write(LAPIC_TIMER_DIV, 0x3);
    lapic_write(LAPIC_TIMER_INITCNT, 0xFFFFFFFF);
    lapic_write(LAPIC_LVT_TIMER, 0x20 | LAPIC_TIMER_PERIODIC);
}

extern "C" fn lapic_timer_handler(_r: *mut Registers) {
    unsafe {
        lapic_write(LAPIC_LVT_TIMER, 0x20 | LAPIC_TIMER_PERIODIC);
        lapic_write(LAPIC_EOI, 0);
    }
}

unsafe fn ioapic_init_hw() {
    // Stub — IOAPIC setup will be added when needed
}

unsafe fn pic_disable_hw() {
    crate::irq::outb(0xA1, 0xFF);
    crate::irq::outb(0x21, 0xFF);
}

unsafe fn lapic_send_ipi(icr_lo: u32, icr_hi: u32) {
    lapic_write(LAPIC_ICR_HI, icr_hi);
    lapic_write(LAPIC_ICR_LO, icr_lo);
    while lapic_read(LAPIC_ICR_LO) & LAPIC_ICR_DELIVS != 0 {
        core::arch::asm!("pause");
    }
}

unsafe fn prepare_trampoline() {
    let smp = &crate::acpi::SMP_INFO as *const crate::acpi::SMPInfo;
    for i in 0..(*smp).cpu_count {
        PERCPU_DATA[i as usize].cpu_id = i as u32;
        PERCPU_DATA[i as usize].apic_id = (*smp).apic_ids[i as usize] as u32;
        let stack = krust_pmm_alloc_frame();
        if stack != 0 {
            PERCPU_DATA[i as usize].kernel_stack = (stack as u64 + 0x1000) as u32;
        }
    }
    let bsp_id = lapic_read(LAPIC_ID) >> 24;
    let smp_mut = &mut crate::acpi::SMP_INFO;
    smp_mut.bsp_apic_id = bsp_id as u8;
    AP_READY_COUNT = 1;
}

// ─── Public API (called from kernel_init.rs) ──────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_apic_init() {
    pic_disable_hw();
    lapic_init_hw();
    ioapic_init_hw();
}

#[no_mangle]
pub unsafe extern "C" fn krust_apic_init_full() {
    if crate::acpi::krust_acpi_is_available() == 0 {
        krust_vga_writestring_color(b"APIC: ACPI not available\n\0" as *const u8, 0x0C);
        return;
    }
    map_lapic();
    if LAPIC_VADDR.is_null() { return; }
    lapic_init_hw();
    ioapic_init_hw();
    pic_disable_hw();
    krust_irq_install_handler(0x20, lapic_timer_handler);
    ENABLED = true;
    krust_vga_writestring_color(b"APIC: initialized\n\0" as *const u8, 0x0A);
}

#[no_mangle]
pub unsafe extern "C" fn krust_apic_eoi() {
    lapic_write(LAPIC_EOI, 0);
}

#[no_mangle]
pub unsafe extern "C" fn krust_apic_ap_init() {
    if LAPIC_VADDR.is_null() {
        map_lapic();
    }
    if LAPIC_VADDR.is_null() { return; }
    lapic_write(LAPIC_SVR, LAPIC_SVR_ENABLE | 0xFF);
    lapic_write(LAPIC_TPR, 0);
    lapic_write(LAPIC_LVT_LINT0, LAPIC_LVT_MASKED);
    lapic_write(LAPIC_LVT_LINT1, LAPIC_LVT_MASKED);
    lapic_write(LAPIC_LVT_ERROR, LAPIC_LVT_MASKED);
    lapic_write(LAPIC_TIMER_DIV, 0x3);
    lapic_write(LAPIC_TIMER_INITCNT, 0xFFFFFFFF);
    lapic_write(LAPIC_LVT_TIMER, 0x20 | LAPIC_TIMER_PERIODIC);
}

#[no_mangle]
pub unsafe extern "C" fn krust_apic_start_aps() {
    prepare_trampoline();
    lapic_send_ipi(LAPIC_ICR_INIT | LAPIC_ICR_BCAST, 0);
    krust_pittimer_sleep(10);
    let smp = &crate::acpi::SMP_INFO as *const crate::acpi::SMPInfo;
    for i in 0..(*smp).cpu_count {
        if (*smp).apic_ids[i as usize] == (*smp).bsp_apic_id {
            continue;
        }
        for _sipi in 0..2 {
            lapic_send_ipi(
                LAPIC_ICR_STARTUP | 0x10,
                ((*smp).apic_ids[i as usize] as u32) << 24,
            );
            krust_pittimer_sleep(1);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_apic_wait_for_aps() {
    let smp = &crate::acpi::SMP_INFO as *const crate::acpi::SMPInfo;
    while AP_READY_COUNT < (*smp).cpu_count as u32 {
        core::arch::asm!("pause");
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_apic_ap_ready() {
    AP_READY_COUNT += 1;
}

#[no_mangle]
pub unsafe extern "C" fn krust_apic_get_cpu_count() -> u8 {
    crate::acpi::SMP_INFO.cpu_count
}

#[no_mangle]
pub unsafe extern "C" fn krust_apic_is_enabled() -> bool {
    ENABLED
}

// ─── Stub functions referenced by old C++ code ────────────────

#[no_mangle]
pub extern "C" fn parse_madt() {}

#[no_mangle]
pub extern "C" fn map_lapic_stub() {}

#[no_mangle]
pub extern "C" fn lapic_verify() -> i32 { 1 }

#[no_mangle]
pub extern "C" fn lapic_init() {}

#[no_mangle]
pub extern "C" fn ioapic_init() {}

#[no_mangle]
pub extern "C" fn pic_disable() {}

#[no_mangle]
pub extern "C" fn lapic_write_stub(_reg: u32, _value: u32) {}

#[no_mangle]
pub extern "C" fn lapic_read_stub(_reg: u32) -> u32 { 0 }

#[no_mangle]
pub extern "C" fn lapic_timer_init(_count: u32, _vector: u8) {}

#[no_mangle]
pub static mut madt: *const u8 = core::ptr::null();

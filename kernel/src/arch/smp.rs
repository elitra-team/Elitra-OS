

/// Per-CPU data structure, one instance per CPU core
/// IMPORTANT: kernel_stack must be at offset 0 for syscall_entry via gs:[0]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct PerCpuData {
    pub kernel_stack: u64,
    pub cpu_id: u32,
    pub apic_id: u32,
    pub is_bsp: bool,
    pub current_task: *mut crate::scheduler::Task,
    pub idle_task: *mut crate::scheduler::Task,
    pub fpu_saved: bool,
    pub padding: [u8; 7],
}

unsafe impl Send for PerCpuData {}

impl PerCpuData {
    pub const fn empty() -> Self {
        Self {
            kernel_stack: 0,
            cpu_id: 0,
            apic_id: 0,
            is_bsp: false,
            current_task: core::ptr::null_mut(),
            idle_task: core::ptr::null_mut(),
            fpu_saved: false,
            padding: [0; 7],
        }
    }
}

const MAX_CPUS: usize = 256;

/// Per-CPU data array
static mut CPU_DATA: [PerCpuData; MAX_CPUS] = [PerCpuData::empty(); MAX_CPUS];
static mut CPU_COUNT: u32 = 0;
static mut BSP_ID: u32 = 0;

/// Trampoline binary blob (compiled from arch/x86_64/trampoline.asm)
/// This gets copied to physical address 0x8000 before AP startup
#[link_section = ".trampoline"]
static TRAMPOLINE_BIN: [u8; 238] = *include_bytes!("../../../arch/x86_64/trampoline.bin");

/// Communication block offsets relative to 0x8000
const COMM_APIC_ID: u64 = 0x8040;
const COMM_ACK: u64 = 0x8044;
const COMM_CR3: u64 = 0x8048;
const COMM_CR4: u64 = 0x8050;
const COMM_STACK: u64 = 0x8058;
const COMM_PERCPU_PTR: u64 = 0x8060;
const COMM_ENTRY: u64 = 0x8068;

/// LAPIC MMIO base (set during APIC init)
static mut LAPIC_BASE: *mut u64 = core::ptr::null_mut();

// LAPIC register offsets
const LAPIC_ICR_LO: u32 = 0x0300;
const LAPIC_ICR_HI: u32 = 0x0310;
const LAPIC_ICR_DELIVS: u32 = 0x00001000;
const LAPIC_ICR_INIT: u32 = 0x00000500;
const LAPIC_ICR_STARTUP: u32 = 0x00000600;
const LAPIC_ICR_BCAST: u32 = 0x00080000;

unsafe fn lapic_write(reg: u32, val: u32) {
    if !LAPIC_BASE.is_null() {
        core::ptr::write_volatile(LAPIC_BASE.add((reg / 4) as usize), val as u64);
    }
}

unsafe fn lapic_read(reg: u32) -> u32 {
    if !LAPIC_BASE.is_null() {
        core::ptr::read_volatile(LAPIC_BASE.add((reg / 4) as usize)) as u32
    } else {
        0
    }
}

unsafe fn lapic_send_ipi(icr_lo: u32, icr_hi: u32) {
    lapic_write(LAPIC_ICR_HI, icr_hi);
    lapic_write(LAPIC_ICR_LO, icr_lo);
    while lapic_read(LAPIC_ICR_LO) & LAPIC_ICR_DELIVS != 0 {
        core::arch::asm!("pause");
    }
}

unsafe fn write_phys_u8(addr: u64, val: u8) {
    let ptr = addr as *mut u8;
    core::ptr::write_volatile(ptr, val);
}

unsafe fn write_phys_u32(addr: u64, val: u32) {
    let ptr = addr as *mut u32;
    core::ptr::write_volatile(ptr, val);
}

unsafe fn write_phys_u64(addr: u64, val: u64) {
    let ptr = addr as *mut u64;
    core::ptr::write_volatile(ptr, val);
}

unsafe fn read_phys_u32(addr: u64) -> u32 {
    let ptr = addr as *const u32;
    core::ptr::read_volatile(ptr)
}

/// Copy trampoline binary to physical address 0x8000
fn install_trampoline() {
    unsafe {
        for (i, &byte) in TRAMPOLINE_BIN.iter().enumerate() {
            write_phys_u8(0x8000 + i as u64, byte);
        }
    }
}

unsafe fn set_kernel_gs_base(base: u64) {
    const MSR_GS_BASE: u32 = 0xC0000101;
    core::arch::asm!(
        "wrmsr",
        in("eax") (base & 0xFFFFFFFF) as u32,
        in("edx") ((base >> 32) & 0xFFFFFFFF) as u32,
        in("ecx") MSR_GS_BASE,
    );
}

/// Sleep using PIT timer (port I/O)
fn pit_sleep(ms: u32) {
    // Use a simple busy-loop approximation based on PIT tick rate (~1.193 MHz)
    // For 1ms ≈ 1193 iterations, but we'll use a simpler approximation
    // calling the existing PIT sleep function
    crate::pittimer::krust_pittimer_sleep(ms);
}

/// Set up communication block at 0x8040 for a specific AP
fn setup_comm_block(
    apic_id: u8,
    cr3: u64,
    kernel_stack: u64,
    per_cpu_ptr: *const PerCpuData,
    entry_fn: u64,
) {
    unsafe {
        write_phys_u32(COMM_APIC_ID, apic_id as u32);
        write_phys_u32(COMM_ACK, 0);
        write_phys_u64(COMM_CR3, cr3);
        // Compute CR4: PAE + PGE + SMEP + SMAP (if supported)
        let mut cr4: u64 = (1 << 5) | (1 << 7); // PAE + PGE
        if crate::cpuid::has_smep() { cr4 |= 1 << 20; }
        if crate::cpuid::has_smap() { cr4 |= 1 << 21; }
        write_phys_u64(COMM_CR4, cr4);
        write_phys_u64(COMM_STACK, kernel_stack);
        write_phys_u64(COMM_PERCPU_PTR, per_cpu_ptr as u64);
        write_phys_u64(COMM_ENTRY, entry_fn);
    }
}

/// Start all Application Processors using INIT-SIPI-SIPI sequence
#[no_mangle]
pub unsafe extern "C" fn krust_smp_start_aps() {
    let cpu_count = crate::acpi::SMP_INFO.cpu_count as u32;

    BSP_ID = lapic_read(0x0020) >> 24; // LAPIC ID register
    CPU_COUNT = if cpu_count > 0 { cpu_count } else { 1 };

    // Set up per-CPU data for BSP
    CPU_DATA[0].cpu_id = 0;
    CPU_DATA[0].apic_id = BSP_ID;
    CPU_DATA[0].is_bsp = true;

    let bsp_ptr = &CPU_DATA[0] as *const PerCpuData as u64;
    set_kernel_gs_base(bsp_ptr);

    if cpu_count <= 1 {
        return;
    }

    // Get current PML4 (CR3)
    let cr3: u64;
    core::arch::asm!("mov {}, cr3", out(reg) cr3);

    // Install trampoline at physical 0x8000
    install_trampoline();

    // Start each AP
    for i in 0..cpu_count {
        let apic_id = crate::acpi::SMP_INFO.apic_ids[i as usize];
        if apic_id == BSP_ID as u8 {
            continue;
        }

        let cpu_idx = i as usize;
        CPU_DATA[cpu_idx].cpu_id = i;
        CPU_DATA[cpu_idx].apic_id = apic_id as u32;
        CPU_DATA[cpu_idx].is_bsp = false;

        // Allocate kernel stack for this AP
        let stack_frame = crate::pmm::krust_pmm_alloc_frame();
        if stack_frame == 0 {
            continue;
        }
        let stack_top = (stack_frame as u64 + 4096) as u64;
        CPU_DATA[cpu_idx].kernel_stack = stack_top;

        // Set up communication block
        setup_comm_block(
            apic_id,
            cr3,
            stack_top,
            &CPU_DATA[cpu_idx] as *const PerCpuData,
            ap_entry as u64,
        );

        // INIT IPI (broadcast)
        lapic_send_ipi(LAPIC_ICR_INIT | LAPIC_ICR_BCAST, 0);
        pit_sleep(10);

        // SIPI x2 (vector 0x08 → physical 0x8000)
        for _ in 0..2 {
            lapic_send_ipi(
                LAPIC_ICR_STARTUP | 0x08,
                (apic_id as u32) << 24,
            );
            pit_sleep(1);
        }

        // Wait for AP to ack (up to 100ms)
        for _ in 0..100000 {
            if read_phys_u32(COMM_ACK) != 0 {
                break;
            }
            core::arch::asm!("pause");
        }
    }
}

/// Rust entry point for Application Processors (called from trampoline)
#[no_mangle]
pub unsafe extern "C" fn ap_entry(per_cpu: *mut PerCpuData) {
    let cpu_id = (*per_cpu).cpu_id;

    set_kernel_gs_base(per_cpu as u64);

    crate::idt::install();

    crate::apic_hw::krust_apic_ap_init();

    crate::ns16550::krust_ns16550_write_str(b"smp: AP \0" as *const u8);
    crate::ns16550::krust_ns16550_write_str(b" online\n\0" as *const u8);

    // Create idle task for this AP and set it as current
    crate::scheduler::krust_sched_create_idle_for_cpu(cpu_id);

    crate::apic_hw::krust_apic_ap_ready();

    // AP idle loop: LAPIC timer fires vector 0x20 → krust_sched_preempt()
    // which dequeues tasks from the ready queue and runs them
    loop {
        core::arch::asm!("sti");
        core::arch::asm!("hlt");
    }
}

// ─── Public API ────────────────────────────────────────────────

/// Get number of CPUs
#[no_mangle]
pub unsafe extern "C" fn krust_smp_cpu_count() -> u32 {
    CPU_COUNT
}

/// Get per-CPU data for current CPU (reads LAPIC ID)
#[no_mangle]
pub unsafe extern "C" fn krust_smp_current_cpu_id() -> u32 {
    let apic_id = lapic_read(0x0020) >> 24;
    for i in 0..CPU_COUNT {
        if CPU_DATA[i as usize].apic_id == apic_id {
            return i;
        }
    }
    0
}

/// Get PerCpuData pointer for a given CPU index
#[no_mangle]
pub unsafe extern "C" fn krust_smp_get_cpu_data(cpu_id: u32) -> *mut PerCpuData {
    if (cpu_id as usize) < MAX_CPUS {
        &mut CPU_DATA[cpu_id as usize] as *mut PerCpuData
    } else {
        core::ptr::null_mut()
    }
}

/// Send reschedule IPI to a specific CPU
pub unsafe fn smp_reschedule(cpu_id: u32) {
    if cpu_id as usize >= MAX_CPUS { return; }
    let apic_id = CPU_DATA[cpu_id as usize].apic_id;
    if apic_id == 0 { return; }

    // Fixed delivery, physical destination, vector 0x40 (reschedule)
    // ICR_LO: vector=0x40, delivery=Fixed, dest_mode=Physical, shorthand=None
    // ICR_HI: destination APIC ID in bits [31:24]
    lapic_send_ipi(
        0x40,
        apic_id << 24,
    );
}

/// Send reschedule IPI to all CPUs
pub unsafe fn smp_reschedule_all() {
    // Broadcast
    lapic_send_ipi(
        0x40 | LAPIC_ICR_BCAST,
        0,
    );
}

/// Get BSP CPU ID
#[no_mangle]
pub unsafe extern "C" fn krust_smp_bsp_id() -> u32 {
    BSP_ID
}

/// Set the kernel stack pointer for the current CPU (gs:0 = PerCpuData.kernel_stack)
pub unsafe fn smp_set_kernel_stack(ptr: u64) {
    let cpu_id = krust_smp_current_cpu_id();
    if (cpu_id as usize) < MAX_CPUS {
        CPU_DATA[cpu_id as usize].kernel_stack = ptr;
    }
}

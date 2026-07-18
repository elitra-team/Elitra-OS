use core::ptr;

pub const HPET_GEN_CAP: u64 = 0x000;
pub const HPET_CFG: u64 = 0x010;
pub const HPET_ISR: u64 = 0x020;
pub const HPET_MAIN_COUNTER: u64 = 0x0F0;
pub const HPET_CFG_ENABLE: u32 = 1;
pub const HPET_CFG_LEGACY: u32 = 2;

pub const HPET_TIMER_CFG: u64 = 0x100;
pub const HPET_TIMER_CMP: u64 = 0x108;
pub const HPET_TIMER_ROUTE: u64 = 0x110;
pub const HPET_TIMER_PERIODIC: u32 = 1 << 3;
pub const HPET_TIMER_FSB: u32 = 1 << 4;
pub const HPET_TIMER_INT_ENB: u32 = 1 << 2;

pub const TIMER_STRIDE: u64 = 0x20;

static mut HPET_BASE: *mut u8 = core::ptr::null_mut();
static mut FREQUENCY: u64 = 0;
static mut AVAILABLE: bool = false;

unsafe fn hpet_read(offset: u64) -> u64 {
    ptr::read_volatile(HPET_BASE.add(offset as usize) as *const u64)
}

unsafe fn hpet_write(offset: u64, val: u64) {
    ptr::write_volatile(HPET_BASE.add(offset as usize) as *mut u64, val);
}

pub fn is_available() -> bool {
    unsafe { AVAILABLE }
}

pub fn frequency() -> u64 {
    unsafe { FREQUENCY }
}

pub fn nanoseconds_per_tick() -> u64 {
    if unsafe { FREQUENCY } == 0 { return 0; }
    1_000_000_000 / unsafe { FREQUENCY }
}

pub fn init(mmio_addr: u64) -> bool {
    unsafe {
        HPET_BASE = crate::paging::krust_map_mmio(mmio_addr, 0x1000) as *mut u8;
        if HPET_BASE.is_null() { return false; }

        let cap = hpet_read(HPET_GEN_CAP);
        let clock_period_fs = (cap >> 32) as u32;
        if clock_period_fs == 0 { return false; }
        let num_timers = ((cap >> 8) & 0x1F) as u32;

        FREQUENCY = 1_000_000_000_000_000u64 / clock_period_fs as u64;

        hpet_write(HPET_CFG, 0);
        for i in 0..num_timers {
            let cfg_offset = HPET_TIMER_CFG + i as u64 * TIMER_STRIDE;
            hpet_write(cfg_offset, 0);
            let cmp_offset = HPET_TIMER_CMP + i as u64 * TIMER_STRIDE;
            hpet_write(cmp_offset, u64::MAX);
        }
        hpet_write(HPET_MAIN_COUNTER, 0);
        hpet_write(HPET_CFG, (HPET_CFG_ENABLE | HPET_CFG_LEGACY) as u64);

        AVAILABLE = true;
        true
    }
}

pub fn stop() {
    unsafe {
        hpet_write(HPET_CFG, 0);
    }
}

pub fn counter() -> u64 {
    unsafe { hpet_read(HPET_MAIN_COUNTER) }
}

pub fn ticks_to_ns(ticks: u64) -> u64 {
    ticks * nanoseconds_per_tick()
}

pub fn ns_to_ticks(ns: u64) -> u64 {
    if unsafe { FREQUENCY } == 0 { return 0; }
    ns * unsafe { FREQUENCY } / 1_000_000_000
}

pub fn busy_wait_ns(ns: u64) {
    let target = ns_to_ticks(ns);
    let start = counter();
    while counter().wrapping_sub(start) < target {
        core::hint::spin_loop();
    }
}

pub fn timer_config(timer: u32, enable: bool, periodic: bool, fsb: bool, irq: u32) {
    unsafe {
        let mut cfg: u32 = 0;
        if enable { cfg |= HPET_TIMER_INT_ENB; }
        if periodic { cfg |= HPET_TIMER_PERIODIC; }
        if fsb { cfg |= HPET_TIMER_FSB; }
        cfg |= irq & 0x1F;
        let offset = HPET_TIMER_CFG + timer as u64 * TIMER_STRIDE;
        hpet_write(offset, cfg as u64);
    }
}

pub fn timer_set_compare(timer: u32, value: u64) {
    unsafe {
        let offset = HPET_TIMER_CMP + timer as u64 * TIMER_STRIDE;
        hpet_write(offset, value);
    }
}

pub fn timer_set_periodic(timer: u32, interval_ticks: u64) {
    unsafe {
        let cfg_offset = HPET_TIMER_CFG + timer as u64 * TIMER_STRIDE;
        let cmp_offset = HPET_TIMER_CMP + timer as u64 * TIMER_STRIDE;
        hpet_write(cfg_offset, 0);
        let next = counter() + interval_ticks;
        hpet_write(cmp_offset, next);
        hpet_write(cfg_offset, (HPET_TIMER_INT_ENB | HPET_TIMER_PERIODIC) as u64);
    }
}

pub fn timer_stop(timer: u32) {
    unsafe {
        let offset = HPET_TIMER_CFG + timer as u64 * TIMER_STRIDE;
        hpet_write(offset, 0);
    }
}

pub fn clear_isr(timer: u32) {
    unsafe {
        hpet_write(HPET_ISR, 1 << timer);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_hpet_init(mmio_addr: u64) -> bool {
    init(mmio_addr)
}

#[no_mangle]
pub unsafe extern "C" fn krust_hpet_counter() -> u64 {
    counter()
}

#[no_mangle]
pub unsafe extern "C" fn krust_hpet_busy_wait_ns(ns: u64) {
    busy_wait_ns(ns);
}

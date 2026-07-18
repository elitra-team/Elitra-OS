use core::arch::asm;

static mut RDRAND_AVAILABLE: bool = false;
static mut RDSEED_AVAILABLE: bool = false;

pub fn init() {
    unsafe {
        let mut regs: [u32; 4] = [1, 0, 0, 0];
        asm!(
            "push rbx",
            "mov eax, [{p}]",
            "mov ecx, [{p} + 8]",
            "cpuid",
            "mov [{p}], eax",
            "mov [{p} + 4], ebx",
            "mov [{p} + 8], ecx",
            "mov [{p} + 12], edx",
            "pop rbx",
            p = in(reg) regs.as_mut_ptr(),
            out("eax") _,
            out("ecx") _,
            out("edx") _,
        );
        RDRAND_AVAILABLE = regs[2] & (1 << 30) != 0;

        regs = [7, 0, 0, 0];
        asm!(
            "push rbx",
            "mov eax, [{p}]",
            "mov ecx, [{p} + 8]",
            "cpuid",
            "mov [{p}], eax",
            "mov [{p} + 4], ebx",
            "mov [{p} + 8], ecx",
            "mov [{p} + 12], edx",
            "pop rbx",
            p = in(reg) regs.as_mut_ptr(),
            out("eax") _,
            out("ecx") _,
            out("edx") _,
        );
        RDSEED_AVAILABLE = regs[2] & (1 << 18) != 0;
    }
}

pub fn is_available() -> bool {
    unsafe { RDRAND_AVAILABLE }
}

pub fn has_rdseed() -> bool {
    unsafe { RDSEED_AVAILABLE }
}

unsafe fn do_rdrand_u16(val: &mut u16) -> bool {
    let ok: u8;
    let mut v = *val as u64;
    asm!(
        "stc",
        "rdrand {v}",
        "setc {ok_b}",
        v = inout(reg) v,
        ok_b = out(reg_byte) ok,
        options(nostack),
    );
    *val = v as u16;
    ok != 0
}

unsafe fn do_rdrand_u32(val: &mut u32) -> bool {
    let ok: u8;
    let mut v = *val as u64;
    asm!(
        "stc",
        "rdrand {v}",
        "setc {ok_b}",
        v = inout(reg) v,
        ok_b = out(reg_byte) ok,
        options(nostack),
    );
    *val = v as u32;
    ok != 0
}

unsafe fn do_rdrand_u64(val: &mut u64) -> bool {
    let ok: u8;
    asm!(
        "stc",
        "rdrand {v}",
        "setc {ok_b}",
        v = inout(reg) *val => *val,
        ok_b = out(reg_byte) ok,
        options(nostack),
    );
    ok != 0
}

unsafe fn do_rdseed_u16(val: &mut u16) -> bool {
    let ok: u8;
    let mut v = *val as u64;
    asm!(
        "stc",
        "rdseed {v}",
        "setc {ok_b}",
        v = inout(reg) v,
        ok_b = out(reg_byte) ok,
        options(nostack),
    );
    *val = v as u16;
    ok != 0
}

unsafe fn do_rdseed_u32(val: &mut u32) -> bool {
    let ok: u8;
    let mut v = *val as u64;
    asm!(
        "stc",
        "rdseed {v}",
        "setc {ok_b}",
        v = inout(reg) v,
        ok_b = out(reg_byte) ok,
        options(nostack),
    );
    *val = v as u32;
    ok != 0
}

unsafe fn do_rdseed_u64(val: &mut u64) -> bool {
    let ok: u8;
    asm!(
        "stc",
        "rdseed {v}",
        "setc {ok_b}",
        v = inout(reg) *val => *val,
        ok_b = out(reg_byte) ok,
        options(nostack),
    );
    ok != 0
}

pub fn rdrand_u16() -> Option<u16> {
    if !unsafe { RDRAND_AVAILABLE } { return None; }
    let mut val = 0u16;
    if unsafe { do_rdrand_u16(&mut val) } { Some(val) } else { None }
}

pub fn rdrand_u32() -> Option<u32> {
    if !unsafe { RDRAND_AVAILABLE } { return None; }
    let mut val = 0u32;
    if unsafe { do_rdrand_u32(&mut val) } { Some(val) } else { None }
}

pub fn rdrand_u64() -> Option<u64> {
    if !unsafe { RDRAND_AVAILABLE } { return None; }
    let mut val = 0u64;
    if unsafe { do_rdrand_u64(&mut val) } { Some(val) } else { None }
}

pub fn rdseed_u16() -> Option<u16> {
    if !unsafe { RDSEED_AVAILABLE } { return None; }
    let mut val = 0u16;
    if unsafe { do_rdseed_u16(&mut val) } { Some(val) } else { None }
}

pub fn rdseed_u32() -> Option<u32> {
    if !unsafe { RDSEED_AVAILABLE } { return None; }
    let mut val = 0u32;
    if unsafe { do_rdseed_u32(&mut val) } { Some(val) } else { None }
}

pub fn rdseed_u64() -> Option<u64> {
    if !unsafe { RDSEED_AVAILABLE } { return None; }
    let mut val = 0u64;
    if unsafe { do_rdseed_u64(&mut val) } { Some(val) } else { None }
}

pub fn fill_bytes(buf: &mut [u8]) {
    let mut i = 0;
    while i + 8 <= buf.len() {
        if let Some(val) = rdrand_u64() {
            buf[i..i + 8].copy_from_slice(&val.to_ne_bytes());
            i += 8;
        } else { break; }
    }
    while i + 4 <= buf.len() {
        if let Some(val) = rdrand_u32() {
            buf[i..i + 4].copy_from_slice(&val.to_ne_bytes());
            i += 4;
        } else { break; }
    }
    while i + 2 <= buf.len() {
        if let Some(val) = rdrand_u16() {
            buf[i..i + 2].copy_from_slice(&val.to_ne_bytes());
            i += 2;
        } else { break; }
    }
    while i < buf.len() {
        if let Some(val) = rdrand_u16() {
            buf[i] = val as u8;
            i += 1;
        } else { break; }
    }
}

pub fn random_u64() -> u64 {
    if let Some(val) = rdrand_u64() {
        return val;
    }
    let mut state: u64 = 0xDEAD_BEEF_CAFE_1234;
    unsafe {
        let tsc_lo: u32;
        let tsc_hi: u32;
        asm!("rdtsc", out("eax") tsc_lo, out("edx") tsc_hi);
        state ^= (tsc_hi as u64) << 32 | tsc_lo as u64;
    }
    state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    state ^= state >> 33;
    state = state.wrapping_mul(0xff51afd7ed558ccd);
    state ^= state >> 33;
    state
}

pub fn random_u32() -> u32 {
    random_u64() as u32
}

#[no_mangle]
pub unsafe extern "C" fn krust_rng_get_bytes(buf: *mut u8, len: usize) -> i32 {
    if buf.is_null() || len == 0 { return -1; }
    let slice = core::slice::from_raw_parts_mut(buf, len);
    fill_bytes(slice);
    len as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_rng_random() -> u64 {
    random_u64()
}

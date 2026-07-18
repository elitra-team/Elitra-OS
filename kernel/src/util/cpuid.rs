use core::arch::asm;
use core::ptr;

pub struct CpuidResult {
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
}

static mut HAS_RDRAND: bool = false;
static mut HAS_RDTSCP: bool = false;
static mut HAS_FSGSBASE: bool = false;
static mut HAS_SMEP: bool = false;
static mut HAS_SMAP: bool = false;
static mut HAS_SSE: bool = false;
static mut HAS_SSE2: bool = false;
static mut HAS_SSE3: bool = false;
static mut HAS_SSSE3: bool = false;
static mut HAS_SSE41: bool = false;
static mut HAS_SSE42: bool = false;
static mut HAS_AVX: bool = false;
static mut HAS_AVX2: bool = false;
static mut HAS_XSAVE: bool = false;
static mut HAS_FXSAVE: bool = false;
static mut HAS_HYPERVISOR: bool = false;
static mut MAX_BASIC_CPUID: u32 = 0;
static mut MAX_EXT_CPUID: u32 = 0;
static mut VENDOR: [u8; 13] = [0; 13];
static mut BRAND: [u8; 49] = [0; 49];
static mut INITIALIZED: bool = false;

pub fn cpuid(leaf: u32, subleaf: u32) -> CpuidResult {
    let mut out_eax: u32 = 0;
    let mut out_ebx: u32 = 0;
    let mut out_ecx: u32 = 0;
    let mut out_edx: u32 = 0;
    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov [{p_ebx}], ebx",
            "mov [{p_ecx}], ecx",
            "mov [{p_edx}], edx",
            "pop rbx",
            inout("eax") leaf => out_eax,
            inout("ecx") subleaf => out_ecx,
            out("edx") _,
            p_ebx = inout(reg) &mut out_ebx => _,
            p_ecx = inout(reg) &mut out_ecx => _,
            p_edx = inout(reg) &mut out_edx => _,
        );
    }
    CpuidResult { eax: out_eax, ebx: out_ebx, ecx: out_ecx, edx: out_edx }
}

pub fn vendor() -> &'static [u8; 13] {
    unsafe { &VENDOR }
}

pub fn brand() -> &'static [u8; 49] {
    unsafe { &BRAND }
}

pub fn has_rdrand() -> bool { unsafe { HAS_RDRAND } }
pub fn has_rdtscp() -> bool { unsafe { HAS_RDTSCP } }
pub fn has_fsgsbase() -> bool { unsafe { HAS_FSGSBASE } }
pub fn has_smep() -> bool { unsafe { HAS_SMEP } }
pub fn has_smap() -> bool { unsafe { HAS_SMAP } }
pub fn has_sse() -> bool { unsafe { HAS_SSE } }
pub fn has_sse2() -> bool { unsafe { HAS_SSE2 } }
pub fn has_sse3() -> bool { unsafe { HAS_SSE3 } }
pub fn has_ssse3() -> bool { unsafe { HAS_SSSE3 } }
pub fn has_sse41() -> bool { unsafe { HAS_SSE41 } }
pub fn has_sse42() -> bool { unsafe { HAS_SSE42 } }
pub fn has_avx() -> bool { unsafe { HAS_AVX } }
pub fn has_avx2() -> bool { unsafe { HAS_AVX2 } }
pub fn has_xsave() -> bool { unsafe { HAS_XSAVE } }
pub fn has_fxsave() -> bool { unsafe { HAS_FXSAVE } }
pub fn has_hypervisor() -> bool { unsafe { HAS_HYPERVISOR } }
pub fn max_basic_leaf() -> u32 { unsafe { MAX_BASIC_CPUID } }
pub fn max_ext_leaf() -> u32 { unsafe { MAX_EXT_CPUID } }

pub fn init() {
    unsafe {
        if INITIALIZED { return; }

        let r = cpuid(0, 0);
        MAX_BASIC_CPUID = r.eax;
        ptr::copy_nonoverlapping(&r.ebx as *const u32 as *const u8, VENDOR.as_mut_ptr(), 4);
        ptr::copy_nonoverlapping(&r.edx as *const u32 as *const u8, VENDOR.as_mut_ptr().add(4), 4);
        ptr::copy_nonoverlapping(&r.ecx as *const u32 as *const u8, VENDOR.as_mut_ptr().add(8), 4);
        VENDOR[12] = 0;

        let re = cpuid(0x80000000, 0);
        MAX_EXT_CPUID = re.eax;

        if MAX_EXT_CPUID >= 0x80000004 {
            let b1 = cpuid(0x80000002, 0);
            let b2 = cpuid(0x80000003, 0);
            let b3 = cpuid(0x80000004, 0);
            ptr::copy_nonoverlapping(&b1.eax as *const u32 as *const u8, BRAND.as_mut_ptr(), 16);
            ptr::copy_nonoverlapping(&b2.eax as *const u32 as *const u8, BRAND.as_mut_ptr().add(16), 16);
            ptr::copy_nonoverlapping(&b3.eax as *const u32 as *const u8, BRAND.as_mut_ptr().add(32), 16);
            BRAND[48] = 0;
        }

        let r1 = cpuid(1, 0);
        HAS_SSE = r1.edx & (1 << 25) != 0;
        HAS_SSE2 = r1.edx & (1 << 26) != 0;
        HAS_SSE3 = r1.ecx & (1 << 0) != 0;
        HAS_SSSE3 = r1.ecx & (1 << 9) != 0;
        HAS_SSE41 = r1.ecx & (1 << 19) != 0;
        HAS_SSE42 = r1.ecx & (1 << 20) != 0;
        HAS_AVX = r1.ecx & (1 << 28) != 0;
        HAS_XSAVE = r1.ecx & (1 << 26) != 0;
        HAS_FXSAVE = r1.edx & (1 << 24) != 0;
        HAS_RDRAND = r1.ecx & (1 << 30) != 0;
        HAS_HYPERVISOR = r1.ecx & (1 << 31) != 0;

        if MAX_BASIC_CPUID >= 7 {
            let r7 = cpuid(7, 0);
            HAS_AVX2 = r7.ebx & (1 << 5) != 0;
            HAS_FSGSBASE = r7.ebx & (1 << 0) != 0;
            HAS_SMEP = r7.ebx & (1 << 20) != 0;
            HAS_SMAP = r7.ecx & (1 << 20) != 0;
        }

        if MAX_EXT_CPUID >= 0x80000001 {
            let re1 = cpuid(0x80000001, 0);
            HAS_RDTSCP = re1.edx & (1 << 27) != 0;
        }

        INITIALIZED = true;
    }
}

pub fn print_info() {
    unsafe {
        crate::serial::krust_serial_writestring(b"CPUID: vendor=\0" as *const u8);
        crate::serial::krust_serial_writestring(VENDOR.as_ptr());
        crate::serial::krust_serial_writestring(b"\n\0" as *const u8);

        if MAX_EXT_CPUID >= 0x80000004 {
            crate::serial::krust_serial_writestring(b"CPUID: brand=\0" as *const u8);
            crate::serial::krust_serial_writestring(BRAND.as_ptr());
            crate::serial::krust_serial_writestring(b"\n\0" as *const u8);
        }

        let mut features = [0u8; 256];
        let mut pos = 0;

        macro_rules! check_feat {
            ($name:expr, $flag:expr) => {
                if $flag {
                    let name_bytes = $name.as_bytes();
                    let remaining = features.len() - pos - 1;
                    if remaining >= name_bytes.len() + 1 {
                        features[pos..pos + name_bytes.len()].copy_from_slice(name_bytes);
                        pos += name_bytes.len();
                        features[pos] = b' ';
                        pos += 1;
                    }
                }
            };
        }

        check_feat!("SSE", HAS_SSE);
        check_feat!("SSE2", HAS_SSE2);
        check_feat!("SSE3", HAS_SSE3);
        check_feat!("SSSE3", HAS_SSSE3);
        check_feat!("SSE4.1", HAS_SSE41);
        check_feat!("SSE4.2", HAS_SSE42);
        check_feat!("AVX", HAS_AVX);
        check_feat!("AVX2", HAS_AVX2);
        check_feat!("XSAVE", HAS_XSAVE);
        check_feat!("FXSAVE", HAS_FXSAVE);
        check_feat!("RDRAND", HAS_RDRAND);
        check_feat!("RDTSCP", HAS_RDTSCP);
        check_feat!("FSGSBASE", HAS_FSGSBASE);
        check_feat!("HYPERVISOR", HAS_HYPERVISOR);

        features[pos] = 0;

        crate::serial::krust_serial_writestring(b"CPUID: features:\0" as *const u8);
        crate::serial::krust_serial_writestring(features.as_ptr());
        crate::serial::krust_serial_writestring(b"\n\0" as *const u8);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_cpuid_init() {
    init();
}

#[no_mangle]
pub unsafe extern "C" fn krust_cpuid_has_sse() -> bool { has_sse() }
#[no_mangle]
pub unsafe extern "C" fn krust_cpuid_has_avx() -> bool { has_avx() }
#[no_mangle]
pub unsafe extern "C" fn krust_cpuid_has_rdrand() -> bool { has_rdrand() }

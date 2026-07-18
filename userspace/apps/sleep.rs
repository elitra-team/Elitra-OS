#![no_std]
#![no_main]

include!("../src/rt.rs");

fn parse_int(s: &[u8]) -> u32 {
    let mut n = 0u32;
    for &b in s {
        if b < b'0' || b > b'9' { break; }
        n = n.wrapping_mul(10).wrapping_add((b - b'0') as u32);
    }
    n
}

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) -> ! {
    if argc < 2 {
        sys_write(b"Usage: sleep <ms>\n");
        sys_exit();
    }
    let s = unsafe { core::str::from_utf8_unchecked(
        core::slice::from_raw_parts(*argv.add(1), strlen(*argv.add(1)))
    ) };
    let ms = parse_int(s.as_bytes());
    sys_sleep(ms);
    sys_exit();
}

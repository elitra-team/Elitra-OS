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

fn print_u32(n: u32) {
    let mut buf = [0u8; 12];
    let mut i = buf.len();
    if n == 0 {
        buf[0] = b'0';
        sys_write(&buf[..1]);
        return;
    }
    let mut x = n;
    while x > 0 {
        i -= 1;
        buf[i] = b'0' + (x % 10) as u8;
        x /= 10;
    }
    sys_write(&buf[i..]);
}

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) -> ! {
    let (first, last) = if argc < 2 {
        (1u32, 1u32)
    } else if argc < 3 {
        let a1 = unsafe { core::str::from_utf8_unchecked(
            core::slice::from_raw_parts(*argv.add(1), strlen(*argv.add(1)))
        ) };
        (1u32, parse_int(a1.as_bytes()))
    } else {
        let a1 = unsafe { core::str::from_utf8_unchecked(
            core::slice::from_raw_parts(*argv.add(1), strlen(*argv.add(1)))
        ) };
        let a2 = unsafe { core::str::from_utf8_unchecked(
            core::slice::from_raw_parts(*argv.add(2), strlen(*argv.add(2)))
        ) };
        (parse_int(a1.as_bytes()), parse_int(a2.as_bytes()))
    };

    let mut n = first;
    loop {
        print_u32(n);
        sys_write(b"\n");
        if n >= last { break; }
        n = n.wrapping_add(1);
    }
    sys_exit();
}

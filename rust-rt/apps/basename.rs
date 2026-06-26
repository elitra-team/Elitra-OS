#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) -> ! {
    if argc < 2 {
        sys_write(b"Usage: basename <path>\n");
        sys_exit();
    }
    let s = unsafe { core::str::from_utf8_unchecked(
        core::slice::from_raw_parts(*argv.add(1), strlen(*argv.add(1)))
    ) };
    let bytes = s.as_bytes();
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == b'/' { end -= 1; }
    if end == 0 {
        sys_write(b"/\n");
        sys_exit();
    }
    let mut start = end;
    while start > 0 && bytes[start - 1] != b'/' { start -= 1; }
    sys_write(&bytes[start..end]);
    sys_write(b"\n");
    sys_exit();
}

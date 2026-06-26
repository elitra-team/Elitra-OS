#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) -> ! {
    let mut first = true;
    for i in 1..argc as usize {
        let ptr = unsafe { *argv.add(i) };
        if ptr.is_null() { break; }
        let s = unsafe { core::str::from_utf8_unchecked(
            core::slice::from_raw_parts(ptr, strlen(ptr))
        ) };
        if !first {
            sys_write(b" ");
        }
        sys_write(s.as_bytes());
        first = false;
    }
    sys_write(b"\n");
    sys_exit();
}

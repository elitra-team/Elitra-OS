#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) -> ! {
    let mut buf = [0u8; 256];
    if sys_getcwd(&mut buf) == 0 {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
        if len > 0 {
            sys_write(&buf[..len]);
        }
        sys_write(b"\n");
    } else {
        sys_write(b"/\n");
    }
    sys_exit();
}

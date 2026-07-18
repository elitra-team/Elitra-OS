#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) -> ! {
    sys_write(b"HOME=/\n");
    sys_write(b"PATH=/bin:/mnt/bin\n");
    sys_write(b"USER=root\n");
    sys_exit();
}

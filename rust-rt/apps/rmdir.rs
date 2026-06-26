#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 2 {
        println!("Usage: rmdir <path>");
        sys_exit();
    }
    let path = unsafe { arg_at(argv, 1) };
    if sys_rmdir(path) < 0 {
        println!("rmdir: failed to remove directory '{}'", path);
    }
}

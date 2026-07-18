#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 3 {
        println!("Usage: mv <old> <new>");
        sys_exit();
    }
    let old = unsafe { arg_at(argv, 1) };
    let new = unsafe { arg_at(argv, 2) };
    if sys_rename(old, new) < 0 {
        println!("mv: failed to rename '{}' -> '{}'", old, new);
    }
}

#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    let dir = if argc < 2 {
        "/"
    } else {
        unsafe {
            let ptr = *argv.add(1);
            let mut len = 0;
            while *ptr.add(len) != 0 { len += 1; }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
        }
    };

    let mut buf = [0u8; 256];
    let n = sys_readdir(dir, &mut buf);
    if n < 0 {
        println!("ls: failed to read '{}'", dir);
        sys_exit();
    }
    sys_write(&buf[..n as usize]);
}

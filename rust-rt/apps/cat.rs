#![no_std]
#![no_main]

include!("../src/rt.rs");

fn cat(path: &str) {
    let fd = sys_open(path);
    if fd < 0 {
        println!("cat: failed to open '{}'", path);
        return;
    }

    let mut buf = [0u8; 512];
    loop {
        let n = sys_read(fd, &mut buf);
        if n <= 0 { break; }
        sys_write(&buf[..n as usize]);
    }
    sys_close(fd);
}

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, argv: *const *const u8) {
    if _argc < 2 {
        println!("Usage: cat <path>");
        sys_exit();
    }

    let path = unsafe {
        let ptr = *argv.add(1);
        let mut len = 0;
        while *ptr.add(len) != 0 { len += 1; }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    };

    cat(path);
}

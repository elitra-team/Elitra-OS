#![no_std]
#![no_main]

include!("../src/rt.rs");

fn cp(src: &str, dst: &str) -> isize {
    let fd_src = sys_open(src);
    if fd_src < 0 { return -1; }

    let mut st = FileStat {
        type_: 0,
        size: 0,
        name: [0u8; 64],
    };

    if sys_stat(src, &mut st) < 0 {
        sys_close(fd_src);
        return -1;
    }

    let fd_dst = sys_open_write(dst);
    if fd_dst < 0 {
        sys_close(fd_src);
        return -1;
    }

    let mut buf = [0u8; 4096];
    loop {
        let n = sys_read(fd_src, &mut buf);
        if n <= 0 { break; }
        let chunk = &buf[..n as usize];
        if sys_write_fd(fd_dst as i32, chunk) < 0 {
            sys_close(fd_src);
            sys_close(fd_dst);
            return -1;
        }
    }

    sys_close(fd_src);
    sys_close(fd_dst);
    0
}

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 3 {
        println!("Usage: cp <src> <dst>");
        sys_exit();
    }
    let src = unsafe { arg_at(argv, 1) };
    let dst = unsafe { arg_at(argv, 2) };
    if cp(src, dst) < 0 {
        println!("cp: failed to copy '{}' -> '{}'", src, dst);
    }
}

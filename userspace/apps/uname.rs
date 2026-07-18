#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) -> ! {
    // utsname layout: sysname(65) nodename(65) release(65) version(65) machine(65)
    let mut buf = [0u8; 325];
    let ticks = sys_system_info(&mut buf);
    // Print version (field 3: version)
    let version = &buf[65 * 3..];
    let mut len = 0;
    while len < 64 && version[len] != 0 { len += 1; }
    if len > 0 {
        sys_write(&version[..len]);
        sys_write(b"\n");
    }
    sys_write(b"Uptime: ");
    let mut tb = [0u8; 12];
    let mut i = tb.len();
    let mut t = ticks;
    if t == 0 {
        tb[0] = b'0';
        sys_write(&tb[..1]);
    } else {
        while t > 0 {
            i -= 1;
            tb[i] = b'0' + (t % 10) as u8;
            t /= 10;
        }
        sys_write(&tb[i..]);
    }
    sys_write(b" ticks (10ms each)\n");
    sys_exit();
}

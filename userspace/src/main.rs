#![no_std]
#![no_main]

include!("rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) {
    println!("Hello from Rust on Elitra-OS!");
    println!("syscalls: write, exit, sleep, yield, open, read, close, readdir");

    let n: u32 = 42;
    print!("The answer is ");
    print!("{}", n);
    print!(" (0x");
    print!("{:x}", n);
    println!(")");
}

use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Compile crt0.asm
    let status = std::process::Command::new("nasm")
        .args(["-f", "elf64", "-o", out_dir.join("crt0.o").to_str().unwrap(), "crt0.asm"])
        .status()
        .expect("nasm failed");
    assert!(status.success(), "nasm exit code: {}", status);

    // Link crt0.o
    println!("cargo::rustc-link-arg={}", out_dir.join("crt0.o").display());

    // Custom linker script
    println!("cargo::rustc-link-arg=-T{}", 
             std::env::current_dir().unwrap().join("linker.ld").display());

    // Linker flags
    println!("cargo::rustc-link-arg=-m");
    println!("cargo::rustc-link-arg=elf_x86_64");
    println!("cargo::rustc-link-arg=-nostdlib");

    // Ensure rebuild when asm or linker script changes
    println!("cargo::rerun-if-changed=crt0.asm");
    println!("cargo::rerun-if-changed=linker.ld");
}

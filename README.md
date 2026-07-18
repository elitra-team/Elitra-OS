# Elitra OS

64-bit hobby kernel for x86-64, written entirely in **Rust** (`#![no_std]`) with NASM assembly for boot, interrupts, context switch, and SMP trampoline. Userspace utilities are also written in Rust (custom rust-rt runtime, no libc).

## Features

### Kernel

- Boot via GRUB (Multiboot2) or directly QEMU (PVH ELF Note)
- **64-bit long mode** (x86-64), 4-level paging with 2MB huge pages
- NX (No-Execute) bit, SMEP/SMAP — hardware memory protection (conditional on CPUID)
- APIC/LAPIC/IOAPIC — advanced interrupt controller (8259 PIC disabled)
- **SMP** — trampoline (16→32→64 bit) for APs, INIT-SIPI-SIPI, per-CPU scheduler state, SpinLock
- Physical memory allocator (PMM bitmap) + heap allocator (Rust)
- **VMA subsystem** — virtual memory management (mmap, munmap, brk, mprotect)
- **Copy-on-Write** fork via reference counting table in Rust
- Interrupts (IDT, APIC/PIC) + Task State Segment (TSS)
- Syscall fast path via MSR (IA32_STAR/LSTAR/FMASK) + `int 0x80`
- FPU/SSE context save/restore (FXSAVE/FXRSTOR)
- System timers (PIT 100Hz, LAPIC timer, HPET)
- **79 syscalls** (see Syscalls section)
- File permission model (uid/gid/mode bits, check_permission)

### Drivers

- **APIC** — LAPIC (MMIO, SIVR, LVT, EOI, timer, IPI), IOAPIC (redirection entries), MADT parsing
- **AHCI** — SATA controller (DMA, PRD, bounce buffers, LBA48)
- **NVMe** — NVM Express controller (admin/I/O queues, identify, read/write sectors, PCI enumeration)
- **VirtIO Block** — MMIO transport, virtqueue management, feature negotiation
- **USB UHCI** (USB 1.x) + **EHCI** (USB 2.0) — full stack in Rust, HID keyboard/mouse
- **VGA text** (80×25) in Rust
- **Framebuffer** (VBE, configurable resolution) in Rust — put_pixel, fill_rect, draw_char
- **GUI Compositor** — windowing system with drag-and-drop, z-order, taskbar, event queue, mouse cursor
- **Terminal** in Rust — 4 virtual terminals (F1–F4), scrollback (PgUp/PgDn)
- PS/2 keyboard + mouse (mouse in Rust)
- CMOS RTC (real-time clock) in Rust
- PIT timer in Rust (100 Hz)
- ATA PIO + DMA (Bus Mastering), write-back cache for FAT32 in Rust
- Intel e1000 (network adapter — ARP/UDP/ICMP over Ethernet)
- **TCP/IP** — full TCP state machine (SYN/ACK/FIN/RST), flow control (send window, backpressure), retransmission timer with exponential backoff
- **DHCP** — automatic network configuration
- ACPI (RSDP/RSDT/FADT/DSDT/MADT) — shutdown, reboot, APIC discovery
- Serial port (NS16550, debug) in Rust
- PCI config space read/write + **PCI bus enumeration** (by vendor/device and class/subclass)

### Filesystems

- **FAT32** — read/write in Rust (boot sector, cluster chain, file CRUD, directory ops)
- **ext2** — read/write in Rust (superblock, block group descriptors, inode read/write)
- **VFS** — unified virtual filesystem (VNode tree, mount table, device nodes, fd table, file permissions)
- Pipes, output redirection, mount/unmount

### Multitasking

- Preemptive (PIT, 100 Hz), scheduler in Rust
- **SMP** — per-CPU scheduler state, SpinLock for ready/sleep queue, INIT-SIPI-SIPI AP startup
- FPU/SSE context save/restore
- ELF loader in Rust (32-bit and 64-bit static ELF)
- **User-mode processes** (ring 3), up to 64 tasks
- Signals (SIGKILL, SIGSEGV, SIGTERM, SIGCHLD)
- waitpid, process tree, zombie collection
- clone, thread_create/join/detach
- Pipe support with inter-process I/O

### Syscalls

79 system calls via `int 0x80` + MSR fast path:

| Category | Syscalls |
|---|---|
| File I/O | open, close, read, write, write_fd, lseek, stat, open_write, dup, dup2, fcntl |
| Directories | mkdir, rmdir, readdir, getdents, rename, unlink |
| Permissions | chmod, chown, fchmod, fchown, getuid, setuid, getgid, setgid, geteuid, getegid |
| Process | exit, yield, fork, execve, waitpid, getpid, getppid, kill, clone, system |
| Threads | thread_create, thread_join, thread_detach |
| Signals | sigaction, sigreturn |
| Memory | mmap, munmap, brk, mprotect |
| Pipes | pipe_create, pipe_read, pipe_write, pipe_close |
| Network | socket, connect, sendto, recvfrom, bind, close_socket, listen, accept |
| Time | clock_gettime (CLOCK_MONOTONIC, CLOCK_REALTIME), nanosleep, sleep, get_rtc |
| Terminal | ioctl (TIOCGWINSZ, FIONREAD, TIOCSCTTY), poll, select, getchar |
| System | uname, reboot, poweroff, chdir, getcwd, clear_screen, arch_prctl |

### Userspace

- Built-in shell (kernel shell) with 31 commands
- 24 ELF programs on Rust (init, cat, ls, echo, cp, mv, pwd, tee, sleep, seq, yes, clear, mkdir, rm, rmdir, touch, basename, dirname, which, true, false, env, uname)
- Pipe (`|`) and output redirection (`>`) support
- Init process (auto-starts on `/mnt/bin/init.elf`)

## Architecture

| Component | Language | Lines of code |
|---|---|---|
| Kernel (drivers, FS, scheduler, VMM, COW, networking, GUI, SMP, syscalls) | Rust | ~18 000+ |
| Userspace utilities (24 .elf) | Rust | ~2 000 |
| Boot, interrupts, context switch, trampoline | ASM (nasm) | ~700 |

```
                 .─────────.
                (  GRUB/PVH )
                 `────┬────'
                      │ boot.asm (32→64)
                      ▼
         ┌──────────────────────────────────┐
         │  Rust (krust staticlib)          │
         │  PMM, VGA, serial, terminal      │
         │  FAT32, ext2, USB (UHCI+EHCI)   │
         │  AHCI, VirtIO Block, NVMe        │
         │  COW, ELF loader, framebuffer    │
         │  ATA PIO/DMA, ns16550, RTC       │
         │  PIT timer, PS/2 mouse           │
         │  heap, paging, VMM, scheduler    │
         │  VFS, shell, syscalls, PCI       │
         │  APIC, e1000, TCP/IP, ACPI, HPET │
         │  SMP (SpinLock, trampoline)      │
         │  GUI compositor, socket API      │
         └────────┬─────────────────────────┘
                  │ int 0x80 / MSR syscall
                  ▼
         ┌──────────────────────────────┐
         │  rust-rt userspace .elf      │
         │  init, ls, cat, echo ...     │
         └──────────────────────────────┘
```

## Building and Running

### Quick start (PVH, TCG)
```
make -j$(nproc)
qemu-system-x86_64 -kernel elitra-kernel.bin -m 256M \
  -drive file=fat32.img,format=raw,index=0,media=disk \
  -machine accel=tcg -nographic -no-reboot -no-shutdown
```

### GRUB (ISO)
```
make -j$(nproc)
rm -rf iso && mkdir -p iso/boot/grub
cp elitra-kernel.bin iso/boot/kernel.elf
cat > iso/boot/grub/grub.cfg << 'GRUB'
set timeout=0
set default=0
menuentry "Elitra-OS" {
    multiboot2 /boot/kernel.elf
}
GRUB
grub-mkrescue -o elitra.iso iso
qemu-system-x86_64 -cdrom elitra.iso -m 256M \
  -drive file=fat32.img,format=raw,index=0,media=disk \
  -nographic -no-reboot -no-shutdown
```

### Dependencies
- `nasm`, `ld` (binutils)
- `rustc +nightly` (for krust and userspace programs)
- `cargo +nightly -Z build-std`
- `mtools` (mcopy, mmd — for fat32.img)
- `grub-mkrescue` (for ISO image)
- `qemu-system-x86_64`

## Shell Commands

| Command | Description |
|---|---|
| `?` / `help` | Show help |
| `clr` | Clear screen |
| `echo` / `say <text>` | Print text to terminal |
| `list [path]` | List directory contents |
| `dump <file>` | Display file contents |
| `create <path>` | Create empty file |
| `del <path>` | Delete file or node |
| `md <path>` | Create directory |
| `put <path> <text>` | Write text to file |
| `exec <file>` | Execute ELF binary |
| `cd [path]` | Change directory |
| `ps` | Show process list |
| `kill <pid>` | Kill process by PID |
| `df` | Show disk usage |
| `date` | Show current date |
| `mem` | Show memory information |
| `cpu` | Show CPU information |
| `upt` | Show uptime |
| `ver` | Show version |
| `rst` | Reboot |
| `off` | Shut down |
| `mnt` | List mounted filesystems |
| `unm <path>` | Unmount filesystem |
| `sync` | Flush to disk |
| `ata` | ATA drive information |
| `fs` | VFS information |
| `jobs` | Task information |
| `newt` | Create test tasks |
| `mall` | Test allocator |
| `pt` | Test paging |

Terminal: **F1–F4** — virtual terminals, **PgUp/PgDn** — scrollback.

Pipes (`|`) and output redirection (`>`) are supported.

## Changelog

See [CHANGELOG.md](CHANGELOG.md)

## Roadmap

- **SMP scheduler** — per-CPU current task exists, but inter-CPU rescheduling via IPI is not integrated into context switch
- **Sound** — no driver (HDA/AC97)
- **Dynamic linker** — only static ELF
- **shared memory** — no mmap MAP_SHARED
- **futex** — not implemented
- **IRQ-driven e1000** — polling only
- **NVMe VFS integration** — driver works but not integrated as block device in VFS
- **epoll** — poll exists, no epoll for scalability

## License

GNU General Public License v3.0 — see [LICENSE](LICENSE)

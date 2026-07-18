# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

#### 15 new syscalls (64–78) — ioctl, poll, time, permissions, directory listing
- **ioctl** (64) — device I/O control:
  - TIOCGWINSZ (0x5413) — get terminal window size (cols, rows, pixel dimensions)
  - TIOCSWINSZ (0x5414) — set terminal window size (no-op placeholder)
  - FIONREAD (0x541B) — query bytes available for reading (VFS files + network sockets)
  - TIOCSCTTY (0x540E) / TIOCNOTTY (0x5422) — controlling terminal (placeholder)
- **poll** (65) — multiplexed I/O on file descriptors:
  - Reads `pollfd` array from userspace, checks readability (VFS offset < size, or `net_socket_has_data`)
  - POLLOUT always returns ready
  - Busy-wait with PIT-tick-based timeout, yields between iterations
  - Writes back `revents` to userspace
- **clock_gettime** (66):
  - CLOCK_REALTIME (0) — reads CMOS RTC epoch seconds
  - CLOCK_MONOTONIC (1) — PIT tick count converted to seconds+nanoseconds
  - CLOCK_MONOTONIC_RAW (4) — same as MONOTONIC (no frequency correction)
- **nanosleep** (67):
  - Converts `timespec` (sec + nsec) to PIT ticks; uses `krust_sched_sleep_ticks` for coarse sleep
  - Sub-tick durations use `krust_hpet_busy_wait_ns`
  - Writes zero to `rem` (no interrupted-sleep tracking)
- **uid/gid syscalls** (68–73):
  - `getuid` (68), `setuid` (69), `getgid` (70), `setgid` (71)
  - `geteuid` (72), `getegid` (73) — effective IDs (currently same as real)
- **Permission syscalls** (74–77):
  - `fchmod` (74) — change mode bits by fd
  - `fchown` (77) — change owner uid/gid by fd
  - `chmod` (76) — change mode bits by path
  - `chown` (77) — change owner uid/gid by path
- **getdents** (78) — Linux `getdents64`-style directory listing from fd:
  - Fills buffer with `linux_dirent64` structs (ino, offset, reclen, type, name)
  - Tracks directory read offset across calls

#### TCP retransmission timer
- `TcpConnection::check_retransmit()` — checks if `send_unack` data is unACKed beyond `retransmit_timeout`
- Exponential backoff: timeout doubles on each retry, capped at 30 seconds
- Max 5 retries before connection is closed
- Sends bare ACK to solicit re-ACK from peer (sufficient for recovery)
- Called from `krust_net_poll()` after packet receive loop

#### Userspace syscall wrappers
- `sys_ioctl`, `sys_ioctl_winsize`, `sys_poll`, `sys_clock_gettime`, `sys_nanosleep`
- `sys_getuid`, `sys_setuid`, `sys_getgid`, `sys_setgid`, `sys_geteuid`, `sys_getegid`
- `sys_fchmod`, `sys_fchown`, `sys_chmod`, `sys_chown`, `sys_getdents`
- Structs: `Winsize`, `PollFd`, `Timespec`, `LinuxDirent64`
- Constants: `POLLIN`, `POLLOUT`, `POLLERR`, `POLLHUP`, `POLLNVAL`, `CLOCK_REALTIME`, `CLOCK_MONOTONIC`

### Fixed
- **TCP `process_segment` window_size bug** — `header.window_size` referenced out-of-scope `header` variable; added `window_size: u16` parameter to `process_segment()`, updated both call sites in `net/mod.rs`
- **DNS `TXID` race** — `static mut u16` → `AtomicU16` (was UB under preemption)
- **E1000 TX busy-wait** — early `break` on DD status bit instead of spinning for 10000 iterations
- **IPv4 checksum missing validation** — `parse_ipv4_packet` now returns `None` on checksum mismatch
- **TCP checksum double-swap** — removed redundant `.to_be()` in `net/tcp.rs:575`
- **IPv4 checksum double-swap** — removed redundant `.to_be()` in `net/ipv4.rs:107`
- **DNS busy-wait** — bounded with `DNS_TIMEOUT_RETRIES=200`, `spin_loop()`, early return
- **ELF loader memory leaks** — added `elf_cleanup_pages()` helper, cleanup on every error path
- **Guard pages for user stacks** — all 3 stack allocators skip mapping page 0; stack overflow now triggers SIGBUS
- **`static mut` in net/ (UB)** — all 11 `static mut` variables → `SpinLock<>` or `AtomicU16`
- **`static mut` in gui (UB)** — mouse state → `SpinLock<(i32,i32,bool,u32,i32,i32)>`
- **ARP cache overflow** — 16 → 64 entries
- **Duplicate CPUID in shell** — replaced 30-line inline impl with calls to `cpuid.rs` module

### Changed
- Syscall number limit raised from 63 to 79
- README rewritten with accurate syscall table, shell commands, and architecture overview

## [Previous] - до этой сессии

- Миграция rust-rt/krust на `x86_64-unknown-none` (встроенный target Rust)
- Добавлены секции `.init_array`, `.fini_array`, `.got`, `.got.plt`, `.eh_frame` в linker.ld
- Исправлен spinlock (volatile + `__atomic_test_and_set` + `pause`)
- Добавлена доставка сигналов в syscall и yield
- Исправлены утечки памяти в ext2 init
- Исправлен баг загрузки 64-bit: inverted page table identity mapping
- Добавлена ELF-нота PVH (`.note.Xen`) для прямой загрузки через QEMU
- Добавлен Multiboot2 header для загрузки через GRUB
- Serial debug output на COM1 в boot.asm
- Отключены прерывания e1000 (interrupt storm без хендлера)
- Базовая файловая система (FAT32 R/W, EXT2 R/O), VFS, pipes
- Базовая мультизадачность (RR scheduler, fork, execve, waitpid, signals)
- Базовые драйверы (PS/2, ATA PIO, e1000, framebuffer, terminal)
- Физический аллокатор (PMM bitmap), кучевой аллокатор, 4-level paging

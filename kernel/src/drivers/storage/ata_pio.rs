use core::arch::asm;
use core::ptr;

const PRIMARY_BASE: u16 = 0x1F0;
const SECONDARY_BASE: u16 = 0x170;
const MAX_DRIVES: usize = 4;
const PRDT_MAX_ENTRIES: u32 = 256;

#[derive(Clone, Copy)]
#[repr(C)]
struct Drive {
    present: bool,
    ident: [u16; 256],
    total_sectors: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Partition {
    pub valid: bool,
    pub type_: u8,
    pub lba_start: u32,
    pub sector_count: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct DmaState {
    valid: bool,
    bmide_base: u32,
    prdt_phys: u64,
    prdt: *mut u16,
}

static mut DRIVES: [Drive; MAX_DRIVES] = [Drive { present: false, ident: [0u16; 256], total_sectors: 0 }; 4];
static mut DRIVE_COUNT: i32 = 0;

static mut PART_BUFFER: *mut u8 = ptr::null_mut();
static mut PART_LBA_START: u32 = 0;
static mut PART_SECTORS: u32 = 0;
static mut PART_DRIVE: i32 = -1;
static mut DIRTY_BITS: *mut u8 = ptr::null_mut();
static mut DIRTY_SIZE: u32 = 0;

static mut DMA_STATE: DmaState = DmaState {
    valid: false,
    bmide_base: 0,
    prdt_phys: 0,
    prdt: ptr::null_mut(),
};

extern "C" {
    fn krust_malloc(size: u32) -> *mut u8;
    fn krust_pmm_alloc_frame() -> usize;
    fn krust_paging_get_phys(virt: u64) -> u64;
    fn krust_ns16550_write_str(s: *const u8);
}

unsafe fn outb(port: u16, val: u8) {
    asm!("out dx, al", in("dx") port, in("al") val);
}

unsafe fn outw(port: u16, val: u16) {
    asm!("out dx, ax", in("dx") port, in("ax") val);
}

unsafe fn outl(port: u16, val: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") val);
}

unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    asm!("in al, dx", out("al") val, in("dx") port);
    val
}

unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    asm!("in ax, dx", out("ax") val, in("dx") port);
    val
}

unsafe fn io_wait() {
    inb(0x80);
    inb(0x80);
    inb(0x80);
}

unsafe fn base_port(drive: i32) -> u16 {
    if drive < 2 { PRIMARY_BASE } else { SECONDARY_BASE }
}

unsafe fn slave_bit(drive: i32) -> u8 {
    if drive == 0 || drive == 2 { 0 } else { 1 }
}

unsafe fn read_reg(drive: i32, reg: u8) -> u8 {
    inb(base_port(drive) + reg as u16)
}

unsafe fn poll_busy(drive: i32, timeout_ms: i32) -> bool {
    for _ in 0..timeout_ms * 10 {
        io_wait();
        if read_reg(drive, 7) & 0x80 == 0 {
            return true;
        }
    }
    false
}

unsafe fn poll_drq(drive: i32, timeout_ms: i32) -> bool {
    for _ in 0..timeout_ms * 10 {
        io_wait();
        let st = read_reg(drive, 7);
        if st & 0x01 != 0 { return false; }
        if st & 0x08 != 0 { return true; }
        if st & 0x80 == 0 { return false; }
    }
    false
}

unsafe fn wait_ready(drive: i32, timeout_ms: i32) -> bool {
    poll_busy(drive, timeout_ms)
}

unsafe fn ns16550_write(s: &[u8]) {
    krust_ns16550_write_str(s.as_ptr());
}

unsafe fn print_dec(val: u32) -> [u8; 12] {
    let mut buf = [0u8; 12];
    let mut i: usize = 11;
    let mut v = val;
    loop {
        i = i.wrapping_sub(1);
        buf[i] = (v % 10) as u8 + b'0';
        v /= 10;
        if v == 0 { break; }
    }
    let mut out = [0u8; 12];
    let len = 11 - i;
    let mut j: usize = 0;
    while j < len {
        out[j] = buf[i + j];
        j += 1;
    }
    out[len] = b'\0';
    out
}

unsafe fn print_info(drive: i32) {
    let ident = &DRIVES[drive as usize].ident;
    let channel: &[u8] = if drive % 2 == 0 { b"primary\0" } else { b"secondary\0" };
    let role: &[u8] = if drive < 2 { b"master\0" } else { b"slave\0" };

    let mut model = [0u8; 41];
    for i in 0..20 {
        let w = ident[27 + i];
        model[i * 2] = (w >> 8) as u8;
        model[i * 2 + 1] = (w & 0xFF) as u8;
    }
    let mut mi = 39;
    while mi > 0 && model[mi] == b' ' {
        model[mi] = 0;
        mi -= 1;
    }
    model[40] = 0;

    let sectors = ident[60] as u32 | (ident[61] as u32) << 16;
    let size_mb = (sectors / 2) / 1024;

    ns16550_write(b"ata");
    ns16550_write(&[b'0' + drive as u8, b':', b' ']);
    ns16550_write(channel);
    ns16550_write(b"/");
    ns16550_write(role);
    ns16550_write(b" ");
    ns16550_write(&model);
    ns16550_write(b" (");
    let sz = print_dec(size_mb);
    ns16550_write(&sz);
    ns16550_write(b" MB, ");
    let sc = print_dec(sectors);
    ns16550_write(&sc);
    ns16550_write(b" sectors)\n\0");
}

// --- Public API ---

#[no_mangle]
pub unsafe extern "C" fn krust_ata_identify(drive: i32, buf: *mut u16) -> bool {
    if drive < 0 || drive >= MAX_DRIVES as i32 { return false; }

    let base = base_port(drive);
    let slave = slave_bit(drive);

    outb(base + 6, 0xA0 | (slave << 4));
    io_wait();

    outb(base + 2, 0);
    io_wait();
    outb(base + 3, 0);
    io_wait();
    outb(base + 4, 0);
    io_wait();
    outb(base + 5, 0);
    io_wait();

    outb(base + 7, 0xEC);
    io_wait();

    let st = inb(base + 7);
    if st == 0 { return false; }

    if !poll_busy(drive, 100) { return false; }

    let mid = inb(base + 4);
    let hi = inb(base + 5);
    if mid == 0x14 && hi == 0xEB { return false; }
    if mid == 0x69 && hi == 0x96 { return false; }

    if !poll_drq(drive, 100) { return false; }

    for i in 0..256 {
        ptr::write(buf.add(i), inw(base));
        io_wait();
    }

    true
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_read(drive: i32, lba: u32, count_: u8, buf: *mut u8) -> bool {
    if drive < 0 || drive >= MAX_DRIVES as i32 || buf.is_null() { return false; }
    let count = if count_ == 0 { 256 } else { count_ as u32 };

    let base = base_port(drive);
    let slave = slave_bit(drive);

    if !wait_ready(drive, 1000) { return false; }

    let dh: u8 = 0xE0 | (slave << 4) | ((lba >> 24) as u8 & 0x0F);
    outb(base + 6, dh);
    io_wait();

    outb(base + 2, count_);
    io_wait();
    outb(base + 3, lba as u8);
    io_wait();
    outb(base + 4, (lba >> 8) as u8);
    io_wait();
    outb(base + 5, (lba >> 16) as u8);
    io_wait();

    outb(base + 7, 0x20);
    io_wait();

    let ptr_buf = buf as *mut u16;
    for s in 0..count {
        if !poll_busy(drive, 1000) { return false; }
        if !poll_drq(drive, 1000) { return false; }

        for i in 0..256 {
            ptr::write(ptr_buf.add((s * 256 + i) as usize), inw(base));
            io_wait();
        }
    }

    true
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_write(drive: i32, lba: u32, count_: u8, buf: *const u8) -> bool {
    if drive < 0 || drive >= MAX_DRIVES as i32 || buf.is_null() { return false; }
    let count = if count_ == 0 { 256 } else { count_ as u32 };

    let base = base_port(drive);
    let slave = slave_bit(drive);

    if !wait_ready(drive, 1000) { return false; }

    let dh: u8 = 0xE0 | (slave << 4) | ((lba >> 24) as u8 & 0x0F);
    outb(base + 6, dh);
    io_wait();

    outb(base + 2, count_);
    io_wait();
    outb(base + 3, lba as u8);
    io_wait();
    outb(base + 4, (lba >> 8) as u8);
    io_wait();
    outb(base + 5, (lba >> 16) as u8);
    io_wait();

    outb(base + 7, 0x30);
    io_wait();

    let ptr_buf = buf as *const u16;
    for s in 0..count {
        if !poll_busy(drive, 1000) { return false; }
        if !poll_drq(drive, 1000) { return false; }

        for i in 0..256 {
            outw(base, ptr::read(ptr_buf.add((s * 256 + i) as usize)));
            io_wait();
        }
    }

    outb(base + 7, 0xE7);
    io_wait();
    wait_ready(drive, 1000);

    true
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_init() {
    DRIVE_COUNT = 0;
    ptr::write_bytes(DRIVES.as_mut_ptr(), 0, 1);

    for d in 0..MAX_DRIVES as i32 {
        let mut ident: [u16; 256] = [0; 256];
        if krust_ata_identify(d, ident.as_mut_ptr()) {
            DRIVES[d as usize].present = true;
            ptr::copy_nonoverlapping(ident.as_ptr(), DRIVES[d as usize].ident.as_mut_ptr(), 256);
            DRIVES[d as usize].total_sectors = (ident[60] as u32) | ((ident[61] as u32) << 16);
            DRIVE_COUNT += 1;
        }
    }

    ns16550_write(b"ata: ");
    let dc = print_dec(DRIVE_COUNT as u32);
    ns16550_write(&dc);
    ns16550_write(b" drive(s) detected\n\0");
    for d in 0..DRIVE_COUNT {
        print_info(d);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_drive_count() -> i32 {
    DRIVE_COUNT
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_present(drive: i32) -> bool {
    if drive < 0 || drive >= MAX_DRIVES as i32 { return false; }
    DRIVES[drive as usize].present
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_get_total_sectors(drive: i32) -> u32 {
    if drive < 0 || drive >= MAX_DRIVES as i32 { return 0; }
    DRIVES[drive as usize].total_sectors
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_find_partitions(
    drive: i32,
    parts: *mut Partition,
    max_parts: i32,
) -> i32 {
    if !krust_ata_present(drive) || parts.is_null() || max_parts < 1 { return 0; }

    let mut mbr: [u8; 512] = [0; 512];
    if !krust_ata_read(drive, 0, 1, mbr.as_mut_ptr()) { return 0; }

    if mbr[510] != 0x55 || mbr[511] != 0xAA { return 0; }

    let mut found = 0i32;
    for i in 0..4 {
        if found >= max_parts { break; }
        let entry = &mbr[0x1BE + i * 16..];
        let type_ = entry[4];
        if type_ == 0x0B || type_ == 0x0C {
            let p = &mut *parts.add(found as usize);
            p.valid = true;
            p.type_ = type_;
            p.lba_start = u32::from_le_bytes([
                entry[8], entry[9], entry[10], entry[11],
            ]);
            p.sector_count = u32::from_le_bytes([
                entry[12], entry[13], entry[14], entry[15],
            ]);
            found += 1;
        }
    }

    found
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_mount_partition_buffer(
    drive: i32,
    lba_start: u32,
    sectors: u32,
    buffer: *mut u8,
) -> bool {
    if buffer.is_null() || sectors == 0 { return false; }
    PART_DRIVE = drive;
    PART_LBA_START = lba_start;
    PART_SECTORS = sectors;
    PART_BUFFER = buffer;

    DIRTY_SIZE = (sectors + 7) / 8;
    DIRTY_BITS = krust_malloc(DIRTY_SIZE);
    if DIRTY_BITS.is_null() {
        PART_BUFFER = ptr::null_mut();
        return false;
    }
    ptr::write_bytes(DIRTY_BITS, 0, DIRTY_SIZE as usize);
    true
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_mark_dirty(byte_offset: u32, size: u32) {
    if PART_BUFFER.is_null() || DIRTY_BITS.is_null() { return; }

    let start = byte_offset / 512;
    let mut end = (byte_offset + size + 511) / 512;
    if end > PART_SECTORS { end = PART_SECTORS; }

    for s in start..end {
        let idx = (s / 8) as usize;
        let bit = 1 << (s % 8);
        ptr::write(DIRTY_BITS.add(idx), ptr::read(DIRTY_BITS.add(idx)) | bit);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_flush() {
    if PART_BUFFER.is_null() || PART_DRIVE < 0 || DIRTY_BITS.is_null() { return; }

    let mut flushed = 0;
    for s in 0..PART_SECTORS {
        let idx = (s / 8) as usize;
        let bit = 1 << (s % 8);
        if ptr::read(DIRTY_BITS.add(idx)) & bit != 0 {
            if krust_ata_write(PART_DRIVE, PART_LBA_START + s, 1,
                PART_BUFFER.add((s * 512) as usize))
            {
                ptr::write(DIRTY_BITS.add(idx), ptr::read(DIRTY_BITS.add(idx)) & !bit);
                flushed += 1;
            }
        }
    }

    if flushed > 0 {
        ns16550_write(b"ata: flushed ");
        let f = print_dec(flushed as u32);
        ns16550_write(&f);
        ns16550_write(b" sectors\n\0");
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_print_info(drive: i32) {
    if !krust_ata_present(drive) {
        ns16550_write(b"ata");
        ns16550_write(&[b'0' + drive as u8]);
        ns16550_write(b": not present\n\0");
        return;
    }
    print_info(drive);
}

// --- DMA support ---

unsafe fn init_bmide() -> bool {
    if DMA_STATE.valid { return true; }

    for bus in 0u32..256 {
        for slot in 0u32..32 {
            for func in 0u32..8 {
                let vendor = krust_pci_read_word(bus as u8, slot as u8, func as u8, 0);
                if vendor == 0xFFFF {
                    if func == 0 { break; }
                    continue;
                }

                let class_rev = krust_pci_read_dword(bus as u8, slot as u8, func as u8, 0x08);
                let class_code = ((class_rev >> 24) & 0xFF) as u8;
                let subclass = ((class_rev >> 16) & 0xFF) as u8;

                if class_code == 0x01 && subclass == 0x01 {
                    let bar4 = krust_pci_read_dword(bus as u8, slot as u8, func as u8, 0x20);
                    if bar4 & 0x01 != 0 {
                        let bmide_base = bar4 & 0xFFF0;
                        if bmide_base == 0 { continue; }

                        let cmd = krust_pci_read_word(bus as u8, slot as u8, func as u8, 0x04);
                        krust_pci_write_word(bus as u8, slot as u8, func as u8, 0x04, cmd | 0x04);

                        let prdt_virt = krust_pmm_alloc_frame();
                        if prdt_virt == 0 { return false; }
                        ptr::write_bytes(prdt_virt as *mut u8, 0, 4096);
                        let prdt_phys = krust_paging_get_phys(prdt_virt as u64);

                        DMA_STATE.valid = true;
                        DMA_STATE.bmide_base = bmide_base;
                        DMA_STATE.prdt_phys = prdt_phys;
                        DMA_STATE.prdt = prdt_virt as *mut u16;

                        ns16550_write(b"ata: BMIDE at ");
                        // hex print
                        ns16550_write(b"0x");
                        // simple hex for bmide_base
                        let hb = print_hex(bmide_base as u64);
                        ns16550_write(&hb);
                        ns16550_write(b", PRDT at 0x");
                        let hp = print_hex(prdt_phys);
                        ns16550_write(&hp);
                        ns16550_write(b"\n\0");

                        return true;
                    }
                }

                if func == 0 { break; }
            }
        }
    }

    ns16550_write(b"ata: no IDE controller found for DMA\n\0");
    false
}

unsafe fn print_hex(val: u64) -> [u8; 19] {
    let hex_chars = b"0123456789ABCDEF";
    let mut buf = [0u8; 19];
    let mut i: usize = 18;
    let mut v = val;
    loop {
        i = i.wrapping_sub(1);
        buf[i] = hex_chars[(v & 0xF) as usize];
        v >>= 4;
        if v == 0 { break; }
    }
    let mut out = [0u8; 19];
    let len = 18 - i;
    let mut j: usize = 0;
    while j < len {
        out[j] = buf[i + j];
        j += 1;
    }
    out[len] = b'\0';
    out
}

extern "C" {
    fn krust_pci_read_word(bus: u8, slot: u8, func: u8, offset: u8) -> u16;
    fn krust_pci_read_dword(bus: u8, slot: u8, func: u8, offset: u8) -> u32;
    fn krust_pci_write_word(bus: u8, slot: u8, func: u8, offset: u8, value: u16);
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_dma_available(_drive: i32) -> bool {
    DMA_STATE.valid
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_dma_read(
    drive: i32,
    lba: u32,
    count_: u8,
    buf: *mut u8,
) -> bool {
    if drive < 0 || drive >= MAX_DRIVES as i32 || buf.is_null() { return false; }
    let mut count = if count_ == 0 { 256 } else { count_ as u32 };
    if count == 0 { return false; }

    if !init_bmide() { return false; }

    let base = base_port(drive);
    let slave = slave_bit(drive);

    if count > PRDT_MAX_ENTRIES { count = PRDT_MAX_ENTRIES; }

    let buf_base = krust_paging_get_phys(buf as u64);
    let mut entries = 0u32;

    for s in 0..count {
        let off = (entries * 8) as usize;
        let entry_buf_phys = buf_base + (s as u64) * 512;
        ptr::write(DMA_STATE.prdt.add(off / 2), entry_buf_phys as u16);
        ptr::write(DMA_STATE.prdt.add(off / 2 + 1), (entry_buf_phys >> 16) as u16);
        ptr::write(DMA_STATE.prdt.add(off / 2 + 2), 0x1FF | 0x8000);
        ptr::write(DMA_STATE.prdt.add(off / 2 + 3), 0);
        if entries > 0 {
            let prev = ptr::read(DMA_STATE.prdt.add(off / 2 - 2));
            ptr::write(DMA_STATE.prdt.add(off / 2 - 2), prev & !0x8000);
        }
        entries += 1;
    }

    outb(DMA_STATE.bmide_base as u16, 0x00);
    io_wait();

    outl(DMA_STATE.bmide_base as u16 + 4, DMA_STATE.prdt_phys as u32);
    io_wait();

    outb(DMA_STATE.bmide_base as u16 + 2, 0x04);
    io_wait();

    if !wait_ready(drive, 1000) { return false; }

    let dh: u8 = 0xE0 | (slave << 4) | ((lba >> 24) as u8 & 0x0F);
    outb(base + 6, dh);
    io_wait();

    outb(base + 2, count_);
    io_wait();
    outb(base + 3, lba as u8);
    io_wait();
    outb(base + 4, (lba >> 8) as u8);
    io_wait();
    outb(base + 5, (lba >> 16) as u8);
    io_wait();

    outb(base + 7, 0xC8);
    io_wait();

    outb(DMA_STATE.bmide_base as u16, 0x09);
    io_wait();

    for _ in 0..50000 {
        io_wait();
        let bmstat = inb(DMA_STATE.bmide_base as u16 + 2);
        let ata_st = inb(base + 7);
        if bmstat & 0x01 == 0 {
            outb(DMA_STATE.bmide_base as u16, 0x00);
            outb(DMA_STATE.bmide_base as u16 + 2, 0x04);
            if bmstat & 0x02 != 0 {
                let msg = b"ata: DMA read error on drive ";
                ns16550_write(msg);
                ns16550_write(&[b'0' + drive as u8]);
                ns16550_write(b", LBA ");
                let l = print_dec(lba);
                ns16550_write(&l);
                ns16550_write(b"\n\0");
                return false;
            }
            if ata_st & 0x01 != 0 {
                ns16550_write(b"ata: ATA error on drive ");
                ns16550_write(&[b'0' + drive as u8]);
                ns16550_write(b", LBA ");
                let l = print_dec(lba);
                ns16550_write(&l);
                ns16550_write(b", status=0x");
                let hs = print_hex(ata_st as u64);
                ns16550_write(&hs);
                ns16550_write(b"\n\0");
                return false;
            }
            return true;
        }
        if ata_st & 0x01 != 0 {
            outb(DMA_STATE.bmide_base as u16, 0x00);
            outb(DMA_STATE.bmide_base as u16 + 2, 0x04);
            ns16550_write(b"ata: DMA read ATA error drive ");
            ns16550_write(&[b'0' + drive as u8]);
            ns16550_write(b" LBA ");
            let l = print_dec(lba);
            ns16550_write(&l);
            ns16550_write(b" status=0x");
            let hs = print_hex(ata_st as u64);
            ns16550_write(&hs);
            ns16550_write(b"\n\0");
            return false;
        }
    }

    outb(DMA_STATE.bmide_base as u16, 0x00);
    outb(DMA_STATE.bmide_base as u16 + 2, 0x04);
    ns16550_write(b"ata: DMA read timeout on drive ");
    ns16550_write(&[b'0' + drive as u8]);
    ns16550_write(b", LBA ");
    let l = print_dec(lba);
    ns16550_write(&l);
    ns16550_write(b"\n\0");
    false
}

#[no_mangle]
pub unsafe extern "C" fn krust_ata_dma_write(
    drive: i32,
    lba: u32,
    count_: u8,
    buf: *const u8,
) -> bool {
    if drive < 0 || drive >= MAX_DRIVES as i32 || buf.is_null() { return false; }
    let mut count = if count_ == 0 { 256 } else { count_ as u32 };
    if count == 0 { return false; }

    if !init_bmide() { return false; }

    let base = base_port(drive);
    let slave = slave_bit(drive);

    if count > PRDT_MAX_ENTRIES { count = PRDT_MAX_ENTRIES; }

    let buf_base = krust_paging_get_phys(buf as u64);
    let mut entries = 0u32;

    for s in 0..count {
        let off = (entries * 8) as usize;
        let entry_buf_phys = buf_base + (s as u64) * 512;
        ptr::write(DMA_STATE.prdt.add(off / 2), entry_buf_phys as u16);
        ptr::write(DMA_STATE.prdt.add(off / 2 + 1), (entry_buf_phys >> 16) as u16);
        ptr::write(DMA_STATE.prdt.add(off / 2 + 2), 0x1FF | 0x8000);
        ptr::write(DMA_STATE.prdt.add(off / 2 + 3), 0);
        if entries > 0 {
            let prev = ptr::read(DMA_STATE.prdt.add(off / 2 - 2));
            ptr::write(DMA_STATE.prdt.add(off / 2 - 2), prev & !0x8000);
        }
        entries += 1;
    }

    outb(DMA_STATE.bmide_base as u16, 0x00);
    io_wait();

    outl(DMA_STATE.bmide_base as u16 + 4, DMA_STATE.prdt_phys as u32);
    io_wait();

    outb(DMA_STATE.bmide_base as u16 + 2, 0x04);
    io_wait();

    if !wait_ready(drive, 1000) { return false; }

    let dh: u8 = 0xE0 | (slave << 4) | ((lba >> 24) as u8 & 0x0F);
    outb(base + 6, dh);
    io_wait();

    outb(base + 2, count_);
    io_wait();
    outb(base + 3, lba as u8);
    io_wait();
    outb(base + 4, (lba >> 8) as u8);
    io_wait();
    outb(base + 5, (lba >> 16) as u8);
    io_wait();

    outb(base + 7, 0xCA);
    io_wait();

    outb(DMA_STATE.bmide_base as u16, 0x01);
    io_wait();

    for _ in 0..50000 {
        io_wait();
        let bmstat = inb(DMA_STATE.bmide_base as u16 + 2);
        let ata_st = inb(base + 7);
        if bmstat & 0x01 == 0 {
            outb(DMA_STATE.bmide_base as u16, 0x00);
            outb(DMA_STATE.bmide_base as u16 + 2, 0x04);
            if bmstat & 0x02 != 0 {
                ns16550_write(b"ata: DMA write error on drive ");
                ns16550_write(&[b'0' + drive as u8]);
                ns16550_write(b", LBA ");
                let l = print_dec(lba);
                ns16550_write(&l);
                ns16550_write(b"\n\0");
                return false;
            }
            if ata_st & 0x01 != 0 {
                ns16550_write(b"ata: ATA write error on drive ");
                ns16550_write(&[b'0' + drive as u8]);
                ns16550_write(b", LBA ");
                let l = print_dec(lba);
                ns16550_write(&l);
                ns16550_write(b", status=0x");
                let hs = print_hex(ata_st as u64);
                ns16550_write(&hs);
                ns16550_write(b"\n\0");
                return false;
            }
            return true;
        }
        if ata_st & 0x01 != 0 {
            outb(DMA_STATE.bmide_base as u16, 0x00);
            outb(DMA_STATE.bmide_base as u16 + 2, 0x04);
            ns16550_write(b"ata: DMA write ATA error drive ");
            ns16550_write(&[b'0' + drive as u8]);
            ns16550_write(b" LBA ");
            let l = print_dec(lba);
            ns16550_write(&l);
            ns16550_write(b" status=0x");
            let hs = print_hex(ata_st as u64);
            ns16550_write(&hs);
            ns16550_write(b"\n\0");
            return false;
        }
    }

    outb(DMA_STATE.bmide_base as u16, 0x00);
    outb(DMA_STATE.bmide_base as u16 + 2, 0x04);
    ns16550_write(b"ata: DMA write timeout on drive ");
    ns16550_write(&[b'0' + drive as u8]);
    ns16550_write(b", LBA ");
    let l = print_dec(lba);
    ns16550_write(&l);
    ns16550_write(b"\n\0");
    false
}

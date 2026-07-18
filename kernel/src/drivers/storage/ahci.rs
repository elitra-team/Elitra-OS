use core::ptr;

const MAX_PORTS: usize = 32;
const MAX_DRIVES: usize = 8;

// HBA registers
const CAP: u64 = 0x00;
const GHC: u64 = 0x04;
const IS: u64 = 0x08;
const PI: u64 = 0x0C;
const VS: u64 = 0x10;

// GHC bits
const GHC_AE: u32 = 0x80000000;
const GHC_IE: u32 = 0x00000002;
const GHC_HR: u32 = 0x00000001;

// Port registers (each port occupies 0x80 bytes, starting at 0x100)
const PORT_BASE: u64 = 0x100;
const PORT_SIZE: u64 = 0x80;
const PxCLB: u64 = 0x00;
const PxCLBU: u64 = 0x04;
const PxFB: u64 = 0x08;
const PxFBU: u64 = 0x0C;
const PxIS: u64 = 0x10;
const PxIE: u64 = 0x14;
const PxCMD: u64 = 0x18;
const PxTFD: u64 = 0x20;
const PxSIG: u64 = 0x24;
const PxSSTS: u64 = 0x28;
const PxSCTL: u64 = 0x2C;
const PxSERR: u64 = 0x30;
const PxCI: u64 = 0x38;

// PxCMD bits
const CMD_ST: u32 = 0x0001;
const CMD_SUD: u32 = 0x0002;
const CMD_POD: u32 = 0x0004;
const CMD_FRE: u32 = 0x0010;
const CMD_CPD: u32 = 0x0020;
const CMD_CR: u32 = 0x8000;
const CMD_FR: u32 = 0x4000;
const CMD_ICC: u32 = 0x0F000000;

// PxSSTS bits
const SSTS_DET: u32 = 0x0000000F;
const DET_PRESENT: u32 = 0x00000003;
const DET_ACTIVE: u32 = 0x00000007;

// PxSIG masks
const SIG_ATA: u32 = 0x00000101;
const SIG_ATAPI: u32 = 0xEB140101;

// PxTFD bits
const TFD_BSY: u32 = 0x00000080;
const TFD_DRQ: u32 = 0x00000008;

// ATA commands
const CMD_IDENTIFY: u8 = 0xEC;
const CMD_READ_DMA_EXT: u8 = 0x25;
const CMD_WRITE_DMA_EXT: u8 = 0x35;

#[repr(C, align(1024))]
struct CmdList([CmdEntry; 32]);

#[repr(C)]
struct CmdEntry {
    dw0: u32,
    prdbc: u32,
    ctba: u32,
    ctbau: u32,
    _rsvd: [u32; 4],
}

#[repr(C, align(128))]
struct CmdTable {
    fis: [u8; 64],
    prdt: [PrdtEntry; 16],
    _pad: [u8; 16],
}

#[repr(C)]
struct PrdtEntry {
    dba: u32,
    dbau: u32,
    _rsvd: u32,
    dbc: u32,
}

#[repr(C, align(256))]
struct FisRecv {
    fis: [u8; 256],
}

pub struct Drive {
    port: u8,
    present: bool,
    lba48: bool,
    sectors: u64,
    model: [u8; 40],
    serial: [u8; 20],
}

static mut HBA: u64 = 0;
static mut DRIVES: [Drive; MAX_DRIVES] = unsafe { core::mem::zeroed() };
static mut NDRIVE: usize = 0;
static mut CMD_PAGES: [u32; MAX_PORTS] = [0; MAX_PORTS];
static mut FIS_PAGES: [u32; MAX_PORTS] = [0; MAX_PORTS];
static mut CTBL_PAGES: [u32; MAX_PORTS] = [0; MAX_PORTS];

// Single-sector bounce buffer for DMA
static mut BOUNCE_PHYS: u32 = 0;
static mut BOUNCE_VIRT: *mut u8 = ptr::null_mut();

extern "C" {
    fn krust_pmm_alloc_frame() -> usize;
    fn krust_pmm_free_frame(frame: usize);
    fn krust_map_mmio(phys: u64, size: u64) -> u64;
    fn krust_serial_write(p: *const u8, l: usize);
    fn krust_serial_putchar(c: u8);
}

unsafe fn dbg(s: &[u8]) { krust_serial_write(s.as_ptr(), s.len()); }
unsafe fn dhex(v: u32) {
    let h = b"0123456789ABCDEF";
    krust_serial_putchar(b'0'); krust_serial_putchar(b'x');
    for i in (0..8).rev() { krust_serial_putchar(h[((v >> (i*4)) & 0xF) as usize]); }
}
unsafe fn dln() { krust_serial_putchar(b'\n'); }
unsafe fn ds(s: &[u8], v: u32) { dbg(s); dhex(v); dln(); }

unsafe fn hba_reg(off: u64) -> *mut u32 { (HBA + off) as *mut u32 }
unsafe fn port_reg(port: u8, off: u64) -> *mut u32 {
    (HBA + PORT_BASE + ((port as u64) * PORT_SIZE) + off) as *mut u32
}
unsafe fn rr(off: u64) -> u32 { ptr::read_volatile(hba_reg(off)) }
unsafe fn ww(off: u64, v: u32) { ptr::write_volatile(hba_reg(off), v) }
unsafe fn pr(port: u8, off: u64) -> u32 { ptr::read_volatile(port_reg(port, off)) }
unsafe fn pw(port: u8, off: u64, v: u32) { ptr::write_volatile(port_reg(port, off), v) }

unsafe fn page_alloc() -> (u32, *mut u8) {
    let f = krust_pmm_alloc_frame();
    if f == !0 { (0, ptr::null_mut()) } else { ((f * 4096) as u32, (f * 4096) as *mut u8) }
}

unsafe fn ahci_reset() {
    ww(GHC, rr(GHC) | GHC_HR);
    for _ in 0..100000 { if rr(GHC) & GHC_HR == 0 { break; } }
}

unsafe fn port_wait_idle(port: u8) -> bool {
    for _ in 0..100000 {
        let cmd = pr(port, PxCMD);
        if (cmd & (CMD_CR | CMD_FR)) == 0 { return true; }
    }
    false
}

unsafe fn port_start(port: u8) -> bool {
    if !port_wait_idle(port) { return false; }
    pw(port, PxCMD, pr(port, PxCMD) | CMD_FRE | CMD_SUD | CMD_POD);
    pw(port, PxCMD, pr(port, PxCMD) | CMD_ST);
    true
}

unsafe fn port_stop(port: u8) {
    pw(port, PxCMD, pr(port, PxCMD) & !CMD_ST);
    port_wait_idle(port);
    pw(port, PxCMD, pr(port, PxCMD) & !CMD_FRE);
}

unsafe fn port_init(port: u8) -> bool {
    port_stop(port);

    // Clear errors
    pw(port, PxSERR, 0xFFFFFFFF);
    pw(port, PxIE, 0);

    // Allocate command list (1024 bytes = 1 page)
    let (clp, clv) = page_alloc();
    if clv.is_null() { return false; }
    ptr::write_bytes(clv, 0, 4096);
    CMD_PAGES[port as usize] = clp;
    pw(port, PxCLB, clp);
    pw(port, PxCLBU, 0);

    // Allocate FIS receive area (256 bytes)
    let (fip, fiv) = page_alloc();
    if fiv.is_null() { return false; }
    ptr::write_bytes(fiv, 0, 4096);
    FIS_PAGES[port as usize] = fip;
    pw(port, PxFB, fip);
    pw(port, PxFBU, 0);

    // Allocate command table (128 bytes, share page for all 32 cmds)
    let (ctp, ctv) = page_alloc();
    if ctv.is_null() { return false; }
    ptr::write_bytes(ctv, 0, 4096);
    CTBL_PAGES[port as usize] = ctp;

    // Set up command list entries to all point to the same command table
    let cl = clv as *mut CmdList;
    let cmd_size = 0x80; // 128 bytes per cmd table slot
    for i in 0..32 {
        let entry = &mut (*cl).0[i as usize];
        entry.dw0 = 0;
        entry.prdbc = 0;
        entry.ctba = ctp + i * cmd_size;
        entry.ctbau = 0;
    }

    port_start(port)
}

unsafe fn port_recover(port: u8) -> bool {
    port_stop(port);
    // Clear error registers
    pw(port, PxSERR, 0xFFFFFFFF);
    pw(port, PxIS, 0xFFFFFFFF);
    // Re-enable port
    pw(port, PxCMD, pr(port, PxCMD) | CMD_SUD | CMD_POD);
    port_start(port)
}

unsafe fn send_cmd(port: u8, fis: &[u8], prdt_phys: u32, prdt_count: u16) -> bool {
    let clp = CMD_PAGES[port as usize];
    let cl = clp as *mut CmdList;
    let ctba = ptr::read_volatile(ptr::addr_of!((*cl).0[0].ctba)) as u32;

    // Write FIS to command table
    let ctv = ctba as *mut CmdTable;
    ptr::write_bytes(ctv, 0, 128);
    ptr::copy_nonoverlapping(fis.as_ptr(), ctv as *mut u8, fis.len());

    // Write PRDT if any
    if prdt_count > 0 {
        let prdt = ptr::addr_of_mut!((*ctv).prdt[0]) as *mut PrdtEntry;
        let prdt_src = prdt_phys as *const PrdtEntry;
        ptr::copy_nonoverlapping(prdt_src, prdt, prdt_count as usize);
    }

    // Set up command entry
    let atapi = if fis[0] == 0xA0 { 0x40 } else { 0x00 };
    let fl = atapi | (prdt_count as u32) << 16;
    ptr::write_volatile(ptr::addr_of_mut!((*cl).0[0].dw0), fl);
    ptr::write_volatile(ptr::addr_of_mut!((*cl).0[0].dw0), fl);
    ptr::write_volatile(ptr::addr_of_mut!((*cl).0[0].prdbc), 0);

    // Wait for any previous command
    for _ in 0..100000 {
        if pr(port, PxCI) == 0 && (pr(port, PxTFD) & (TFD_BSY | TFD_DRQ)) == 0 { break; }
    }
    if pr(port, PxCI) != 0 { return false; }

    // Issue command
    pw(port, PxCI, 1);

    // Wait for completion
    for _ in 0..1000000 {
        if pr(port, PxCI) & 1 == 0 {
            let tfd = pr(port, PxTFD);
            if tfd & TFD_BSY == 0 {
                let err = pr(port, PxIS);
                if err & 0xFFFFFFFF != 0 { pw(port, PxIS, err); }
                return (tfd & 0xFF) == 0;
            }
        }
    }
    port_recover(port);
    false
}

unsafe fn build_cmd_fis(cmd: u8, lba: u64, count: u16, dev: u8) -> [u8; 64] {
    let mut fis = [0u8; 64];
    fis[0] = 0x27; // FIS type: host to device
    fis[1] = 0x80; // bit 7 = update command register
    fis[2] = cmd;
    fis[3] = dev;
    fis[4] = (lba & 0xFF) as u8;
    fis[5] = ((lba >> 8) & 0xFF) as u8;
    fis[6] = ((lba >> 16) & 0xFF) as u8;
    fis[7] = ((lba >> 24) & 0xFF) as u8;
    fis[8] = ((lba >> 32) & 0xFF) as u8;
    fis[9] = ((lba >> 40) & 0xFF) as u8;
    fis[10] = 0x40; // LBA mode
    fis[12] = (count & 0xFF) as u8;
    fis[13] = ((count >> 8) & 0xFF) as u8;
    fis
}

unsafe fn identify(port: u8, buf: *mut u8) -> bool {
    let fis = build_cmd_fis(CMD_IDENTIFY, 0, 0, 0);
    let prdt = BOUNCE_PHYS as *mut PrdtEntry;
    // For IDENTIFY, the data buffer size is 512 bytes
    ptr::write_volatile(ptr::addr_of_mut!((*prdt).dba), BOUNCE_PHYS);
    ptr::write_volatile(ptr::addr_of_mut!((*prdt).dbau), 0);
    ptr::write_volatile(ptr::addr_of_mut!((*prdt)._rsvd), 0);
    ptr::write_volatile(ptr::addr_of_mut!((*prdt).dbc), (512 - 1) | 0x80000000); // I bit = interrupt on completion

    if !send_cmd(port, &fis, BOUNCE_PHYS, 1) { return false; }
    ptr::copy_nonoverlapping(BOUNCE_VIRT, buf, 512);
    true
}

unsafe fn read_sectors(port: u8, lba: u64, count: u16, buf: *mut u8) -> bool {
    let fis = build_cmd_fis(CMD_READ_DMA_EXT, lba, count, 0);
    let total = (count as u32) * 512;
    // For simplicity, set up PRDT for all sectors
    // Note: This requires contiguous physical pages, which is fine for single-sector
    let prdt = BOUNCE_PHYS as *mut PrdtEntry;
    ptr::write_volatile(ptr::addr_of_mut!((*prdt).dba), BOUNCE_PHYS);
    ptr::write_volatile(ptr::addr_of_mut!((*prdt).dbau), 0);
    ptr::write_volatile(ptr::addr_of_mut!((*prdt)._rsvd), 0);
    ptr::write_volatile(ptr::addr_of_mut!((*prdt).dbc), (total - 1) | 0x80000000);

    if !send_cmd(port, &fis, BOUNCE_PHYS, 1) { return false; }
    ptr::copy_nonoverlapping(BOUNCE_VIRT, buf, total as usize);
    true
}

unsafe fn write_sectors(port: u8, lba: u64, count: u16, buf: *const u8) -> bool {
    let fis = build_cmd_fis(CMD_WRITE_DMA_EXT, lba, count, 0);
    let total = (count as u32) * 512;
    ptr::copy_nonoverlapping(buf, BOUNCE_VIRT, total as usize);

    let prdt = BOUNCE_PHYS as *mut PrdtEntry;
    ptr::write_volatile(ptr::addr_of_mut!((*prdt).dba), BOUNCE_PHYS);
    ptr::write_volatile(ptr::addr_of_mut!((*prdt).dbau), 0);
    ptr::write_volatile(ptr::addr_of_mut!((*prdt)._rsvd), 0);
    ptr::write_volatile(ptr::addr_of_mut!((*prdt).dbc), (total - 1) | 0x80000000);

    send_cmd(port, &fis, BOUNCE_PHYS, 1)
}

unsafe fn init_ahci(mmio: *mut u8) -> bool {
    HBA = mmio as u64;
    let cap = rr(CAP);
    let pi = rr(PI);
    let nports = (cap & 0x1F) as usize;
    ds(b"ahci: version=", rr(VS));
    ds(b"ahci: ports_impl=0x", pi);
    ds(b"ahci: nports=", nports as u32);

    ahci_reset();
    ww(GHC, rr(GHC) | GHC_AE); // Enable AHCI mode
    ds(b"ahci: ghc post-reset=0x", rr(GHC));

    // Allocate bounce buffer (1 page for PRDT + data)
    let (bp, bv) = page_alloc();
    if bv.is_null() { return false; }
    BOUNCE_PHYS = bp;
    BOUNCE_VIRT = bv;
    ptr::write_bytes(bv, 0, 4096);

    // Initialize each implemented port
    for p in 0..32 {
        if pi & (1 << p) == 0 { continue; }
        let ssts = pr(p as u8, PxSSTS) & SSTS_DET;
        if ssts != DET_PRESENT && ssts != DET_ACTIVE { continue; }
        ds(b"ahci: port ", p); ds(b" ssts=", ssts);

        if !port_init(p as u8) {
            ds(b"ahci: port init failed", p);
            continue;
        }

        // Wait for device to be ready
        for _ in 0..100000 {
            let tfd = pr(p as u8, PxTFD);
            if (tfd & (TFD_BSY | TFD_DRQ)) == 0 { break; }
        }

        // Check signature
        let sig = pr(p as u8, PxSIG);
        if sig != SIG_ATA {
            ds(b"ahci: port non-ATA sig=0x", sig);
            continue;
        }

        // Identify device
        let mut id_buf = [0u8; 512];
        if !identify(p as u8, id_buf.as_mut_ptr()) {
            ds(b"ahci: identify failed port ", p);
            continue;
        }

        let id = id_buf.as_ptr() as *const u16;
        let lba48 = (ptr::read_volatile(id.add(83)) & 0x0400) != 0;
        let sectors = if lba48 {
            (ptr::read_volatile(id.add(100)) as u64)
            | ((ptr::read_volatile(id.add(101)) as u64) << 16)
            | ((ptr::read_volatile(id.add(102)) as u64) << 32)
            | ((ptr::read_volatile(id.add(103)) as u64) << 48)
        } else {
            (ptr::read_volatile(id.add(60)) as u64) | ((ptr::read_volatile(id.add(61)) as u64) << 16)
        };

        let mut model = [0u8; 40];
        for i in 0..20 {
            model[i*2] = (ptr::read_volatile(id.add(27 + i)) >> 8) as u8;
            model[i*2 + 1] = (ptr::read_volatile(id.add(27 + i)) & 0xFF) as u8;
        }
        let mut serial = [0u8; 20];
        for i in 0..10 {
            serial[i*2] = (ptr::read_volatile(id.add(10 + i)) >> 8) as u8;
            serial[i*2 + 1] = (ptr::read_volatile(id.add(10 + i)) & 0xFF) as u8;
        }

        let di = NDRIVE;
        DRIVES[di] = Drive {
            port: p as u8, present: true, lba48, sectors,
            model, serial,
        };
        NDRIVE += 1;

        ds(b"ahci: drive ", di as u32);
        ds(b"  sectors=", (sectors & 0xFFFFFFFF) as u32);
        ds(b"  lba48=", lba48 as u32);
    }

    ds(b"ahci: drives=", NDRIVE as u32);
    true
}

// --- C API ---

#[no_mangle]
pub unsafe extern "C" fn krust_ahci_init(mmio_phys: u64) -> i32 {
    let base = krust_map_mmio(mmio_phys, 4096);
    if base == 0 { return -1; }
    if init_ahci(base as *mut u8) { 0 } else { -1 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ahci_drive_count() -> i32 {
    NDRIVE as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_ahci_read(drive: i32, lba: u64, count: u32, buf: *mut u8) -> i32 {
    if drive < 0 || drive as usize >= NDRIVE { return -1; }
    let d = &DRIVES[drive as usize];
    if !d.present { return -1; }
    if count > 256 { return -1; }

    // Read one sector at a time through bounce buffer
    for s in 0..count {
        if !read_sectors(d.port, lba + s as u64, 1, buf.offset((s * 512) as isize)) {
            return -1;
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_ahci_write(drive: i32, lba: u64, count: u32, buf: *const u8) -> i32 {
    if drive < 0 || drive as usize >= NDRIVE { return -1; }
    let d = &DRIVES[drive as usize];
    if !d.present { return -1; }
    if count > 256 { return -1; }

    for s in 0..count {
        if !write_sectors(d.port, lba + s as u64, 1, buf.offset((s * 512) as isize)) {
            return -1;
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_ahci_get_sectors(drive: i32) -> u64 {
    if drive < 0 || drive as usize >= NDRIVE { return 0; }
    DRIVES[drive as usize].sectors
}

#[no_mangle]
pub unsafe extern "C" fn krust_ahci_present(drive: i32) -> i32 {
    if drive < 0 || drive as usize >= NDRIVE { return 0; }
    DRIVES[drive as usize].present as i32
}

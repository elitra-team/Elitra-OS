use core::ptr;

// ================================================================
// UHCI USB 1.x Driver + HID Keyboard/Mouse
// ================================================================

// --- I/O Registers ---
const REG_CMD: u16   = 0x00;
const REG_STS: u16   = 0x02;
const REG_INTR: u16  = 0x04;
const REG_FRNUM: u16 = 0x06;
const REG_SOF: u16   = 0x08;
const REG_PORT: u16  = 0x10;

// Command
const CMD_RUN: u16    = 0x0001;
const CMD_HCRST: u16  = 0x0002;
const CMD_GRST: u16   = 0x0004;
const CMD_CF: u16     = 0x0040;
const CMD_MAXP: u16   = 0x0080;

// Status
const STS_HALTED: u16 = 0x0020;

// Port
const P_CCS: u16   = 0x0001;
const P_CSC: u16   = 0x0002;
const P_PED: u16   = 0x0004;
const P_PEDC: u16  = 0x0008;
const P_LSDA: u16  = 0x0100;
const P_RST: u16   = 0x0200;

// TD ctrl bits
const TDA: u32 = 0x00800000; // active (bit 23)
const TDST: u32 = 0x00400000; // stalled
const TDE: u32 = 0x00200000; // data buf err
const TDB: u32 = 0x00100000; // babble
const TDN: u32 = 0x00080000; // nak
const TDC: u32 = 0x00040000; // crc/timeout
const TDIOC: u32 = 0x00010000; // interrupt on complete (bit 16)
const TDLS: u32 = 0x02000000; // low speed (bit 25)

// PID
const PID_SETUP: u32 = 0x2D;
const PID_IN: u32    = 0x69;
const PID_OUT: u32   = 0xE1;

// Link
const LK_T: u32 = 0x00000001;
const LK_QH: u32 = 0x00000002;
const LK_BR: u32 = 0x00000004;
const LK_ADDR: u32 = 0xFFFFFFF0;

// QH
const QH_EMP: u32 = 0x00000001;

// USB standard requests
const REQ_GET_DESC: u8 = 6;
const REQ_SET_ADDR: u8 = 5;
const REQ_SET_CFG: u8 = 9;

const DT_DEV: u16 = 1;
const DT_CFG: u16 = 2;
const DT_IFACE: u16 = 4;
const DT_ENDP: u16 = 5;

const HID_CLS: u8 = 3;
const HID_KBD: u8 = 1;
const HID_MOU: u8 = 2;

const MAX_DEV: usize = 8;
const MAX_HID: usize = 4;
const RETRIES: u32 = 300000;

#[repr(C, packed)]
struct DevReq {
    rt: u8, rq: u8, wv: u16, wi: u16, wl: u16,
}

#[repr(C, packed)]
struct DevDesc {
    len: u8, typ: u8, usb: u16, cls: u8, sub: u8,
    proto: u8, maxp: u8, vid: u16, pid: u16, dev: u16,
    man: u8, prod: u8, ser: u8, cn: u8,
}

#[repr(C, packed)]
struct CfgDesc {
    len: u8, typ: u8, total: u16, ni: u8, val: u8,
    iconf: u8, attr: u8, maxpwr: u8,
}

#[repr(C, packed)]
struct IfDesc {
    len: u8, typ: u8, num: u8, alt: u8, ne: u8,
    cls: u8, sub: u8, proto: u8, iface: u8,
}

#[repr(C, packed)]
struct EpDesc {
    len: u8, typ: u8, addr: u8, attr: u8, maxsz: u16, ival: u8,
}

#[repr(C, align(16))]
struct Td { link: u32, ctrl: u32, token: u32, buf: u32 }

#[repr(C, align(8))]
struct Qh { head: u32, elt: u32 }

#[derive(Clone, Copy)]
struct Device { addr: u8, port: u8, ls: bool, maxp: u8, active: bool }

struct Hid {
    di: usize,       // device index
    iface: u8,
    ep: u8,          // endpoint address
    epsz: u16,
    proto: u8,       // HID protocol (1=kbd, 2=mouse)
    active: bool,
    toggle: u32,     // current data toggle (0=DATA0, 1=DATA1)
    qh: *mut Qh, qhp: u32,
    td: *mut Td, tdp: u32,
    buf: [u8; 64],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct UsbEvent {
    pub typ: u8, pub mods: u8, pub key: u8,
    pub dx: i8, pub dy: i8, pub btn: u8,
}

// --- Globals ---
static mut IO: u16 = 0;
static mut DEVS: [Device; MAX_DEV] = unsafe { core::mem::zeroed() };
static mut NDEV: usize = 0;
static mut NADDR: u8 = 1;
static mut HIDS: [Hid; MAX_HID] = unsafe { core::mem::zeroed() };
static mut NHID: usize = 0;

// Frame list (page)
static mut FL: *mut u32 = ptr::null_mut();
static mut FLP: u32 = 0;

// Control transfer workspace (page): [setup(8)] [td*3+tdw(64)] [qh(8)] [data(512)]
static mut WP: *mut u8 = ptr::null_mut();
static mut WPP: u32 = 0;

// Event ring
static mut EVS: [UsbEvent; 16] = unsafe { core::mem::zeroed() };
static mut EVH: usize = 0;
static mut EVT: usize = 0;

extern "C" {
    fn krust_pmm_alloc_frame() -> u32;
    fn krust_pmm_free_frame(f: u32);
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

unsafe fn inw(p: u16) -> u16 {
    let r: u16;
    core::arch::asm!("in ax, dx", out("ax") r, in("dx") p, options(nostack, preserves_flags));
    r
}
unsafe fn outw(p: u16, v: u16) {
    core::arch::asm!("out dx, ax", in("dx") p, in("ax") v, options(nostack, preserves_flags));
}

unsafe fn page() -> (u32, *mut u8) {
    let f = krust_pmm_alloc_frame();
    if f == 0xFFFFFFFF { (0, ptr::null_mut()) } else { (f*4096, (f*4096) as *mut u8) }
}

fn tkn(pid: u32, dev: u8, ep: u8, maxp: u32, toggle: u32) -> u32 {
    pid | (dev as u32) << 8 | (ep as u32) << 15 | ((toggle & 1) << 19) | ((maxp & 0x7FF) << 21)
}

fn tctrl(ioc: bool, ls: bool) -> u32 {
    TDA | if ioc { TDIOC } else { 0 } | if ls { TDLS } else { 0 }
}

fn lk(addr: u32, term: bool, qh: bool, br: bool) -> u32 {
    (addr & LK_ADDR) | if term { LK_T } else { 0 } | if qh { LK_QH } else { 0 } | if br { LK_BR } else { 0 }
}

unsafe fn wait_td(td: *const Td) -> bool {
    for _ in 0..RETRIES {
        let c = ptr::read_volatile(&(*td).ctrl);
        if c & TDA == 0 { return c & (TDST|TDE|TDB|TDC) == 0; }
        inw(IO | REG_STS); // kick QEMU virtual clock
    }
    false
}

// === Controller ===
unsafe fn stop() {
    outw(IO | REG_CMD, inw(IO | REG_CMD) & !CMD_RUN);
    for _ in 0..10000 { if inw(IO | REG_STS) & STS_HALTED != 0 { break; } }
}

unsafe fn rst() {
    outw(IO | REG_CMD, CMD_HCRST);
    for _ in 0..10000 { if inw(IO | REG_CMD) & CMD_HCRST == 0 { break; } }
}

unsafe fn start() {
    outw(IO | REG_CMD, CMD_RUN | CMD_CF | CMD_MAXP);
    for _ in 0..10000 { if inw(IO | REG_STS) & STS_HALTED == 0 { break; } }
}

unsafe fn init(io: u16) -> bool {
    IO = io; ds(b"uhci: io=", io as u32);
    stop(); rst();

    let (fp, fv) = page();
    if fv.is_null() { return false; }
    FLP = fp; FL = fv as *mut u32;
    for i in 0..1024 { ptr::write_volatile(FL.add(i), LK_T); }
    outw(IO | REG_SOF, (fp & 0xFFFF) as u16);
    outw(IO | REG_SOF + 2, ((fp >> 16) & 0xFFFF) as u16);

    let (wp, wv) = page();
    if wv.is_null() { return false; }
    WPP = wp; WP = wv;
    ptr::write_bytes(wv, 0, 4096);

    // Self-test: put dummy TD in ALL frame entries before starting
    // Use the SAME page for buf as ctrl will (WPP) to test DMA from workspace page
    let td = wv as *mut Td;
    ptr::write_volatile(ptr::addr_of_mut!((*td).link), LK_T);
    ptr::write_volatile(ptr::addr_of_mut!((*td).ctrl), TDA);
    ptr::write_volatile(ptr::addr_of_mut!((*td).token), tkn(PID_IN, 0, 0, 63, 0));
    ptr::write_volatile(ptr::addr_of_mut!((*td).buf), wp);
    for i in 0..1024 { ptr::write_volatile(FL.add(i), lk(wp, false, false, false)); }

    outw(IO | REG_STS, 0x3F);
    start();

    // Wait for TD to be processed (long wait)
    let mut _ok = false;
    for _ in 0..200000 {
        if ptr::read_volatile(ptr::addr_of!((*td).ctrl)) & TDA == 0 {
            _ok = true;
            break;
        }
        inw(IO | REG_STS); // kick QEMU virtual clock
    }
    let tdv = ptr::read_volatile(ptr::addr_of!((*td).ctrl));
    ds(b"tdtest=", tdv);
    // Restore all frame entries
    for i in 0..1024 { ptr::write_volatile(FL.add(i), LK_T); }

    // Probe ports: reset any that have a device connected
    for p in 0..2u32 {
        let addr = IO | REG_PORT + (p*2) as u16;
        let sc = inw(addr);
        if sc & P_CCS != 0 {
            // Device present, reset + enable + try enumerate
            outw(addr, sc | P_RST);
            for _ in 0..50000 { if inw(addr) & P_RST == 0 { break; } }
            let sc2 = inw(addr);
            outw(addr, (sc2 | P_CSC) & !P_RST);
            let sc3 = inw(addr);
            outw(addr, sc3 | P_PED);
            // Wait for device to reconnect
            for _ in 0..10000 { if inw(addr) & P_CCS != 0 { break; } }
            if inw(addr) & P_CCS != 0 {
                wait_ms(100); // reset recovery (USB spec: 10ms min)
                enum_dev(p);
            }
        } else {
            outw(addr, P_PED);
        }
    }

    // Verify controller is running and generating frames
    let fr = inw(IO | REG_FRNUM);
    let cmd = inw(IO | REG_CMD);
    let sts = inw(IO | REG_STS);
    ds(b"fr=", fr as u32);
    ds(b"cmd=", cmd as u32);
    ds(b"sts=", sts as u32);
    let fla = (inw(IO | REG_SOF) as u32) | ((inw(IO | REG_SOF + 2) as u32) << 16);
    ds(b"fla=", fla);
    ds(b"flp=", FLP);
    if sts & STS_HALTED != 0 { ds(b"HALTED", 0); }
    if fla != FLP { ds(b"FLMISMATCH", 0); }

    dbg(b"uhci: ok\n");
    true
}

// Busy-wait for given number of milliseconds using frame counter (1 kHz)
unsafe fn wait_ms(ms: u32) {
    let start = inw(IO | REG_FRNUM) as u32;
    loop {
        let cur = inw(IO | REG_FRNUM) as u32;
        if (cur.wrapping_sub(start) & 0x3FF) >= ms {
            break;
        }
        inw(IO | REG_STS);
    }
}

// === Control Transfer ===
unsafe fn ctrl(dev_addr: u8, req: *const DevReq, data: *mut u8, dlen: u16) -> bool {
    let dir_in = (*req).rt & 0x80 != 0;
    let has_data = dlen > 0;
    let mp = if dev_addr == 0 { 8 } else { DEVS[(dev_addr-1) as usize].maxp as u32 };

    // Layout:
    // [0..8]     setup packet (8 bytes)
    // [256..]    TD working area (for active TD)
    // [2048..]   data buffer
    const TD_OFF: u32 = 256;

    ptr::write_bytes(WP, 0, 4096);

    let td = WP.add(TD_OFF as usize) as *mut Td;
    let tdp = WPP + TD_OFF;

    // Setup stage: SETUP TD needs non-terminated link (QEMU quirk)
    // Point it to a dummy TD right after
    ptr::copy_nonoverlapping(req as *const u8, WP, 8);
    ptr::write_volatile(ptr::addr_of_mut!((*td).link), lk(tdp + 16, false, false, false));
    ptr::write_volatile(ptr::addr_of_mut!((*td).ctrl), TDA);
    ptr::write_volatile(ptr::addr_of_mut!((*td).token), tkn(PID_SETUP, dev_addr, 0, 7, 0));
    ptr::write_volatile(ptr::addr_of_mut!((*td).buf), WPP);
    // Dummy TD prevents QEMU skipping SETUP
    ptr::write_volatile(ptr::addr_of_mut!((*td.add(16)).link), LK_T);
    ptr::write_volatile(ptr::addr_of_mut!((*td.add(16)).ctrl), TDA);
    ptr::write_volatile(ptr::addr_of_mut!((*td.add(16)).token), tkn(PID_IN, dev_addr, 0, 0, 0));
    ptr::write_volatile(ptr::addr_of_mut!((*td.add(16)).buf), 0);
    for i in 0..1024 { ptr::write_volatile(FL.add(i), lk(tdp, false, false, false)); }

    if !wait_td(td as *const Td) { for i in 0..1024 { ptr::write_volatile(FL.add(i), LK_T); } return false; }
    for i in 0..1024 { ptr::write_volatile(FL.add(i), LK_T); }

    // Data stage (multi-packet support)
    if has_data {
        let maxp_val = mp as usize;
        let npkts = (dlen as usize + maxp_val - 1) / maxp_val;
        for p in 0..npkts {
            let is_last = p == npkts - 1;
            let toggle = p as u32 & 1;
            let buf_off = 2048 + p * maxp_val;

            if !dir_in {
                let remaining = dlen as usize - p * maxp_val;
                let chunk = if remaining > maxp_val { maxp_val } else { remaining };
                ptr::copy_nonoverlapping(data.add(p * maxp_val), WP.add(buf_off), chunk);
                ptr::write_volatile(ptr::addr_of_mut!((*td).token), tkn(
                    PID_OUT, dev_addr, 0, (chunk - 1) as u32, toggle));
            } else {
                ptr::write_volatile(ptr::addr_of_mut!((*td).token), tkn(
                    PID_IN, dev_addr, 0, (maxp_val - 1) as u32, toggle));
            }

            ptr::write_volatile(ptr::addr_of_mut!((*td).link), LK_T);
            ptr::write_volatile(ptr::addr_of_mut!((*td).ctrl), TDA | if is_last { TDIOC } else { 0 });
            ptr::write_volatile(ptr::addr_of_mut!((*td).buf), WPP + buf_off as u32);

            for i in 0..1024 { ptr::write_volatile(FL.add(i), lk(tdp, false, false, false)); }
            if !wait_td(td as *const Td) { for i in 0..1024 { ptr::write_volatile(FL.add(i), LK_T); } return false; }
            for i in 0..1024 { ptr::write_volatile(FL.add(i), LK_T); }
        }
    }

    // Status stage
    {
        let status_pid = if dir_in { PID_OUT } else { PID_IN };
        ptr::write_volatile(ptr::addr_of_mut!((*td).link), LK_T);
        ptr::write_volatile(ptr::addr_of_mut!((*td).ctrl), TDA | TDIOC);
        ptr::write_volatile(ptr::addr_of_mut!((*td).token), tkn(status_pid, dev_addr, 0, 0, 1));
        ptr::write_volatile(ptr::addr_of_mut!((*td).buf), 0);
        for i in 0..1024 { ptr::write_volatile(FL.add(i), lk(tdp, false, false, false)); }
        if !wait_td(td as *const Td) { for i in 0..1024 { ptr::write_volatile(FL.add(i), LK_T); } return false; }
        for i in 0..1024 { ptr::write_volatile(FL.add(i), LK_T); }
    }

    if dir_in && has_data {
        ptr::copy_nonoverlapping(WP.add(2048), data, dlen as usize);
    }
    true
}

// === USB requests ===
unsafe fn get_desc(addr: u8, typ: u16, idx: u16, buf: *mut u8, len: u16) -> bool {
    let req = DevReq { rt: 0x80|0x00|0x00, rq: REQ_GET_DESC, wv: (typ<<8)|idx, wi: 0, wl: len };
    ctrl(addr, &req as *const DevReq, buf, len)
}

unsafe fn dev_desc(addr: u8, buf: *mut u8, len: u16) -> bool { get_desc(addr, DT_DEV, 0, buf, len) }
unsafe fn cfg_desc(addr: u8, buf: *mut u8, len: u16) -> bool { get_desc(addr, DT_CFG, 0, buf, len) }

unsafe fn set_addr(addr: u8) -> bool {
    let req = DevReq { rt: 0x00|0x00|0x00, rq: REQ_SET_ADDR, wv: addr as u16, wi: 0, wl: 0 };
    ctrl(0, &req as *const DevReq, ptr::null_mut(), 0)
}

unsafe fn set_cfg(addr: u8, val: u8) -> bool {
    let req = DevReq { rt: 0x00, rq: REQ_SET_CFG, wv: val as u16, wi: 0, wl: 0 };
    ctrl(addr, &req as *const DevReq, ptr::null_mut(), 0)
}

unsafe fn set_proto(addr: u8, iface: u8, proto: u8) -> bool {
    let req = DevReq { rt: 0x00|0x20|0x01, rq: 0x0B, wv: proto as u16, wi: iface as u16, wl: 0 };
    ctrl(addr, &req as *const DevReq, ptr::null_mut(), 0)
}

// === Enumeration ===
unsafe fn enum_dev(port: u32) {
    if NDEV >= MAX_DEV { return; }
    let idx = NDEV;

    let sc = inw(IO | REG_PORT + (port*2) as u16);
    let ls = (sc & P_LSDA) != 0;
    DEVS[idx] = Device { addr: 0, port: port as u8, ls, maxp: 8, active: true };
    ds(b"usb: enum port=", port);

    // First 8 bytes of dev desc on addr 0
    let dbuf = WP.add(2048);
    ds(b"pst=", inw(IO | REG_PORT + (port*2) as u16) as u32);
    if !dev_desc(0, dbuf, 8) { DEVS[idx].active = false; return; }
    let mp = (*(dbuf as *const DevDesc)).maxp;
    DEVS[idx].maxp = mp;
    ds(b"usb: maxp=", mp as u32);
    // Debug first 4 bytes of descriptor
    let dw = ptr::read_volatile(dbuf as *const u32);
    ds(b"ddw=", dw);

    let addr = NADDR; NADDR += 1;
    if !set_addr(addr) { DEVS[idx].active = false; return; }
    DEVS[idx].addr = addr;
    ds(b"usb: addr=", addr as u32);

    // Full dev desc
    if !dev_desc(addr, dbuf, 18) { DEVS[idx].active = false; return; }
    let _dd = &*(dbuf as *const DevDesc);

    // Config desc
    if !cfg_desc(addr, dbuf, 9) { DEVS[idx].active = false; return; }
    let total = (*(dbuf as *const CfgDesc)).total;
    if total > 500 { DEVS[idx].active = false; return; }
    if !cfg_desc(addr, dbuf, total) { DEVS[idx].active = false; return; }

    if !set_cfg(addr, 1) { DEVS[idx].active = false; return; }

    let mut off: usize = 0;
    while off + 2 < total as usize {
        let len = *dbuf.add(off) as usize;
        let typ = *dbuf.add(off + 1);
        if len == 0 || off + len > total as usize { break; }
        if typ == (DT_IFACE as u8) {
            let iface = &*(dbuf.add(off) as *const IfDesc);
            if iface.cls == HID_CLS {
                ds(b"usb: HID proto=", iface.proto as u32);
                setup_hid(idx, iface, dbuf, off, total as usize);
            }
        }
        off += len;
    }

    NDEV += 1;
    dbg(b"usb: enumerated\n");
}

unsafe fn setup_hid(di: usize, iface: &IfDesc, cfg: *mut u8, start: usize, total: usize) {
    if NHID >= MAX_HID { return; }
    let mut ep_in: u8 = 0;
    let mut ep_sz: u16 = 8;
    let mut off = start + iface.len as usize;

    while off + 2 < total {
        let len = *cfg.add(off) as usize;
        let typ = *cfg.add(off + 1);
        if len == 0 || off + len > total { break; }
        if typ == (DT_ENDP as u8) {
            let ep = &*(cfg.add(off) as *const EpDesc);
            if ep.addr & 0x80 != 0 { ep_in = ep.addr; ep_sz = ep.maxsz; }
        }
        off += len;
    }
    if ep_in == 0 { return; }
    if ep_sz > 64 { ep_sz = 64; }

    let (pp, pv) = page();
    if pv.is_null() { return; }
    ptr::write_bytes(pv, 0, 4096);

    let qh = pv as *mut Qh;
    let td = pv.add(16) as *mut Td;
    let qhp = pp;
    let tdp = pp + 16;

    let dev = &DEVS[di];
    let _eid = ep_in & 0x7F;

    set_proto(dev.addr, iface.num, 0);

    let hi = NHID;
    HIDS[hi] = Hid {
        di, iface: iface.num, ep: ep_in, epsz: ep_sz,
        proto: iface.proto, active: true, toggle: 0,
        qh, qhp, td, tdp, buf: [0u8; 64],
    };
    let hid = &mut HIDS[hi];

    // Rearm the interrupt transfer
    rearm_hid(hid);

    // Place QH in a dedicated frame entry (entry 0 reserved for control)
    // HID i uses entry i+1. Controller processes ~1000 entries/s so each HID
    // gets polled ~1000/s (one entry per frame on average every 1.024s cycle,
    // but with 1024 entries it wraps ~1/s so each entry is hit ~1/s).
    // With NHID active HIDs, each still gets ~1 poll per second which is fine.
    ptr::write_volatile(FL.add(1 + hi), lk(qhp, false, true, true));

    NHID += 1;
    dbg(b"usb: HID ready\n");
}

unsafe fn rearm_hid(hid: &mut Hid) {
    let dev = &DEVS[hid.di];
    let eid = hid.ep & 0x7F;
    // TD terminates directly; QH.elt serves as entry point
    ptr::write_volatile(ptr::addr_of_mut!((*hid.td).link), LK_T);
    ptr::write_volatile(ptr::addr_of_mut!((*hid.td).ctrl), tctrl(true, dev.ls));
    let maxp = hid.epsz.wrapping_sub(1) as u32;
    ptr::write_volatile(ptr::addr_of_mut!((*hid.td).token), tkn(PID_IN, dev.addr, eid, maxp, hid.toggle));
    ptr::write_volatile(ptr::addr_of_mut!((*hid.td).buf), hid.buf.as_mut_ptr() as u32);
    ptr::write_volatile(ptr::addr_of_mut!((*hid.qh).head), LK_T);
    ptr::write_volatile(ptr::addr_of_mut!((*hid.qh).elt), hid.tdp & !0xF);
}

unsafe fn port_has_device(port: u32) -> bool {
    for i in 0..NDEV {
        if DEVS[i].active && DEVS[i].port == port as u8 { return true; }
    }
    false
}

unsafe fn port_reset_enum(port: u32) {
    let addr = IO | REG_PORT + (port*2) as u16;
    let sc = inw(addr);
    outw(addr, sc | P_RST);
    for _ in 0..50000 { if inw(addr) & P_RST == 0 { break; } }
    let sc2 = inw(addr);
    outw(addr, (sc2 | P_CSC) & !P_RST);
    let sc3 = inw(addr);
    outw(addr, sc3 | P_PED);
    for _ in 0..10000 { if inw(addr) & P_CCS != 0 { break; } }
    wait_ms(100);
    enum_dev(port);
}

unsafe fn poll_port(port: u32) {
    let sc = inw(IO | REG_PORT + (port*2) as u16);
    if sc & P_CSC != 0 {
        if sc & P_CCS != 0 {
            port_reset_enum(port);
        } else {
            outw(IO | REG_PORT + (port*2) as u16, sc | P_CSC);
        }
    } else if sc & P_CCS != 0 && !port_has_device(port) {
        port_reset_enum(port);
    }
}

unsafe fn poll_hid(hid: &mut Hid) {
    if !hid.active { return; }
    let ctrl_val = ptr::read_volatile(&(*hid.td).ctrl);
    if ctrl_val & TDA != 0 { return; }
    if ctrl_val & (TDST|TDE|TDB|TDC) != 0 { rearm_hid(hid); return; }

    let len = ((ctrl_val >> 1) & 0x3FF) as usize;
    if len > 0 {
        let ev = match hid.proto {
            HID_KBD => {
                let m = hid.buf[0];
                let k = hid.buf[2];
                UsbEvent { typ: 1, mods: m, key: k, dx: 0, dy: 0, btn: 0 }
            }
            HID_MOU => {
                let b = hid.buf[0] & 7;
                let dx = hid.buf[1] as i8;
                let dy = hid.buf[2] as i8;
                UsbEvent { typ: 2, mods: 0, key: 0, dx, dy, btn: b }
            }
            _ => UsbEvent { typ: 0, mods: 0, key: 0, dx: 0, dy: 0, btn: 0 },
        };
        if ev.typ != 0 {
            let n = (EVH + 1) & 15;
            if n != EVT { EVS[EVH] = ev; EVH = n; }
        }
    }
    // Alternate toggle for next interrupt IN
    hid.toggle ^= 1;
    rearm_hid(hid);
}

// === C API ===
#[no_mangle]
pub unsafe extern "C" fn krust_usb_init_uhci(io_base: u16) -> i32 {
    if init(io_base) { 0 } else { -1 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_usb_poll() {
    poll_port(0);
    poll_port(1);
    for i in 0..NHID { poll_hid(&mut HIDS[i]); }
    let sts = inw(IO | REG_STS);
    if sts != 0 { outw(IO | REG_STS, sts); }
}



#[no_mangle]
pub unsafe extern "C" fn krust_usb_get_event() -> UsbEvent {
    let z = UsbEvent { typ: 0, mods: 0, key: 0, dx: 0, dy: 0, btn: 0 };
    if EVT == EVH { return z; }
    let e = EVS[EVT]; EVT = (EVT + 1) & 15; e
}

#[no_mangle]
pub unsafe extern "C" fn krust_usb_device_count() -> i32 { NDEV as i32 }

// === Bulk Transfers (for USB Mass Storage) ===
#[no_mangle]
pub unsafe extern "C" fn krust_usb_bulk_out(addr: u8, ep: u8, data: *const u8, len: usize) -> bool {
    if addr == 0 || data.is_null() || len == 0 { return false; }
    let dev_idx = (addr - 1) as usize;
    if dev_idx >= NDEV { return false; }
    let mp = DEVS[dev_idx].maxp as u32;
    let _ls = DEVS[dev_idx].ls;
    let eid = ep & 0x7F;

    const TD_OFF: u32 = 256;
    ptr::write_bytes(WP, 0, 4096);
    let td = WP.add(TD_OFF as usize) as *mut Td;
    let tdp = WPP + TD_OFF;

    let mut offset = 0usize;
    let mut toggle = 0u32;
    let mut first_td = td;
    let mut prev_td: *mut Td = ptr::null_mut();

    while offset < len {
        let chunk = if len - offset > mp as usize { mp as usize } else { len - offset };
        let t = if prev_td.is_null() { td } else { prev_td.add(1) };

        ptr::copy_nonoverlapping(data.add(offset), WP.add(2048), chunk);
        ptr::write_volatile(ptr::addr_of_mut!((*t).link), LK_T);
        ptr::write_volatile(ptr::addr_of_mut!((*t).ctrl), TDA);
        ptr::write_volatile(ptr::addr_of_mut!((*t).token),
            tkn(PID_OUT, addr, eid, (chunk - 1) as u32, toggle));
        ptr::write_volatile(ptr::addr_of_mut!((*t).buf), WPP + 2048);

        if prev_td.is_null() { first_td = t; }
        else { ptr::write_volatile(ptr::addr_of_mut!((*prev_td).link), lk(tdp + ((t as u32 - WPP) as i32) as u32, false, false, false)); }
        prev_td = t;
        toggle ^= 1;
        offset += chunk;
    }

    if first_td == td {
        ptr::write_volatile(ptr::addr_of_mut!((*td).link), LK_T);
        ptr::write_volatile(ptr::addr_of_mut!((*td).ctrl), TDA | TDIOC);
        ptr::write_volatile(ptr::addr_of_mut!((*td).token),
            tkn(PID_OUT, addr, eid, 0, 0));
        ptr::write_volatile(ptr::addr_of_mut!((*td).buf), 0);
    } else {
        ptr::write_volatile(ptr::addr_of_mut!((*prev_td).ctrl),
            ptr::read_volatile(ptr::addr_of!((*prev_td).ctrl)) | TDIOC);
    }

    for i in 0..1024 { ptr::write_volatile(FL.add(i), lk(tdp, false, false, false)); }
    let ok = wait_td(first_td as *const Td);
    for i in 0..1024 { ptr::write_volatile(FL.add(i), LK_T); }
    ok
}

#[no_mangle]
pub unsafe extern "C" fn krust_usb_bulk_in(addr: u8, ep: u8, buf: *mut u8, len: usize) -> bool {
    if addr == 0 || buf.is_null() || len == 0 { return false; }
    let dev_idx = (addr - 1) as usize;
    if dev_idx >= NDEV { return false; }
    let mp = DEVS[dev_idx].maxp as u32;
    let _ls = DEVS[dev_idx].ls;
    let eid = ep & 0x7F;

    const TD_OFF: u32 = 256;
    ptr::write_bytes(WP, 0, 4096);
    let td = WP.add(TD_OFF as usize) as *mut Td;
    let tdp = WPP + TD_OFF;

    let mut offset = 0usize;
    let mut toggle = 0u32;
    let mut first_td = td;
    let mut prev_td: *mut Td = ptr::null_mut();

    while offset < len {
        let chunk = if len - offset > mp as usize { mp as usize } else { len - offset };
        let t = if prev_td.is_null() { td } else { prev_td.add(1) };

        ptr::write_volatile(ptr::addr_of_mut!((*t).link), LK_T);
        ptr::write_volatile(ptr::addr_of_mut!((*t).ctrl), TDA);
        ptr::write_volatile(ptr::addr_of_mut!((*t).token),
            tkn(PID_IN, addr, eid, (mp - 1) as u32, toggle));
        ptr::write_volatile(ptr::addr_of_mut!((*t).buf), WPP + 2048);

        if prev_td.is_null() { first_td = t; }
        else { ptr::write_volatile(ptr::addr_of_mut!((*prev_td).link), lk(tdp + ((t as u32 - WPP) as i32) as u32, false, false, false)); }
        prev_td = t;
        toggle ^= 1;
        offset += chunk;
    }

    if first_td == td {
        ptr::write_volatile(ptr::addr_of_mut!((*td).link), LK_T);
        ptr::write_volatile(ptr::addr_of_mut!((*td).ctrl), TDA | TDIOC);
        ptr::write_volatile(ptr::addr_of_mut!((*td).token),
            tkn(PID_IN, addr, eid, (mp - 1) as u32, 0));
        ptr::write_volatile(ptr::addr_of_mut!((*td).buf), WPP + 2048);
    } else {
        ptr::write_volatile(ptr::addr_of_mut!((*prev_td).ctrl),
            ptr::read_volatile(ptr::addr_of!((*prev_td).ctrl)) | TDIOC);
    }

    for i in 0..1024 { ptr::write_volatile(FL.add(i), lk(tdp, false, false, false)); }
    let ok = wait_td(first_td as *const Td);
    for i in 0..1024 { ptr::write_volatile(FL.add(i), LK_T); }

    if ok {
        let mut copied = 0usize;
        let mut src_off = 0usize;
        while copied < len {
            let chunk = if len - copied > mp as usize { mp as usize } else { len - copied };
            ptr::copy_nonoverlapping(WP.add(2048 + src_off), buf.add(copied), chunk);
            copied += chunk;
            src_off += mp as usize;
        }
    }
    ok
}

#[no_mangle]
pub unsafe extern "C" fn krust_usb_enumerate_storage() {
    for i in 0..NDEV {
        if !DEVS[i].active { continue; }
        let addr = DEVS[i].addr;
        let dbuf = WP.add(2048);
        if !cfg_desc(addr, dbuf, 9) { continue; }
        let total = (*(dbuf as *const CfgDesc)).total;
        if total > 500 { continue; }
        if !cfg_desc(addr, dbuf, total) { continue; }

        let mut off: usize = 0;
        while off + 2 < total as usize {
            let len = *dbuf.add(off) as usize;
            let typ = *dbuf.add(off + 1);
            if len == 0 || off + len > total as usize { break; }
            if typ == (DT_IFACE as u8) {
                let iface = &*(dbuf.add(off) as *const IfDesc);
                if iface.cls == crate::usb_storage::MSC_CLASS && iface.sub == crate::usb_storage::MSC_SUBCLASS_SCSI && iface.proto == crate::usb_storage::MSC_PROTO_BBB {
                    let mut ep_in: u8 = 0;
                    let mut ep_out: u8 = 0;
                    let mut max_pkt: u16 = 512;
                    let mut eoff = off + iface.len as usize;
                    while eoff + 2 < total as usize {
                        let elen = *dbuf.add(eoff) as usize;
                        let etyp = *dbuf.add(eoff + 1);
                        if elen == 0 || eoff + elen > total as usize { break; }
                        if etyp == (DT_ENDP as u8) {
                            let ep = &*(dbuf.add(eoff) as *const EpDesc);
                            if ep.addr & 0x80 != 0 { ep_in = ep.addr; max_pkt = ep.maxsz; }
                            else { ep_out = ep.addr; }
                        }
                        eoff += elen;
                    }
                    if ep_in != 0 && ep_out != 0 {
                        crate::usb_storage::register_storage(addr, ep_in, ep_out, max_pkt);
                    }
                }
            }
            off += len;
        }
    }
}

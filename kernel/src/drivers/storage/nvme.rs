

use crate::pci::PCI;

// NVMe controller registers (offsets from BAR0)
const REG_CAP: u16 = 0x00;   // Controller Capabilities
const REG_VER: u16 = 0x08;   // Version
const REG_INTMS: u16 = 0x0C; // Interrupt Mask Set
const REG_INTMC: u16 = 0x10; // Interrupt Mask Clear
const REG_CC: u16 = 0x14;    // Controller Configuration
const REG_CSTS: u16 = 0x1C;  // Controller Status
const REG_AQA: u16 = 0x24;   // Admin Queue Attributes
const REG_ASQ: u16 = 0x28;   // Admin Submission Queue Base Address
const REG_ACQ: u16 = 0x30;   // Admin Completion Queue Base Address

// CC register bits
const CC_EN: u32 = 1 << 0;   // Enable
const CC_CSS_NVM: u32 = 0 << 4; // NVM Command Set
const CC_MPS_SHIFT: u32 = 7;
const CC_SHN_NONE: u32 = 0 << 14;
const CC_IOSQES_SHIFT: u32 = 16;
const CC_IOCQES_SHIFT: u32 = 20;

// CSTS register bits
const CSTS_RDY: u32 = 1 << 0;
const CSTS_CFS: u32 = 1 << 1;

// Submission queue entry opcodes
const ADMIN_CREATE_IO_SQ: u8 = 0x01;
const ADMIN_CREATE_IO_CQ: u8 = 0x05;
const ADMIN_IDENTIFY: u8 = 0x06;

// I/O command opcodes
const IO_READ: u8 = 0x02;
const IO_WRITE: u8 = 0x01;

// Identify CNS values
const CNS_IDENTIFY_NAMESPACE: u32 = 0;
const CNS_IDENTIFY_CONTROLLER: u32 = 1;

#[repr(C, packed)]
struct NvmeSubmissionEntry {
    cdw0: u32,      // opcode(8) + flags(8) + psdt(2) + fused(2) + rsvd(12)
    nsid: u32,      // Namespace Identifier
    cdw2: u32,
    cdw3: u32,
    mptr: u64,      // Metadata Pointer
    prp1: u64,      // PRP Entry 1
    prp2: u64,      // PRP Entry 2
    cdw10: u32,
    cdw11: u32,
    cdw12: u32,
    cdw13: u32,
    cdw14: u32,
    cdw15: u32,
}

#[repr(C, packed)]
struct NvmeCompletionEntry {
    result: u32,    // Command Specific
    rsvd: u32,
    sq_head: u16,
    sq_id: u16,
    command_id: u16,
    status: u16,    // P(1) + SC(8) + SCT(3) + CRD(2) + M(1) + DNR(1)
}

// NVMe uses crate::paging and crate::pmm directly

pub struct NvmeDevice {
    mmio_base: *mut u8,
    admin_sq: *mut NvmeSubmissionEntry,
    admin_cq: *mut NvmeCompletionEntry,
    admin_sq_phys: u64,
    admin_cq_phys: u64,
    admin_sq_tail: u16,
    admin_cq_head: u16,
    admin_cmd_id: u16,
    // I/O queues
    io_sq: *mut NvmeSubmissionEntry,
    io_cq: *mut NvmeCompletionEntry,
    io_sq_phys: u64,
    io_cq_phys: u64,
    io_sq_tail: u16,
    io_cq_head: u16,
    io_cmd_id: u16,
    // Namespace info
    ns_id: u32,
    block_size: u32,
    block_count: u64,
    initialized: bool,
}

unsafe impl Send for NvmeDevice {}

impl NvmeDevice {
    pub fn new() -> Self {
        Self {
            mmio_base: core::ptr::null_mut(),
            admin_sq: core::ptr::null_mut(),
            admin_cq: core::ptr::null_mut(),
            admin_sq_phys: 0,
            admin_cq_phys: 0,
            admin_sq_tail: 0,
            admin_cq_head: 0,
            admin_cmd_id: 0,
            io_sq: core::ptr::null_mut(),
            io_cq: core::ptr::null_mut(),
            io_sq_phys: 0,
            io_cq_phys: 0,
            io_sq_tail: 0,
            io_cq_head: 0,
            io_cmd_id: 0,
            ns_id: 1,
            block_size: 512,
            block_count: 0,
            initialized: false,
        }
    }

    fn mmio_read(&self, reg: u16) -> u32 {
        unsafe {
            let ptr = self.mmio_base.add(reg as usize) as *const u32;
            core::ptr::read_volatile(ptr)
        }
    }

    fn mmio_write(&self, reg: u16, val: u32) {
        unsafe {
            let ptr = self.mmio_base.add(reg as usize) as *mut u32;
            core::ptr::write_volatile(ptr, val);
        }
    }

    fn mmio_read64(&self, reg: u16) -> u64 {
        unsafe {
            let ptr = self.mmio_base.add(reg as usize) as *const u64;
            core::ptr::read_volatile(ptr)
        }
    }

    fn mmio_write64(&self, reg: u16, val: u64) {
        unsafe {
            let ptr = self.mmio_base.add(reg as usize) as *mut u64;
            core::ptr::write_volatile(ptr, val);
        }
    }

    fn doorbell_write(&self, sqid: u16, value: u16) {
        unsafe {
            // Doorbell offset = 0x1000 + (2 * sqid + 0) * (4 << CAP.DSTRD)
            let cap = self.mmio_read64(REG_CAP);
            let dstrd = ((cap >> 32) >> 24) & 0xF; // DSTRD field
            let offset = 0x1000 + (2 * sqid as u64) * (4 << dstrd);
            let ptr = self.mmio_base.add(offset as usize) as *mut u32;
            core::ptr::write_volatile(ptr, value as u32);
        }
    }

    fn cpl_doorbell_write(&self, cqid: u16, value: u16) {
        unsafe {
            let cap = self.mmio_read64(REG_CAP);
            let dstrd = ((cap >> 32) >> 24) & 0xF;
            let offset = 0x1000 + (2 * cqid as u64 + 1) * (4 << dstrd);
            let ptr = self.mmio_base.add(offset as usize) as *mut u32;
            core::ptr::write_volatile(ptr, value as u32);
        }
    }

    fn submit_admin_cmd(&mut self, cmd: &NvmeSubmissionEntry, timeout: u32) -> Option<NvmeCompletionEntry> {
        unsafe {
            let tail = self.admin_sq_tail as usize;
            core::ptr::copy_nonoverlapping(
                cmd as *const NvmeSubmissionEntry,
                self.admin_sq.add(tail),
                1,
            );
            self.admin_sq_tail = self.admin_sq_tail.wrapping_add(1);
            self.doorbell_write(0, self.admin_sq_tail);

            // Poll for completion
            for _ in 0..timeout {
                let cpl = &*self.admin_cq.add(self.admin_cq_head as usize);
                if cpl.status & 0x0001 != 0 { // P bit set = valid
                    let result = core::ptr::read(cpl);
                    self.admin_cq_head = self.admin_cq_head.wrapping_add(1);
                    self.cpl_doorbell_write(0, self.admin_cq_head);
                    return Some(result);
                }
            }
            None
        }
    }

    fn submit_io_cmd(&mut self, cmd: &NvmeSubmissionEntry, timeout: u32) -> Option<NvmeCompletionEntry> {
        unsafe {
            let tail = self.io_sq_tail as usize;
            core::ptr::copy_nonoverlapping(
                cmd as *const NvmeSubmissionEntry,
                self.io_sq.add(tail),
                1,
            );
            self.io_sq_tail = self.io_sq_tail.wrapping_add(1);
            self.doorbell_write(1, self.io_sq_tail);

            for _ in 0..timeout {
                let cpl = &*self.io_cq.add(self.io_cq_head as usize);
                if cpl.status & 0x0001 != 0 {
                    let result = core::ptr::read(cpl);
                    self.io_cq_head = self.io_cq_head.wrapping_add(1);
                    self.cpl_doorbell_write(1, self.io_cq_head);
                    return Some(result);
                }
            }
            None
        }
    }

    fn identify_controller(&mut self, buffer: *mut u8) -> bool {
        let buffer_phys = unsafe { crate::paging::krust_paging_get_phys(buffer as u64) };
        let mut cmd: NvmeSubmissionEntry = unsafe { core::mem::zeroed() };
        cmd.cdw0 = (ADMIN_IDENTIFY as u32) | (0 << 16); // CDW0.OPC
        cmd.nsid = 0;
        cmd.prp1 = buffer_phys;
        cmd.prp2 = 0;
        cmd.cdw10 = CNS_IDENTIFY_CONTROLLER;

        if let Some(cpl) = self.submit_admin_cmd(&cmd, 100000) {
            (cpl.status >> 1) & 0xFF == 0 // SC == 0 means success
        } else {
            false
        }
    }

    fn identify_namespace(&mut self, nsid: u32, buffer: *mut u8) -> bool {
        let buffer_phys = unsafe { crate::paging::krust_paging_get_phys(buffer as u64) };
        let mut cmd: NvmeSubmissionEntry = unsafe { core::mem::zeroed() };
        cmd.cdw0 = ADMIN_IDENTIFY as u32;
        cmd.nsid = nsid;
        cmd.prp1 = buffer_phys;
        cmd.prp2 = 0;
        cmd.cdw10 = CNS_IDENTIFY_NAMESPACE;

        if let Some(cpl) = self.submit_admin_cmd(&cmd, 100000) {
            (cpl.status >> 1) & 0xFF == 0
        } else {
            false
        }
    }

    fn create_io_queues(&mut self) -> bool {
        // Allocate I/O submission queue
        self.io_sq = unsafe { crate::pmm::krust_pmm_alloc_frame() as *mut NvmeSubmissionEntry };
        if self.io_sq.is_null() { return false; }
        unsafe {
            core::ptr::write_bytes(self.io_sq, 0, 1);
            self.io_sq_phys = crate::paging::krust_paging_get_phys(self.io_sq as u64);
        }

        // Allocate I/O completion queue
        self.io_cq = unsafe { crate::pmm::krust_pmm_alloc_frame() as *mut NvmeCompletionEntry };
        if self.io_cq.is_null() { return false; }
        unsafe {
            core::ptr::write_bytes(self.io_cq, 0, 1);
            self.io_cq_phys = crate::paging::krust_paging_get_phys(self.io_cq as u64);
        }

        // Create I/O Completion Queue (CQID=1)
        {
            let mut cmd: NvmeSubmissionEntry = unsafe { core::mem::zeroed() };
            cmd.cdw0 = ADMIN_CREATE_IO_CQ as u32;
            cmd.prp1 = self.io_cq_phys;
            cmd.cdw10 = 0 | ((4096 / 64 - 1) << 16); // QID=1, QSIZE=entries-1
            cmd.cdw11 = 0x0001; // PC=1 (physically contiguous), IV=0
            if let Some(cpl) = self.submit_admin_cmd(&cmd, 100000) {
                if (cpl.status >> 1) & 0xFF != 0 { return false; }
            } else {
                return false;
            }
        }

        // Create I/O Submission Queue (SQID=1, associated with CQID=1)
        {
            let mut cmd: NvmeSubmissionEntry = unsafe { core::mem::zeroed() };
            cmd.cdw0 = ADMIN_CREATE_IO_SQ as u32;
            cmd.prp1 = self.io_sq_phys;
            cmd.cdw10 = 0 | ((4096 / 64 - 1) << 16); // QID=1, QSIZE=entries-1
            cmd.cdw11 = 0x0001 | (1 << 16); // PC=1, CQID=1
            if let Some(cpl) = self.submit_admin_cmd(&cmd, 100000) {
                if (cpl.status >> 1) & 0xFF != 0 { return false; }
            } else {
                return false;
            }
        }

        true
    }

    pub fn init(&mut self, mmio_phys: u64) -> bool {
        // Map MMIO registers
        let vaddr = unsafe { crate::paging::krust_map_mmio(mmio_phys, 0x10000) };
        self.mmio_base = vaddr as *mut u8;
        if self.mmio_base.is_null() { return false; }

        // Disable controller
        let cc = self.mmio_read(REG_CC);
        if cc & CC_EN != 0 {
            self.mmio_write(REG_CC, cc & !CC_EN);
            // Wait for CSTS.RDY = 0
            for _ in 0..100000 {
                if self.mmio_read(REG_CSTS) & CSTS_RDY == 0 { break; }
            }
        }

        // Allocate admin submission queue (4KB page)
        self.admin_sq = unsafe { crate::pmm::krust_pmm_alloc_frame() as *mut NvmeSubmissionEntry };
        if self.admin_sq.is_null() { return false; }
        unsafe {
            core::ptr::write_bytes(self.admin_sq, 0, 1);
            self.admin_sq_phys = crate::paging::krust_paging_get_phys(self.admin_sq as u64);
        }

        // Allocate admin completion queue (4KB page)
        self.admin_cq = unsafe { crate::pmm::krust_pmm_alloc_frame() as *mut NvmeCompletionEntry };
        if self.admin_cq.is_null() { return false; }
        unsafe {
            core::ptr::write_bytes(self.admin_cq, 0, 1);
            self.admin_cq_phys = crate::paging::krust_paging_get_phys(self.admin_cq as u64);
        }

        // Set AQA (Admin Queue Attributes): ASQS=0 (4096/64-1), ACQS=0
        self.mmio_write(REG_AQA, 0 | (0 << 16)); // 1 entry each (simplified)

        // Set ASQ and ACQ base addresses
        self.mmio_write64(REG_ASQ, self.admin_sq_phys);
        self.mmio_write64(REG_ACQ, self.admin_cq_phys);

        // Configure controller: enable, NVM command set, 4K page size
        let mut new_cc = CC_EN | CC_CSS_NVM;
        new_cc |= 0 << CC_MPS_SHIFT;    // MPS=0 => 4KB pages
        new_cc |= 6 << CC_IOSQES_SHIFT; // IOSQES=64 (2^6)
        new_cc |= 4 << CC_IOCQES_SHIFT; // IOCQES=16 (2^4)
        self.mmio_write(REG_CC, new_cc);

        // Wait for CSTS.RDY = 1
        for _ in 0..100000 {
            if self.mmio_read(REG_CSTS) & CSTS_RDY != 0 { break; }
        }

        // Identify controller (get page size info, etc.)
        let ctrl_id_buf = unsafe { crate::pmm::krust_pmm_alloc_frame() as *mut u8 };
        if ctrl_id_buf.is_null() { return false; }
        if !self.identify_controller(ctrl_id_buf) {
            return false;
        }

        // Identify namespace 1
        let ns_buf = unsafe { crate::pmm::krust_pmm_alloc_frame() as *mut u8 };
        if ns_buf.is_null() { return false; }
        if !self.identify_namespace(1, ns_buf) {
            return false;
        }

        // Parse namespace data
        unsafe {
            let ns_data = ns_buf as *const u8;
            // Namespace features (offset 0): FLBAS
            let flbas = *ns_data.add(26) as u32;
            let _ = flbas;

            // LBA format (offset 128): LBADS field (bits 23:16)
            let lba_fmt = *(ns_data.add(128) as *const u32);
            let lbads = (lba_fmt >> 16) & 0xFF;
            self.block_size = 1u32 << lbads; // 2^lbads

            // Namespace size (offset 0 in ns data, 8 bytes)
            self.block_count = *(ns_data as *const u64);

            // Free the buffers
            crate::pmm::krust_pmm_free_frame((ctrl_id_buf as u64 / 4096) as usize);
            crate::pmm::krust_pmm_free_frame((ns_buf as u64 / 4096) as usize);
        }

        // Create I/O queues
        if !self.create_io_queues() {
            return false;
        }

        self.initialized = true;
        true
    }

    pub fn read_sectors(&mut self, lba: u64, count: u32, buffer: *mut u8) -> Result<(), ()> {
        if !self.initialized { return Err(()); }

        let buf_phys = unsafe { crate::paging::krust_paging_get_phys(buffer as u64) };

        let mut cmd: NvmeSubmissionEntry = unsafe { core::mem::zeroed() };
        cmd.cdw0 = IO_READ as u32;
        cmd.nsid = self.ns_id;
        cmd.prp1 = buf_phys;
        cmd.prp2 = 0;
        cmd.cdw10 = lba as u32;
        cmd.cdw11 = (lba >> 32) as u32;
        cmd.cdw12 = count - 1; // NLB (0-based)

        if let Some(cpl) = self.submit_io_cmd(&cmd, 1000000) {
            if (cpl.status >> 1) & 0xFF == 0 {
                Ok(())
            } else {
                Err(())
            }
        } else {
            Err(())
        }
    }

    pub fn write_sectors(&mut self, lba: u64, count: u32, buffer: *const u8) -> Result<(), ()> {
        if !self.initialized { return Err(()); }

        let buf_phys = unsafe { crate::paging::krust_paging_get_phys(buffer as u64) };

        let mut cmd: NvmeSubmissionEntry = unsafe { core::mem::zeroed() };
        cmd.cdw0 = IO_WRITE as u32;
        cmd.nsid = self.ns_id;
        cmd.prp1 = buf_phys;
        cmd.prp2 = 0;
        cmd.cdw10 = lba as u32;
        cmd.cdw11 = (lba >> 32) as u32;
        cmd.cdw12 = count - 1;

        if let Some(cpl) = self.submit_io_cmd(&cmd, 1000000) {
            if (cpl.status >> 1) & 0xFF == 0 {
                Ok(())
            } else {
                Err(())
            }
        } else {
            Err(())
        }
    }

    pub fn block_size(&self) -> u32 { self.block_size }
    pub fn block_count(&self) -> u64 { self.block_count }
    pub fn is_initialized(&self) -> bool { self.initialized }
}

static mut NVME_DEVICE: Option<NvmeDevice> = None;

pub fn nvme_init() -> bool {
    // NVMe is class 0x01, subclass 0x08
    if let Some(dev) = PCI::enumerate_class(0x01, 0x08) {
        let bar0 = PCI::read_bar(dev.bus, dev.slot, dev.func, 0);
        let mmio_phys = (bar0 & 0xFFFFFFF0) as u64;

        PCI::enable_bus_mastering(dev.bus, dev.slot, dev.func);

        let mut nvme = NvmeDevice::new();
        if nvme.init(mmio_phys) {
            unsafe { NVME_DEVICE = Some(nvme); }
            true
        } else {
            false
        }
    } else {
        false
    }
}

pub fn nvme_read(lba: u64, count: u32, buffer: *mut u8) -> i32 {
    unsafe {
        if let Some(ref mut dev) = NVME_DEVICE {
            if dev.read_sectors(lba, count, buffer).is_ok() { 0 } else { -1 }
        } else {
            -1
        }
    }
}

pub fn nvme_write(lba: u64, count: u32, buffer: *const u8) -> i32 {
    unsafe {
        if let Some(ref mut dev) = NVME_DEVICE {
            if dev.write_sectors(lba, count, buffer).is_ok() { 0 } else { -1 }
        } else {
            -1
        }
    }
}

pub fn nvme_block_size() -> u32 {
    unsafe {
        if let Some(ref dev) = NVME_DEVICE { dev.block_size() } else { 0 }
    }
}

pub fn nvme_block_count() -> u64 {
    unsafe {
        if let Some(ref dev) = NVME_DEVICE { dev.block_count() } else { 0 }
    }
}

pub fn nvme_is_ready() -> bool {
    unsafe {
        if let Some(ref dev) = NVME_DEVICE { dev.is_initialized() } else { false }
    }
}

// ─── Block device / VFS integration ──────────────────────────

use crate::heap::krust_malloc;
use crate::klib::krust_memset;
use crate::ns16550;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct NvmePartition {
    pub valid: bool,
    pub type_: u8,
    pub lba_start: u64,
    pub sector_count: u64,
}

static mut NVME_PART_BUF: *mut u8 = core::ptr::null_mut();
static mut NVME_PART_LBA_START: u64 = 0;
static mut NVME_PART_SECTORS: u64 = 0;
static mut NVME_DIRTY_BITS: *mut u8 = core::ptr::null_mut();
static mut NVME_DIRTY_SIZE: u32 = 0;

pub fn nvme_read_sector(lba: u64, buf: *mut u8) -> bool {
    unsafe {
        if let Some(ref mut dev) = NVME_DEVICE {
            dev.read_sectors(lba, 1, buf).is_ok()
        } else {
            false
        }
    }
}

pub fn nvme_write_sector(lba: u64, buf: *const u8) -> bool {
    unsafe {
        if let Some(ref mut dev) = NVME_DEVICE {
            dev.write_sectors(lba, 1, buf).is_ok()
        } else {
            false
        }
    }
}

pub fn nvme_find_partitions(parts: *mut NvmePartition, max_parts: i32) -> i32 {
    if parts.is_null() || max_parts < 1 { return 0; }

    let mut mbr: [u8; 512] = [0; 512];
    if !nvme_read_sector(0, mbr.as_mut_ptr()) { return 0; }

    if mbr[510] != 0x55 || mbr[511] != 0xAA { return 0; }

    let mut found = 0i32;
    for i in 0..4 {
        if found >= max_parts { break; }
        let entry = &mbr[0x1BE + i * 16..];
        let type_ = entry[4];
        if type_ == 0x0B || type_ == 0x0C {
            let p = unsafe { &mut *parts.add(found as usize) };
            p.valid = true;
            p.type_ = type_;
            p.lba_start = u32::from_le_bytes([
                entry[8], entry[9], entry[10], entry[11],
            ]) as u64;
            p.sector_count = u32::from_le_bytes([
                entry[12], entry[13], entry[14], entry[15],
            ]) as u64;
            found += 1;
        }
    }

    found
}

pub fn nvme_mount_partition_buffer(lba_start: u64, sectors: u64, buffer: *mut u8) -> bool {
    if buffer.is_null() || sectors == 0 { return false; }
    unsafe {
        NVME_PART_BUF = buffer;
        NVME_PART_LBA_START = lba_start;
        NVME_PART_SECTORS = sectors;

        NVME_DIRTY_SIZE = ((sectors + 7) / 8) as u32;
        NVME_DIRTY_BITS = krust_malloc(NVME_DIRTY_SIZE);
        if NVME_DIRTY_BITS.is_null() {
            NVME_PART_BUF = core::ptr::null_mut();
            return false;
        }
        krust_memset(NVME_DIRTY_BITS, 0, NVME_DIRTY_SIZE as usize);
    }
    true
}

pub fn nvme_mark_dirty(byte_offset: u32, size: u32) {
    unsafe {
        if NVME_PART_BUF.is_null() || NVME_DIRTY_BITS.is_null() { return; }

        let start = byte_offset / 512;
        let mut end = (byte_offset + size + 511) / 512;
        if end as u64 > NVME_PART_SECTORS { end = NVME_PART_SECTORS as u32; }

        for s in start..end {
            let idx = (s / 8) as usize;
            let bit = 1 << (s % 8);
            core::ptr::write(
                NVME_DIRTY_BITS.add(idx),
                core::ptr::read(NVME_DIRTY_BITS.add(idx)) | bit,
            );
        }
    }
}

pub fn nvme_flush() {
    unsafe {
        if NVME_PART_BUF.is_null() || NVME_DIRTY_BITS.is_null() { return; }

        let mut flushed = 0u32;
        for s in 0..NVME_PART_SECTORS {
            let idx = (s / 8) as usize;
            let bit = 1 << (s % 8);
            if core::ptr::read(NVME_DIRTY_BITS.add(idx)) & bit != 0 {
                if nvme_write_sector(
                    NVME_PART_LBA_START + s,
                    NVME_PART_BUF.add((s * 512) as usize),
                ) {
                    core::ptr::write(
                        NVME_DIRTY_BITS.add(idx),
                        core::ptr::read(NVME_DIRTY_BITS.add(idx)) & !bit,
                    );
                    flushed += 1;
                }
            }
        }

        if flushed > 0 {
            ns16550::krust_ns16550_write_str(b"nvme: flushed \0".as_ptr());
            let mut buf = [0u8; 12];
            let mut tmp = flushed;
            let mut i = 10;
            buf[11] = 0;
            if tmp == 0 { buf[i] = b'0'; i -= 1; }
            while tmp > 0 { buf[i] = b'0' + (tmp % 10) as u8; tmp /= 10; i -= 1; }
            ns16550::krust_ns16550_write_str(buf.as_ptr().add(i + 1));
            ns16550::krust_ns16550_write_str(b" sectors\n\0".as_ptr());
        }
    }
}

// ─── C-compatible wrappers ───────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_nvme_read(lba_lo: u64, count: u32, buffer: *mut u8) -> i32 {
    nvme_read(lba_lo, count, buffer)
}

#[no_mangle]
pub unsafe extern "C" fn krust_nvme_write(lba_lo: u64, count: u32, buffer: *const u8) -> i32 {
    nvme_write(lba_lo, count, buffer)
}

#[no_mangle]
pub unsafe extern "C" fn krust_nvme_block_size() -> u32 {
    nvme_block_size()
}

#[no_mangle]
pub unsafe extern "C" fn krust_nvme_block_count() -> u64 {
    nvme_block_count()
}

#[no_mangle]
pub unsafe extern "C" fn krust_nvme_is_ready() -> bool {
    nvme_is_ready()
}

#[no_mangle]
pub unsafe extern "C" fn krust_nvme_find_partitions(
    parts: *mut NvmePartition,
    max_parts: i32,
) -> i32 {
    nvme_find_partitions(parts, max_parts)
}

#[no_mangle]
pub unsafe extern "C" fn krust_nvme_mount_partition_buffer(
    lba_start: u64,
    sectors: u64,
    buffer: *mut u8,
) -> bool {
    nvme_mount_partition_buffer(lba_start, sectors, buffer)
}

#[no_mangle]
pub unsafe extern "C" fn krust_nvme_mark_dirty(byte_offset: u32, size: u32) {
    nvme_mark_dirty(byte_offset, size);
}

#[no_mangle]
pub unsafe extern "C" fn krust_nvme_flush() {
    nvme_flush();
}

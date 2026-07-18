use core::ptr;

const VIRTIO_CONFIG_S_ACKNOWLEDGE: u8 = 1;
const VIRTIO_CONFIG_S_DRIVER: u8 = 2;
const VIRTIO_CONFIG_S_DRIVER_OK: u8 = 4;
const VIRTIO_CONFIG_S_FEATURES_OK: u8 = 8;

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;

const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

const VIRTIO_BLK_F_BLK_SIZE: u32 = 6;

const QUEUE_SIZE: usize = 128;

#[repr(C)]
struct VirtIOBlkReqHeader {
    type_: u32,
    reserved: u32,
    sector: u64,
}

#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; QUEUE_SIZE],
}

#[repr(C)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; QUEUE_SIZE],
}

pub struct VirtIOBlkDevice {
    mmio_base: *mut u8,
    capacity: u64,
    block_size: u32,
    queue_ready: bool,
    desc: *mut VirtqDesc,
    avail: *mut VirtqAvail,
    used: *mut VirtqUsed,
    free_bitmap: [u16; QUEUE_SIZE],
    free_head: u16,
    num_free: u16,
    last_used: u16,
}

unsafe impl Send for VirtIOBlkDevice {}

impl VirtIOBlkDevice {
    pub fn new() -> Self {
        Self {
            mmio_base: core::ptr::null_mut(),
            capacity: 0,
            block_size: 512,
            queue_ready: false,
            desc: core::ptr::null_mut(),
            avail: core::ptr::null_mut(),
            used: core::ptr::null_mut(),
            free_bitmap: [0; QUEUE_SIZE],
            free_head: 0,
            num_free: QUEUE_SIZE as u16,
            last_used: 0,
        }
    }

    unsafe fn read32(&self, offset: u64) -> u32 {
        ptr::read_volatile(self.mmio_base.add(offset as usize) as *const u32)
    }

    unsafe fn write32(&self, offset: u64, val: u32) {
        ptr::write_volatile(self.mmio_base.add(offset as usize) as *mut u32, val);
    }

    unsafe fn write8(&self, offset: u64, val: u8) {
        ptr::write_volatile(self.mmio_base.add(offset as usize) as *mut u8, val);
    }

    fn init_free_list(&mut self) {
        for i in 0..(QUEUE_SIZE - 1) {
            self.free_bitmap[i] = (i + 1) as u16;
        }
        self.free_bitmap[QUEUE_SIZE - 1] = 0xFFFF;
        self.free_head = 0;
        self.num_free = QUEUE_SIZE as u16;
    }

    fn alloc_desc(&mut self) -> Option<u16> {
        if self.num_free == 0 { return None; }
        let idx = self.free_head;
        self.free_head = self.free_bitmap[idx as usize];
        self.free_bitmap[idx as usize] = 0xFFFF;
        self.num_free -= 1;
        Some(idx)
    }

    fn free_desc(&mut self, idx: u16) {
        self.free_bitmap[idx as usize] = self.free_head;
        self.free_head = idx;
        self.num_free += 1;
    }

    pub fn init(&mut self, mmio_phys: u64) -> bool {
        unsafe {
            self.mmio_base = crate::paging::krust_map_mmio(mmio_phys, 0x200) as *mut u8;
            if self.mmio_base.is_null() { return false; }

            self.write32(0x00, 0);
            core::hint::spin_loop();

            self.write8(0x03, VIRTIO_CONFIG_S_ACKNOWLEDGE);
            self.write8(0x03, VIRTIO_CONFIG_S_ACKNOWLEDGE | VIRTIO_CONFIG_S_DRIVER);

            let features = self.read32(0x04);
            if features & (1 << VIRTIO_BLK_F_BLK_SIZE) != 0 {
                self.block_size = self.read32(0x24);
                if self.block_size == 0 { self.block_size = 512; }
            }

            self.init_free_list();

            self.write32(0x08, 0);
            core::hint::spin_loop();

            let desc_size = core::mem::size_of::<VirtqDesc>() * QUEUE_SIZE;
            let avail_size = 4 + QUEUE_SIZE * 2;
            let used_size = 4 + QUEUE_SIZE * core::mem::size_of::<VirtqUsedElem>();
            let total = desc_size + avail_size + used_size;
            let frame = crate::pmm::krust_pmm_alloc_frame();
            let buf_phys = frame as u64 * 0x1000;
            if buf_phys == 0 { return false; }

            let buf_virt = crate::paging::krust_map_mmio(buf_phys, (total as u64 + 0xFFF) & !0xFFF) as *mut u8;
            if buf_virt.is_null() { return false; }
            ptr::write_bytes(buf_virt, 0, total);

            self.desc = buf_virt as *mut VirtqDesc;
            self.avail = buf_virt.add(desc_size) as *mut VirtqAvail;
            self.used = buf_virt.add(desc_size + avail_size) as *mut VirtqUsed;

            self.write32(0x08, desc_size as u32);
            self.write32(0x0C, (buf_phys >> 32) as u32);
            self.write32(0x10, buf_phys as u32);
            self.write32(0x14, avail_size as u32);
            self.write32(0x18, ((buf_phys + desc_size as u64) >> 32) as u32);
            self.write32(0x1C, (buf_phys + desc_size as u64) as u32);
            self.write32(0x20, used_size as u32);
            self.write32(0x24, ((buf_phys + (desc_size + avail_size) as u64) >> 32) as u32);
            self.write32(0x28, (buf_phys + (desc_size + avail_size) as u64) as u32);

            self.write32(0x08, 0);
            self.write32(0x08, 1);

            self.write8(0x03, VIRTIO_CONFIG_S_ACKNOWLEDGE | VIRTIO_CONFIG_S_DRIVER
                | VIRTIO_CONFIG_S_DRIVER_OK | VIRTIO_CONFIG_S_FEATURES_OK);
            self.queue_ready = true;
        }
        true
    }

    fn submit_wait(&mut self, head: u16) {
        unsafe {
            let avail_idx = ptr::read_volatile(&(*self.avail).idx);
            ptr::write_volatile(
                &mut (*self.avail).ring[(avail_idx as usize) % QUEUE_SIZE],
                head,
            );
            ptr::write_volatile(&mut (*self.avail).idx, avail_idx.wrapping_add(1));
            self.write32(0x50, 0);

            let mut spins = 0u32;
            loop {
                let used_idx = ptr::read_volatile(&(*self.used).idx);
                if used_idx != self.last_used { break; }
                core::hint::spin_loop();
                spins += 1;
                if spins > 10_000_000 { break; }
            }

            let used_idx = ptr::read_volatile(&(*self.used).idx);
            while self.last_used != used_idx {
                let elem = &(*self.used).ring[(self.last_used as usize) % QUEUE_SIZE];
                let mut cur = elem.id as u16;
                loop {
                    let flags = (*self.desc.add(cur as usize)).flags;
                    let next = (*self.desc.add(cur as usize)).next;
                    self.free_desc(cur);
                    if flags & VIRTQ_DESC_F_NEXT == 0 { break; }
                    cur = next;
                }
                self.last_used = self.last_used.wrapping_add(1);
            }
        }
    }

    pub fn read_sectors(&mut self, sector: u64, buf: &mut [u8]) -> bool {
        if !self.queue_ready { return false; }

        let head_desc = match self.alloc_desc() {
            Some(i) => i,
            None => return false,
        };
        let frame = unsafe { crate::pmm::krust_pmm_alloc_frame() } as u64 * 0x1000;
        if frame == 0 { self.free_desc(head_desc); return false; }
        let virt = unsafe { crate::paging::krust_map_mmio(frame, 0x1000) } as *mut u8;
        if virt.is_null() { self.free_desc(head_desc); return false; }

        unsafe {
            let hdr = virt as *mut VirtIOBlkReqHeader;
            ptr::write_volatile(&mut (*hdr).type_, VIRTIO_BLK_T_IN);
            ptr::write_volatile(&mut (*hdr).reserved, 0);
            ptr::write_volatile(&mut (*hdr).sector, sector);

            (*self.desc.add(head_desc as usize)).addr = frame;
            (*self.desc.add(head_desc as usize)).len = core::mem::size_of::<VirtIOBlkReqHeader>() as u32;
            (*self.desc.add(head_desc as usize)).flags = VIRTQ_DESC_F_NEXT;
        }

        let data_desc = match self.alloc_desc() {
            Some(i) => i,
            None => { self.free_desc(head_desc); return false; }
        };
        let data_frame = unsafe { crate::pmm::krust_pmm_alloc_frame() } as u64 * 0x1000;
        if data_frame == 0 {
            self.free_desc(data_desc);
            self.free_desc(head_desc);
            return false;
        }
        let data_virt = unsafe { crate::paging::krust_map_mmio(data_frame, 0x1000) } as *mut u8;

        unsafe {
            (*self.desc.add(data_desc as usize)).addr = data_frame;
            (*self.desc.add(data_desc as usize)).len = buf.len() as u32;
            (*self.desc.add(data_desc as usize)).flags = VIRTQ_DESC_F_WRITE;
            (*self.desc.add(data_desc as usize)).next = 0;
            (*self.desc.add(head_desc as usize)).next = data_desc;
        }

        self.submit_wait(head_desc);

        if !data_virt.is_null() {
            let src = unsafe { core::slice::from_raw_parts(data_virt, buf.len()) };
            buf.copy_from_slice(src);
        }
        true
    }

    pub fn write_sectors(&mut self, sector: u64, buf: &[u8]) -> bool {
        if !self.queue_ready { return false; }

        let head_desc = match self.alloc_desc() {
            Some(i) => i,
            None => return false,
        };
        let frame = unsafe { crate::pmm::krust_pmm_alloc_frame() } as u64 * 0x1000;
        if frame == 0 { self.free_desc(head_desc); return false; }
        let virt = unsafe { crate::paging::krust_map_mmio(frame, 0x1000) } as *mut u8;
        if virt.is_null() { self.free_desc(head_desc); return false; }

        unsafe {
            let hdr = virt as *mut VirtIOBlkReqHeader;
            ptr::write_volatile(&mut (*hdr).type_, VIRTIO_BLK_T_OUT);
            ptr::write_volatile(&mut (*hdr).reserved, 0);
            ptr::write_volatile(&mut (*hdr).sector, sector);

            (*self.desc.add(head_desc as usize)).addr = frame;
            (*self.desc.add(head_desc as usize)).len = core::mem::size_of::<VirtIOBlkReqHeader>() as u32;
            (*self.desc.add(head_desc as usize)).flags = VIRTQ_DESC_F_NEXT;
        }

        let data_desc = match self.alloc_desc() {
            Some(i) => i,
            None => { self.free_desc(head_desc); return false; }
        };
        let data_frame = unsafe { crate::pmm::krust_pmm_alloc_frame() } as u64 * 0x1000;
        if data_frame == 0 {
            self.free_desc(data_desc);
            self.free_desc(head_desc);
            return false;
        }
        let data_virt = unsafe { crate::paging::krust_map_mmio(data_frame, 0x1000) } as *mut u8;
        if !data_virt.is_null() {
            unsafe {
                ptr::copy_nonoverlapping(buf.as_ptr(), data_virt, buf.len());
            }
        }

        unsafe {
            (*self.desc.add(data_desc as usize)).addr = data_frame;
            (*self.desc.add(data_desc as usize)).len = buf.len() as u32;
            (*self.desc.add(data_desc as usize)).flags = 0;
            (*self.desc.add(data_desc as usize)).next = 0;
            (*self.desc.add(head_desc as usize)).next = data_desc;
        }

        self.submit_wait(head_desc);
        true
    }

    pub fn capacity_sectors(&self) -> u64 { self.capacity }
    pub fn block_size_val(&self) -> u32 { self.block_size }
}

static mut BLK_DEV: Option<VirtIOBlkDevice> = None;

#[no_mangle]
pub unsafe extern "C" fn krust_virtio_blk_init(mmio_phys: u64, _irq: u8) -> i32 {
    let mut dev = VirtIOBlkDevice::new();
    if dev.init(mmio_phys) {
        crate::serial::krust_serial_writestring(b"VirtIO-BLK: initialized\n\0" as *const u8);
        BLK_DEV = Some(dev);
        0
    } else {
        -1
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_virtio_blk_read(sector: u64, buf: *mut u8, count: u32) -> i32 {
    if buf.is_null() { return -1; }
    if let Some(ref mut dev) = BLK_DEV {
        let slice = core::slice::from_raw_parts_mut(buf, (count as usize) * 512);
        if dev.read_sectors(sector, slice) { return 0; }
    }
    -1
}

#[no_mangle]
pub unsafe extern "C" fn krust_virtio_blk_write(sector: u64, buf: *const u8, count: u32) -> i32 {
    if buf.is_null() { return -1; }
    if let Some(ref mut dev) = BLK_DEV {
        let slice = core::slice::from_raw_parts(buf, (count as usize) * 512);
        if dev.write_sectors(sector, slice) { return 0; }
    }
    -1
}

#[no_mangle]
pub unsafe extern "C" fn krust_virtio_blk_capacity() -> u64 {
    if let Some(ref dev) = BLK_DEV { dev.capacity_sectors() } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_virtio_blk_block_size() -> u32 {
    if let Some(ref dev) = BLK_DEV { dev.block_size_val() } else { 512 }
}

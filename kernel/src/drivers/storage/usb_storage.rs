
pub const MSC_CLASS: u8 = 0x08;
pub const MSC_SUBCLASS_SCSI: u8 = 0x06;
pub const MSC_PROTO_BBB: u8 = 0x50;

const CBW_SIGNATURE: u32 = 0x43425355;
const CSW_SIGNATURE: u32 = 0x53425355;

const CBW_DATA_OUT: u8 = 0x00;
const CBW_DATA_IN: u8 = 0x80;

const CSW_STATUS_PASSED: u8 = 0x00;
const CSW_STATUS_FAILED: u8 = 0x01;
const CSW_STATUS_PHASE: u8 = 0x02;

const SCSI_CMD_INQUIRY: u8 = 0x12;
const SCSI_CMD_READ_CAPACITY: u8 = 0x25;
const SCSI_CMD_READ_10: u8 = 0x28;
const SCSI_CMD_WRITE_10: u8 = 0x2A;
const SCSI_CMD_TEST_UNIT_READY: u8 = 0x00;
const SCSI_CMD_REQUEST_SENSE: u8 = 0x03;
const SCSI_CMD_MODE_SENSE: u8 = 0x1A;

#[repr(C, packed)]
struct Cbw {
    signature: u32,
    tag: u32,
    data_length: u32,
    flags: u8,
    lun: u8,
    cb_length: u8,
    cb: [u8; 16],
}

#[repr(C, packed)]
struct Csw {
    signature: u32,
    tag: u32,
    data_residue: u32,
    status: u8,
}

#[derive(Clone, Copy)]
pub struct UsbStorageDevice {
    pub active: bool,
    pub addr: u8,
    pub ep_in: u8,
    pub ep_out: u8,
    pub max_packet: u16,
    pub block_size: u32,
    pub block_count: u64,
    pub tag: u32,
}

static mut STORAGE_DEVICES: [UsbStorageDevice; 4] = [
    UsbStorageDevice { active: false, addr: 0, ep_in: 0, ep_out: 0, max_packet: 512, block_size: 512, block_count: 0, tag: 0 },
    UsbStorageDevice { active: false, addr: 0, ep_in: 0, ep_out: 0, max_packet: 512, block_size: 512, block_count: 0, tag: 0 },
    UsbStorageDevice { active: false, addr: 0, ep_in: 0, ep_out: 0, max_packet: 512, block_size: 512, block_count: 0, tag: 0 },
    UsbStorageDevice { active: false, addr: 0, ep_in: 0, ep_out: 0, max_packet: 512, block_size: 512, block_count: 0, tag: 0 },
];
static mut NUM_STORAGE: usize = 0;

pub fn is_mass_storage(cls: u8, sub: u8, proto: u8) -> bool {
    cls == MSC_CLASS && sub == MSC_SUBCLASS_SCSI && proto == MSC_PROTO_BBB
}

pub unsafe fn register_storage(addr: u8, ep_in: u8, ep_out: u8, max_packet: u16) {
    if NUM_STORAGE >= 4 { return; }
    STORAGE_DEVICES[NUM_STORAGE] = UsbStorageDevice {
        active: true,
        addr,
        ep_in,
        ep_out,
        max_packet,
        block_size: 512,
        block_count: 0,
        tag: 1,
    };
    NUM_STORAGE += 1;
    crate::vga::krust_vga_writestring(b"USB Storage: registered device\n\0" as *const u8);
}

pub unsafe fn storage_device_count() -> usize { NUM_STORAGE }

pub unsafe fn storage_device(index: usize) -> Option<&'static mut UsbStorageDevice> {
    if index >= NUM_STORAGE { return None; }
    Some(&mut STORAGE_DEVICES[index])
}

unsafe fn build_cbw(cb: &[u8], data_len: u32, flags: u8, tag: u32) -> Cbw {
    let mut cbw = Cbw {
        signature: CBW_SIGNATURE,
        tag,
        data_length: data_len,
        flags,
        lun: 0,
        cb_length: cb.len() as u8,
        cb: [0u8; 16],
    };
    let mut i = 0;
    while i < cb.len() && i < 16 {
        cbw.cb[i] = cb[i];
        i += 1;
    }
    cbw
}

pub unsafe fn usb_stor_read(dev_idx: usize, lba: u64, count: u32, buf: *mut u8) -> i32 {
    if dev_idx >= NUM_STORAGE { return -1; }
    let dev = &STORAGE_DEVICES[dev_idx];
    if !dev.active { return -1; }

    let tag = dev.tag;
    STORAGE_DEVICES[dev_idx].tag = tag.wrapping_add(1);

    let mut cb = [0u8; 10];
    cb[0] = SCSI_CMD_READ_10;
    cb[2] = (lba >> 24) as u8;
    cb[3] = (lba >> 16) as u8;
    cb[4] = (lba >> 8) as u8;
    cb[5] = lba as u8;
    cb[7] = (count >> 8) as u8;
    cb[8] = count as u8;

    let data_len = count * dev.block_size;
    let cbw = build_cbw(&cb, data_len, CBW_DATA_IN, tag);

    let cbw_bytes = core::slice::from_raw_parts(&cbw as *const Cbw as *const u8, 31);
    let _ = crate::usb::krust_usb_bulk_out(dev.addr, dev.ep_out, cbw_bytes.as_ptr(), cbw_bytes.len());

    let _ = crate::usb::krust_usb_bulk_in(dev.addr, dev.ep_in, buf, data_len as usize);

    let mut csw_buf = [0u8; 13];
    let _ = crate::usb::krust_usb_bulk_in(dev.addr, dev.ep_in, csw_buf.as_mut_ptr(), 13);

    let csw = &*(csw_buf.as_ptr() as *const Csw);
    if csw.signature != CSW_SIGNATURE { return -1; }
    if csw.status == CSW_STATUS_PASSED { count as i32 } else { -1 }
}

pub unsafe fn usb_stor_write(dev_idx: usize, lba: u64, count: u32, buf: *const u8) -> i32 {
    if dev_idx >= NUM_STORAGE { return -1; }
    let dev = &STORAGE_DEVICES[dev_idx];
    if !dev.active { return -1; }

    let tag = dev.tag;
    STORAGE_DEVICES[dev_idx].tag = tag.wrapping_add(1);

    let mut cb = [0u8; 10];
    cb[0] = SCSI_CMD_WRITE_10;
    cb[2] = (lba >> 24) as u8;
    cb[3] = (lba >> 16) as u8;
    cb[4] = (lba >> 8) as u8;
    cb[5] = lba as u8;
    cb[7] = (count >> 8) as u8;
    cb[8] = count as u8;

    let data_len = count * dev.block_size;
    let cbw = build_cbw(&cb, data_len, CBW_DATA_OUT, tag);

    let cbw_bytes = core::slice::from_raw_parts(&cbw as *const Cbw as *const u8, 31);
    let _ = crate::usb::krust_usb_bulk_out(dev.addr, dev.ep_out, cbw_bytes.as_ptr(), cbw_bytes.len());

    let _ = crate::usb::krust_usb_bulk_out(dev.addr, dev.ep_out, buf, data_len as usize);

    let mut csw_buf = [0u8; 13];
    let _ = crate::usb::krust_usb_bulk_in(dev.addr, dev.ep_in, csw_buf.as_mut_ptr(), 13);

    let csw = &*(csw_buf.as_ptr() as *const Csw);
    if csw.signature != CSW_SIGNATURE { return -1; }
    if csw.status == CSW_STATUS_PASSED { count as i32 } else { -1 }
}

pub unsafe fn usb_stor_read_capacity(dev_idx: usize) -> bool {
    if dev_idx >= NUM_STORAGE { return false; }
    let dev = &STORAGE_DEVICES[dev_idx];
    if !dev.active { return false; }

    let tag = dev.tag;
    STORAGE_DEVICES[dev_idx].tag = tag.wrapping_add(1);

    let cb = [SCSI_CMD_READ_CAPACITY, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let cbw = build_cbw(&cb, 8, CBW_DATA_IN, tag);

    let cbw_bytes = core::slice::from_raw_parts(&cbw as *const Cbw as *const u8, 31);
    let _ = crate::usb::krust_usb_bulk_out(dev.addr, dev.ep_out, cbw_bytes.as_ptr(), cbw_bytes.len());

    let mut data = [0u8; 8];
    let _ = crate::usb::krust_usb_bulk_in(dev.addr, dev.ep_in, data.as_mut_ptr(), 8);

    let mut csw_buf = [0u8; 13];
    let _ = crate::usb::krust_usb_bulk_in(dev.addr, dev.ep_in, csw_buf.as_mut_ptr(), 13);

    let last_lba = ((data[0] as u32) << 24) | ((data[1] as u32) << 16) | ((data[2] as u32) << 8) | (data[3] as u32);
    let block_size = ((data[4] as u32) << 24) | ((data[5] as u32) << 16) | ((data[6] as u32) << 8) | (data[7] as u32);

    STORAGE_DEVICES[dev_idx].block_count = (last_lba as u64) + 1;
    STORAGE_DEVICES[dev_idx].block_size = block_size;

    crate::vga::krust_vga_writestring(b"USB Storage: capacity \0" as *const u8);
    let mut buf = [0u8; 16];
    let mut v = last_lba as u64 + 1;
    let mut tmp = [0u8; 20];
    let mut n = 0;
    if v == 0 { tmp[0] = b'0'; n = 1; }
    else { while v > 0 { tmp[n] = b'0' + (v % 10) as u8; v /= 10; n += 1; } }
    let mut i = 0;
    while i < n { buf[i] = tmp[n - 1 - i]; i += 1; }
    buf[n] = 0;
    crate::vga::krust_vga_writestring(buf.as_ptr());
    crate::vga::krust_vga_writestring(b" blocks\n\0" as *const u8);

    true
}

pub unsafe fn usb_stor_test_unit_ready(dev_idx: usize) -> bool {
    if dev_idx >= NUM_STORAGE { return false; }
    let dev = &STORAGE_DEVICES[dev_idx];
    if !dev.active { return false; }

    let tag = dev.tag;
    STORAGE_DEVICES[dev_idx].tag = tag.wrapping_add(1);

    let cb = [SCSI_CMD_TEST_UNIT_READY, 0, 0, 0, 0, 0];
    let cbw = build_cbw(&cb, 0, CBW_DATA_IN, tag);

    let cbw_bytes = core::slice::from_raw_parts(&cbw as *const Cbw as *const u8, 31);
    let _ = crate::usb::krust_usb_bulk_out(dev.addr, dev.ep_out, cbw_bytes.as_ptr(), cbw_bytes.len());

    let mut csw_buf = [0u8; 13];
    let _ = crate::usb::krust_usb_bulk_in(dev.addr, dev.ep_in, csw_buf.as_mut_ptr(), 13);

    let csw = &*(csw_buf.as_ptr() as *const Csw);
    csw.signature == CSW_SIGNATURE && csw.status == CSW_STATUS_PASSED
}

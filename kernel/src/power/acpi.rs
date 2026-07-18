use core::ptr;

extern "C" {
    fn krust_memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8;
    fn krust_memset(s: *mut u8, c: i32, n: usize) -> *mut u8;
    fn krust_memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32;
}

unsafe fn inb(port: u16) -> u8 {
    let result: u8;
    core::arch::asm!("in al, dx", out("al") result, in("dx") port, options(nostack, preserves_flags));
    result
}

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nostack, preserves_flags));
}

unsafe fn inw(port: u16) -> u16 {
    let result: u16;
    core::arch::asm!("in ax, dx", out("ax") result, in("dx") port, options(nostack, preserves_flags));
    result
}

unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, preserves_flags));
}

unsafe fn io_wait() {
    outb(0x80, 0);
}

#[repr(C, packed)]
struct RSDPDescriptor {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

#[repr(C, packed)]
struct RSDPDescriptor20 {
    first: RSDPDescriptor,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

#[repr(C, packed)]
struct SDTHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

#[repr(C, packed)]
struct RSDT {
    header: SDTHeader,
}

#[repr(C, packed)]
struct FADT {
    header: SDTHeader,
    firmware_ctrl: u32,
    dsdt: u32,
    _reserved1: u8,
    preferred_pm_profile: u8,
    sci_int: u16,
    smi_cmd: u32,
    acpi_enable: u8,
    acpi_disable: u8,
    s4bios_req: u8,
    pstate_cnt: u8,
    pm1a_evt_blk: u32,
    pm1b_evt_blk: u32,
    pm1a_cnt_blk: u32,
    pm1b_cnt_blk: u32,
    pm2_cnt_blk: u32,
    pm_tmr_blk: u32,
    gpe0_blk: u32,
    gpe1_blk: u32,
    pm1_evt_len: u8,
    pm1_cnt_len: u8,
    pm2_cnt_len: u8,
    pm_tmr_len: u8,
    gpe0_blk_len: u8,
    gpe1_blk_len: u8,
    gpe1_base: u8,
    _reserved2: u8,
    p_lvl2_lat: u16,
    p_lvl3_lat: u16,
    flush_size: u16,
    flush_stride: u16,
    duty_offset: u8,
    duty_width: u8,
    day_alarm: u8,
    month_alarm: u8,
    century: u8,
    iapc_boot_arch: u16,
    _reserved3: u8,
    flags: u32,
    reset_reg: [u8; 12],
    reset_value: u8,
    _reserved4: [u16; 3],
    x_firmware_ctrl: u64,
    x_dsdt: u64,
    x_pm1a_evt_blk: [u8; 12],
    x_pm1b_evt_blk: [u8; 12],
    x_pm1a_cnt_blk: [u8; 12],
    x_pm1b_cnt_blk: [u8; 12],
    x_pm2_cnt_blk: [u8; 12],
    x_pm_tmr_blk: [u8; 12],
    x_gpe0_blk: [u8; 12],
    x_gpe1_blk: [u8; 12],
}

#[repr(C, packed)]
struct MADT {
    header: SDTHeader,
    lapic_addr: u32,
    flags: u32,
}

#[repr(C, packed)]
struct MADTEntryHeader {
    entry_type: u8,
    length: u8,
}

#[repr(C, packed)]
struct MADTLocalAPIC {
    header: MADTEntryHeader,
    acpi_processor_id: u8,
    apic_id: u8,
    flags: u32,
}

#[repr(C, packed)]
pub struct SMPInfo {
    pub lapic_addr: u32,
    pub bsp_apic_id: u8,
    pub cpu_count: u8,
    pub apic_ids: [u8; 256],
}

static mut RSDP_ADDR: u64 = 0;
static mut RSDT_ADDR: u64 = 0;
static mut MADT_ADDR: u64 = 0;
static mut FADT_ADDR: u64 = 0;
static mut ACPI_AVAILABLE: bool = false;
static mut PM1A_CNT_BLK: u16 = 0;
static mut PM1_CNT_LEN: u8 = 0;
static mut SLP_TYPEA: u8 = 5;
static mut SLP_TYPEB: u8 = 5;
pub static mut IOAPIC_ADDR: u64 = 0;
pub static mut HPET_ADDR: u64 = 0;

#[no_mangle]
pub static mut SMP_INFO: SMPInfo = SMPInfo {
    lapic_addr: 0,
    bsp_apic_id: 0,
    cpu_count: 0,
    apic_ids: [0; 256],
};

unsafe fn rsdp_checksum(rsdp: *const u8) -> bool {
    let mut sum: u8 = 0;
    for i in 0..20 {
        sum = sum.wrapping_add(ptr::read_volatile(rsdp.add(i)));
    }
    sum == 0
}

unsafe fn find_rsdp() -> *const u8 {
    let ebda_ptr = 0x40E as *const u16;
    let ebda_seg = ptr::read_volatile(ebda_ptr);

    if ebda_seg != 0 {
        let ebda_addr = (ebda_seg as u32) << 4;
        let mut addr = ebda_addr;
        while addr < ebda_addr + 1024 {
            let rsdp = addr as *const u8;
            if krust_memcmp(rsdp, b"RSD PTR \0".as_ptr(), 8) == 0 {
                if rsdp_checksum(rsdp) {
                    return rsdp;
                }
            }
            addr += 16;
        }
    }

    let mut addr = 0xE0000u32;
    while addr < 0x100000 {
        let rsdp = addr as *const u8;
        if krust_memcmp(rsdp, b"RSD PTR \0".as_ptr(), 8) == 0 {
            if rsdp_checksum(rsdp) {
                return rsdp;
            }
        }
        addr += 16;
    }

    ptr::null()
}

unsafe fn sdt_checksum(sdt: *const u8) -> bool {
    let length = read_le32(sdt.add(4)) as usize;
    let mut sum: u8 = 0;
    for i in 0..length {
        sum = sum.wrapping_add(ptr::read_volatile(sdt.add(i)));
    }
    sum == 0
}

unsafe fn read_le16(p: *const u8) -> u16 {
    (ptr::read_volatile(p) as u16) | ((ptr::read_volatile(p.add(1)) as u16) << 8)
}

unsafe fn read_le32(p: *const u8) -> u32 {
    (ptr::read_volatile(p) as u32)
        | ((ptr::read_volatile(p.add(1)) as u32) << 8)
        | ((ptr::read_volatile(p.add(2)) as u32) << 16)
        | ((ptr::read_volatile(p.add(3)) as u32) << 24)
}

unsafe fn parse_madt(rsdt_addr: u64) {
    let rsdt = rsdt_addr as *const u8;
    if rsdt.is_null() {
        return;
    }

    let length = read_le32(rsdt.add(4));
    let num_entries = (length - 36) / 4;

    let mut i = 0u32;
    while i < num_entries {
        let entry_addr = read_le32(rsdt.add(36 + i as usize * 4)) as u64;
        let entry = entry_addr as *const u8;
        if entry.is_null() {
            i += 1;
            continue;
        }

        if krust_memcmp(entry, b"APIC\0".as_ptr(), 4) == 0 {
            if !sdt_checksum(entry) {
                i += 1;
                continue;
            }

            let madt = entry as *const MADT;
            MADT_ADDR = entry_addr;

            SMP_INFO.lapic_addr = (*madt).lapic_addr;
            SMP_INFO.cpu_count = 0;

            let mut offset = core::mem::size_of::<MADT>() as u32;
            while offset < length {
                let entry_header = entry.add(offset as usize) as *const MADTEntryHeader;
                if entry_header.is_null() {
                    break;
                }

                let entry_type = ptr::read_volatile(&(*entry_header).entry_type);
                let entry_length = ptr::read_volatile(&(*entry_header).length);

                if entry_length == 0 {
                    break;
                }

                if entry_type == 0 && offset + entry_length as u32 <= length {
                    let lapic = entry.add(offset as usize) as *const MADTLocalAPIC;
                    let flags = (*lapic).flags;
                    if flags & 1 != 0 {
                        let id = (*lapic).apic_id;
                        let idx = SMP_INFO.cpu_count as usize;
                        if idx < 256 {
                            SMP_INFO.apic_ids[idx] = id;
                            SMP_INFO.cpu_count += 1;
                        }
                    }
                }

                if entry_type == 1 && entry_length >= 12 {
                    let ioapic_addr = ptr::read_volatile(
                        entry.add(offset as usize + 4) as *const u32
                    ) as u64;
                    if ioapic_addr != 0 && IOAPIC_ADDR == 0 {
                        IOAPIC_ADDR = ioapic_addr;
                    }
                }

                offset += entry_length as u32;
            }
            return;
        }

        i += 1;
    }
}

unsafe fn parse_dsdt_s5(fadt: *const FADT) {
    let dsdt_addr = (*fadt).dsdt as u64;
    if dsdt_addr == 0 {
        return;
    }

    let dsdt = dsdt_addr as *const u8;
    let dsdt_length = read_le32(dsdt.add(4));

    let mut off = 0u32;
    while off < dsdt_length.saturating_sub(20) {
        if ptr::read_volatile(dsdt.add(off as usize)) == 0x08
            && ptr::read_volatile(dsdt.add(off as usize + 1)) == 0x5F
            && ptr::read_volatile(dsdt.add(off as usize + 2)) == 0x53
            && ptr::read_volatile(dsdt.add(off as usize + 3)) == 0x35
        {
            let mut j = off + 4;
            let mut found = 0u32;
            while j < off + 20 && j < dsdt_length.saturating_sub(2) {
                if ptr::read_volatile(dsdt.add(j as usize)) == 0x12 {
                    let mut k = j + 2;
                    if k >= dsdt_length {
                        break;
                    }
                    k += 1;
                    while k < dsdt_length && found < 2 {
                        let byte = ptr::read_volatile(dsdt.add(k as usize));
                        if byte == 0x0A {
                            if k + 1 < dsdt_length {
                                let val = ptr::read_volatile(dsdt.add(k as usize + 1));
                                if found == 0 {
                                    SLP_TYPEA = val;
                                } else {
                                    SLP_TYPEB = val;
                                }
                                found += 1;
                                k += 2;
                            } else {
                                break;
                            }
                        } else if byte == 0x0B {
                            if k + 2 < dsdt_length {
                                let val = read_le16(dsdt.add(k as usize + 1));
                                if found == 0 {
                                    SLP_TYPEA = (val & 0xFF) as u8;
                                } else {
                                    SLP_TYPEB = (val & 0xFF) as u8;
                                }
                                found += 1;
                                k += 3;
                            } else {
                                break;
                            }
                        } else {
                            k += 1;
                        }
                    }
                    break;
                }
                j += 1;
            }
            break;
        }
        off += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_acpi_init() -> i32 {
    ACPI_AVAILABLE = false;
    RSDP_ADDR = 0;
    RSDT_ADDR = 0;
    MADT_ADDR = 0;
    FADT_ADDR = 0;

    let rsdp = find_rsdp();
    if rsdp.is_null() {
        return -1;
    }

    RSDP_ADDR = rsdp as u64;
    RSDT_ADDR = read_le32(rsdp.add(16)) as u64;

    if RSDT_ADDR == 0 {
        return -1;
    }

    let rsdt = RSDT_ADDR as *const u8;
    if !sdt_checksum(rsdt) {
        return -1;
    }

    let length = read_le32(rsdt.add(4));
    let num_entries = (length - 36) / 4;

    let mut i = 0u32;
    while i < num_entries {
        let entry_addr = read_le32(rsdt.add(36 + i as usize * 4)) as u64;
        let entry = entry_addr as *const u8;
        if entry.is_null() {
            i += 1;
            continue;
        }

        if krust_memcmp(entry, b"FACP\0".as_ptr(), 4) == 0 {
            if !sdt_checksum(entry) {
                i += 1;
                continue;
            }

            let fadt = entry as *const FADT;
            FADT_ADDR = entry_addr;
            PM1A_CNT_BLK = (*fadt).pm1a_cnt_blk as u16;
            PM1_CNT_LEN = (*fadt).pm1_cnt_len;

            parse_dsdt_s5(fadt);

            if (*fadt).smi_cmd != 0 && (*fadt).acpi_enable != 0 {
                let pm1_cnt = inw(PM1A_CNT_BLK);
                if pm1_cnt & (1 << 0) == 0 {
                    outb((*fadt).smi_cmd as u16, (*fadt).acpi_enable);
                    for _timeout in 0..1000 {
                        io_wait();
                        let pm1_cnt = inw(PM1A_CNT_BLK);
                        if pm1_cnt & (1 << 0) != 0 {
                            break;
                        }
                    }
                }
            }

            parse_madt(RSDT_ADDR);
            break;
        }

        i += 1;
    }

    ACPI_AVAILABLE = true;
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_acpi_is_available() -> i32 {
    if ACPI_AVAILABLE { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_acpi_reboot() -> i32 {
    for _i in 0..3 {
        for _timeout in 0..1000 {
            let st = inb(0x64);
            if st & 0x02 == 0 {
                break;
            }
            io_wait();
        }
        outb(0x64, 0xFE);
        io_wait();
    }

    // Triple fault as fallback
    let idt_zero = [0u8; 6];
    core::arch::asm!(
        "lidt [{ptr}]",
        "int3",
        ptr = in(reg) idt_zero.as_ptr(),
        options(nostack),
    );

    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_acpi_poweroff() -> i32 {
    if !ACPI_AVAILABLE {
        return -1;
    }

    let pm1a_cnt = inw(PM1A_CNT_BLK);
    let mut new_val = pm1a_cnt & !0x1C00;
    new_val |= (SLP_TYPEA as u16) << 10;
    new_val |= 1 << 13; // SLP_EN

    outw(PM1A_CNT_BLK, new_val);

    for _timeout in 0..100000 {
        io_wait();
        core::arch::asm!("hlt", options(nostack));
    }

    // Fallback: try ACPI power off via QEMU/Bochs reset port
    outw(0x604, 0x2000);
    outw(0xB004, 0x2000);

    0
}

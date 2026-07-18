
use crate::klib::{uint8_t, uint16_t, uint32_t};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PCIDevice {
    pub bus: uint8_t,
    pub slot: uint8_t,
    pub func: uint8_t,
    pub vendor_id: uint16_t,
    pub device_id: uint16_t,
    pub class_code: uint8_t,
    pub subclass: uint8_t,
    pub prog_if: uint8_t,
    pub revision_id: uint8_t,
}

pub struct PCI;

impl PCI {
    pub fn config_read_word(bus: uint8_t, slot: uint8_t, func: uint8_t, offset: uint8_t) -> uint16_t {
        unsafe {
            let addr = 0xCF8 | ((bus as u32) << 16) | ((slot as u32) << 11) | ((func as u32) << 8) | ((offset as u32) & 0xFC);
            core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") addr);
            let data: u32;
            core::arch::asm!("in eax, dx", out("eax") data, in("dx") 0xCFCu16);
            ((data >> ((offset & 2) * 8)) & 0xFFFF) as uint16_t
        }
    }

    pub fn config_read_dword(bus: uint8_t, slot: uint8_t, func: uint8_t, offset: uint8_t) -> uint32_t {
        unsafe {
            let addr = 0xCF8 | ((bus as u32) << 16) | ((slot as u32) << 11) | ((func as u32) << 8) | ((offset as u32) & 0xFC);
            core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") addr);
            let data: u32;
            core::arch::asm!("in eax, dx", out("eax") data, in("dx") 0xCFCu16);
            data
        }
    }

    pub fn config_write_word(bus: uint8_t, slot: uint8_t, func: uint8_t, offset: uint8_t, value: uint16_t) {
        unsafe {
            let addr = 0xCF8 | ((bus as u32) << 16) | ((slot as u32) << 11) | ((func as u32) << 8) | ((offset as u32) & 0xFC);
            core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") addr);
            let data = value as u32;
            core::arch::asm!("out dx, eax", in("dx") 0xCFCu16, in("eax") data);
        }
    }

    pub fn config_write_dword(bus: uint8_t, slot: uint8_t, func: uint8_t, offset: uint8_t, value: uint32_t) {
        unsafe {
            let addr = 0xCF8 | ((bus as u32) << 16) | ((slot as u32) << 11) | ((func as u32) << 8) | ((offset as u32) & 0xFC);
            core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") addr);
            core::arch::asm!("out dx, eax", in("dx") 0xCFCu16, in("eax") value);
        }
    }

    /// Enumerate PCI bus for a specific vendor/device pair
    pub fn enumerate(vendor: u16, device: u16) -> Option<PCIDevice> {
        for bus in 0..=255u16 {
            for slot in 0..32u8 {
                for func in 0..8u8 {
                    let vid = Self::config_read_word(bus as u8, slot, func, 0x00);
                    if vid == 0xFFFF { continue; }
                    if vid == vendor {
                        let did = Self::config_read_word(bus as u8, slot, func, 0x02);
                        if did == device {
                            let class_reg = Self::config_read_dword(bus as u8, slot, func, 0x08);
                            return Some(PCIDevice {
                                bus: bus as u8,
                                slot,
                                func,
                                vendor_id: vid,
                                device_id: did,
                                class_code: ((class_reg >> 24) & 0xFF) as u8,
                                subclass: ((class_reg >> 16) & 0xFF) as u8,
                                prog_if: ((class_reg >> 8) & 0xFF) as u8,
                                revision_id: (class_reg & 0xFF) as u8,
                            });
                        }
                    }
                }
            }
        }
        None
    }

    /// Enumerate PCI bus for a specific class/subclass
    pub fn enumerate_class(class: u8, subclass: u8) -> Option<PCIDevice> {
        for bus in 0..=255u16 {
            for slot in 0..32u8 {
                for func in 0..8u8 {
                    let vid = Self::config_read_word(bus as u8, slot, func, 0x00);
                    if vid == 0xFFFF { continue; }
                    let class_reg = Self::config_read_dword(bus as u8, slot, func, 0x08);
                    let cc = ((class_reg >> 24) & 0xFF) as u8;
                    let sc = ((class_reg >> 16) & 0xFF) as u8;
                    if cc == class && sc == subclass {
                        let did = Self::config_read_word(bus as u8, slot, func, 0x02);
                        return Some(PCIDevice {
                            bus: bus as u8,
                            slot,
                            func,
                            vendor_id: vid,
                            device_id: did,
                            class_code: cc,
                            subclass: sc,
                            prog_if: ((class_reg >> 8) & 0xFF) as u8,
                            revision_id: (class_reg & 0xFF) as u8,
                        });
                    }
                }
            }
        }
        None
    }

    /// Read a BAR (Base Address Register) for a PCI device
    pub fn read_bar(bus: u8, slot: u8, func: u8, bar_index: u8) -> u32 {
        let offset = 0x10 + (bar_index * 4);
        Self::config_read_dword(bus, slot, func, offset)
    }

    /// Enable bus mastering (DMA) for a PCI device
    pub fn enable_bus_mastering(bus: u8, slot: u8, func: u8) {
        let mut cmd = Self::config_read_word(bus, slot, func, 0x04);
        cmd |= 1 << 2; // Bus Master Enable
        cmd |= 1 << 1; // Memory Space Enable
        Self::config_write_word(bus, slot, func, 0x04, cmd);
    }

    /// Read the interrupt line (byte 0 of reg 0x3C) for a PCI device
    pub fn read_irq_line(bus: u8, slot: u8, func: u8) -> u8 {
        let val = Self::config_read_word(bus, slot, func, 0x3C);
        (val & 0xFF) as u8
    }

    /// Read the interrupt pin (byte 1 of reg 0x3C) for a PCI device
    pub fn read_irq_pin(bus: u8, slot: u8, func: u8) -> u8 {
        let val = Self::config_read_word(bus, slot, func, 0x3C);
        ((val >> 8) & 0xFF) as u8
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_pci_read_word(bus: u8, slot: u8, func: u8, offset: u8) -> u16 {
    PCI::config_read_word(bus, slot, func, offset)
}

#[no_mangle]
pub unsafe extern "C" fn krust_pci_read_dword(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    PCI::config_read_dword(bus, slot, func, offset)
}

#[no_mangle]
pub unsafe extern "C" fn krust_pci_write_word(bus: u8, slot: u8, func: u8, offset: u8, value: u16) {
    PCI::config_write_word(bus, slot, func, offset, value)
}
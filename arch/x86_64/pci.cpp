#include "pci.hpp"
#include "port.hpp"
#include "vga.hpp"
#include "ns16550.hpp"
#include "lib.hpp"

using namespace arch::x86;

PCIDevice PCI::devices[MAX_DEVICES];
int PCI::num_devices = 0;
PCI::VendorDriver PCI::vendor_drivers[MAX_DRIVERS];
int PCI::num_vendor_drivers = 0;
PCI::ClassDriver PCI::class_drivers[MAX_DRIVERS];
int PCI::num_class_drivers = 0;

static const uint16_t CONFIG_ADDRESS = 0xCF8;
static const uint16_t CONFIG_DATA = 0xCFC;

uint32_t PCI::config_read_dword(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset) {
    uint32_t address = (uint32_t)((uint32_t)1 << 31)
                     | ((uint32_t)bus << 16)
                     | ((uint32_t)slot << 11)
                     | ((uint32_t)func << 8)
                     | (offset & 0xFC);
    outl(CONFIG_ADDRESS, address);
    return inl(CONFIG_DATA);
}

void PCI::config_write_dword(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset, uint32_t value) {
    uint32_t address = (uint32_t)((uint32_t)1 << 31)
                     | ((uint32_t)bus << 16)
                     | ((uint32_t)slot << 11)
                     | ((uint32_t)func << 8)
                     | (offset & 0xFC);
    outl(CONFIG_ADDRESS, address);
    outl(CONFIG_DATA, value);
}

uint16_t PCI::config_read_word(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset) {
    uint32_t dword = config_read_dword(bus, slot, func, offset & ~3);
    return (offset & 2) ? (dword >> 16) : (dword & 0xFFFF);
}

void PCI::config_write_word(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset, uint16_t value) {
    uint32_t dword = config_read_dword(bus, slot, func, offset & ~3);
    uint32_t mask = (offset & 2) ? 0xFFFF0000 : 0x0000FFFF;
    uint32_t shift = (offset & 2) ? 16 : 0;
    dword = (dword & ~mask) | ((uint32_t)value << shift);
    config_write_dword(bus, slot, func, offset & ~3, dword);
}

void PCI::check_function(uint8_t bus, uint8_t slot, uint8_t func) {
    if (num_devices >= MAX_DEVICES) return;

    uint16_t vendor_id = config_read_word(bus, slot, func, 0);
    if (vendor_id == 0xFFFF) return;

    uint16_t device_id = config_read_word(bus, slot, func, 2);
    uint16_t class_sub = config_read_word(bus, slot, func, 10); // class + subclass at offset 0x0A
    uint8_t class_code = (class_sub >> 8) & 0xFF;
    uint8_t subclass = class_sub & 0xFF;
    uint16_t prog_rev = config_read_word(bus, slot, func, 8); // prog_if + revision at offset 0x08
    uint8_t prog_if = (prog_rev >> 8) & 0xFF;
    uint8_t revision = prog_rev & 0xFF;

    PCIDevice &dev = devices[num_devices++];
    dev.bus = bus;
    dev.slot = slot;
    dev.func = func;
    dev.vendor_id = vendor_id;
    dev.device_id = device_id;
    dev.class_code = class_code;
    dev.subclass = subclass;
    dev.prog_if = prog_if;
    dev.revision = revision;

    drivers::NS16550::printf("pci: %02x:%02x.%x %04x:%04x class=%02x subclass=%02x\n",
                           bus, slot, func, vendor_id, device_id, class_code, subclass);

    // Probe vendor+device-specific drivers
    for (int i = 0; i < num_vendor_drivers; i++) {
        if (vendor_drivers[i].vendor_id == vendor_id && vendor_drivers[i].device_id == device_id) {
            vendor_drivers[i].probe(dev);
        }
    }

    // Probe class drivers
    for (int i = 0; i < num_class_drivers; i++) {
        if (class_drivers[i].class_code == class_code && class_drivers[i].subclass == subclass) {
            class_drivers[i].probe(dev);
        }
    }
}

void PCI::check_device(uint8_t bus, uint8_t slot) {
    uint16_t vendor_id = config_read_word(bus, slot, 0, 0);
    if (vendor_id == 0xFFFF) return;
    check_function(bus, slot, 0);
    uint16_t header_type = config_read_word(bus, slot, 0, 14); // header type at offset 0x0E
    if (header_type & 0x80) {
        for (int func = 1; func < 8; func++) {
            if (config_read_word(bus, slot, func, 0) != 0xFFFF) {
                check_function(bus, slot, func);
            }
        }
    }
}

void PCI::scan_bus(uint8_t bus) {
    for (int slot = 0; slot < 32; slot++) {
        check_device(bus, slot);
    }
}

void PCI::init() {
    lib::memset(devices, 0, sizeof(devices));
    lib::memset(vendor_drivers, 0, sizeof(vendor_drivers));
    lib::memset(class_drivers, 0, sizeof(class_drivers));
    num_devices = 0;
    num_vendor_drivers = 0;
    num_class_drivers = 0;

    drivers::VGA::writestring("Scanning PCI buses...\n");

    uint16_t header_type = config_read_word(0, 0, 0, 14);
    if (!(header_type & 0x80)) {
        scan_bus(0);
    } else {
        for (int func = 0; func < 8; func++) {
            if (config_read_word(0, 0, func, 0) != 0xFFFF) {
                scan_bus(func);
            }
        }
    }

    drivers::VGA::printf("PCI: found %d device(s)\n", num_devices);
}

int PCI::device_count() {
    return num_devices;
}

const PCIDevice &PCI::device(int index) {
    return devices[index];
}

void PCI::install_driver(uint16_t vendor_id, uint16_t device_id, PCIDriverProbe probe) {
    if (num_vendor_drivers >= MAX_DRIVERS) return;
    vendor_drivers[num_vendor_drivers].vendor_id = vendor_id;
    vendor_drivers[num_vendor_drivers].device_id = device_id;
    vendor_drivers[num_vendor_drivers].probe = probe;
    num_vendor_drivers++;
}

void PCI::install_class_driver(uint8_t class_code, uint8_t subclass, PCIDriverProbe probe) {
    if (num_class_drivers >= MAX_DRIVERS) return;
    class_drivers[num_class_drivers].class_code = class_code;
    class_drivers[num_class_drivers].subclass = subclass;
    class_drivers[num_class_drivers].probe = probe;
    num_class_drivers++;
}

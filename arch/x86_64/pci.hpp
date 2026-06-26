#ifndef ELITRA_PCI_HPP
#define ELITRA_PCI_HPP

#include <cstdint>

namespace arch::x86 {

struct PCIDevice {
    uint8_t bus;
    uint8_t slot;
    uint8_t func;
    uint16_t vendor_id;
    uint16_t device_id;
    uint8_t class_code;
    uint8_t subclass;
    uint8_t prog_if;
    uint8_t revision;
};

using PCIDriverProbe = void (*)(const PCIDevice &dev);

class PCI {
public:
    static void init();
    static uint16_t config_read_word(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset);
    static void config_write_word(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset, uint16_t value);
    static uint32_t config_read_dword(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset);
    static void config_write_dword(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset, uint32_t value);
    static int device_count();
    static const PCIDevice &device(int index);

    static void install_driver(uint16_t vendor_id, uint16_t device_id, PCIDriverProbe probe);
    static void install_class_driver(uint8_t class_code, uint8_t subclass, PCIDriverProbe probe);

private:
    static const int MAX_DEVICES = 64;
    static const int MAX_DRIVERS = 16;

    static PCIDevice devices[MAX_DEVICES];
    static int num_devices;

    struct VendorDriver {
        uint16_t vendor_id;
        uint16_t device_id;
        PCIDriverProbe probe;
    };
    static VendorDriver vendor_drivers[MAX_DRIVERS];
    static int num_vendor_drivers;

    struct ClassDriver {
        uint8_t class_code;
        uint8_t subclass;
        PCIDriverProbe probe;
    };
    static ClassDriver class_drivers[MAX_DRIVERS];
    static int num_class_drivers;

    static void check_function(uint8_t bus, uint8_t slot, uint8_t func);
    static void check_device(uint8_t bus, uint8_t slot);
    static void scan_bus(uint8_t bus);
};

}

#endif

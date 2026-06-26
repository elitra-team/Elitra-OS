#ifndef ELITRA_E1000_HPP
#define ELITRA_E1000_HPP

#include <cstdint>
#include "pci.hpp"

namespace drivers {

struct e1000_tx_desc {
    volatile uint64_t addr;
    volatile uint16_t length;
    volatile uint8_t  cso;
    volatile uint8_t  cmd;
    volatile uint8_t  status;
    volatile uint8_t  css;
    volatile uint16_t special;
} __attribute__((packed));

struct e1000_rx_desc {
    volatile uint64_t addr;
    volatile uint16_t length;
    volatile uint16_t checksum;
    volatile uint8_t  status;
    volatile uint8_t  errors;
    volatile uint16_t special;
} __attribute__((packed));

class E1000 {
public:
    static void init();
    static bool send_packet(const uint8_t *data, uint16_t len);
    static bool receive_packet(uint8_t *buf, uint16_t *len);
    static void get_mac(uint8_t mac[6]);

    static void probe(const arch::x86::PCIDevice &dev);

private:
    static const int NUM_RX_DESC = 32;
    static const int NUM_TX_DESC = 32;
    static const int RX_BUF_SIZE = 2048;

    static uint8_t *mmio_base;
    static uint8_t mac_addr[6];
    static bool initialized;

    // Descriptor rings (physically contiguous)
    static e1000_rx_desc *rx_descs;
    static e1000_tx_desc *tx_descs;
    static uint8_t *rx_buffers[NUM_RX_DESC];
    static uint64_t rx_descs_phys;
    static uint64_t tx_descs_phys;

    static int tx_cur;
    static int rx_cur;

    static uint32_t mmio_read(uint16_t reg);
    static void mmio_write(uint16_t reg, uint32_t val);
    static bool eeprom_read(uint16_t addr, uint16_t *data);
    static void link_status();
};

}

#endif

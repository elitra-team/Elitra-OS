#include "e1000.hpp"
#include "pci.hpp"
#include "port.hpp"
#include "vga.hpp"
#include "ns16550.hpp"
#include "paging.hpp"
#include "pmm.hpp"
#include "heap.hpp"
#include "lib.hpp"

using namespace drivers;

// MMIO register offsets
enum {
    REG_CTRL    = 0x0000,
    REG_STATUS  = 0x0008,
    REG_EERD    = 0x4014,
    REG_RCTL    = 0x0100,
    REG_TCTL    = 0x0400,
    REG_TIPG    = 0x0410,
    REG_RDBAL   = 0x2800,
    REG_RDBAH   = 0x2804,
    REG_RDLEN   = 0x2808,
    REG_RDH     = 0x2810,
    REG_RDT     = 0x2818,
    REG_TDBAL   = 0x3800,
    REG_TDBAH   = 0x3804,
    REG_TDLEN   = 0x3808,
    REG_TDH     = 0x3810,
    REG_TDT     = 0x3818,
    REG_IMS     = 0x00D0,
    REG_ICR     = 0x00C0,
    REG_RA      = 0x5400,
    REG_MTA     = 0x5200,
    REG_RAL     = 0x5400,
    REG_RAH     = 0x5404,
};

// CTRL register bits
enum {
    CTRL_FD     = 0x00000001,
    CTRL_ASDE   = 0x00000020,
    CTRL_SLU    = 0x00000040,
    CTRL_ILOS   = 0x00000080,
    CTRL_RST    = 0x04000000,
};

// RCTL bits
enum {
    RCTL_EN     = 0x00000002,
    RCTL_SBP    = 0x00000004,
    RCTL_UPE    = 0x00000008,
    RCTL_MPE    = 0x00000010,
    RCTL_LPE    = 0x00000020,
    RCTL_LBM_NONE = 0x00000000,
    RCTL_RDMTS_HALF = 0x00000000,
    RCTL_BAM    = 0x00008000,
    RCTL_BSIZE_2048 = 0x00000000,
    RCTL_SECRC  = 0x04000000,
};

// TCTL bits
enum {
    TCTL_EN     = 0x00000002,
    TCTL_PSP    = 0x00000008,
    TCTL_CT     = 0x00000FF0,
    TCTL_COLD   = 0x003FF000,
};

// TX command bits
enum {
    CMD_EOP     = 0x01,
    CMD_IFCS    = 0x02,
    CMD_IC      = 0x04,
    CMD_RS      = 0x08,
    CMD_RPS     = 0x10,
    CMD_DEXT    = 0x20,
    CMD_VLE     = 0x40,
    CMD_IDE     = 0x80,
};

// RX status bits
enum {
    RX_STA_DD   = 0x01,
    RX_STA_EOP  = 0x02,
};

// TX status bits
enum {
    TX_STA_DD   = 0x01,
};

uint8_t *E1000::mmio_base = nullptr;
uint8_t E1000::mac_addr[6] = {0};
bool E1000::initialized = false;
e1000_rx_desc *E1000::rx_descs = nullptr;
e1000_tx_desc *E1000::tx_descs = nullptr;
uint8_t *E1000::rx_buffers[NUM_RX_DESC] = {0};
uint64_t E1000::rx_descs_phys = 0;
uint64_t E1000::tx_descs_phys = 0;
int E1000::tx_cur = 0;
int E1000::rx_cur = 0;

#define MMIO_VADDR 0xFD000000u

uint32_t E1000::mmio_read(uint16_t reg) {
    return *reinterpret_cast<volatile uint32_t *>(mmio_base + reg);
}

void E1000::mmio_write(uint16_t reg, uint32_t val) {
    *reinterpret_cast<volatile uint32_t *>(mmio_base + reg) = val;
}

bool E1000::eeprom_read(uint16_t addr, uint16_t *data) {
    mmio_write(REG_EERD, (addr << 8) | 1);
    for (int i = 0; i < 1000; i++) {
        uint32_t val = mmio_read(REG_EERD);
        if (val & (1 << 4)) {
            *data = (val >> 16) & 0xFFFF;
            return true;
        }
    }
    return false;
}

void E1000::probe(const arch::x86::PCIDevice &dev) {
    drivers::NS16550::printf("e1000: found at %02x:%02x.%x\n", dev.bus, dev.slot, dev.func);

    // Read BAR0 (offset 0x10 in config space)
    uint32_t bar0 = arch::x86::PCI::config_read_dword(dev.bus, dev.slot, dev.func, 0x10);
    drivers::NS16550::printf("e1000: BAR0 = 0x%x\n", bar0);

    if (!bar0) return;

    // BAR0 is MMIO (bit 0 = 0 for MMIO, bit 0 = 1 for I/O)
    uint32_t mmio_phys = bar0 & 0xFFFFFFF0;
    drivers::NS16550::printf("e1000: MMIO phys = 0x%x\n", mmio_phys);

    // Map 128KB of MMIO space
    for (uint32_t off = 0; off < 0x20000; off += 4096) {
        mm::Paging::map_page(MMIO_VADDR + off, mmio_phys + off,
                            mm::Paging::PAGE_PRESENT | mm::Paging::PAGE_WRITE);
    }
    mmio_base = reinterpret_cast<uint8_t *>(MMIO_VADDR);

    // Read MAC address from EEPROM (stored at words 0-2)
    uint16_t tmp;
    if (eeprom_read(0, &tmp)) {
        mac_addr[0] = tmp & 0xFF;
        mac_addr[1] = tmp >> 8;
    }
    if (eeprom_read(1, &tmp)) {
        mac_addr[2] = tmp & 0xFF;
        mac_addr[3] = tmp >> 8;
    }
    if (eeprom_read(2, &tmp)) {
        mac_addr[4] = tmp & 0xFF;
        mac_addr[5] = tmp >> 8;
    }
    drivers::NS16550::printf("e1000: MAC %02x:%02x:%02x:%02x:%02x:%02x\n",
                            mac_addr[0], mac_addr[1], mac_addr[2],
                            mac_addr[3], mac_addr[4], mac_addr[5]);

    // Enable bus master (PCI command register at offset 0x04)
    uint16_t cmd = arch::x86::PCI::config_read_word(dev.bus, dev.slot, dev.func, 0x04);
    cmd |= 0x04; // bus master enable
    cmd |= 0x02; // memory space enable
    arch::x86::PCI::config_write_word(dev.bus, dev.slot, dev.func, 0x04, cmd);

    // Get IRQ line
    uint8_t irq = arch::x86::PCI::config_read_word(dev.bus, dev.slot, dev.func, 0x3C) & 0xFF;
    drivers::NS16550::printf("e1000: IRQ = %d\n", irq);
}

void E1000::init() {
    if (!mmio_base) return;

    // Software reset
    mmio_write(REG_CTRL, mmio_read(REG_CTRL) | CTRL_RST);
    for (int i = 0; i < 10000; i++) {
        if (!(mmio_read(REG_CTRL) & CTRL_RST)) break;
    }

    // Set link up
    uint32_t ctrl = mmio_read(REG_CTRL);
    ctrl |= CTRL_SLU;
    ctrl &= ~CTRL_ILOS;
    mmio_write(REG_CTRL, ctrl);

    // Allocate RX descriptors (32 * 16 = 512 bytes, one page)
    rx_descs = reinterpret_cast<e1000_rx_desc *>(mm::PMM::alloc_frame());
    if (!rx_descs) {
        drivers::NS16550::write("e1000: failed to allocate RX descs\n");
        return;
    }
    lib::memset(rx_descs, 0, mm::Paging::PAGE_SIZE);
    rx_descs_phys = mm::Paging::get_phys(reinterpret_cast<uint64_t>(rx_descs));

    // Allocate and set up RX buffers
    for (int i = 0; i < NUM_RX_DESC; i++) {
        rx_buffers[i] = reinterpret_cast<uint8_t *>(mm::malloc(RX_BUF_SIZE));
        if (!rx_buffers[i]) {
            drivers::NS16550::printf("e1000: failed RX buf %d\n", i);
            return;
        }
        uint64_t phys = mm::Paging::get_phys(reinterpret_cast<uint64_t>(rx_buffers[i]));
        rx_descs[i].addr = phys;
        rx_descs[i].status = 0;
    }

    // Setup RX
    mmio_write(REG_RDBAL, rx_descs_phys & 0xFFFFFFFF);
    mmio_write(REG_RDBAH, (rx_descs_phys >> 32) & 0xFFFFFFFF);
    mmio_write(REG_RDLEN, sizeof(e1000_rx_desc) * NUM_RX_DESC);
    mmio_write(REG_RDH, 0);
    mmio_write(REG_RDT, NUM_RX_DESC - 1);
    mmio_write(REG_RCTL, RCTL_EN | RCTL_SBP | RCTL_UPE | RCTL_MPE | RCTL_BAM | RCTL_BSIZE_2048);

    // Allocate TX descriptors (32 * 16 = 512 bytes, one page)
    tx_descs = reinterpret_cast<e1000_tx_desc *>(mm::PMM::alloc_frame());
    if (!tx_descs) {
        drivers::NS16550::write("e1000: failed to allocate TX descs\n");
        return;
    }
    lib::memset(tx_descs, 0, mm::Paging::PAGE_SIZE);
    tx_descs_phys = mm::Paging::get_phys(reinterpret_cast<uint64_t>(tx_descs));

    // Setup TX
    mmio_write(REG_TDBAL, tx_descs_phys & 0xFFFFFFFF);
    mmio_write(REG_TDBAH, (tx_descs_phys >> 32) & 0xFFFFFFFF);
    mmio_write(REG_TDLEN, sizeof(e1000_tx_desc) * NUM_TX_DESC);
    mmio_write(REG_TDH, 0);
    mmio_write(REG_TDT, 0);
    mmio_write(REG_TCTL, TCTL_EN | TCTL_PSP | (15 << 4) | (64 << 12));
    mmio_write(REG_TIPG, 8 | (4 << 10) | (6 << 20));

    // NOTE: interrupts disabled — no handler registered yet.
    // Enable when IRQ-driven receive is implemented:
    // mmio_write(REG_IMS, 0x1F6); // RXT0, RXO, RXDMT, RXSEQ, LSC

    tx_cur = 0;
    rx_cur = 0;
    initialized = true;

    drivers::VGA::writestring_color("e1000: initialized\n",
        static_cast<uint8_t>(drivers::VGAColor::GREEN));
}

bool E1000::send_packet(const uint8_t *data, uint16_t len) {
    if (!initialized) return false;

    e1000_tx_desc *desc = &tx_descs[tx_cur];
    desc->addr = mm::Paging::get_phys(reinterpret_cast<uint64_t>(data));
    desc->length = len;
    desc->cmd = CMD_EOP | CMD_IFCS | CMD_RS;
    desc->status = 0;

    int next = (tx_cur + 1) % NUM_TX_DESC;
    mmio_write(REG_TDT, next);

    // Wait for transmission
    for (int i = 0; i < 10000; i++) {
        if (desc->status & TX_STA_DD) break;
    }

    tx_cur = next;
    return true;
}

bool E1000::receive_packet(uint8_t *buf, uint16_t *len) {
    if (!initialized) return false;

    int next = (rx_cur + 1) % NUM_RX_DESC;
    if (!(rx_descs[rx_cur].status & RX_STA_DD)) return false;

    *len = rx_descs[rx_cur].length;
    if (*len > RX_BUF_SIZE) *len = RX_BUF_SIZE;
    lib::memcpy(buf, rx_buffers[rx_cur], *len);
    rx_descs[rx_cur].status = 0;
    mmio_write(REG_RDT, rx_cur);
    rx_cur = next;
    return true;
}

void E1000::get_mac(uint8_t mac[6]) {
    lib::memcpy(mac, mac_addr, 6);
}

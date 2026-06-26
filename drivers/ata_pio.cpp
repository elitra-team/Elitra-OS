#include "ata_pio.hpp"
#include "port.hpp"
#include "ns16550.hpp"
#include "vga.hpp"
#include "lib.hpp"
#include "heap.hpp"
#include "pci.hpp"
#include "pmm.hpp"
#include "paging.hpp"

namespace drivers {
namespace ata_pio {

// --- internal state ---

struct Drive {
    bool present;
    uint16_t ident[256];
};

static Drive drives[MAX_DRIVES];
static int drive_count_ = 0;

// Partition buffer state for write-back
static uint8_t *part_buffer = nullptr;
static uint32_t part_lba_start = 0;
static uint32_t part_sectors = 0;
static int part_drive = -1;
static uint8_t *dirty_bits = nullptr;
static uint32_t dirty_size = 0;

// --- I/O helpers ---

static inline void io_wait() {
    arch::x86::inb(0x80);
    arch::x86::inb(0x80);
    arch::x86::inb(0x80);
}

static int base_port(int drive) {
    return (drive < 2) ? PRIMARY_BASE : SECONDARY_BASE;
}

static int slave_bit(int drive) {
    return (drive == 0 || drive == 2) ? 0 : 1;
}

// --- ATA register access ---

static uint8_t read_reg(int drive, uint8_t reg) {
    return arch::x86::inb(base_port(drive) + reg);
}

static bool poll_busy(int drive, int timeout_ms) {
    for (int i = 0; i < timeout_ms * 10; i++) {
        io_wait();
        if (!(read_reg(drive, 7) & 0x80)) return true;
    }
    return false;
}

static bool poll_drq(int drive, int timeout_ms) {
    for (int i = 0; i < timeout_ms * 10; i++) {
        io_wait();
        uint8_t st = read_reg(drive, 7);
        if (st & 0x01) return false;
        if (st & 0x08) return true;
        if (!(st & 0x80)) return false;
    }
    return false;
}

static bool wait_ready(int drive, int timeout_ms) {
    return poll_busy(drive, timeout_ms);
}

// --- IDENTIFY ---

bool identify(int drive, uint16_t *buf) {
    if (drive < 0 || drive >= MAX_DRIVES) return false;

    int base = base_port(drive);
    int slave = slave_bit(drive);

    arch::x86::outb(base + 6, 0xA0 | (slave << 4));
    io_wait();

    arch::x86::outb(base + 2, 0);
    arch::x86::outb(base + 3, 0);
    arch::x86::outb(base + 4, 0);
    arch::x86::outb(base + 5, 0);

    arch::x86::outb(base + 7, 0xEC);
    io_wait();

    uint8_t st = arch::x86::inb(base + 7);
    if (st == 0) return false;

    if (!poll_busy(drive, 100)) return false;

    uint8_t mid = arch::x86::inb(base + 4);
    uint8_t hi  = arch::x86::inb(base + 5);
    if (mid == 0x14 && hi == 0xEB) return false;
    if (mid == 0x69 && hi == 0x96) return false;

    if (!poll_drq(drive, 100)) return false;

    for (int i = 0; i < 256; i++) {
        buf[i] = arch::x86::inw(base);
        io_wait();
    }

    return true;
}

// --- Read sectors ---

bool read(int drive, uint32_t lba, uint8_t count_, void *buf) {
    if (drive < 0 || drive >= MAX_DRIVES || !buf) return false;
    // ATA spec: 0 in sector count register means 256 sectors
    uint32_t count = (count_ == 0) ? 256 : count_;

    int base = base_port(drive);
    int slave = slave_bit(drive);

    if (!wait_ready(drive, 1000)) return false;

    uint8_t dh = 0xE0 | (slave << 4) | ((lba >> 24) & 0x0F);
    arch::x86::outb(base + 6, dh);
    io_wait();

    arch::x86::outb(base + 2, count_); // write 0 for 256 (ATA protocol)
    io_wait();
    arch::x86::outb(base + 3, lba & 0xFF);
    io_wait();
    arch::x86::outb(base + 4, (lba >> 8) & 0xFF);
    io_wait();
    arch::x86::outb(base + 5, (lba >> 16) & 0xFF);
    io_wait();

    arch::x86::outb(base + 7, 0x20);
    io_wait();

    uint16_t *ptr = reinterpret_cast<uint16_t *>(buf);
    for (uint32_t s = 0; s < count; s++) {
        if (!poll_busy(drive, 1000)) return false;
        if (!poll_drq(drive, 1000)) return false;

        for (int i = 0; i < 256; i++) {
            ptr[s * 256 + i] = arch::x86::inw(base);
            io_wait();
        }
    }

    return true;
}

// --- Write sectors ---

bool write(int drive, uint32_t lba, uint8_t count_, const void *buf) {
    if (drive < 0 || drive >= MAX_DRIVES || !buf) return false;
    uint32_t count = (count_ == 0) ? 256 : count_;

    int base = base_port(drive);
    int slave = slave_bit(drive);

    if (!wait_ready(drive, 1000)) return false;

    uint8_t dh = 0xE0 | (slave << 4) | ((lba >> 24) & 0x0F);
    arch::x86::outb(base + 6, dh);
    io_wait();

    arch::x86::outb(base + 2, count_);
    io_wait();
    arch::x86::outb(base + 3, lba & 0xFF);
    io_wait();
    arch::x86::outb(base + 4, (lba >> 8) & 0xFF);
    io_wait();
    arch::x86::outb(base + 5, (lba >> 16) & 0xFF);
    io_wait();

    arch::x86::outb(base + 7, 0x30);
    io_wait();

    const uint16_t *ptr = reinterpret_cast<const uint16_t *>(buf);
    for (uint32_t s = 0; s < count; s++) {
        if (!poll_busy(drive, 1000)) return false;
        if (!poll_drq(drive, 1000)) return false;

        for (int i = 0; i < 256; i++) {
            arch::x86::outw(base, ptr[s * 256 + i]);
            io_wait();
        }
    }

    arch::x86::outb(base + 7, 0xE7);
    io_wait();
    wait_ready(drive, 1000);

    return true;
}

// --- Initialization ---

void init() {
    drive_count_ = 0;
    lib::memset(drives, 0, sizeof(drives));

    for (int d = 0; d < MAX_DRIVES; d++) {
        uint16_t ident[256];
        if (identify(d, ident)) {
            drives[d].present = true;
            lib::memcpy(drives[d].ident, ident, sizeof(ident));
            drive_count_++;
        }
    }

    drivers::NS16550::printf("ata: %d drive(s) detected\n", drive_count_);
    for (int d = 0; d < drive_count_; d++) {
        print_info(d);
    }
}

int drive_count() {
    return drive_count_;
}

bool present(int drive) {
    if (drive < 0 || drive >= MAX_DRIVES) return false;
    return drives[drive].present;
}

// --- Partition table parsing ---

int find_partitions(int drive, Partition *parts, int max_parts) {
    if (!present(drive) || !parts || max_parts < 1) return 0;

    uint8_t mbr[512];
    if (!read(drive, 0, 1, mbr)) return 0;

    if (mbr[510] != 0x55 || mbr[511] != 0xAA) return 0;

    int found = 0;
    for (int i = 0; i < 4 && found < max_parts; i++) {
        uint8_t *entry = mbr + 0x1BE + i * 16;
        uint8_t type = entry[4];
        if (type == 0x0B || type == 0x0C) {
            parts[found].valid = true;
            parts[found].type = type;
            lib::memcpy(&parts[found].lba_start, entry + 8, 4);
            lib::memcpy(&parts[found].sector_count, entry + 12, 4);
            found++;
        }
    }

    return found;
}

// --- Write-back tracking ---

bool mount_partition_buffer(int drive, uint32_t lba_start, uint32_t sectors, uint8_t *buffer) {
    if (!buffer || sectors == 0) return false;
    part_drive = drive;
    part_lba_start = lba_start;
    part_sectors = sectors;
    part_buffer = buffer;

    dirty_size = (sectors + 7) / 8;
    dirty_bits = reinterpret_cast<uint8_t *>(mm::malloc(dirty_size));
    if (!dirty_bits) {
        part_buffer = nullptr;
        return false;
    }
    lib::memset(dirty_bits, 0, dirty_size);
    return true;
}

void mark_dirty(uint32_t byte_offset, uint32_t size) {
    if (!part_buffer || !dirty_bits) return;

    uint32_t start = byte_offset / 512;
    uint32_t end = (byte_offset + size + 511) / 512;
    if (end > part_sectors) end = part_sectors;

    for (uint32_t s = start; s < end; s++) {
        dirty_bits[s / 8] |= (1 << (s % 8));
    }
}

void flush() {
    if (!part_buffer || part_drive < 0 || !dirty_bits) return;

    int flushed = 0;
    for (uint32_t s = 0; s < part_sectors; s++) {
        if (dirty_bits[s / 8] & (1 << (s % 8))) {
            if (write(part_drive, part_lba_start + s, 1, part_buffer + s * 512)) {
                dirty_bits[s / 8] &= ~(1 << (s % 8));
                flushed++;
            }
        }
    }

    if (flushed > 0)
        drivers::NS16550::printf("ata: flushed %d sectors\n", flushed);
}

// --- Info ---

void print_info(int drive) {
    if (!present(drive)) {
        drivers::VGA::printf("ata%d: not present\n", drive);
        return;
    }

    uint16_t *ident = drives[drive].ident;
    const char *channel = (drive % 2 == 0) ? "primary" : "secondary";
    const char *role = (drive < 2) ? "master" : "slave";

    char model[41];
    for (int i = 0; i < 20; i++) {
        uint16_t w = ident[27 + i];
        model[i * 2]     = w >> 8;
        model[i * 2 + 1] = w & 0xFF;
    }
    model[40] = '\0';
    for (int i = 39; i >= 0 && model[i] == ' '; i--) model[i] = '\0';

    uint32_t sectors = ident[60] | (static_cast<uint32_t>(ident[61]) << 16);
    uint32_t size_mb = (sectors / 2) / 1024;

    drivers::VGA::printf("ata%d: %s/%s %s (%d MB)\n",
                             drive, channel, role, model, size_mb);
    drivers::NS16550::printf("ata%d: %s/%s %s (%d MB, %d sectors)\n",
                             drive, channel, role, model, size_mb, sectors);
}

// =====================================================================
// ATA Bus-Mastering DMA
// =====================================================================

static struct {
    bool     valid;
    uint32_t bmide_base;  // Bus Master IDE base address (from PCI BAR4)
    uint64_t prdt_phys;   // Physical address of PRDT
    uint16_t *prdt;       // Virtual address of PRDT (256 entries, one page)
} dma_state;

static const uint32_t PRDT_MAX_ENTRIES = 256;

static bool init_bmide() {
    if (dma_state.valid) return true;

    // Scan PCI for IDE controller (class 0x01, subclass 0x01)
    for (uint32_t bus = 0; bus < 256; bus++) {
        for (uint32_t slot = 0; slot < 32; slot++) {
            for (uint32_t func = 0; func < 8; func++) {
                uint16_t vendor = arch::x86::PCI::config_read_word(bus, slot, func, 0);
                if (vendor == 0xFFFF) {
                    if (func == 0) break;
                    continue;
                }

                uint32_t class_rev = arch::x86::PCI::config_read_dword(bus, slot, func, 0x08);
                uint8_t class_code = (class_rev >> 24) & 0xFF;
                uint8_t subclass  = (class_rev >> 16) & 0xFF;

                if (class_code == 0x01 && subclass == 0x01) {
                    uint32_t bar4 = arch::x86::PCI::config_read_dword(bus, slot, func, 0x20);
                    if (bar4 & 0x01) {
                        dma_state.bmide_base = bar4 & 0xFFF0;
                        if (dma_state.bmide_base == 0) continue;

                        // Enable bus mastering in PCI command register
                        uint16_t cmd = arch::x86::PCI::config_read_word(bus, slot, func, 0x04);
                        cmd |= 0x04; // Bus Master enable
                        arch::x86::PCI::config_write_word(bus, slot, func, 0x04, cmd);

                        // Allocate PRDT (one page of physically contiguous memory)
                        dma_state.prdt = reinterpret_cast<uint16_t *>(mm::PMM::alloc_frame());
                        if (!dma_state.prdt) return false;
                        lib::memset(dma_state.prdt, 0, mm::Paging::PAGE_SIZE);
                        dma_state.prdt_phys = mm::Paging::get_phys(reinterpret_cast<uint64_t>(dma_state.prdt));
                        if (!(dma_state.prdt_phys & 0xFFF)) {
                            // Already page-aligned
                        }

                        dma_state.valid = true;
                        drivers::NS16550::printf("ata: BMIDE at 0x%x, PRDT at 0x%x\n",
                            dma_state.bmide_base, dma_state.prdt_phys);
                        return true;
                    }
                }

                if (func == 0) break;
            }
        }
    }

    drivers::NS16550::write("ata: no IDE controller found for DMA\n");
    return false;
}

bool dma_available(int drive) {
    (void)drive;
    return dma_state.valid;
}

bool dma_read(int drive, uint32_t lba, uint8_t count_, void *buf) {
    if (drive < 0 || drive >= MAX_DRIVES || !buf) return false;
    uint32_t count = (count_ == 0) ? 256 : count_;
    if (count == 0) return false;

    if (!init_bmide()) return false;

    int base = base_port(drive);
    int slave = slave_bit(drive);

    // Build PRDT
    if (count > PRDT_MAX_ENTRIES) count = PRDT_MAX_ENTRIES;

    uint64_t buf_base = mm::Paging::get_phys(reinterpret_cast<uint64_t>(buf));
    uint32_t entries = 0;

    for (uint32_t s = 0; s < count; s++) {
        uint32_t offset = entries * 8;
        uint64_t entry_buf_phys = buf_base + s * 512;
        dma_state.prdt[offset / 2]     = entry_buf_phys & 0xFFFF;
        dma_state.prdt[offset / 2 + 1] = (entry_buf_phys >> 16) & 0xFFFF;
        dma_state.prdt[offset / 2 + 2] = 0x1FF | 0x8000; // byte_count-1 + EOT
        dma_state.prdt[offset / 2 + 3] = 0;               // reserved
        // Clear EOT from previous entry
        if (entries > 0)
            dma_state.prdt[offset / 2 - 2] &= ~0x8000;
        entries++;
    }
    // Stop any previous DMA
    arch::x86::outb(dma_state.bmide_base + 0, 0x00);
    io_wait();

    // Write PRDT physical address
    arch::x86::outl(dma_state.bmide_base + 4, dma_state.prdt_phys);
    io_wait();

    // Clear interrupt status
    arch::x86::outb(dma_state.bmide_base + 2, 0x04);
    io_wait();

    // Program ATA registers
    if (!wait_ready(drive, 1000)) return false;

    uint8_t dh = 0xE0 | (slave << 4) | ((lba >> 24) & 0x0F);
    arch::x86::outb(base + 6, dh);
    io_wait();

    arch::x86::outb(base + 2, count_);
    io_wait();
    arch::x86::outb(base + 3, lba & 0xFF);
    io_wait();
    arch::x86::outb(base + 4, (lba >> 8) & 0xFF);
    io_wait();
    arch::x86::outb(base + 5, (lba >> 16) & 0xFF);
    io_wait();

    // Enable DMA on device (bit 0 of feature register, or use SETFEATURES)
    // For ATA, we need to set the READ_DMA command
    // Write command: READ_DMA = 0xC8
    arch::x86::outb(base + 7, 0xC8);
    io_wait();

    // Start DMA (bit 0 = 1, bit 3 = 1 for read from device)
    arch::x86::outb(dma_state.bmide_base + 0, 0x09); // start + read
    io_wait();

    // Wait for DMA to complete
    for (int timeout = 0; timeout < 50000; timeout++) {
        io_wait();
        uint8_t bmstat = arch::x86::inb(dma_state.bmide_base + 2);
        uint8_t ata_st = arch::x86::inb(base + 7);
        if (!(bmstat & 0x01)) {
            // DMA completed
            arch::x86::outb(dma_state.bmide_base + 0, 0x00); // stop
            arch::x86::outb(dma_state.bmide_base + 2, 0x04); // clear interrupt
            if (bmstat & 0x02) {
                drivers::NS16550::printf("ata: DMA read error on drive %d, LBA %u\n", drive, lba);
                return false;
            }
            if (ata_st & 0x01) {
                drivers::NS16550::printf("ata: ATA error on drive %d, LBA %u, status=0x%02x\n", drive, lba, ata_st);
                return false;
            }
            return true;
        }
        // Check for ATA error
        if (ata_st & 0x01) {
            arch::x86::outb(dma_state.bmide_base + 0, 0x00); // stop
            arch::x86::outb(dma_state.bmide_base + 2, 0x04);
            drivers::NS16550::printf("ata: DMA read ATA error drive %d LBA %u status=0x%02x\n", drive, lba, ata_st);
            return false;
        }
    }

    // Timeout
    arch::x86::outb(dma_state.bmide_base + 0, 0x00); // stop
    arch::x86::outb(dma_state.bmide_base + 2, 0x04); // clear interrupt
    drivers::NS16550::printf("ata: DMA read timeout on drive %d, LBA %u\n", drive, lba);
    return false;
}

bool dma_write(int drive, uint32_t lba, uint8_t count_, const void *buf) {
    if (drive < 0 || drive >= MAX_DRIVES || !buf) return false;
    uint32_t count = (count_ == 0) ? 256 : count_;
    if (count == 0) return false;

    if (!init_bmide()) return false;

    int base = base_port(drive);
    int slave = slave_bit(drive);

    // Build PRDT
    if (count > PRDT_MAX_ENTRIES) count = PRDT_MAX_ENTRIES;

    uint64_t buf_base = mm::Paging::get_phys(reinterpret_cast<uint64_t>(buf));
    uint32_t entries = 0;

    for (uint32_t s = 0; s < count; s++) {
        uint32_t offset = entries * 8;
        uint64_t entry_buf_phys = buf_base + s * 512;
        dma_state.prdt[offset / 2]     = entry_buf_phys & 0xFFFF;
        dma_state.prdt[offset / 2 + 1] = (entry_buf_phys >> 16) & 0xFFFF;
        dma_state.prdt[offset / 2 + 2] = 0x1FF | 0x8000; // byte_count-1 + EOT
        dma_state.prdt[offset / 2 + 3] = 0;               // reserved
        // Clear EOT from previous entry
        if (entries > 0)
            dma_state.prdt[offset / 2 - 2] &= ~0x8000;
        entries++;
    }

    // Stop any previous DMA
    arch::x86::outb(dma_state.bmide_base + 0, 0x00);
    io_wait();

    // Write PRDT physical address
    arch::x86::outl(dma_state.bmide_base + 4, dma_state.prdt_phys);
    io_wait();

    // Clear interrupt status
    arch::x86::outb(dma_state.bmide_base + 2, 0x04);
    io_wait();

    // Program ATA registers
    if (!wait_ready(drive, 1000)) return false;

    uint8_t dh = 0xE0 | (slave << 4) | ((lba >> 24) & 0x0F);
    arch::x86::outb(base + 6, dh);
    io_wait();

    arch::x86::outb(base + 2, count_);
    io_wait();
    arch::x86::outb(base + 3, lba & 0xFF);
    io_wait();
    arch::x86::outb(base + 4, (lba >> 8) & 0xFF);
    io_wait();
    arch::x86::outb(base + 5, (lba >> 16) & 0xFF);
    io_wait();

    // WRITE_DMA = 0xCA
    arch::x86::outb(base + 7, 0xCA);
    io_wait();

    // Start DMA (bit 0 = 1, bit 3 = 0 for write to device)
    arch::x86::outb(dma_state.bmide_base + 0, 0x01);
    io_wait();

    // Wait for completion
    for (int timeout = 0; timeout < 50000; timeout++) {
        io_wait();
        uint8_t bmstat = arch::x86::inb(dma_state.bmide_base + 2);
        uint8_t ata_st = arch::x86::inb(base + 7);
        if (!(bmstat & 0x01)) {
            arch::x86::outb(dma_state.bmide_base + 0, 0x00);
            arch::x86::outb(dma_state.bmide_base + 2, 0x04);
            if (bmstat & 0x02) {
                drivers::NS16550::printf("ata: DMA write error on drive %d, LBA %u\n", drive, lba);
                return false;
            }
            if (ata_st & 0x01) {
                drivers::NS16550::printf("ata: ATA write error on drive %d, LBA %u, status=0x%02x\n", drive, lba, ata_st);
                return false;
            }
            return true;
        }
        if (ata_st & 0x01) {
            arch::x86::outb(dma_state.bmide_base + 0, 0x00);
            arch::x86::outb(dma_state.bmide_base + 2, 0x04);
            drivers::NS16550::printf("ata: DMA write ATA error drive %d LBA %u status=0x%02x\n", drive, lba, ata_st);
            return false;
        }
    }

    arch::x86::outb(dma_state.bmide_base + 0, 0x00);
    arch::x86::outb(dma_state.bmide_base + 2, 0x04);
    drivers::NS16550::printf("ata: DMA write timeout on drive %d, LBA %u\n", drive, lba);
    return false;
}

} // namespace ata_pio
} // namespace drivers

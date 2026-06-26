#ifndef ELITRA_ATA_PIO_HPP
#define ELITRA_ATA_PIO_HPP

#include <cstdint>

namespace drivers {
namespace ata_pio {

static const int PRIMARY_BASE   = 0x1F0;
static const int PRIMARY_CTRL   = 0x3F6;
static const int SECONDARY_BASE = 0x170;
static const int SECONDARY_CTRL = 0x376;

static const int DRIVE_MASTER = 0;
static const int DRIVE_SLAVE  = 1;

static const int MAX_DRIVES = 4;

void init();

int  drive_count();
bool present(int drive);
bool identify(int drive, uint16_t *buf);
bool read(int drive, uint32_t lba, uint8_t count, void *buf);
bool write(int drive, uint32_t lba, uint8_t count, const void *buf);

struct Partition {
    bool     valid;
    uint8_t  type;
    uint32_t lba_start;
    uint32_t sector_count;
};

int find_partitions(int drive, Partition *parts, int max_parts);

// Write-back: track dirty sectors in the FAT32 partition buffer
bool mount_partition_buffer(int drive, uint32_t lba_start, uint32_t sectors, uint8_t *buffer);
void mark_dirty(uint32_t byte_offset, uint32_t size);
void flush();
void print_info(int drive);

// DMA support
bool dma_available(int drive);
bool dma_read(int drive, uint32_t lba, uint8_t count, void *buf);
bool dma_write(int drive, uint32_t lba, uint8_t count, const void *buf);

} // namespace ata_pio
} // namespace drivers

#endif

#ifndef ELITRA_FAT32_HPP
#define ELITRA_FAT32_HPP

#include <cstdint>
#include <cstddef>

namespace fs {
namespace fat32 {

struct DirEntry {
    char     name[11];
    uint8_t  attr;
    uint8_t  nt_res;
    uint8_t  ctime_tenth;
    uint16_t ctime;
    uint16_t cdate;
    uint16_t adate;
    uint16_t cluster_hi;
    uint16_t mtime;
    uint16_t mdate;
    uint16_t cluster_lo;
    uint32_t size;
} __attribute__((packed));

static const uint8_t ATTR_DIRECTORY = 0x10;
static const uint8_t ATTR_ARCHIVE   = 0x20;
static const uint8_t ATTR_LFN       = 0x0F;

static const uint32_t FAT32_EOC      = 0x0FFFFFF8;
static const uint32_t FAT32_FREE     = 0x00000000;
static const uint32_t FAT32_BAD      = 0x0FFFFFF7;

typedef void (*WriteCallback)(struct Instance *fs, uint32_t byte_offset, uint32_t size);

struct Instance {
    uint8_t  *image;
    size_t    image_size;
    uint16_t  bytes_per_sector;
    uint8_t   sectors_per_cluster;
    uint16_t  reserved_sectors;
    uint8_t   num_fats;
    uint32_t  sectors_per_fat;
    uint32_t  root_cluster;
    uint32_t  first_data_sector;
    uint32_t  first_fat_sector;
    uint32_t  total_clusters;

    WriteCallback write_callback;
};

bool init(Instance *fs, const uint8_t *image, size_t image_size);
bool mount(Instance *fs, const char *vfs_path);

uint32_t alloc_cluster(Instance *fs);
uint32_t get_fat_entry(Instance *fs, uint32_t cluster);
void     name_to_sfn(const char *long_name, char sfn[11]);

uint32_t resolve_dir_cluster(Instance *fs, const char *vfs_path, const char *mount_point);

int  write_file(Instance *fs, uint32_t dir_cluster, const char *name, const uint8_t *data, uint32_t size);
int  create_dir(Instance *fs, uint32_t dir_cluster, const char *name);
int  delete_file(Instance *fs, uint32_t dir_cluster, const char *name);
int  delete_dir(Instance *fs, uint32_t dir_cluster, const char *name);
int  rename_entry(Instance *fs, uint32_t old_dir_cluster, const char *old_name, uint32_t new_dir_cluster, const char *new_name);

} // namespace fat32
} // namespace fs

#endif

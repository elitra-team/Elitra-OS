#ifndef ELITRA_EXT2_HPP
#define ELITRA_EXT2_HPP

#include <cstdint>
#include <cstddef>

namespace fs {
namespace ext2 {

struct Superblock {
    uint32_t inodes_count;
    uint32_t blocks_count;
    uint32_t r_blocks_count;
    uint32_t free_blocks_count;
    uint32_t free_inodes_count;
    uint32_t first_data_block;
    uint32_t log_block_size;
    uint32_t log_frag_size;
    uint32_t blocks_per_group;
    uint32_t frags_per_group;
    uint32_t inodes_per_group;
    uint32_t mtime;
    uint32_t wtime;
    uint16_t mnt_count;
    uint16_t max_mnt_count;
    uint16_t magic;
    uint16_t state;
    uint16_t errors;
    uint16_t minor_rev_level;
    uint32_t lastcheck;
    uint32_t checkinterval;
    uint32_t creator_os;
    uint32_t rev_level;
    uint16_t def_resuid;
    uint16_t def_resgid;
} __attribute__((packed));

struct BlockGroupDescriptor {
    uint32_t block_bitmap;
    uint32_t inode_bitmap;
    uint32_t inode_table;
    uint16_t free_blocks_count;
    uint16_t free_inodes_count;
    uint16_t used_dirs_count;
    uint16_t padding;
} __attribute__((packed));

struct Inode {
    uint16_t mode;
    uint16_t uid;
    uint32_t size;
    uint32_t atime;
    uint32_t ctime;
    uint32_t mtime;
    uint32_t dtime;
    uint16_t gid;
    uint16_t links_count;
    uint32_t blocks;
    uint32_t flags;
    uint32_t osd1;
    uint32_t block[15];
    uint32_t generation;
    uint32_t file_acl;
    uint32_t dir_acl;
    uint32_t faddr;
    uint32_t osd2[3];
} __attribute__((packed));

struct DirEntry {
    uint32_t inode;
    uint16_t rec_len;
    uint8_t  name_len;
    uint8_t  file_type;
    char     name[];
} __attribute__((packed));

static const uint16_t EXT2_MAGIC = 0xEF53;
static const uint16_t EXT2_S_IFMT   = 0xF000;
static const uint16_t EXT2_S_IFDIR  = 0x4000;
static const uint16_t EXT2_S_IFREG  = 0x8000;
static const uint16_t EXT2_S_IFLNK  = 0xA000;
static const uint8_t  EXT2_FT_DIR   = 2;
static const uint8_t  EXT2_FT_REG   = 1;

typedef void (*Ext2WriteCallback)(uint32_t offset, uint32_t size);

struct Instance {
    uint8_t  *image;
    size_t    image_size;
    uint32_t  block_size;
    uint32_t  inodes_per_group;
    uint32_t  blocks_per_group;
    uint32_t  inode_size;
    uint32_t  inode_table_blocks;
    uint32_t  first_data_block;
    uint32_t  bgdt_block;
    uint32_t  block_bitmap_start;
    uint32_t  inode_bitmap_start;
    uint32_t  inode_table_start;
    Ext2WriteCallback write_callback;
};

bool init(Instance *fs, const uint8_t *image, size_t image_size);
bool mount(Instance *fs, const char *vfs_path);

// Write operations
bool write_file(Instance *fs, const char *path, const uint8_t *data, uint32_t size);
bool mkdir(Instance *fs, const char *path);
bool unlink(Instance *fs, const char *path);
bool rmdir(Instance *fs, const char *path);
bool rename(Instance *fs, const char *old_path, const char *new_path);

} // namespace ext2
} // namespace fs

#endif

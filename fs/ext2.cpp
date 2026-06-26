#include "ext2.hpp"
#include "vfs.hpp"
#include "mount.hpp"
#include "lib.hpp"
#include "ns16550.hpp"
#include "heap.hpp"

using namespace fs;
using namespace fs::ext2;

static const uint8_t *read_block(const Instance *fs, uint32_t block_num) {
    uint64_t offset = static_cast<uint64_t>(block_num) * fs->block_size;
    if (offset + fs->block_size > fs->image_size)
        return nullptr;
    return fs->image + offset;
}

static int find_bg(const Instance *fs, uint32_t inode_num, BlockGroupDescriptor *bg) {
    uint32_t bg_id = (inode_num - 1) / fs->inodes_per_group;
    uint32_t bgdt_block = fs->bgdt_block;
    uint32_t bg_per_block = fs->block_size / sizeof(BlockGroupDescriptor);
    uint32_t block_off = bg_id / bg_per_block;
    uint32_t entry_off = bg_id % bg_per_block;

    const uint8_t *data = read_block(fs, bgdt_block + block_off);
    if (!data) return -1;

    *bg = reinterpret_cast<const BlockGroupDescriptor *>(data)[entry_off];
    return 0;
}

static const Inode *read_inode(const Instance *fs, uint32_t inode_num) {
    BlockGroupDescriptor bg;
    if (find_bg(fs, inode_num, &bg) < 0) return nullptr;

    uint32_t inode_index = (inode_num - 1) % fs->inodes_per_group;
    uint32_t inodes_per_block = fs->block_size / fs->inode_size;
    uint32_t block_off = inode_index / inodes_per_block;
    uint32_t entry_off = (inode_index % inodes_per_block) * fs->inode_size;

    const uint8_t *data = read_block(fs, bg.inode_table + block_off);
    if (!data) return nullptr;

    return reinterpret_cast<const Inode *>(data + entry_off);
}

static bool read_block_data(const Instance *fs, const Inode *inode, uint32_t block_index, uint8_t *out) {
    // Direct blocks (0-11)
    if (block_index < 12) {
        uint32_t b = inode->block[block_index];
        if (b == 0) return false;
        const uint8_t *data = read_block(fs, b);
        if (!data) return false;
        lib::memcpy(out, data, fs->block_size);
        return true;
    }

    block_index -= 12;
    uint32_t ptrs_per_block = fs->block_size / 4;

    // Single indirect (12)
    if (block_index < ptrs_per_block) {
        const uint8_t *indirect = read_block(fs, inode->block[12]);
        if (!indirect) return false;
        uint32_t b = reinterpret_cast<const uint32_t *>(indirect)[block_index];
        if (b == 0) return false;
        const uint8_t *data = read_block(fs, b);
        if (!data) return false;
        lib::memcpy(out, data, fs->block_size);
        return true;
    }

    block_index -= ptrs_per_block;

    // Double indirect (13)
    if (block_index < ptrs_per_block * ptrs_per_block) {
        const uint8_t *dindirect = read_block(fs, inode->block[13]);
        if (!dindirect) return false;
        uint32_t idx1 = block_index / ptrs_per_block;
        uint32_t idx2 = block_index % ptrs_per_block;
        uint32_t b1 = reinterpret_cast<const uint32_t *>(dindirect)[idx1];
        if (b1 == 0) return false;
        const uint8_t *indirect = read_block(fs, b1);
        if (!indirect) return false;
        uint32_t b = reinterpret_cast<const uint32_t *>(indirect)[idx2];
        if (b == 0) return false;
        const uint8_t *data = read_block(fs, b);
        if (!data) return false;
        lib::memcpy(out, data, fs->block_size);
        return true;
    }

    return false; // triple indirect not needed for common files
}

// Forward declaration
static VNode *mount_inode(Instance *fs, uint32_t inode_num, const char *vfs_name, VNode *parent);

static int mount_dir_entries(Instance *fs, uint32_t inode_num, VNode *parent) {
    const Inode *inode = read_inode(fs, inode_num);
    if (!inode || !(inode->mode & EXT2_S_IFDIR)) return -1;

    // Allocate buffer for one block
    uint8_t *block_buf = reinterpret_cast<uint8_t *>(mm::malloc(fs->block_size));
    if (!block_buf) return -1;

    uint32_t num_blocks = (inode->size + fs->block_size - 1) / fs->block_size;
    for (uint32_t bi = 0; bi < num_blocks; bi++) {
        if (!read_block_data(fs, inode, bi, block_buf)) continue;

        uint32_t offset = 0;
        while (offset < fs->block_size) {
            auto *de = reinterpret_cast<const DirEntry *>(block_buf + offset);
            if (de->rec_len < 8) break; // invalid
            if (de->inode != 0 && de->name_len > 0) {
                // Extract name
                char name[256];
                uint32_t nl = de->name_len;
                if (nl > 255) nl = 255;
                lib::memcpy(name, de->name, nl);
                name[nl] = '\0';

                if (name[0] != '.') {
                    mount_inode(fs, de->inode, name, parent);
                }
            }
            if (de->rec_len == 0) break;
            offset += de->rec_len;
        }
    }

    mm::free(block_buf);
    return 0;
}

static VNode *mount_inode(Instance *fs, uint32_t inode_num, const char *vfs_name, VNode *parent) {
    const Inode *inode = read_inode(fs, inode_num);
    if (!inode) return nullptr;

    if (VFS::find_child(parent, vfs_name)) return nullptr;

    if (inode->mode & EXT2_S_IFDIR) {
        VNode *dir = VFS::create_node(vfs_name, NodeType::DIRECTORY);
        dir->parent = parent;
        VFS::add_child(parent, dir);
        mount_dir_entries(fs, inode_num, dir);
        return dir;
    }

    if (inode->mode & EXT2_S_IFREG) {
        uint32_t file_size = inode->size;
        uint8_t *file_data = nullptr;

        if (file_size > 0) {
            file_data = reinterpret_cast<uint8_t *>(mm::malloc(file_size));
            if (!file_data) return nullptr;

            uint32_t num_blocks = (file_size + fs->block_size - 1) / fs->block_size;
            uint32_t remaining = file_size;
            uint32_t offset = 0;
            uint8_t *block_buf = reinterpret_cast<uint8_t *>(mm::malloc(fs->block_size));

            for (uint32_t bi = 0; bi < num_blocks && remaining > 0; bi++) {
                if (!read_block_data(fs, inode, bi, block_buf)) break;
                uint32_t chunk = (remaining < fs->block_size) ? remaining : fs->block_size;
                lib::memcpy(file_data + offset, block_buf, chunk);
                offset += chunk;
                remaining -= chunk;
            }
            mm::free(block_buf);

            if (offset < file_size) {
                // Read less than expected, truncate
                mm::free(file_data);
                file_data = nullptr;
                return nullptr;
            }
        }

        VNode *file = VFS::create_node(vfs_name, NodeType::FILE);
        file->parent = parent;
        file->size = file_size;
        file->data = file_data;
        VFS::add_child(parent, file);
        return file;
    }

    // Symlink or other type - skip
    return nullptr;
}

bool ext2::init(Instance *fs, const uint8_t *image, size_t image_size) {
    lib::memset(fs, 0, sizeof(*fs));

    if (image_size < 2048) return false;

    // Superblock at offset 1024
    const auto *sb = reinterpret_cast<const Superblock *>(image + 1024);
    if (sb->magic != EXT2_MAGIC) {
        drivers::NS16550::printf("ext2: bad magic 0x%04x\n", sb->magic);
        return false;
    }

    fs->image = const_cast<uint8_t *>(image);
    fs->image_size = image_size;
    fs->block_size = 1024 << sb->log_block_size;
    fs->inodes_per_group = sb->inodes_per_group;
    fs->blocks_per_group = sb->blocks_per_group;
    fs->first_data_block = sb->first_data_block;

    // Inode size (rev 1+)
    if (sb->rev_level >= 1) {
        fs->inode_size = *(reinterpret_cast<const uint16_t *>(image + 1024 + 128));
    } else {
        fs->inode_size = 128;
    }

    // BGDT block location
    if (fs->block_size == 1024) {
        fs->bgdt_block = 2; // block 2 (after boot block + superblock)
    } else {
        fs->bgdt_block = 1;
    }

    drivers::NS16550::printf("ext2: block_size=%d inodes_per_group=%d bgdt_block=%d inode_size=%d\n",
                            fs->block_size, fs->inodes_per_group, fs->bgdt_block, fs->inode_size);
    return true;
}

bool ext2::mount(Instance *fs, const char *vfs_path) {
    if (!fs || !fs->image) return false;

    VNode *parent = VFS::resolve(vfs_path);
    if (!parent) {
        if (lib::strcmp(vfs_path, "/") == 0)
            parent = VFS::root_node();
        if (!parent) return false;
    }

    VNode *root = mount_inode(fs, 2, vfs_path + 1, parent);
    return root != nullptr;
}

// =====================================================================
// NEW WRITE OPERATIONS
// =====================================================================

static uint8_t *write_block_ptr(Instance *fs, uint32_t block_num) {
    uint64_t offset = static_cast<uint64_t>(block_num) * fs->block_size;
    if (offset + fs->block_size > fs->image_size)
        return nullptr;
    if (fs->write_callback)
        fs->write_callback(offset, fs->block_size);
    return fs->image + offset;
}

static int find_bg_by_index(const Instance *fs, uint32_t bg_id, BlockGroupDescriptor *bg) {
    uint32_t bgdt_block = fs->bgdt_block;
    uint32_t bg_per_block = fs->block_size / sizeof(BlockGroupDescriptor);
    uint32_t block_off = bg_id / bg_per_block;
    uint32_t entry_off = bg_id % bg_per_block;

    const uint8_t *data = read_block(fs, bgdt_block + block_off);
    if (!data) return -1;

    *bg = reinterpret_cast<const BlockGroupDescriptor *>(data)[entry_off];
    return 0;
}

static inline bool bitmap_test(const uint8_t *bm, uint32_t bit) {
    return bm[bit / 8] & (1 << (bit % 8));
}

static inline void bitmap_set(uint8_t *bm, uint32_t bit) {
    bm[bit / 8] |= (1 << (bit % 8));
}

static inline void bitmap_clear(uint8_t *bm, uint32_t bit) {
    bm[bit / 8] &= ~(1 << (bit % 8));
}

static uint32_t alloc_block(Instance *fs) {
    Superblock *sb = reinterpret_cast<Superblock *>(fs->image + 1024);
    uint32_t total_blocks = sb->blocks_count;
    uint32_t num_groups = (total_blocks + fs->blocks_per_group - 1) / fs->blocks_per_group;

    for (uint32_t bg_id = 0; bg_id < num_groups; bg_id++) {
        BlockGroupDescriptor bg;
        if (find_bg_by_index(fs, bg_id, &bg) < 0) continue;
        if (bg.free_blocks_count == 0) continue;

        uint8_t *bitmap = write_block_ptr(fs, bg.block_bitmap);
        if (!bitmap) continue;

        uint32_t start = bg_id * fs->blocks_per_group;
        uint32_t end = start + fs->blocks_per_group;
        if (end > total_blocks) end = total_blocks;
        uint32_t num_bits = end - start;

        for (uint32_t bit = 0; bit < num_bits; bit++) {
            if (!bitmap_test(bitmap, bit)) {
                bitmap_set(bitmap, bit);
                uint32_t block_num = start + bit;

                // Update BG descriptor
                bg.free_blocks_count--;
                uint32_t bg_per_block = fs->block_size / sizeof(BlockGroupDescriptor);
                uint32_t block_off = bg_id / bg_per_block;
                uint32_t entry_off = bg_id % bg_per_block;
                uint8_t *bg_data = write_block_ptr(fs, fs->bgdt_block + block_off);
                if (bg_data)
                    reinterpret_cast<BlockGroupDescriptor *>(bg_data)[entry_off] = bg;

                // Decrement superblock free count
                sb->free_blocks_count--;
                if (fs->write_callback)
                    fs->write_callback(1024, sizeof(Superblock));

                // Zero out the new block
                uint8_t *new_block = write_block_ptr(fs, block_num);
                if (new_block)
                    lib::memset(new_block, 0, fs->block_size);

                return block_num;
            }
        }
    }
    return 0;
}

static void free_block(Instance *fs, uint32_t block_num) {
    if (block_num == 0) return;
    Superblock *sb = reinterpret_cast<Superblock *>(fs->image + 1024);
    uint32_t bg_id = block_num / fs->blocks_per_group;
    uint32_t bit = block_num % fs->blocks_per_group;

    BlockGroupDescriptor bg;
    if (find_bg_by_index(fs, bg_id, &bg) < 0) return;

    uint8_t *bitmap = write_block_ptr(fs, bg.block_bitmap);
    if (!bitmap) return;

    bitmap_clear(bitmap, bit);
    bg.free_blocks_count++;
    sb->free_blocks_count++;

    // Write back BG descriptor
    uint32_t bg_per_block = fs->block_size / sizeof(BlockGroupDescriptor);
    uint32_t block_off = bg_id / bg_per_block;
    uint32_t entry_off = bg_id % bg_per_block;
    uint8_t *bg_data = write_block_ptr(fs, fs->bgdt_block + block_off);
    if (bg_data)
        reinterpret_cast<BlockGroupDescriptor *>(bg_data)[entry_off] = bg;

    if (fs->write_callback)
        fs->write_callback(1024, sizeof(Superblock));
}

static uint32_t alloc_inode(Instance *fs) {
    Superblock *sb = reinterpret_cast<Superblock *>(fs->image + 1024);
    uint32_t total_inodes = sb->inodes_count;
    uint32_t num_groups = (total_inodes + fs->inodes_per_group - 1) / fs->inodes_per_group;

    for (uint32_t bg_id = 0; bg_id < num_groups; bg_id++) {
        BlockGroupDescriptor bg;
        if (find_bg_by_index(fs, bg_id, &bg) < 0) continue;
        if (bg.free_inodes_count == 0) continue;

        uint8_t *bitmap = write_block_ptr(fs, bg.inode_bitmap);
        if (!bitmap) continue;

        uint32_t start = bg_id * fs->inodes_per_group + 1;
        uint32_t end = start + fs->inodes_per_group;
        if (end > total_inodes + 1) end = total_inodes + 1;
        uint32_t num_bits = end - start;

        for (uint32_t bit = 0; bit < num_bits; bit++) {
            if (!bitmap_test(bitmap, bit)) {
                bitmap_set(bitmap, bit);
                uint32_t inode_num = start + bit;

                // Update BG descriptor
                bg.free_inodes_count--;
                uint32_t bg_per_block = fs->block_size / sizeof(BlockGroupDescriptor);
                uint32_t block_off = bg_id / bg_per_block;
                uint32_t entry_off = bg_id % bg_per_block;
                uint8_t *bg_data = write_block_ptr(fs, fs->bgdt_block + block_off);
                if (bg_data)
                    reinterpret_cast<BlockGroupDescriptor *>(bg_data)[entry_off] = bg;

                sb->free_inodes_count--;
                if (fs->write_callback)
                    fs->write_callback(1024, sizeof(Superblock));

                return inode_num;
            }
        }
    }
    return 0;
}

static void free_inode(Instance *fs, uint32_t inode_num) {
    if (inode_num < 2) return;
    Superblock *sb = reinterpret_cast<Superblock *>(fs->image + 1024);
    uint32_t bg_id = (inode_num - 1) / fs->inodes_per_group;
    uint32_t bit = (inode_num - 1) % fs->inodes_per_group;

    BlockGroupDescriptor bg;
    if (find_bg_by_index(fs, bg_id, &bg) < 0) return;

    uint8_t *bitmap = write_block_ptr(fs, bg.inode_bitmap);
    if (!bitmap) return;

    bitmap_clear(bitmap, bit);
    bg.free_inodes_count++;
    sb->free_inodes_count++;

    // Write back BG descriptor
    uint32_t bg_per_block = fs->block_size / sizeof(BlockGroupDescriptor);
    uint32_t block_off = bg_id / bg_per_block;
    uint32_t entry_off = bg_id % bg_per_block;
    uint8_t *bg_data = write_block_ptr(fs, fs->bgdt_block + block_off);
    if (bg_data)
        reinterpret_cast<BlockGroupDescriptor *>(bg_data)[entry_off] = bg;

    if (fs->write_callback)
        fs->write_callback(1024, sizeof(Superblock));
}

static int write_inode_to_disk(Instance *fs, uint32_t inode_num, const Inode *inode) {
    BlockGroupDescriptor bg;
    if (find_bg(fs, inode_num, &bg) < 0) return -1;

    uint32_t inode_index = (inode_num - 1) % fs->inodes_per_group;
    uint32_t inodes_per_block = fs->block_size / fs->inode_size;
    uint32_t block_off = inode_index / inodes_per_block;
    uint32_t entry_off = (inode_index % inodes_per_block) * fs->inode_size;

    uint8_t *data = write_block_ptr(fs, bg.inode_table + block_off);
    if (!data) return -1;

    lib::memcpy(data + entry_off, inode, fs->inode_size);
    return 0;
}

static uint32_t resolve_block(Inode *inode, Instance *fs, uint32_t block_index) {
    if (block_index < 12)
        return inode->block[block_index];

    block_index -= 12;
    uint32_t ptrs_per_block = fs->block_size / 4;

    if (block_index < ptrs_per_block) {
        if (inode->block[12] == 0) return 0;
        const uint8_t *indirect = read_block(fs, inode->block[12]);
        if (!indirect) return 0;
        return reinterpret_cast<const uint32_t *>(indirect)[block_index];
    }

    block_index -= ptrs_per_block;

    if (block_index < ptrs_per_block * ptrs_per_block) {
        if (inode->block[13] == 0) return 0;
        const uint8_t *dindirect = read_block(fs, inode->block[13]);
        if (!dindirect) return 0;
        uint32_t idx1 = block_index / ptrs_per_block;
        uint32_t idx2 = block_index % ptrs_per_block;
        uint32_t b1 = reinterpret_cast<const uint32_t *>(dindirect)[idx1];
        if (b1 == 0) return 0;
        const uint8_t *indirect = read_block(fs, b1);
        if (!indirect) return 0;
        return reinterpret_cast<const uint32_t *>(indirect)[idx2];
    }

    return 0;
}

static int append_block(Instance *fs, Inode *inode, uint32_t inode_num) {
    uint32_t block_factor = fs->block_size / 512;
    uint32_t block_index = inode->blocks / block_factor;
    uint32_t ptrs_per_block = fs->block_size / 4;

    uint32_t block_num = alloc_block(fs);
    if (block_num == 0) return -1;

    if (block_index < 12) {
        inode->block[block_index] = block_num;
    } else if (block_index < 12 + ptrs_per_block) {
        if (inode->block[12] == 0) {
            inode->block[12] = alloc_block(fs);
            if (inode->block[12] == 0) {
                free_block(fs, block_num);
                return -1;
            }
        }
        uint32_t indirect_idx = block_index - 12;
        uint8_t *indirect = write_block_ptr(fs, inode->block[12]);
        if (!indirect) {
            free_block(fs, block_num);
            return -1;
        }
        reinterpret_cast<uint32_t *>(indirect)[indirect_idx] = block_num;
    } else if (block_index < 12 + ptrs_per_block + ptrs_per_block * ptrs_per_block) {
        if (inode->block[13] == 0) {
            inode->block[13] = alloc_block(fs);
            if (inode->block[13] == 0) {
                free_block(fs, block_num);
                return -1;
            }
        }
        uint32_t idx = block_index - 12 - ptrs_per_block;
        uint32_t idx1 = idx / ptrs_per_block;
        uint32_t idx2 = idx % ptrs_per_block;

        uint8_t *dindirect = write_block_ptr(fs, inode->block[13]);
        if (!dindirect) {
            free_block(fs, block_num);
            return -1;
        }
        uint32_t b1 = reinterpret_cast<uint32_t *>(dindirect)[idx1];
        if (b1 == 0) {
            b1 = alloc_block(fs);
            if (b1 == 0) {
                free_block(fs, block_num);
                return -1;
            }
            reinterpret_cast<uint32_t *>(dindirect)[idx1] = b1;
        }
        uint8_t *indirect = write_block_ptr(fs, b1);
        if (!indirect) {
            free_block(fs, block_num);
            return -1;
        }
        reinterpret_cast<uint32_t *>(indirect)[idx2] = block_num;
    } else {
        free_block(fs, block_num);
        return -1;
    }

    inode->blocks += block_factor;
    write_inode_to_disk(fs, inode_num, inode);
    return static_cast<int>(block_num);
}

static int resolve_path(Instance *fs, const char *path, uint32_t *parent_inode_num, char *name_buf) {
    if (!path || !parent_inode_num || !name_buf) return -1;

    while (*path == '/') path++;
    if (!*path) return -1;

    uint32_t current_inode_num = 2;
    const Inode *current_inode = read_inode(fs, current_inode_num);
    if (!current_inode || !(current_inode->mode & EXT2_S_IFDIR)) return -1;

    uint8_t *block_buf = reinterpret_cast<uint8_t *>(mm::malloc(fs->block_size));
    if (!block_buf) return -1;

    char component[256];

    while (*path) {
        int comp_len = 0;
        while (*path && *path != '/') {
            if (comp_len < 255) component[comp_len++] = *path;
            path++;
        }
        component[comp_len] = '\0';

        while (*path == '/') path++;

        if (!*path) {
            lib::strncpy(name_buf, component, 255);
            *parent_inode_num = current_inode_num;
            mm::free(block_buf);
            return 0;
        }

        uint32_t num_blocks = (current_inode->size + fs->block_size - 1) / fs->block_size;
        bool found = false;

        for (uint32_t bi = 0; bi < num_blocks && !found; bi++) {
            if (!read_block_data(fs, current_inode, bi, block_buf)) continue;

            uint32_t offset = 0;
            while (offset < fs->block_size) {
                auto *de = reinterpret_cast<const DirEntry *>(block_buf + offset);
                if (de->rec_len < 8) break;
                if (de->inode != 0 && de->name_len == static_cast<uint8_t>(comp_len)) {
                    char dentry_name[256];
                    uint32_t nl = de->name_len;
                    if (nl > 255) nl = 255;
                    lib::memcpy(dentry_name, de->name, nl);
                    dentry_name[nl] = '\0';
                    if (lib::strcmp(component, dentry_name) == 0) {
                        current_inode_num = de->inode;
                        current_inode = read_inode(fs, current_inode_num);
                        found = true;
                        break;
                    }
                }
                if (de->rec_len == 0) break;
                offset += de->rec_len;
            }
        }

        if (!found) {
            mm::free(block_buf);
            return -1;
        }
        if (!current_inode || !(current_inode->mode & EXT2_S_IFDIR)) {
            mm::free(block_buf);
            return -1;
        }
    }

    mm::free(block_buf);
    return -1;
}

static uint32_t find_in_dir(Instance *fs, uint32_t dir_inode_num, const char *name) {
    const Inode *inode = read_inode(fs, dir_inode_num);
    if (!inode || !(inode->mode & EXT2_S_IFDIR)) return 0;

    uint32_t name_len = lib::strlen(name);
    uint8_t *block_buf = reinterpret_cast<uint8_t *>(mm::malloc(fs->block_size));
    if (!block_buf) return 0;

    uint32_t num_blocks = (inode->size + fs->block_size - 1) / fs->block_size;
    uint32_t result = 0;

    for (uint32_t bi = 0; bi < num_blocks && result == 0; bi++) {
        if (!read_block_data(fs, inode, bi, block_buf)) continue;

        uint32_t offset = 0;
        while (offset < fs->block_size) {
            auto *de = reinterpret_cast<const DirEntry *>(block_buf + offset);
            if (de->rec_len < 8) break;
            if (de->inode != 0 && de->name_len == static_cast<uint8_t>(name_len)) {
                char dentry_name[256];
                uint32_t nl = de->name_len;
                if (nl > 255) nl = 255;
                lib::memcpy(dentry_name, de->name, nl);
                dentry_name[nl] = '\0';
                if (lib::strcmp(name, dentry_name) == 0) {
                    result = de->inode;
                    break;
                }
            }
            if (de->rec_len == 0) break;
            offset += de->rec_len;
        }
    }

    mm::free(block_buf);
    return result;
}

static int add_dir_entry(Instance *fs, uint32_t dir_inode_num, uint32_t new_inode_num, const char *name, uint8_t file_type) {
    Inode *inode = const_cast<Inode *>(read_inode(fs, dir_inode_num));
    if (!inode || !(inode->mode & EXT2_S_IFDIR)) return -1;

    uint32_t name_len = lib::strlen(name);
    uint32_t entry_size = 8 + name_len;
    entry_size = (entry_size + 3) & ~3;
    if (entry_size < 8) entry_size = 8;

    uint8_t *block_buf = reinterpret_cast<uint8_t *>(mm::malloc(fs->block_size));
    if (!block_buf) return -1;

    bool added = false;
    uint32_t target_block = 0;
    uint32_t target_bi = 0;
    uint32_t insert_offset = 0;

    uint32_t num_blocks = (inode->size + fs->block_size - 1) / fs->block_size;

    for (uint32_t bi = 0; bi < num_blocks && !added; bi++) {
        if (!read_block_data(fs, inode, bi, block_buf)) continue;

        uint32_t offset = 0;
        while (offset < fs->block_size && !added) {
            auto *de = reinterpret_cast<DirEntry *>(block_buf + offset);
            if (de->rec_len < 8) {
                uint32_t remaining = fs->block_size - offset;
                if (remaining >= entry_size) {
                    insert_offset = offset;
                    target_bi = bi;
                    added = true;
                }
                break;
            }

            if (de->inode == 0) {
                if (de->rec_len >= entry_size) {
                    insert_offset = offset;
                    target_bi = bi;
                    added = true;
                }
                break;
            }

            uint32_t min_needed = (8 + de->name_len + 3) & ~3;
            uint32_t slack = de->rec_len - min_needed;
            if (slack >= entry_size) {
                insert_offset = offset + min_needed;
                de->rec_len = min_needed;
                target_bi = bi;
                added = true;
            }

            if (de->rec_len == 0) break;
            offset += de->rec_len;
        }

        if (added) {
            target_block = resolve_block(inode, fs, target_bi);
            if (target_block == 0) {
                added = false;
            }
        }
    }

    if (added) {
        if (target_block != 0) {
            // Re-read the block in case it was modified by the scan
            if (!read_block_data(fs, inode, target_bi, block_buf)) {
                mm::free(block_buf);
                return -1;
            }
            auto *new_de = reinterpret_cast<DirEntry *>(block_buf + insert_offset);
            new_de->inode = new_inode_num;
            new_de->name_len = static_cast<uint8_t>(name_len);
            new_de->file_type = file_type;
            lib::memcpy(new_de->name, name, name_len);

            // Compute how much space is left in the block after this entry
            uint32_t used = entry_size;
            uint32_t block_remain = fs->block_size - insert_offset;
            if (block_remain > used) {
                new_de->rec_len = static_cast<uint16_t>(block_remain);
            } else {
                new_de->rec_len = static_cast<uint16_t>(used);
            }

            uint8_t *dest = write_block_ptr(fs, target_block);
            if (dest)
                lib::memcpy(dest, block_buf, fs->block_size);
        }
        mm::free(block_buf);
        return 0;
    }

    // No space found -- append a new block
    int new_block = append_block(fs, inode, dir_inode_num);
    if (new_block < 0) {
        mm::free(block_buf);
        return -1;
    }

    // Re-read inode after append_block modified it
    inode = const_cast<Inode *>(read_inode(fs, dir_inode_num));

    lib::memset(block_buf, 0, fs->block_size);
    auto *new_de = reinterpret_cast<DirEntry *>(block_buf);
    new_de->inode = new_inode_num;
    new_de->rec_len = static_cast<uint16_t>(fs->block_size - 0);
    new_de->name_len = static_cast<uint8_t>(name_len);
    new_de->file_type = file_type;
    lib::memcpy(new_de->name, name, name_len);

    uint8_t *dest = write_block_ptr(fs, static_cast<uint32_t>(new_block));
    if (dest)
        lib::memcpy(dest, block_buf, fs->block_size);

    inode->size += fs->block_size;
    write_inode_to_disk(fs, dir_inode_num, inode);

    mm::free(block_buf);
    return 0;
}

static int remove_dir_entry(Instance *fs, uint32_t dir_inode_num, const char *name) {
    Inode *inode = const_cast<Inode *>(read_inode(fs, dir_inode_num));
    if (!inode || !(inode->mode & EXT2_S_IFDIR)) return -1;

    uint32_t name_len = lib::strlen(name);
    uint8_t *block_buf = reinterpret_cast<uint8_t *>(mm::malloc(fs->block_size));
    if (!block_buf) return -1;

    uint32_t num_blocks = (inode->size + fs->block_size - 1) / fs->block_size;
    bool found = false;
    int target_bi = -1;
    uint32_t target_offset = 0;
    uint32_t target_block = 0;
    uint32_t prev_offset = 0;

    for (int bi = 0; bi < static_cast<int>(num_blocks) && !found; bi++) {
        if (!read_block_data(fs, inode, bi, block_buf)) continue;

        uint32_t offset = 0;
        prev_offset = 0;

        while (offset < fs->block_size) {
            auto *de = reinterpret_cast<DirEntry *>(block_buf + offset);
            if (de->rec_len < 8) break;

            if (de->inode != 0 && de->name_len == static_cast<uint8_t>(name_len)) {
                char dentry_name[256];
                uint32_t nl = de->name_len;
                if (nl > 255) nl = 255;
                lib::memcpy(dentry_name, de->name, nl);
                dentry_name[nl] = '\0';
                if (lib::strcmp(name, dentry_name) == 0) {
                    found = true;
                    target_bi = bi;
                    target_offset = offset;
                    break;
                }
            }

            prev_offset = offset;
            if (de->rec_len == 0) break;
            offset += de->rec_len;
        }
    }

    if (!found) {
        mm::free(block_buf);
        return -1;
    }

    // Re-read the target block
    if (!read_block_data(fs, inode, target_bi, block_buf)) {
        mm::free(block_buf);
        return -1;
    }

    target_block = resolve_block(inode, fs, static_cast<uint32_t>(target_bi));
    if (target_block == 0) {
        mm::free(block_buf);
        return -1;
    }

    auto *de = reinterpret_cast<DirEntry *>(block_buf + target_offset);
    de->inode = 0;

    // Absorb any immediately following deleted entries
    uint32_t absorb_offset = target_offset + de->rec_len;
    while (absorb_offset < fs->block_size) {
        auto *next = reinterpret_cast<DirEntry *>(block_buf + absorb_offset);
        if (next->rec_len < 8) break;
        if (next->inode != 0) break;
        de->rec_len += next->rec_len;
        absorb_offset += next->rec_len;
    }

    // Try to merge with previous deleted entry
    if (prev_offset > 0) {
        auto *prev_de = reinterpret_cast<DirEntry *>(block_buf + prev_offset);
        uint32_t prev_end = prev_offset + prev_de->rec_len;
        if (prev_end == target_offset && prev_de->inode == 0) {
            prev_de->rec_len += de->rec_len;
            de = prev_de;
        }
    }

    uint8_t *dest = write_block_ptr(fs, target_block);
    if (dest)
        lib::memcpy(dest, block_buf, fs->block_size);

    mm::free(block_buf);
    return 0;
}

static void truncate_blocks_from(Instance *fs, Inode *inode, uint32_t inode_num, uint32_t new_block_count) {
    uint32_t block_factor = fs->block_size / 512;
    uint32_t ptrs_per_block = fs->block_size / 4;
    uint32_t old_block_count = inode->blocks / block_factor;

    if (new_block_count >= old_block_count) return;

    // Free direct blocks (0-11)
    for (uint32_t i = new_block_count; i < 12 && i < old_block_count; i++) {
        uint32_t phys = resolve_block(inode, fs, i);
        if (phys) {
            inode->block[i] = 0;
            free_block(fs, phys);
        }
    }

    // Free single indirect block and its entries
    if (new_block_count <= 12 && old_block_count > 12 && inode->block[12]) {
        uint32_t indirect_phys = inode->block[12];
        const uint8_t *indirect_data = read_block(fs, indirect_phys);
        if (indirect_data) {
            uint32_t max_idx = old_block_count - 12;
            if (max_idx > ptrs_per_block) max_idx = ptrs_per_block;
            for (uint32_t i = 0; i < max_idx; i++) {
                uint32_t b = reinterpret_cast<const uint32_t *>(indirect_data)[i];
                if (b) free_block(fs, b);
            }
        }
        inode->block[12] = 0;
        free_block(fs, indirect_phys);
    }

    // Free double indirect block and its entries
    if (new_block_count <= 12 + ptrs_per_block && old_block_count > 12 + ptrs_per_block && inode->block[13]) {
        uint32_t dindirect_phys = inode->block[13];
        const uint8_t *dindirect_data = read_block(fs, dindirect_phys);
        if (dindirect_data) {
            uint32_t max_idx = old_block_count - 12 - ptrs_per_block;
            if (max_idx > ptrs_per_block * ptrs_per_block)
                max_idx = ptrs_per_block * ptrs_per_block;
            for (uint32_t i = 0; i < max_idx; i++) {
                uint32_t b1 = reinterpret_cast<const uint32_t *>(dindirect_data)[i];
                if (b1 == 0) continue;
                const uint8_t *indirect_data = read_block(fs, b1);
                if (indirect_data) {
                    for (uint32_t j = 0; j < ptrs_per_block; j++) {
                        uint32_t b = reinterpret_cast<const uint32_t *>(indirect_data)[j];
                        if (b) free_block(fs, b);
                    }
                }
                free_block(fs, b1);
            }
        }
        inode->block[13] = 0;
        free_block(fs, dindirect_phys);
    }

    inode->blocks = new_block_count * block_factor;
    write_inode_to_disk(fs, inode_num, inode);
}

// =====================================================================
// PUBLIC WRITE OPERATIONS
// =====================================================================

bool ext2::write_file(Instance *fs, const char *path, const uint8_t *data, uint32_t size) {
    if (!fs || !path || !path[0]) return false;

    char name_buf[256];
    uint32_t parent_inode_num;

    if (resolve_path(fs, path, &parent_inode_num, name_buf) < 0)
        return false;

    uint32_t needed_blocks = (size + fs->block_size - 1) / fs->block_size;

    // Check if file already exists
    uint32_t existing_inode_num = find_in_dir(fs, parent_inode_num, name_buf);

    if (existing_inode_num != 0) {
        // File exists -- update in place
        Inode *inode = const_cast<Inode *>(read_inode(fs, existing_inode_num));
        if (!inode || (inode->mode & EXT2_S_IFDIR)) return false;

        uint32_t cur_blocks = (inode->size + fs->block_size - 1) / fs->block_size;

        // Free excess blocks if shrinking
        if (needed_blocks < cur_blocks)
            truncate_blocks_from(fs, inode, existing_inode_num, needed_blocks);

        // Re-read inode after truncation
        inode = const_cast<Inode *>(read_inode(fs, existing_inode_num));

        // Allocate additional blocks if growing
        for (uint32_t i = cur_blocks; i < needed_blocks; i++) {
            int phys = append_block(fs, inode, existing_inode_num);
            if (phys < 0) return false;
            inode = const_cast<Inode *>(read_inode(fs, existing_inode_num));
        }

        // Write data to each block
        uint32_t remaining = size;
        for (uint32_t i = 0; i < needed_blocks; i++) {
            uint32_t phys = resolve_block(inode, fs, i);
            if (phys == 0) return false;

            uint8_t *block_ptr = write_block_ptr(fs, phys);
            if (!block_ptr) return false;

            if (remaining >= fs->block_size) {
                lib::memcpy(block_ptr, data + i * fs->block_size, fs->block_size);
                remaining -= fs->block_size;
            } else {
                lib::memset(block_ptr, 0, fs->block_size);
                lib::memcpy(block_ptr, data + i * fs->block_size, remaining);
                remaining = 0;
            }
        }

        // Update inode size and times
        inode = const_cast<Inode *>(read_inode(fs, existing_inode_num));
        inode->size = size;
        inode->mtime = 0; // would use current time
        inode->ctime = 0;
        write_inode_to_disk(fs, existing_inode_num, inode);

        return true;
    }

    // File does not exist -- create new
    uint32_t new_inode_num = alloc_inode(fs);
    if (new_inode_num == 0) return false;

    Inode new_inode;
    lib::memset(&new_inode, 0, sizeof(new_inode));
    new_inode.mode = EXT2_S_IFREG | 0x1A4; // 0100644
    new_inode.uid = 0;
    new_inode.gid = 0;
    new_inode.size = size;
    new_inode.links_count = 1;
    new_inode.blocks = 0;

    if (write_inode_to_disk(fs, new_inode_num, &new_inode) < 0) {
        free_inode(fs, new_inode_num);
        return false;
    }

    if (add_dir_entry(fs, parent_inode_num, new_inode_num, name_buf, EXT2_FT_REG) < 0) {
        free_inode(fs, new_inode_num);
        return false;
    }

    // Write data blocks
    if (size > 0) {
        Inode *inode = const_cast<Inode *>(read_inode(fs, new_inode_num));

        // Update inode blocks for the data we're about to write
        uint32_t remaining = size;
        for (uint32_t i = 0; i < needed_blocks; i++) {
            int phys = append_block(fs, inode, new_inode_num);
            if (phys < 0) return false;

            uint8_t *block_ptr = write_block_ptr(fs, static_cast<uint32_t>(phys));
            if (!block_ptr) return false;

            if (remaining >= fs->block_size) {
                lib::memcpy(block_ptr, data + i * fs->block_size, fs->block_size);
                remaining -= fs->block_size;
            } else {
                lib::memset(block_ptr, 0, fs->block_size);
                lib::memcpy(block_ptr, data + i * fs->block_size, remaining);
                remaining = 0;
            }

            inode = const_cast<Inode *>(read_inode(fs, new_inode_num));
        }

        // Update size again (append_block may have changed it indirectly)
        inode = const_cast<Inode *>(read_inode(fs, new_inode_num));
        inode->size = size;
        write_inode_to_disk(fs, new_inode_num, inode);
    }

    return true;
}

bool ext2::mkdir(Instance *fs, const char *path) {
    if (!fs || !path || !path[0]) return false;

    char name_buf[256];
    uint32_t parent_inode_num;

    if (resolve_path(fs, path, &parent_inode_num, name_buf) < 0)
        return false;

    // Check if already exists
    if (find_in_dir(fs, parent_inode_num, name_buf) != 0)
        return false;

    uint32_t block_factor = fs->block_size / 512;

    // Allocate inode
    uint32_t new_inode_num = alloc_inode(fs);
    if (new_inode_num == 0) return false;

    // Allocate a data block for the directory
    uint32_t block_num = alloc_block(fs);
    if (block_num == 0) {
        free_inode(fs, new_inode_num);
        return false;
    }

    // Initialize the inode
    Inode new_inode;
    lib::memset(&new_inode, 0, sizeof(new_inode));
    new_inode.mode = EXT2_S_IFDIR | 0x1FF; // 040777
    new_inode.uid = 0;
    new_inode.gid = 0;
    new_inode.size = fs->block_size;
    new_inode.links_count = 2; // . and ..
    new_inode.blocks = block_factor;
    new_inode.block[0] = block_num;

    if (write_inode_to_disk(fs, new_inode_num, &new_inode) < 0) {
        free_block(fs, block_num);
        free_inode(fs, new_inode_num);
        return false;
    }

    // Write "." and ".." entries to the directory block
    uint8_t *block_buf = reinterpret_cast<uint8_t *>(mm::malloc(fs->block_size));
    if (!block_buf) {
        free_block(fs, block_num);
        free_inode(fs, new_inode_num);
        return false;
    }
    lib::memset(block_buf, 0, fs->block_size);

    // "." entry
    auto *de = reinterpret_cast<DirEntry *>(block_buf);
    de->inode = new_inode_num;
    de->rec_len = 12; // 8 + 1 (.) padded to 4 -> 12
    de->name_len = 1;
    de->file_type = EXT2_FT_DIR;
    de->name[0] = '.';

    // ".." entry
    auto *de2 = reinterpret_cast<DirEntry *>(block_buf + 12);
    de2->inode = parent_inode_num;
    de2->rec_len = fs->block_size - 12;
    de2->name_len = 2;
    de2->file_type = EXT2_FT_DIR;
    de2->name[0] = '.';
    de2->name[1] = '.';

    uint8_t *dest = write_block_ptr(fs, block_num);
    if (dest)
        lib::memcpy(dest, block_buf, fs->block_size);

    mm::free(block_buf);

    // Add dir entry in parent
    if (add_dir_entry(fs, parent_inode_num, new_inode_num, name_buf, EXT2_FT_DIR) < 0) {
        // Rollback: free the block and inode
        free_block(fs, block_num);
        free_inode(fs, new_inode_num);
        return false;
    }

    // Increment parent's link count (for "..")
    Inode *parent_inode = const_cast<Inode *>(read_inode(fs, parent_inode_num));
    if (parent_inode) {
        parent_inode->links_count++;
        write_inode_to_disk(fs, parent_inode_num, parent_inode);
    }

    return true;
}

bool ext2::unlink(Instance *fs, const char *path) {
    if (!fs || !path || !path[0]) return false;

    char name_buf[256];
    uint32_t parent_inode_num;

    if (resolve_path(fs, path, &parent_inode_num, name_buf) < 0)
        return false;

    uint32_t inode_num = find_in_dir(fs, parent_inode_num, name_buf);
    if (inode_num == 0) return false;

    // Remove the directory entry
    if (remove_dir_entry(fs, parent_inode_num, name_buf) < 0)
        return false;

    // Decrement links_count and free if 0
    Inode *inode = const_cast<Inode *>(read_inode(fs, inode_num));
    if (!inode) return false;

    if (inode->links_count > 0)
        inode->links_count--;

    if (inode->links_count == 0) {
        // Free all data blocks
            truncate_blocks_from(fs, inode, inode_num, 0);
        free_inode(fs, inode_num);
    } else {
        write_inode_to_disk(fs, inode_num, inode);
    }

    return true;
}

bool ext2::rmdir(Instance *fs, const char *path) {
    if (!fs || !path || !path[0]) return false;

    char name_buf[256];
    uint32_t parent_inode_num;

    if (resolve_path(fs, path, &parent_inode_num, name_buf) < 0)
        return false;

    uint32_t inode_num = find_in_dir(fs, parent_inode_num, name_buf);
    if (inode_num == 0) return false;

    Inode *inode = const_cast<Inode *>(read_inode(fs, inode_num));
    if (!inode || !(inode->mode & EXT2_S_IFDIR)) return false;

    // Check if directory is empty (only . and ..)
    uint8_t *block_buf = reinterpret_cast<uint8_t *>(mm::malloc(fs->block_size));
    if (!block_buf) return false;

    uint32_t num_blocks = (inode->size + fs->block_size - 1) / fs->block_size;
    bool has_entries = false;

    for (uint32_t bi = 0; bi < num_blocks && !has_entries; bi++) {
        if (!read_block_data(fs, inode, bi, block_buf)) continue;

        uint32_t offset = 0;
        while (offset < fs->block_size) {
            auto *de = reinterpret_cast<const DirEntry *>(block_buf + offset);
            if (de->rec_len < 8) break;
            if (de->inode != 0 && de->name_len > 0) {
                // Check if it's not . or ..
                bool is_dot = (de->name_len == 1 && de->name[0] == '.');
                bool is_dotdot = (de->name_len == 2 && de->name[0] == '.' && de->name[1] == '.');
                if (!is_dot && !is_dotdot) {
                    has_entries = true;
                    break;
                }
            }
            if (de->rec_len == 0) break;
            offset += de->rec_len;
        }
    }

    mm::free(block_buf);

    if (has_entries) return false;

    // Remove from parent
    if (remove_dir_entry(fs, parent_inode_num, name_buf) < 0)
        return false;

    // Decrement parent's links_count
    Inode *parent_inode = const_cast<Inode *>(read_inode(fs, parent_inode_num));
    if (parent_inode && parent_inode->links_count > 0) {
        parent_inode->links_count--;
        write_inode_to_disk(fs, parent_inode_num, parent_inode);
    }

    // Free the directory's blocks and inode
    truncate_blocks_from(fs, inode, inode_num, 0);
    free_inode(fs, inode_num);

    return true;
}

bool ext2::rename(Instance *fs, const char *old_path, const char *new_path) {
    if (!fs || !old_path || !new_path) return false;

    char old_name[256], new_name[256];
    uint32_t old_parent_inode, new_parent_inode;

    if (resolve_path(fs, old_path, &old_parent_inode, old_name) < 0)
        return false;

    if (resolve_path(fs, new_path, &new_parent_inode, new_name) < 0)
        return false;

    uint32_t inode_num = find_in_dir(fs, old_parent_inode, old_name);
    if (inode_num == 0) return false;

    const Inode *inode = read_inode(fs, inode_num);
    if (!inode) return false;

    uint8_t file_type = (inode->mode & EXT2_S_IFDIR) ? EXT2_FT_DIR : EXT2_FT_REG;

    // Add new directory entry first (increments link count conceptually)
    if (add_dir_entry(fs, new_parent_inode, inode_num, new_name, file_type) < 0)
        return false;

    // Increment links_count (for directories, both . and .. point here)
    Inode *mutable_inode = const_cast<Inode *>(inode);
    mutable_inode->links_count++;
    write_inode_to_disk(fs, inode_num, mutable_inode);

    // Remove old directory entry
    if (remove_dir_entry(fs, old_parent_inode, old_name) < 0) {
        // Rollback: remove the new entry
        remove_dir_entry(fs, new_parent_inode, new_name);
        mutable_inode->links_count--;
        write_inode_to_disk(fs, inode_num, mutable_inode);
        return false;
    }

    // Decrement links_count
    mutable_inode = const_cast<Inode *>(read_inode(fs, inode_num));
    if (mutable_inode->links_count > 0)
        mutable_inode->links_count--;
    write_inode_to_disk(fs, inode_num, mutable_inode);

    // If directory was moved, update parent pointer in ".."
    if (file_type == EXT2_FT_DIR && old_parent_inode != new_parent_inode) {
        // Update ".." entry in the moved directory
        Inode *dir_inode = mutable_inode;
        uint8_t *block_buf = reinterpret_cast<uint8_t *>(mm::malloc(fs->block_size));
        if (block_buf) {
            uint32_t num_blocks = (dir_inode->size + fs->block_size - 1) / fs->block_size;
            for (uint32_t bi = 0; bi < num_blocks; bi++) {
                if (!read_block_data(fs, dir_inode, bi, block_buf)) continue;
                uint32_t offset = 0;
                while (offset < fs->block_size) {
                    auto *de = reinterpret_cast<DirEntry *>(block_buf + offset);
                    if (de->rec_len < 8) break;
                    if (de->inode != 0 && de->name_len == 2 &&
                        de->name[0] == '.' && de->name[1] == '.') {
                        de->inode = new_parent_inode;
                        uint32_t phys = resolve_block(mutable_inode, fs, bi);
                        if (phys) {
                            uint8_t *dest = write_block_ptr(fs, phys);
                            if (dest)
                                lib::memcpy(dest, block_buf, fs->block_size);
                        }
                        break;
                    }
                    if (de->rec_len == 0) break;
                    offset += de->rec_len;
                }
            }
            mm::free(block_buf);
        }

        // Adjust parent directory link counts
        Inode *old_parent = const_cast<Inode *>(read_inode(fs, old_parent_inode));
        if (old_parent && old_parent->links_count > 0) {
            old_parent->links_count--;
            write_inode_to_disk(fs, old_parent_inode, old_parent);
        }
        Inode *new_parent = const_cast<Inode *>(read_inode(fs, new_parent_inode));
        if (new_parent) {
            new_parent->links_count++;
            write_inode_to_disk(fs, new_parent_inode, new_parent);
        }
    }

    return true;
}

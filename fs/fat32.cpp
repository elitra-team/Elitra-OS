#include "fat32.hpp"
#include "vfs.hpp"
#include "mount.hpp"
#include "lib.hpp"
#include "ns16550.hpp"
#include "heap.hpp"

using namespace fs;
using namespace fs::fat32;

extern "C" {
    uint8_t krust_fat32_init(void *fs, const uint8_t *image, size_t image_size);
    uint32_t krust_fat32_get_fat_entry(void *fs, uint32_t cluster);
    uint32_t krust_fat32_alloc_cluster(void *fs);
    void krust_fat32_name_to_sfn(const char *long_name, char sfn[11]);
    int krust_fat32_write_file(void *fs, uint32_t dir_cluster, const char *name, const uint8_t *data, uint32_t size);
    int krust_fat32_create_dir(void *fs, uint32_t dir_cluster, const char *name);
    int krust_fat32_delete_file(void *fs, uint32_t dir_cluster, const char *name);
    int krust_fat32_delete_dir(void *fs, uint32_t dir_cluster, const char *name);
    int krust_fat32_rename_entry(void *fs, uint32_t old_dir_cluster, const char *old_name, uint32_t new_dir_cluster, const char *new_name);
}

struct ReadClusterResult { const uint8_t *data; uint32_t size; };

static uint32_t cluster_to_sector(const Instance *fs, uint32_t cluster) {
    if (cluster < 2) return 0;
    return fs->first_data_sector + (cluster - 2) * fs->sectors_per_cluster;
}

static const uint8_t *read_sector(const Instance *fs, uint32_t sector) {
    uint64_t offset = static_cast<uint64_t>(sector) * fs->bytes_per_sector;
    if (offset + fs->bytes_per_sector > fs->image_size)
        return nullptr;
    return fs->image + offset;
}

static uint32_t next_cluster(const Instance *fs, uint32_t cluster) {
    return krust_fat32_get_fat_entry(const_cast<Instance *>(fs), cluster);
}

static ReadClusterResult read_cluster(const Instance *fs, uint32_t cluster) {
    uint32_t sector = cluster_to_sector(fs, cluster);
    uint32_t cluster_size = fs->sectors_per_cluster * fs->bytes_per_sector;
    return {read_sector(fs, sector), cluster_size};
}

static bool is_free(const DirEntry *e) {
    return e->name[0] == 0x00 || static_cast<uint8_t>(e->name[0]) == 0xE5;
}
static bool is_lfn(const DirEntry *e) { return e->attr == ATTR_LFN; }
static bool is_volume(const DirEntry *e) {
    return (e->attr & 0x08) && !(e->attr & 0x10) && !(e->attr & 0x20);
}
static bool is_dir_entry(const DirEntry *e) { return (e->attr & ATTR_DIRECTORY) != 0; }

static void parse_sfn(const char sfn[11], char *out, size_t out_size) {
    int pos = 0;
    char name[9] = {0};
    char ext[4] = {0};
    for (int i = 0; i < 8 && sfn[i] != ' '; i++) name[i] = sfn[i];
    for (int i = 0; i < 3 && sfn[8 + i] != ' '; i++) ext[i] = sfn[8 + i];
    for (int i = 0; name[i]; i++) {
        if (pos < (int)out_size - 1) out[pos++] = name[i];
    }
    if (ext[0]) {
        if (pos < (int)out_size - 1) out[pos++] = '.';
        for (int i = 0; ext[i]; i++)
            if (pos < (int)out_size - 1) out[pos++] = ext[i];
    }
    out[pos] = '\0';
    for (int i = 0; out[i]; i++)
        if (out[i] >= 'A' && out[i] <= 'Z')
            out[i] += 0x20;
}

static uint32_t get_cluster(const DirEntry *e) {
    return (static_cast<uint32_t>(e->cluster_hi) << 16) | e->cluster_lo;
}

// --- LFN helpers ---

static uint8_t sfn_checksum(const char *sfn) {
    uint8_t sum = 0;
    for (int i = 0; i < 11; i++)
        sum = ((sum >> 1) | (sum << 7)) + static_cast<uint8_t>(sfn[i]);
    return sum;
}

static int utf16le_to_utf8(uint16_t wc, char *out, int out_size) {
    if (wc < 0x80) {
        if (out_size < 1) return 0;
        out[0] = static_cast<char>(wc);
        return 1;
    } else if (wc < 0x800) {
        if (out_size < 2) return 0;
        out[0] = 0xC0 | static_cast<char>(wc >> 6);
        out[1] = 0x80 | static_cast<char>(wc & 0x3F);
        return 2;
    } else {
        if (out_size < 3) return 0;
        out[0] = 0xE0 | static_cast<char>(wc >> 12);
        out[1] = 0x80 | static_cast<char>((wc >> 6) & 0x3F);
        out[2] = 0x80 | static_cast<char>(wc & 0x3F);
        return 3;
    }
}

static int collect_lfn_name(const DirEntry *lfn_entries, int num_lfn, char *out, int out_size) {
    int pos = 0;
    // LFN entries are in reverse order; process from last to first
    for (int i = num_lfn - 1; i >= 0; i--) {
        const DirEntry *lfn = &lfn_entries[i];
        const uint8_t *raw = reinterpret_cast<const uint8_t *>(lfn);
        // Extract 13 UTF-16LE characters from the entry
        uint16_t chars[13];
        // Chars 0-4: bytes 1,3,5,7,9
        for (int j = 0; j < 5; j++) {
            chars[j] = static_cast<uint16_t>(raw[1 + j * 2]) | (static_cast<uint16_t>(raw[2 + j * 2]) << 8);
        }
        // Chars 5-10: bytes 14,16,18,20,22,24
        for (int j = 0; j < 6; j++) {
            chars[5 + j] = static_cast<uint16_t>(raw[14 + j * 2]) | (static_cast<uint16_t>(raw[15 + j * 2]) << 8);
        }
        // Chars 11-12: bytes 28,30
        for (int j = 0; j < 2; j++) {
            chars[11 + j] = static_cast<uint16_t>(raw[28 + j * 2]) | (static_cast<uint16_t>(raw[29 + j * 2]) << 8);
        }
        // Convert each char
        for (int j = 0; j < 13; j++) {
            if (chars[j] == 0xFFFF) break; // padding
            int n = utf16le_to_utf8(chars[j], out + pos, out_size - pos);
            if (n <= 0) break;
            pos += n;
        }
    }
    if (pos < out_size) out[pos] = '\0';
    return pos;
}

static VNode *mount_entry(Instance *fs, uint32_t cluster, const char *vfs_name, VNode *parent, uint32_t file_size, bool is_directory);

static int mount_entries_in_cluster(Instance *fs, uint32_t cluster, VNode *parent) {
    DirEntry lfn_buffer[20]; // max 20 LFN entries = 260 chars
    int num_lfn = 0;
    while (true) {
        auto [data, cl_size] = read_cluster(fs, cluster);
        if (!data) {
            drivers::NS16550::printf("fat32: read_cluster null for cluster %d\n", cluster);
            break;
        }

        uint32_t entries_per_cluster = cl_size / 32;
        for (uint32_t i = 0; i < entries_per_cluster; i++) {
            auto *e = reinterpret_cast<const DirEntry *>(data + i * 32);

            if (e->name[0] == 0x00) goto done_cluster;
            if (is_free(e)) {
                num_lfn = 0;
                continue;
            }
            if (is_lfn(e)) {
                uint8_t seq = static_cast<uint8_t>(e->name[0]);
                int order = seq & 0x1F;
                if (order >= 1 && order <= 20 && num_lfn < 20) {
                    lfn_buffer[num_lfn++] = *e;
                }
                continue;
            }
            if (is_volume(e)) {
                num_lfn = 0;
                continue;
            }

            char name[64];
            // Try LFN first
            bool used_lfn = false;
            if (num_lfn > 0) {
                uint8_t expected_csum = sfn_checksum(e->name);
                // Check if any LFN buffer matches (last entry has the checksum)
                uint8_t actual_csum = reinterpret_cast<const uint8_t *>(&lfn_buffer[num_lfn - 1])[13];
                if (actual_csum == expected_csum) {
                    collect_lfn_name(lfn_buffer, num_lfn, name, sizeof(name));
                    used_lfn = true;
                }
            }
            if (!used_lfn) {
                parse_sfn(e->name, name, sizeof(name));
            }
            num_lfn = 0;
            if (name[0] == '.' || name[0] == '\0') continue;

            uint32_t entry_cluster = get_cluster(e);
            if (entry_cluster == 0) {
                if (is_dir_entry(e)) entry_cluster = fs->root_cluster;
                else continue;
            }

            mount_entry(fs, entry_cluster, name, parent, e->size, is_dir_entry(e));
        }

        cluster = next_cluster(fs, cluster);
        if (cluster >= FAT32_EOC) break;
    }

done_cluster:
    return 0;
}

static VNode *mount_entry(Instance *fs, uint32_t cluster, const char *vfs_name, VNode *parent, uint32_t file_size, bool is_directory) {
    if (!vfs_name || !vfs_name[0]) return nullptr;
    if (VFS::find_child(parent, vfs_name)) return nullptr;

    if (cluster == fs->root_cluster) {
        VNode *dir = VFS::create_node(vfs_name, NodeType::DIRECTORY);
        dir->parent = parent;
        VFS::add_child(parent, dir);
        if (dir) mount_entries_in_cluster(fs, cluster, dir);
        return dir;
    }

    if (is_directory) {
        VNode *dir = VFS::create_node(vfs_name, NodeType::DIRECTORY);
        dir->parent = parent;
        VFS::add_child(parent, dir);
        if (dir) mount_entries_in_cluster(fs, cluster, dir);
        return dir;
    }

    VNode *file = VFS::create_node(vfs_name, NodeType::FILE);
    file->parent = parent;
    file->size = file_size;
    file->data = nullptr;

    if (file_size > 0 && cluster >= 2) {
        uint8_t *file_data = reinterpret_cast<uint8_t *>(mm::malloc(file_size));
        if (!file_data) {
            mm::free(file);
            return nullptr;
        }

        uint32_t remaining = file_size;
        uint32_t offset = 0;
        uint32_t c = cluster;
        while (c < FAT32_EOC && remaining > 0) {
            auto [d, sz] = read_cluster(fs, c);
            uint32_t copy = sz;
            if (copy > remaining) copy = remaining;
            if (d) lib::memcpy(file_data + offset, d, copy);
            offset += copy;
            remaining -= copy;
            c = next_cluster(fs, c);
        }

        file->data = file_data;
        file->size = offset;
    }

    VFS::add_child(parent, file);

    return file;
}

// --- Mount ---

bool fat32::mount(Instance *fs, const char *vfs_path) {
    if (!fs || !fs->image) return false;
    VNode *parent = VFS::resolve(vfs_path);
    if (!parent) {
        if (lib::strcmp(vfs_path, "/") == 0)
            parent = VFS::root_node();
        if (!parent) return false;
    }
    mount_entries_in_cluster(fs, fs->root_cluster, parent);
    return true;
}

// --- Resolve directory cluster for subdirectory paths ---

uint32_t fat32::resolve_dir_cluster(Instance *fs, const char *vfs_path, const char *mount_point) {
    // Returns 0 on error
    if (!fs || !vfs_path || !mount_point) return 0;

    // Skip mount point prefix
    const char *rel = vfs_path;
    while (*rel && *mount_point && *rel == *mount_point) { rel++; mount_point++; }

    // Now traverse relative path from root cluster
    uint32_t cluster = fs->root_cluster;

    while (*rel) {
        while (*rel == '/') rel++;
        if (!*rel) break;

        char name[64];
        int i = 0;
        while (*rel && *rel != '/' && i < 63) name[i++] = *rel++;
        name[i] = '\0';
        if (!name[0]) break;

        // Search current directory cluster for this entry
        bool found = false;
        uint32_t c = cluster;
        while (true) {
            auto [data, cl_size] = read_cluster(fs, c);
            if (!data) break;
            uint32_t entries = cl_size / 32;
            for (uint32_t ei = 0; ei < entries; ei++) {
                auto *e = reinterpret_cast<const DirEntry *>(data + ei * 32);
                if (e->name[0] == 0x00) goto dir_next_up;
                if (is_free(e) || is_lfn(e) || is_volume(e)) continue;
                char entry_name[64];
                parse_sfn(e->name, entry_name, sizeof(entry_name));
                if (lib::strcmp(entry_name, name) == 0) {
                    cluster = get_cluster(e);
                    if (cluster == 0) cluster = fs->root_cluster;
                    found = true;
                    goto dir_next_up;
                }
            }
            c = next_cluster(fs, c);
            if (c >= FAT32_EOC) break;
        }
        dir_next_up:
        if (!found) return 0;
    }

    return cluster;
}

// --- Init ---

bool fat32::init(Instance *fs, const uint8_t *image, size_t image_size) {
    lib::memset(fs, 0, sizeof(*fs));
    if (image_size < 512) return false;

    uint8_t ret = krust_fat32_init(fs, image, image_size);
    if (!ret) return false;
    fs->image_size = image_size;

    drivers::NS16550::printf("fat32: bps=%d spc=%d reserved=%d fats=%d spf=%d root_cluster=%d\n",
                            fs->bytes_per_sector, fs->sectors_per_cluster,
                            fs->reserved_sectors, fs->num_fats,
                            fs->sectors_per_fat, fs->root_cluster);

    drivers::NS16550::printf("fat32: first_data=0x%x first_fat=0x%x total_clusters=%d\n",
                            fs->first_data_sector, fs->first_fat_sector, fs->total_clusters);

    return true;
}

// --- Delegated file operations ---

void fat32::name_to_sfn(const char *long_name, char sfn[11]) {
    krust_fat32_name_to_sfn(long_name, sfn);
}

uint32_t fat32::alloc_cluster(Instance *fs) {
    uint32_t c = krust_fat32_alloc_cluster(fs);
    if (c > 0 && c < 0xFFFFFFF0) {
        drivers::NS16550::printf("fat32: alloc cluster=%d\n", c);
    }
    if (c == 0) {
        drivers::NS16550::printf("fat32: no free clusters!\n");
        return 0xFFFFFFFF;
    }
    return c;
}

uint32_t fat32::get_fat_entry(Instance *fs, uint32_t cluster) {
    return krust_fat32_get_fat_entry(fs, cluster);
}

int fat32::write_file(Instance *fs, uint32_t dir_cluster, const char *name, const uint8_t *data, uint32_t size) {
    return krust_fat32_write_file(fs, dir_cluster, name, data, size);
}

int fat32::create_dir(Instance *fs, uint32_t dir_cluster, const char *name) {
    return krust_fat32_create_dir(fs, dir_cluster, name);
}

int fat32::delete_file(Instance *fs, uint32_t dir_cluster, const char *name) {
    return krust_fat32_delete_file(fs, dir_cluster, name);
}

int fat32::delete_dir(Instance *fs, uint32_t dir_cluster, const char *name) {
    return krust_fat32_delete_dir(fs, dir_cluster, name);
}

int fat32::rename_entry(Instance *fs, uint32_t old_dir_cluster, const char *old_name, uint32_t new_dir_cluster, const char *new_name) {
    return krust_fat32_rename_entry(fs, old_dir_cluster, old_name, new_dir_cluster, new_name);
}

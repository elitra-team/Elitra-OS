#include "vfs.hpp"
#include "mount.hpp"
#include "fat32.hpp"
#include "ext2.hpp"
#include "vga.hpp"
#include "lib.hpp"
#include "heap.hpp"
#include "ns16550.hpp"
#include "task.hpp"

using namespace fs;

VNode *VFS::root = nullptr;
FileDescriptor VFS::fds[MAX_FDS];
int VFS::next_fd = 0;
Pipe VFS::pipes[MAX_PIPES];
int VFS::next_pipe_fd = 32; // Pipe FDs start at 32 to avoid conflict with VFS FDs

void VFS::init() {
    lib::memset(fds, 0, sizeof(fds));
    lib::memset(pipes, 0, sizeof(pipes));
    next_fd = 0;

    root = create_node("", NodeType::DIRECTORY);
    root->parent = root;

    drivers::VGA::writestring_color("VFS initialized\n",
        static_cast<uint8_t>(drivers::VGAColor::GREEN));
}

VNode *VFS::create_node(const char *name, NodeType type) {
    VNode *node = reinterpret_cast<VNode *>(mm::malloc(sizeof(VNode)));
    lib::memset(node, 0, sizeof(VNode));
    lib::strncpy(node->name, name, sizeof(node->name) - 1);
    node->type = type;
    return node;
}

VNode *VFS::create_dir(const char *path) {
    VNode *parent = resolve_parent(path);
    if (!parent || parent->type != NodeType::DIRECTORY)
        return nullptr;

    const char *name = path;
    for (const char *p = path; *p; p++)
        if (*p == '/') name = p + 1;

    if (find_child(parent, name))
        return nullptr;

    VNode *node = create_node(name, NodeType::DIRECTORY);
    node->parent = parent;
    add_child(parent, node);
    return node;
}

VNode *VFS::create_file(const char *path, const uint8_t *data, uint32_t size) {
    VNode *parent = resolve_parent(path);
    if (!parent || parent->type != NodeType::DIRECTORY)
        return nullptr;

    const char *name = path;
    for (const char *p = path; *p; p++)
        if (*p == '/') name = p + 1;

    if (!name[0]) return nullptr;

    VNode *node = create_node(name, NodeType::FILE);
    node->parent = parent;
    node->size = size;

    if (size > 0) {
        node->data = reinterpret_cast<uint8_t *>(mm::malloc(size));
        if (!node->data) {
            mm::free(node);
            return nullptr;
        }
        lib::memcpy(node->data, data, size);
    }

    add_child(parent, node);
    return node;
}

// --- Device handlers ---

int dev_null_read(VNode *, uint8_t *, uint32_t, uint32_t) {
    return 0;
}

int dev_null_write(VNode *, const uint8_t *, uint32_t size, uint32_t) {
    return static_cast<int>(size);
}

int dev_zero_read(VNode *, uint8_t *buf, uint32_t size, uint32_t) {
    lib::memset(buf, 0, size);
    return static_cast<int>(size);
}

int dev_zero_write(VNode *, const uint8_t *, uint32_t size, uint32_t) {
    return static_cast<int>(size);
}

// Simple xorshift PRNG
static uint32_t dev_random_seed = 0x1234ABCD;

static uint32_t xorshift32() {
    uint32_t x = dev_random_seed;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    dev_random_seed = x;
    return x;
}

int dev_random_read(VNode *, uint8_t *buf, uint32_t size, uint32_t) {
    for (uint32_t i = 0; i < size; i++)
        buf[i] = static_cast<uint8_t>(xorshift32() >> (i % 4) * 8);
    return static_cast<int>(size);
}

int dev_random_write(VNode *, const uint8_t *, uint32_t size, uint32_t) {
    return static_cast<int>(size);
}

VNode *VFS::create_device(const char *path, DevReadFn read_fn, DevWriteFn write_fn) {
    VNode *parent = resolve_parent(path);
    if (!parent || parent->type != NodeType::DIRECTORY)
        return nullptr;

    const char *name = path;
    for (const char *p = path; *p; p++)
        if (*p == '/') name = p + 1;

    if (!name[0]) return nullptr;
    if (find_child(parent, name))
        return nullptr;

    VNode *node = create_node(name, NodeType::DEVICE);
    node->parent = parent;
    node->size = 0;
    node->dev_read = read_fn;
    node->dev_write = write_fn;

    add_child(parent, node);
    return node;
}

int VFS::remove_node(const char *path) {
    VNode *node = resolve(path);
    if (!node || node == root) return -1;

    // Unlink from parent's children list
    VNode *parent = node->parent;
    if (!parent) return -1;

    VNode **pp = &parent->children;
    while (*pp) {
        if (*pp == node) {
            *pp = node->next;
            break;
        }
        pp = &(*pp)->next;
    }

    // Remove children first if directory
    VNode *child = node->children;
    while (child) {
        VNode *next = child->next;
        if (child->data) mm::free(child->data);
        mm::free(child);
        child = next;
    }

    if (node->data) mm::free(node->data);
    mm::free(node);
    return 0;
}

int VFS::write_file(const char *path, const uint8_t *data, uint32_t size) {
    VNode *node = resolve(path);

    // Check if path is under a mount that has a write handler
    MountInfo *mi = MountTable::find_mount(path);
    if (mi && mi->type == FSType::FAT32) {
        auto *fat = reinterpret_cast<fat32::Instance *>(mi->instance);
        VNode *parent_node = resolve_parent(path);
        if (!parent_node) return -1;

        drivers::NS16550::printf("vfs: write to FAT32 mount '%s'\n", path);
        uint32_t dir_cluster = fat32::resolve_dir_cluster(fat, path, mi->mount_point);
        if (dir_cluster == 0xFFFFFFFF) return -1;

        const char *name = path;
        for (const char *p = path; *p; p++)
            if (*p == '/') name = p + 1;

        if (fat32::write_file(fat, dir_cluster, name, data, size) == 0) {
            // Update VFS node if present
            if (node) {
                if (node->data) mm::free(node->data);
                node->data = nullptr;
                if (size > 0) {
                    node->data = reinterpret_cast<uint8_t *>(mm::malloc(size));
                    lib::memcpy(node->data, data, size);
                }
                node->size = size;
            }
            return 0;
        }
        return -1;
    }

    if (mi && mi->type == FSType::EXT2) {
        auto *ext2 = reinterpret_cast<ext2::Instance *>(mi->instance);
        if (ext2::write_file(ext2, path, data, size)) {
            if (node) {
                if (node->data) { mm::free(node->data); node->data = nullptr; }
                if (size > 0) {
                    node->data = reinterpret_cast<uint8_t *>(mm::malloc(size));
                    lib::memcpy(node->data, data, size);
                }
                node->size = size;
            }
            return 0;
        }
        return -1;
    }

    // RAMFS path
    if (node && node->type == NodeType::FILE) {
        if (node->data) mm::free(node->data);
        node->data = nullptr;
        if (size > 0) {
            node->data = reinterpret_cast<uint8_t *>(mm::malloc(size));
            lib::memcpy(node->data, data, size);
        }
        node->size = size;
        return 0;
    }

    // File doesn't exist — create it
    return create_file(path, data, size) ? 0 : -1;
}

int VFS::truncate(const char *path) {
    return write_file(path, nullptr, 0);
}

int VFS::write_fd(int fd, const uint8_t *data, uint32_t size) {
    if (fd < 0 || fd >= MAX_FDS || !fds[fd].used)
        return -1;
    FileDescriptor *f = &fds[fd];

    if (f->node->type == NodeType::DEVICE) {
        if (f->node->dev_write) {
            int n = f->node->dev_write(f->node, data, size, f->offset);
            if (n > 0) f->offset += n;
            return n;
        }
        return static_cast<int>(size);
    }

    if (f->node->type != NodeType::FILE)
        return -1;

    // Write at current offset, extending the file if needed
    uint32_t new_size = f->offset + size;
    if (new_size > f->node->size) {
        uint8_t *new_data = reinterpret_cast<uint8_t *>(mm::malloc(new_size));
        if (!new_data) return -1;
        if (f->node->data) {
            lib::memcpy(new_data, f->node->data, f->node->size);
            mm::free(f->node->data);
        }
        f->node->data = new_data;
        f->node->size = new_size;
    }
    lib::memcpy(f->node->data + f->offset, data, size);
    f->offset += size;
    return static_cast<int>(size);
}

int VFS::mkdir(const char *path) {
    if (!path || !path[0]) return -1;

    MountInfo *mi = MountTable::find_mount(path);
    if (mi && mi->type == FSType::FAT32) {
        auto *fat = reinterpret_cast<fat32::Instance *>(mi->instance);
        VNode *parent_node = resolve_parent(path);
        if (!parent_node) return -1;

        uint32_t dir_cluster = fat32::resolve_dir_cluster(fat, path, mi->mount_point);
        if (dir_cluster == 0xFFFFFFFF) return -1;

        const char *name = path;
        for (const char *p = path; *p; p++)
            if (*p == '/') name = p + 1;

        if (fat32::create_dir(fat, dir_cluster, name) == 0) {
            VNode *node = create_node(name, NodeType::DIRECTORY);
            node->parent = parent_node;
            add_child(parent_node, node);
            return 0;
        }
        return -1;
    }

    if (mi && mi->type == FSType::EXT2) {
        auto *ext2 = reinterpret_cast<ext2::Instance *>(mi->instance);
        if (ext2::mkdir(ext2, path)) {
            VNode *parent_node = resolve_parent(path);
            if (parent_node) {
                const char *name = path;
                for (const char *p = path; *p; p++)
                    if (*p == '/') name = p + 1;
                VNode *node = create_node(name, NodeType::DIRECTORY);
                node->parent = parent_node;
                add_child(parent_node, node);
            }
            return 0;
        }
        return -1;
    }

    return create_dir(path) ? 0 : -1;
}

int VFS::unlink(const char *path) {
    if (!path || !path[0]) return -1;

    VNode *node = resolve(path);
    if (!node || node->type != NodeType::FILE) return -1;

    MountInfo *mi = MountTable::find_mount(path);
    if (mi && mi->type == FSType::FAT32) {
        auto *fat = reinterpret_cast<fat32::Instance *>(mi->instance);
        uint32_t dir_cluster = fat32::resolve_dir_cluster(fat, path, mi->mount_point);
        if (dir_cluster == 0xFFFFFFFF) return -1;
        const char *name = path;
        for (const char *p = path; *p; p++)
            if (*p == '/') name = p + 1;
        if (fat32::delete_file(fat, dir_cluster, name) != 0) return -1;
    }

    if (mi && mi->type == FSType::EXT2) {
        auto *ext2 = reinterpret_cast<ext2::Instance *>(mi->instance);
        if (!ext2::unlink(ext2, path)) return -1;
    }

    return remove_node(path);
}

int VFS::rmdir(const char *path) {
    if (!path || !path[0]) return -1;

    VNode *node = resolve(path);
    if (!node || node->type != NodeType::DIRECTORY || node == root) return -1;
    if (node->children) return -1;

    MountInfo *mi = MountTable::find_mount(path);
    if (mi && mi->type == FSType::FAT32) {
        auto *fat = reinterpret_cast<fat32::Instance *>(mi->instance);
        uint32_t dir_cluster = fat32::resolve_dir_cluster(fat, path, mi->mount_point);
        if (dir_cluster == 0xFFFFFFFF) return -1;
        const char *name = path;
        for (const char *p = path; *p; p++)
            if (*p == '/') name = p + 1;
        if (fat32::delete_dir(fat, dir_cluster, name) != 0) return -1;
    }

    if (mi && mi->type == FSType::EXT2) {
        auto *ext2 = reinterpret_cast<ext2::Instance *>(mi->instance);
        if (!ext2::rmdir(ext2, path)) return -1;
    }

    return remove_node(path);
}

int VFS::rename(const char *old_path, const char *new_path) {
    if (!old_path || !old_path[0] || !new_path || !new_path[0]) return -1;

    VNode *old_node = resolve(old_path);
    if (!old_node || old_node == root) return -1;

    MountInfo *old_mi = MountTable::find_mount(old_path);
    MountInfo *new_mi = MountTable::find_mount(new_path);

    // If both on same FAT32 mount, use FAT32 rename
    if (old_mi && new_mi && old_mi->instance == new_mi->instance && old_mi->type == FSType::FAT32) {
        auto *fat = reinterpret_cast<fat32::Instance *>(old_mi->instance);
        uint32_t old_dir_cluster = fat32::resolve_dir_cluster(fat, old_path, old_mi->mount_point);
        if (old_dir_cluster == 0xFFFFFFFF) return -1;
        uint32_t new_dir_cluster = fat32::resolve_dir_cluster(fat, new_path, new_mi->mount_point);
        if (new_dir_cluster == 0xFFFFFFFF) return -1;

        const char *old_name = old_path;
        for (const char *p = old_path; *p; p++)
            if (*p == '/') old_name = p + 1;

        const char *new_name = new_path;
        for (const char *p = new_path; *p; p++)
            if (*p == '/') new_name = p + 1;

        if (fat32::rename_entry(fat, old_dir_cluster, old_name, new_dir_cluster, new_name) != 0)
            return -1;
    }

    // If both on same EXT2 mount, use EXT2 rename
    if (old_mi && new_mi && old_mi->instance == new_mi->instance && old_mi->type == FSType::EXT2) {
        auto *ext2 = reinterpret_cast<ext2::Instance *>(old_mi->instance);
        if (!ext2::rename(ext2, old_path, new_path))
            return -1;
    }

    // RAMFS path: remove old node, create new node with copied data
    VNode *old_parent = old_node->parent;
    if (!old_parent) return -1;

    // Unlink from old parent
    VNode **pp = &old_parent->children;
    while (*pp) {
        if (*pp == old_node) {
            *pp = old_node->next;
            break;
        }
        pp = &(*pp)->next;
    }

    // Create new node
    VNode *new_parent = resolve_parent(new_path);
    if (!new_parent) {
        add_child(old_parent, old_node);
        return -1;
    }

    const char *new_name = new_path;
    for (const char *p = new_path; *p; p++)
        if (*p == '/') new_name = p + 1;

    lib::strncpy(old_node->name, new_name, sizeof(old_node->name) - 1);
    old_node->parent = new_parent;
    old_node->next = nullptr;
    add_child(new_parent, old_node);

    return 0;
}

int VFS::stat(const char *path, FileStat *st) {
    if (!path || !st) return -1;
    VNode *node = resolve(path);
    if (!node) return -1;
    st->type = node->type;
    st->size = node->size;
    lib::strncpy(st->name, node->name, sizeof(st->name) - 1);
    return 0;
}

int VFS::open(const char *path) {
    VNode *node = resolve(path);
    if (!node || (node->type != NodeType::FILE && node->type != NodeType::DEVICE))
        return -1;

    for (int i = 0; i < MAX_FDS; i++) {
        int idx = (next_fd + i) % MAX_FDS;
        if (!fds[idx].used) {
            fds[idx].used = true;
            fds[idx].node = node;
            fds[idx].offset = 0;
            fds[idx].flags = 0;
            next_fd = (idx + 1) % MAX_FDS;
            return idx;
        }
    }
    return -1;
}

int VFS::read(int fd, uint8_t *buffer, uint32_t size) {
    if (fd < 0 || fd >= MAX_FDS || !fds[fd].used)
        return -1;

    FileDescriptor *f = &fds[fd];

    if (f->node->type == NodeType::DEVICE) {
        if (f->node->dev_read) {
            int n = f->node->dev_read(f->node, buffer, size, f->offset);
            if (n > 0) f->offset += n;
            return n;
        }
        return 0;
    }

    if (f->offset >= f->node->size)
        return 0;

    uint32_t avail = f->node->size - f->offset;
    uint32_t to_read = size < avail ? size : avail;
    lib::memcpy(buffer, f->node->data + f->offset, to_read);
    f->offset += to_read;
    return static_cast<int>(to_read);
}

int VFS::read_all(const char *path, uint8_t **out, uint32_t *out_size) {
    VNode *node = resolve(path);
    if (!node || node->type != NodeType::FILE)
        return -1;
    *out = node->data;
    *out_size = node->size;
    return 0;
}

int VFS::open_write(const char *path) {
    if (!path || !path[0]) return -1;

    // Truncate if exists, or create new
    VNode *node = resolve(path);
    if (node && node->type == NodeType::FILE) {
        if (node->data) mm::free(node->data);
        node->data = nullptr;
        node->size = 0;
    } else if (!node) {
        VNode *parent = resolve_parent(path);
        if (!parent || parent->type != NodeType::DIRECTORY)
            return -1;
        const char *name = path;
        for (const char *p = path; *p; p++)
            if (*p == '/') name = p + 1;
        if (!name[0]) return -1;
        node = create_node(name, NodeType::FILE);
        node->parent = parent;
        add_child(parent, node);
    } else {
        return -1;
    }

    for (int i = 0; i < MAX_FDS; i++) {
        int idx = (next_fd + i) % MAX_FDS;
        if (!fds[idx].used) {
            fds[idx].used = true;
            fds[idx].node = node;
            fds[idx].offset = 0;
            fds[idx].flags = 0;
            next_fd = (idx + 1) % MAX_FDS;
            return idx;
        }
    }
    return -1;
}

int VFS::close(int fd) {
    if (fd < 0 || fd >= MAX_FDS || !fds[fd].used)
        return -1;
    fds[fd].used = false;
    fds[fd].node = nullptr;
    fds[fd].offset = 0;
    return 0;
}

VNode *VFS::resolve(const char *path) {
    if (!path || !path[0] || !root)
        return root;
    return resolve_path(root, path);
}

VNode *VFS::resolve_parent(const char *path) {
    char buf[256];
    lib::strncpy(buf, path, sizeof(buf) - 1);

    char *last_slash = nullptr;
    for (char *p = buf; *p; p++)
        if (*p == '/') last_slash = p;

    if (!last_slash)
        return root;

    *last_slash = '\0';
    return resolve(buf);
}

VNode *VFS::resolve_path(VNode *base, const char *path) {
    if (!base || !path) return nullptr;

    while (*path == '/') path++;
    if (!*path) return base;

    char segment[64];
    int i = 0;
    while (*path && *path != '/') {
        if (i >= (int)sizeof(segment) - 1) return nullptr;
        segment[i++] = *path++;
    }
    segment[i] = '\0';

    VNode *child = find_child(base, segment);
    if (!child) return nullptr;

    return resolve_path(child, path);
}

VNode *VFS::find_child(VNode *dir, const char *name) {
    if (!dir || !name) return nullptr;
    if (lib::strcmp(name, ".") == 0) return dir;
    if (lib::strcmp(name, "..") == 0) return dir->parent ? dir->parent : dir;

    for (VNode *c = dir->children; c; c = c->next) {
        if (lib::strcmp(c->name, name) == 0)
            return c;
    }
    return nullptr;
}

void VFS::add_child(VNode *parent, VNode *child) {
    child->next = parent->children;
    parent->children = child;
}

// --- Pipe operations ---

Pipe *VFS::pipe_from_fd(int fd, bool is_read) {
    for (int i = 0; i < MAX_PIPES; i++) {
        if (!pipes[i].used) continue;
        if (is_read && pipes[i].read_fd == fd) return &pipes[i];
        if (!is_read && pipes[i].write_fd == fd) return &pipes[i];
    }
    return nullptr;
}

int VFS::pipe_create(int *fds) {
    if (!fds) return -1;

    int pi = -1;
    for (int i = 0; i < MAX_PIPES; i++) {
        if (!pipes[i].used) { pi = i; break; }
    }
    if (pi < 0) return -1;

    Pipe *p = &pipes[pi];
    lib::memset(p, 0, sizeof(Pipe));
    p->used = true;
    p->read_open = true;
    p->write_open = true;
    p->read_fd = next_pipe_fd++;
    p->write_fd = next_pipe_fd++;
    p->readers = 1;
    p->writers = 1;

    fds[0] = p->read_fd;
    fds[1] = p->write_fd;
    return 0;
}

int VFS::pipe_read(int fd, uint8_t *buf, uint32_t len) {
    Pipe *p = pipe_from_fd(fd, true);
    if (!p) return -1;

    uint32_t total = 0;
    while (total < len) {
        if (p->head != p->tail) {
            buf[total++] = p->buf[p->tail];
            p->tail = (p->tail + 1) % sizeof(p->buf);
        } else if (!p->write_open) {
            break;
        } else {
            kernel::Scheduler::yield();
        }
    }
    return static_cast<int>(total);
}

int VFS::pipe_write(int fd, const uint8_t *buf, uint32_t len) {
    Pipe *p = pipe_from_fd(fd, false);
    if (!p) return -1;

    uint32_t total = 0;
    while (total < len) {
        uint32_t next = (p->head + 1) % sizeof(p->buf);
        if (next != p->tail) {
            p->buf[p->head] = buf[total++];
            p->head = next;
        } else if (!p->read_open) {
            break;
        } else {
            kernel::Scheduler::yield();
        }
    }
    return static_cast<int>(total);
}

void VFS::pipe_close(int fd) {
    for (int i = 0; i < MAX_PIPES; i++) {
        if (!pipes[i].used) continue;
        if (pipes[i].read_fd == fd || pipes[i].write_fd == fd) {
            if (pipes[i].read_fd == fd) {
                pipes[i].read_open = false;
                pipes[i].readers--;
            }
            if (pipes[i].write_fd == fd) {
                pipes[i].write_open = false;
                pipes[i].writers--;
            }
            if (pipes[i].readers == 0 && pipes[i].writers == 0)
                pipes[i].used = false;
            return;
        }
    }
}

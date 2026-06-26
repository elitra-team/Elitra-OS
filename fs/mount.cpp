#include "mount.hpp"
#include "lib.hpp"
#include "ns16550.hpp"

using namespace fs;

static MountInfo mounts[MountTable::MAX_MOUNTS];

void MountTable::init() {
    lib::memset(mounts, 0, sizeof(mounts));
}

int MountTable::mount(const char *path, FSType type, void *instance) {
    if (!path || !path[0]) return -1;

    for (int i = 0; i < MAX_MOUNTS; i++) {
        if (!mounts[i].used) {
            lib::strncpy(mounts[i].mount_point, path, sizeof(mounts[i].mount_point) - 1);
            mounts[i].type = type;
            mounts[i].instance = instance;
            mounts[i].used = true;
            drivers::NS16550::printf("mount: %s fs=%d\n", path, (int)type);
            return 0;
        }
    }
    return -1;
}

int MountTable::umount(const char *path) {
    if (!path || !path[0]) return -1;

    for (int i = 0; i < MAX_MOUNTS; i++) {
        if (mounts[i].used && lib::strcmp(mounts[i].mount_point, path) == 0) {
            mounts[i].used = false;
            mounts[i].instance = nullptr;
            mounts[i].type = FSType::NONE;
            drivers::NS16550::printf("umount: %s\n", path);
            return 0;
        }
    }
    return -1;
}

MountInfo *MountTable::find_mount(const char *path) {
    if (!path) return nullptr;

    MountInfo *best = nullptr;
    size_t best_len = 0;

    for (int i = 0; i < MAX_MOUNTS; i++) {
        if (!mounts[i].used) continue;
        size_t len = lib::strlen(mounts[i].mount_point);
        if (lib::strncmp(mounts[i].mount_point, path, len) == 0) {
            if (len > best_len) {
                best_len = len;
                best = &mounts[i];
            }
        }
    }
    return best;
}

MountInfo *MountTable::get(int idx) {
    if (idx < 0 || idx >= MAX_MOUNTS) return nullptr;
    return &mounts[idx];
}

int MountTable::count() {
    int n = 0;
    for (int i = 0; i < MAX_MOUNTS; i++)
        if (mounts[i].used) n++;
    return n;
}

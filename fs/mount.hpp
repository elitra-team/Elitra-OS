#ifndef ELITRA_MOUNT_HPP
#define ELITRA_MOUNT_HPP

#include <cstdint>
#include <cstddef>

namespace fs {

enum class FSType : uint8_t {
    NONE,
    RAMFS,
    FAT32,
    EXT2
};

struct MountInfo {
    char      mount_point[128];
    FSType    type;
    void     *instance;
    bool      used;
};

class MountTable {
public:
    static const int MAX_MOUNTS = 16;

    static void init();
    static int  mount(const char *path, FSType type, void *instance);
    static int  umount(const char *path);
    static MountInfo *find_mount(const char *path);
    static MountInfo *get(int idx);
    static int  count();
};

}

#endif

#ifndef ELITRA_VFS_HPP
#define ELITRA_VFS_HPP

#include <cstdint>
#include <cstddef>

namespace fs {

enum class NodeType : uint8_t {
    FILE,
    DIRECTORY,
    DEVICE
};

struct VNode;

typedef int (*DevReadFn)(VNode *node, uint8_t *buf, uint32_t size, uint32_t offset);
typedef int (*DevWriteFn)(VNode *node, const uint8_t *buf, uint32_t size, uint32_t offset);

struct FileStat {
    NodeType  type;
    uint32_t  size;
    char      name[64];
};

struct VNode {
    char      name[64];
    NodeType  type;
    uint32_t  size;
    VNode    *parent;
    VNode    *children;
    VNode    *next;
    uint8_t  *data;
    DevReadFn dev_read;
    DevWriteFn dev_write;
};

struct FileDescriptor {
    VNode    *node;
    uint32_t  offset;
    uint32_t  flags;
    bool      used;
};

struct Pipe {
    uint8_t  buf[4096];
    uint32_t head;
    uint32_t tail;
    bool     read_open;
    bool     write_open;
    bool     used;
    uint32_t readers;
    uint32_t writers;
    int      read_fd;
    int      write_fd;
};

class VFS {
public:
    static void init();

    static VNode *create_node(const char *name, NodeType type);
    static VNode *create_file(const char *path, const uint8_t *data, uint32_t size);
    static VNode *create_dir(const char *path);
    static VNode *create_device(const char *path, DevReadFn read_fn, DevWriteFn write_fn);
    static int  remove_node(const char *path);
    static int  write_file(const char *path, const uint8_t *data, uint32_t size);
    static int  write_fd(int fd, const uint8_t *data, uint32_t size);
    static int  truncate(const char *path);
    static int  mkdir(const char *path);
    static int  unlink(const char *path);
    static int  rmdir(const char *path);
    static int  rename(const char *old_path, const char *new_path);
    static int  stat(const char *path, FileStat *st);

    static int  open(const char *path);
    static int  open_write(const char *path);
    static int  read(int fd, uint8_t *buffer, uint32_t size);
    static int  read_all(const char *path, uint8_t **out, uint32_t *out_size);
    static int  close(int fd);

    static VNode *resolve(const char *path);
    static VNode *find_child(VNode *dir, const char *name);
    static void   add_child(VNode *parent, VNode *child);

    static VNode *root_node() { return root; }

    // Pipe operations
    static int  pipe_create(int *fds);
    static int  pipe_read(int fd, uint8_t *buf, uint32_t len);
    static int  pipe_write(int fd, const uint8_t *buf, uint32_t len);
    static void pipe_close(int fd);

private:
    static const int MAX_FDS = 64;
    static const int MAX_PIPES = 16;

    static VNode *root;
    static FileDescriptor fds[MAX_FDS];
    static int next_fd;

    static Pipe pipes[MAX_PIPES];
    static int next_pipe_fd;

    static VNode *resolve_path(VNode *base, const char *path);
    static VNode *resolve_parent(const char *path);
    static Pipe  *pipe_from_fd(int fd, bool is_read);
};

}

#endif

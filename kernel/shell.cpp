#include "shell.hpp"
#include "terminal.hpp"
#include "ps2keyboard.hpp"
#include "pittimer.hpp"
#include "memory.hpp"
#include "pmm.hpp"
#include "paging.hpp"
#include "heap.hpp"
#include "port.hpp"
#include "lib.hpp"
#include "ns16550.hpp"
#include "task.hpp"
#include "syscall.hpp"
#include "vfs.hpp"
#include "mount.hpp"
#include "ata_pio.hpp"
#include "fat32.hpp"
#include "elf.hpp"

using namespace kernel;

bool Shell::cpuid_supported() {
    uint64_t supported;
    __asm__ volatile (
        "pushfq\n"
        "popq %%rax\n"
        "movq %%rax, %%rcx\n"
        "xorq $0x200000, %%rax\n"
        "pushq %%rax\n"
        "popfq\n"
        "pushfq\n"
        "popq %%rax\n"
        "xorq %%rcx, %%rax\n"
        "movq %%rax, %0\n"
        : "=r"(supported)
        :
        : "rax", "rcx", "memory"
    );
    return supported != 0;
}

void Shell::get_cpuid(int code, uint32_t *a, uint32_t *b, uint32_t *c, uint32_t *d) {
    __asm__ volatile (
        "cpuid"
        : "=a"(*a), "=b"(*b), "=c"(*c), "=d"(*d)
        : "a"(code)
    );
}

void Shell::cmd_help() {
    drivers::Terminal::writestring_color("Elitra OS Shell Commands:\n",
                                         static_cast<uint8_t>(drivers::VGAColor::CYAN));
    drivers::Terminal::writestring("  ?            - Show this help\n");
    drivers::Terminal::writestring("  clr          - Clear the screen\n");
    drivers::Terminal::writestring("  say          - Print text\n");
    drivers::Terminal::writestring("  upt          - Show system uptime\n");
    drivers::Terminal::writestring("  mem          - Show memory information\n");
    drivers::Terminal::writestring("  cpu          - Show CPU information\n");
    drivers::Terminal::writestring("  ver          - Show kernel version\n");
    drivers::Terminal::writestring("  rst          - Reboot the system\n");
    drivers::Terminal::writestring("  off          - Shutdown the system\n");
    drivers::Terminal::writestring("  jobs         - Show task info\n");
    drivers::Terminal::writestring("  newt         - Create test tasks\n");
    drivers::Terminal::writestring("  mall         - Test heap allocator\n");
    drivers::Terminal::writestring("  pt           - Test paging\n");
    drivers::Terminal::writestring("  list [path]   - List directory\n");
    drivers::Terminal::writestring("  dump <file>   - Print file contents\n");
    drivers::Terminal::writestring("  create <path> - Create empty file\n");
    drivers::Terminal::writestring("  del <path>    - Remove file\n");
    drivers::Terminal::writestring("  md <path>     - Create directory\n");
    drivers::Terminal::writestring("  put <path> <text> - Write text to file\n");
    drivers::Terminal::writestring("  mnt          - List mounted filesystems\n");
    drivers::Terminal::writestring("  unm <path>   - Unmount filesystem\n");
    drivers::Terminal::writestring("  exec <file>  - Load and run ELF program\n");
    drivers::Terminal::writestring("  ata          - Show ATA drive info\n");
    drivers::Terminal::writestring("  sync         - Flush disk writes\n");
    drivers::Terminal::writestring("  fs           - Show VFS info\n");
}

void Shell::cmd_clear() {
    drivers::Terminal::clear();
}

void Shell::cmd_echo(char **args, int argc) {
    for (int i = 1; i < argc; i++) {
        if (i > 1) drivers::Terminal::putchar(' ');
        drivers::Terminal::writestring(args[i]);
    }
    drivers::Terminal::putchar('\n');
}

void Shell::cmd_uptime() {
    uint32_t ticks = drivers::PITTimer::get_ticks();
    uint32_t secs  = ticks / 100;
    uint32_t mins  = secs / 60;
    uint32_t hours = mins / 60;
    secs  %= 60;
    mins  %= 60;
    drivers::Terminal::printf("Uptime: %d:%02d:%02d\n", hours, mins, secs);
}

void Shell::cmd_meminfo() {
    uint32_t total_kb, free_kb;
    mm::info(&total_kb, &free_kb);
    drivers::Terminal::printf("Memory: %d KB total, %d KB free, %d KB used\n",
                             total_kb, free_kb, total_kb - free_kb);
}

void Shell::cmd_cpuinfo() {
    if (!cpuid_supported()) {
        drivers::Terminal::writestring("CPUID not supported\n");
        return;
    }
    uint32_t a, b, c, d;
    get_cpuid(0, &a, &b, &c, &d);
    char vendor[13];
    lib::memcpy(vendor, &b, 4);
    lib::memcpy(vendor + 4, &d, 4);
    lib::memcpy(vendor + 8, &c, 4);
    vendor[12] = '\0';
    drivers::Terminal::printf("CPU Vendor: %s\n", vendor);

    get_cpuid(1, &a, &b, &c, &d);
    uint32_t family   = (a >> 8) & 0xF;
    uint32_t model    = (a >> 4) & 0xF;
    uint32_t stepping = a & 0xF;
    drivers::Terminal::printf("Family: %d, Model: %d, Stepping: %d\n", family, model, stepping);
}

void Shell::cmd_version() {
    drivers::Terminal::writestring_color("Elitra OS v0.1.0\n",
                                         static_cast<uint8_t>(drivers::VGAColor::GREEN));
    drivers::Terminal::printf("Built for x86-64\n");
    drivers::Terminal::printf("Kernel at 0x100000\n");
}

void Shell::cmd_reboot() {
    drivers::Terminal::writestring("Rebooting...\n");
    uint8_t good = 0x02;
    while (good & 0x02)
        good = arch::x86::inb(0x64);
    arch::x86::outb(0x64, 0xFE);
    drivers::Terminal::writestring("Reboot failed!\n");
}

void Shell::cmd_shutdown() {
    drivers::Terminal::writestring("Shutting down...\n");
    arch::x86::outw(0x604, 0x2000);
    arch::x86::outw(0xB004, 0x2000);
    drivers::Terminal::writestring("Shutdown failed!\n");
}

static void test_task1() {
    int count = 0;
    while (count < 5) {
        drivers::Terminal::printf("[Task1] count=%d\n", count++);
        uint32_t i;
        for (i = 0; i < 50000000; i = i + 1);
        kernel::Scheduler::yield();
    }
    drivers::Terminal::writestring("[Task1] exiting\n");
    kernel::Scheduler::exit();
}

static void test_task2() {
    int count = 0;
    while (count < 3) {
        drivers::Terminal::printf("[Task2] hello from task! count=%d\n", count++);
        uint32_t i;
        for (i = 0; i < 30000000; i = i + 1);
        kernel::Scheduler::yield();
    }
    drivers::Terminal::writestring("[Task2] exiting\n");
    kernel::Scheduler::exit();
}

void Shell::cmd_tasks() {
    drivers::Terminal::writestring("Tasks: use 'newt' to spawn a task\n");
}

void Shell::cmd_testmalloc() {
    drivers::Terminal::writestring("Testing malloc/free...\n");

    void *p1 = mm::malloc(64);
    drivers::Terminal::printf("  malloc(64) = 0x%x\n", p1);

    void *p2 = mm::malloc(256);
    drivers::Terminal::printf("  malloc(256) = 0x%x\n", p2);

    void *p3 = mm::malloc(1024);
    drivers::Terminal::printf("  malloc(1024) = 0x%x\n", p3);

    mm::free(p2);
    drivers::Terminal::writestring("  free(p2) OK\n");

    void *p4 = mm::malloc(128);
    drivers::Terminal::printf("  malloc(128) = 0x%x (should reuse p2)\n", p4);

    void *p5 = mm::realloc(p1, 128);
    drivers::Terminal::printf("  realloc(p1, 128) = 0x%x\n", p5);

    mm::free(p3);
    mm::free(p4);
    mm::free(p5);
    drivers::Terminal::writestring("  malloc test passed\n");
}

void Shell::cmd_testpaging() {
    drivers::Terminal::printf("Paging: PD=0x%x\n", mm::Paging::page_directory());

    uint32_t *heap_test = reinterpret_cast<uint32_t *>(0x40001000);
    *heap_test = 0xDEADBEEF;
    drivers::Terminal::printf("  Heap write test: 0x40001000 = 0x%x\n", *heap_test);

    uint64_t phys = mm::Paging::get_phys(0x40001000);
    drivers::Terminal::printf("  Phys addr: 0x%lx\n", phys);
}

void Shell::cmd_createtask() {
    drivers::Terminal::writestring("Creating test tasks...\n");
    int id1 = kernel::Scheduler::create(test_task1);
    int id2 = kernel::Scheduler::create(test_task2);
    drivers::Terminal::printf("  Task %d and task %d created\n", id1, id2);
}

static void print_node(fs::VNode *node, int indent) {
    for (int i = 0; i < indent; i++)
        drivers::Terminal::putchar(' ');
    if (node->type == fs::NodeType::DIRECTORY)
        drivers::Terminal::set_color(static_cast<uint8_t>(drivers::VGAColor::CYAN));
    drivers::Terminal::writestring(node->name);
    drivers::Terminal::set_color(static_cast<uint8_t>(drivers::VGAColor::LIGHT_GREY));
    if (node->type == fs::NodeType::DIRECTORY)
        drivers::Terminal::writestring("/");
    if (node->type == fs::NodeType::FILE && node->size > 0)
        drivers::Terminal::printf("  (%d bytes)", node->size);
    drivers::Terminal::putchar('\n');
}

static void list_dir(fs::VNode *dir, int indent) {
    for (fs::VNode *c = dir->children; c; c = c->next)
        print_node(c, indent);
}

void Shell::cmd_ls(char *args) {
    const char *path = args && args[0] ? args : "/";
    fs::VNode *node = fs::VFS::resolve(path);
    if (!node) {
        drivers::Terminal::printf("list: %s: not found\n", path);
        return;
    }
    if (node->type == fs::NodeType::DIRECTORY) {
        drivers::Terminal::printf("Contents of %s:\n", path);
        list_dir(node, 0);
    } else {
        print_node(node, 0);
    }
}

void Shell::cmd_cat(char *args) {
    if (!args || !args[0]) {
        drivers::Terminal::writestring("Usage: dump <path>\n");
        return;
    }
    fs::VNode *node = fs::VFS::resolve(args);
    if (!node || (node->type != fs::NodeType::FILE && node->type != fs::NodeType::DEVICE)) {
        drivers::Terminal::printf("dump: %s: not found\n", args);
        return;
    }

    if (node->type == fs::NodeType::DEVICE) {
        uint8_t buf[64];
        int n = node->dev_read(node, buf, sizeof(buf), 0);
        if (n > 0)
            drivers::Terminal::write(reinterpret_cast<const char *>(buf), static_cast<uint32_t>(n));
        drivers::Terminal::putchar('\n');
        return;
    }

    if (node->data && node->size > 0)
        drivers::Terminal::write(reinterpret_cast<const char *>(node->data), node->size);
    drivers::Terminal::putchar('\n');
}

static void count_vfs_nodes(fs::VNode *node, int *files, int *dirs) {
    if (!node) return;
    if (node->type == fs::NodeType::FILE) (*files)++;
    if (node->type == fs::NodeType::DIRECTORY) (*dirs)++;
    for (fs::VNode *c = node->children; c; c = c->next) {
        if (c->type == fs::NodeType::DIRECTORY)
            count_vfs_nodes(c, files, dirs);
        else
            (*files)++;
    }
}

void Shell::cmd_vfsinfo() {
    drivers::Terminal::printf("VFS root: 0x%x\n", fs::VFS::root_node());

    int file_count = 0, dir_count = 0;
    count_vfs_nodes(fs::VFS::root_node(), &file_count, &dir_count);
    drivers::Terminal::printf("  Directories: %d\n", dir_count);
    drivers::Terminal::printf("  Files: %d\n", file_count);
}

void Shell::cmd_touch(char *args) {
    if (!args || !args[0]) {
        drivers::Terminal::writestring("Usage: create <path>\n");
        return;
    }
    if (fs::VFS::write_file(args, nullptr, 0) == 0)
        drivers::Terminal::printf("create: created '%s'\n", args);
    else
        drivers::Terminal::printf("create: failed to create '%s'\n", args);
}

void Shell::cmd_rm(char *args) {
    if (!args || !args[0]) {
        drivers::Terminal::writestring("Usage: del <path>\n");
        return;
    }

    fs::MountInfo *mi = fs::MountTable::find_mount(args);
    if (mi && mi->type == fs::FSType::FAT32) {
        auto *fat = reinterpret_cast<fs::fat32::Instance *>(mi->instance);
        const char *name = args;
        for (const char *p = args; *p; p++)
            if (*p == '/') name = p + 1;
        if (fs::fat32::delete_file(fat, fat->root_cluster, name) == 0) {
            fs::VFS::remove_node(args);
            drivers::Terminal::printf("del: removed '%s'\n", args);
        } else {
            drivers::Terminal::printf("del: failed '%s'\n", args);
        }
        return;
    }

    if (fs::VFS::remove_node(args) == 0)
        drivers::Terminal::printf("del: removed '%s'\n", args);
    else
        drivers::Terminal::printf("del: failed '%s'\n", args);
}

void Shell::cmd_mkdir(char *args) {
    if (!args || !args[0]) {
        drivers::Terminal::writestring("Usage: md <path>\n");
        return;
    }

    fs::MountInfo *mi = fs::MountTable::find_mount(args);
    if (mi && mi->type == fs::FSType::FAT32) {
        auto *fat = reinterpret_cast<fs::fat32::Instance *>(mi->instance);
        const char *name = args;
        for (const char *p = args; *p; p++)
            if (*p == '/') name = p + 1;
        if (fs::fat32::create_dir(fat, fat->root_cluster, name) == 0) {
            fs::VFS::create_dir(args);
            drivers::Terminal::printf("md: created '%s'\n", args);
        } else {
            drivers::Terminal::printf("md: failed '%s'\n", args);
        }
        return;
    }

    if (fs::VFS::create_dir(args))
        drivers::Terminal::printf("md: created '%s'\n", args);
    else
        drivers::Terminal::printf("md: failed '%s'\n", args);
}

void Shell::cmd_write(char **args, int argc) {
    if (argc < 2) {
        drivers::Terminal::writestring("Usage: put <path> <text>\n");
        return;
    }
    char *path = args[1];
    /* Join remaining args[2..] with spaces for content */
    char *content = nullptr;
    if (argc > 2) {
        size_t len = 0;
        for (int i = 2; i < argc; i++)
            len += lib::strlen(args[i]) + 1;
        content = reinterpret_cast<char *>(mm::malloc(len + 1));
        if (!content) {
            drivers::Terminal::printf("put: allocation failed\n");
            return;
        }
        char *p = content;
        for (int i = 2; i < argc; i++) {
            size_t sl = lib::strlen(args[i]);
            lib::memcpy(p, args[i], sl);
            p += sl;
            *p++ = (i + 1 < argc) ? ' ' : '\0';
        }
    }

    if (fs::VFS::write_file(path,
                            reinterpret_cast<const uint8_t *>(content),
                            content ? lib::strlen(content) : 0) == 0)
        drivers::Terminal::printf("put: wrote %d bytes to '%s'\n",
                                 content ? lib::strlen(content) : 0, path);
    else
        drivers::Terminal::printf("put: failed\n");

    if (content) mm::free(content);
}

void Shell::cmd_mount(char *args) {
    (void)args;
    drivers::Terminal::printf("Mounted filesystems (%d):\n", fs::MountTable::count());
    for (int i = 0; i < fs::MountTable::MAX_MOUNTS; i++) {
        fs::MountInfo *m = fs::MountTable::get(i);
        if (m && m->used) {
            const char *type_str = "unknown";
            if (m->type == fs::FSType::RAMFS) type_str = "ramfs";
            else if (m->type == fs::FSType::FAT32) type_str = "fat32";
            drivers::Terminal::printf("  %s  type=%s\n", m->mount_point, type_str);
        }
    }
}

static int exec_file(const char *path, int argc = 0, const char **argv = nullptr,
                     int stdin_fd = -1, int stdout_fd = -1) {
    fs::VNode *node = fs::VFS::resolve(path);
    if (!node || node->type != fs::NodeType::FILE) {
        // Try /bin/<name>.elf
        char elf_path[64];
        const char prefix[] = "/bin/";
        const char suffix[] = ".elf";
        size_t plen = lib::strlen(prefix);
        size_t slen = lib::strlen(suffix);
        size_t nlen = lib::strlen(path);
        if (plen + nlen + slen + 1 > sizeof(elf_path)) return -1;
        lib::memcpy(elf_path, prefix, plen);
        lib::memcpy(elf_path + plen, path, nlen);
        lib::memcpy(elf_path + plen + nlen, suffix, slen);
        elf_path[plen + nlen + slen] = '\0';
        node = fs::VFS::resolve(elf_path);
        if (!node || node->type != fs::NodeType::FILE) return -1;
    }

    uint64_t entry;
    if (loader::load_elf(node->data, node->size, &entry) != 0) return -1;

    return kernel::Scheduler::create_elf(entry, argc, argv, stdin_fd, stdout_fd);
}

void Shell::cmd_exec(char *args) {
    if (!args || !args[0]) {
        drivers::Terminal::writestring("Usage: exec <path>\n");
        return;
    }
    const char *argv_list[2] = { args, nullptr };
    int tid = exec_file(args, 1, argv_list);
    if (tid < 0) {
        drivers::Terminal::printf("exec: '%s' not found or failed\n", args);
        return;
    }
    drivers::Terminal::printf("exec: '%s' loaded, task %d running\n", args, tid);
}

void Shell::cmd_ata(char *args) {
    (void)args;
    drivers::Terminal::printf("ata: %d drive(s)\n", drivers::ata_pio::drive_count());
    for (int d = 0; d < drivers::ata_pio::drive_count(); d++) {
        drivers::ata_pio::print_info(d);
    }
}

void Shell::cmd_sync(char *args) {
    (void)args;
    drivers::Terminal::writestring("sync: flushing to disk...\n");
    drivers::ata_pio::flush();
    drivers::Terminal::writestring("sync: done\n");
}

void Shell::cmd_umount(char *args) {
    if (!args || !args[0]) {
        drivers::Terminal::writestring("Usage: unm <path>\n");
        return;
    }
    if (fs::MountTable::umount(args) == 0) {
        fs::VFS::remove_node(args);
        drivers::Terminal::printf("unm: '%s' removed\n", args);
    } else {
        drivers::Terminal::printf("unm: '%s' not found\n", args);
    }
}

void Shell::parse_args(char *cmd, char **args, int *argc) {
    *argc = 0;
    while (*cmd && *cmd == ' ') cmd++;
    while (*cmd && *argc < MAX_ARGS) {
        if (*cmd == '"') {
            cmd++;
            args[*argc] = cmd;
            while (*cmd && *cmd != '"') cmd++;
            if (*cmd) {
                *cmd = '\0';
                cmd++;
            }
            (*argc)++;
        } else {
            args[*argc] = cmd;
            (*argc)++;
            while (*cmd && *cmd != ' ') cmd++;
            if (*cmd) {
                *cmd = '\0';
                cmd++;
            }
        }
        while (*cmd && *cmd == ' ') cmd++;
    }
    args[*argc] = nullptr;
}

void Shell::run() {
    char cmd_line[MAX_CMD_LEN];
    char *args[MAX_ARGS];
    int argc;

    drivers::Terminal::writestring_color("\n  ===== Elitra OS v0.1.0 =====\n",
                                         static_cast<uint8_t>(drivers::VGAColor::GREEN));
    drivers::Terminal::writestring_color("  Type '?' for commands\n\n",
                                         static_cast<uint8_t>(drivers::VGAColor::CYAN));

    while (true) {
        drivers::Terminal::set_color(static_cast<uint8_t>(drivers::VGAColor::GREEN));
        drivers::Terminal::writestring("elitra> ");
        drivers::Terminal::set_color(static_cast<uint8_t>(drivers::VGAColor::LIGHT_GREY));

        drivers::Terminal::readline(cmd_line, MAX_CMD_LEN);

        if (cmd_line[0] == '\0')
            continue;

        // Check for pipe (|)
        char *pipe_pos = nullptr;
        for (char *p = cmd_line; *p; p++) {
            if (*p == '|') { pipe_pos = p; break; }
        }

        if (pipe_pos) {
            *pipe_pos = '\0';
            char *cmd1 = cmd_line;
            char *cmd2 = pipe_pos + 1;

            // Trim trailing spaces from cmd1
            char *end1 = cmd1 + lib::strlen(cmd1);
            while (end1 > cmd1 && *(end1-1) == ' ') *--end1 = '\0';
            // Trim leading spaces from cmd2
            while (*cmd2 == ' ') cmd2++;

            if (!cmd1[0] || !cmd2[0]) {
                drivers::Terminal::writestring("Usage: <cmd1> | <cmd2>\n");
                continue;
            }

            int fds[2];
            if (fs::VFS::pipe_create(fds) != 0) {
                drivers::Terminal::writestring("pipe: creation failed\n");
                continue;
            }

            const char *args1[2] = { cmd1, nullptr };
            const char *args2[2] = { cmd2, nullptr };
            int tid1 = exec_file(cmd1, 1, args1, -1, fds[1]);  // stdout -> pipe write
            if (tid1 < 0) {
                drivers::Terminal::printf("pipe: '%s' not found\n", cmd1);
                fs::VFS::pipe_close(fds[0]);
                fs::VFS::pipe_close(fds[1]);
                continue;
            }
            int tid2 = exec_file(cmd2, 1, args2, fds[0], -1);  // stdin <- pipe read
            if (tid2 < 0) {
                drivers::Terminal::printf("pipe: '%s' not found\n", cmd2);
                kernel::Scheduler::wait_tid(tid1);  // let cmd1 finish
                fs::VFS::pipe_close(fds[0]);
                fs::VFS::pipe_close(fds[1]);
                continue;
            }

            // Wait for both tasks to complete
            kernel::Scheduler::wait_tid(tid1);
            fs::VFS::pipe_close(fds[1]);  // close write end so reader sees EOF
            kernel::Scheduler::wait_tid(tid2);

            fs::VFS::pipe_close(fds[0]);
            drivers::Terminal::writestring("pipe: done\n");
            continue;
        }

        // Check for output redirection (>)
        char *redir_out = nullptr;
        for (char *p = cmd_line; *p; p++) {
            if (*p == '>') { redir_out = p; break; }
        }

        if (redir_out) {
            *redir_out = '\0';
            char *cmd = cmd_line;
            while (*cmd == ' ') cmd++;
            // Trim trailing spaces from cmd
            char *end_cmd = cmd + lib::strlen(cmd);
            while (end_cmd > cmd && *(end_cmd-1) == ' ') *--end_cmd = '\0';
            char *file = redir_out + 1;
            while (*file == ' ') file++;

            if (!cmd[0] || !file[0]) {
                drivers::Terminal::writestring("Usage: <cmd> > <file>\n");
                continue;
            }

            // Create pipe to capture output
            int fds[2];
            if (fs::VFS::pipe_create(fds) != 0) {
                drivers::Terminal::writestring("redir: pipe creation failed\n");
                continue;
            }

            const char *args_cmd[2] = { cmd, nullptr };
            int tid = exec_file(cmd, 1, args_cmd, -1, fds[1]);  // stdout -> pipe
            if (tid < 0) {
                drivers::Terminal::printf("redir: '%s' not found\n", cmd);
                fs::VFS::pipe_close(fds[0]);
                fs::VFS::pipe_close(fds[1]);
                continue;
            }

            // Wait for task, then accumulate pipe content and write to file
            kernel::Scheduler::wait_tid(tid);
            fs::VFS::pipe_close(fds[1]);  // close write end

            uint8_t buf[4096];
            uint8_t accum[8192];
            uint32_t total = 0;
            int n;
            while ((n = fs::VFS::pipe_read(fds[0], buf, sizeof(buf))) > 0 && total + n <= sizeof(accum)) {
                lib::memcpy(accum + total, buf, n);
                total += n;
            }
            fs::VFS::pipe_close(fds[0]);

            if (total > 0) {
                fs::VFS::write_file(file, accum, total);
                drivers::ata_pio::flush();
            }
            drivers::Terminal::writestring("redir: done\n");
            continue;
        }

        parse_args(cmd_line, args, &argc);
        if (argc == 0)
            continue;

        drivers::NS16550::printf("shell: %s\n", args[0]);

        if (lib::strcmp(args[0], "?") == 0) {
            cmd_help();
        } else if (lib::strcmp(args[0], "clr") == 0) {
            cmd_clear();
        } else if (lib::strcmp(args[0], "say") == 0) {
            cmd_echo(args, argc);
        } else if (lib::strcmp(args[0], "upt") == 0) {
            cmd_uptime();
        } else if (lib::strcmp(args[0], "mem") == 0) {
            cmd_meminfo();
        } else if (lib::strcmp(args[0], "cpu") == 0) {
            cmd_cpuinfo();
        } else if (lib::strcmp(args[0], "ver") == 0) {
            cmd_version();
        } else if (lib::strcmp(args[0], "rst") == 0) {
            cmd_reboot();
        } else if (lib::strcmp(args[0], "off") == 0) {
            cmd_shutdown();
        } else if (lib::strcmp(args[0], "jobs") == 0) {
            cmd_tasks();
        } else if (lib::strcmp(args[0], "mall") == 0) {
            cmd_testmalloc();
        } else if (lib::strcmp(args[0], "pt") == 0) {
            cmd_testpaging();
        } else if (lib::strcmp(args[0], "newt") == 0) {
            cmd_createtask();
        } else if (lib::strcmp(args[0], "list") == 0) {
            cmd_ls(argc > 1 ? args[1] : nullptr);
        } else if (lib::strcmp(args[0], "dump") == 0) {
            cmd_cat(argc > 1 ? args[1] : nullptr);
        } else if (lib::strcmp(args[0], "exec") == 0) {
            cmd_exec(argc > 1 ? args[1] : nullptr);
        } else if (lib::strcmp(args[0], "ata") == 0) {
            cmd_ata(argc > 1 ? args[1] : nullptr);
        } else if (lib::strcmp(args[0], "sync") == 0) {
            cmd_sync(argc > 1 ? args[1] : nullptr);
        } else if (lib::strcmp(args[0], "fs") == 0) {
            cmd_vfsinfo();
        } else if (lib::strcmp(args[0], "create") == 0) {
            cmd_touch(argc > 1 ? args[1] : nullptr);
        } else if (lib::strcmp(args[0], "del") == 0) {
            cmd_rm(argc > 1 ? args[1] : nullptr);
        } else if (lib::strcmp(args[0], "md") == 0) {
            cmd_mkdir(argc > 1 ? args[1] : nullptr);
        } else if (lib::strcmp(args[0], "put") == 0) {
            cmd_write(args, argc);
        } else if (lib::strcmp(args[0], "mnt") == 0) {
            cmd_mount(argc > 1 ? args[1] : nullptr);
        } else if (lib::strcmp(args[0], "unm") == 0) {
            cmd_umount(argc > 1 ? args[1] : nullptr);
        } else {
            drivers::Terminal::printf("Unknown command: %s\n", args[0]);
        }

        // Auto-flush after write commands
        if (lib::strcmp(args[0], "create") == 0 ||
            lib::strcmp(args[0], "del") == 0 ||
            lib::strcmp(args[0], "md") == 0 ||
            lib::strcmp(args[0], "put") == 0) {
            drivers::ata_pio::flush();
        }
    }
}

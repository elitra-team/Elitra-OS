#include "syscall.hpp"
#include "idt.hpp"
#include "vga.hpp"
#include "ns16550.hpp"
#include "ps2keyboard.hpp"
#include "task.hpp"
#include "pittimer.hpp"
#include "vfs.hpp"
#include "mount.hpp"
#include "lib.hpp"
#include "port.hpp"
#include "cmos_rtc.hpp"
#include "access_user.hpp"
#include "acpi.hpp"

using namespace kernel;

extern "C" {
extern void syscall_stub(void);
void syscall_handler_c(arch::x86::Registers *r);
}

void Syscall::init() {
    arch::x86::IDT::set_gate(0x80, reinterpret_cast<uint64_t>(syscall_stub),
                              0x08, 0xEE);
    drivers::VGA::writestring_color("Syscall int 0x80 registered\n",
                                         static_cast<uint8_t>(drivers::VGAColor::GREEN));
}

extern "C" void syscall_handler_c(arch::x86::Registers *r) {
    Syscall::handler(r);
}

void Syscall::handler(arch::x86::Registers *r) {
    Scheduler::deliver_signals(r);

    uint64_t num = r->rax;
    uint64_t arg1 = r->rbx;
    uint64_t arg2 = r->rcx;
    uint64_t arg3 = r->rdx;

    switch (static_cast<SyscallNum>(num)) {
        case SyscallNum::WRITE: {
            if (!is_user_range(reinterpret_cast<const void *>(arg1), arg2)) {
                r->rax = 0xFFFFFFFF; break;
            }
            const char *str = reinterpret_cast<const char *>(arg1);
            if (Scheduler::current && Scheduler::current->stdout_fd >= 0) {
                fs::VFS::pipe_write(Scheduler::current->stdout_fd,
                                    reinterpret_cast<const uint8_t *>(str), arg2);
            } else {
                drivers::VGA::write(str, arg2);
                drivers::NS16550::write(str, arg2);
            }
            break;
        }
        case SyscallNum::EXIT: {
            Scheduler::exit(arg1);
            break;
        }
        case SyscallNum::SLEEP: {
            uint32_t wake = drivers::PITTimer::get_ticks() + arg1 / 10;
            while (drivers::PITTimer::get_ticks() < wake) {
                Scheduler::yield();
            }
            break;
        }
        case SyscallNum::YIELD: {
            Scheduler::yield();
            break;
        }
        case SyscallNum::OPEN: {
            char path[256];
            if (copy_string_from_user(path, reinterpret_cast<const char *>(arg1),
                                      sizeof(path)) < 0) {
                r->rax = 0xFFFFFFFF; break;
            }
            int fd = fs::VFS::open(path);
            r->rax = (fd >= 0) ? static_cast<uint32_t>(fd) : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::READ: {
            int fd = static_cast<int>(arg1);
            if (!is_user_range(reinterpret_cast<const void *>(arg2), arg3)) {
                r->rax = 0xFFFFFFFF; break;
            }
            int result = fs::VFS::read(fd, reinterpret_cast<uint8_t *>(arg2), arg3);
            r->rax = (result >= 0) ? static_cast<uint32_t>(result) : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::CLOSE: {
            int fd = static_cast<int>(arg1);
            r->rax = (fs::VFS::close(fd) >= 0) ? 0 : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::READDIR: {
            char path[256];
            if (copy_string_from_user(path, reinterpret_cast<const char *>(arg1),
                                      sizeof(path)) < 0) {
                r->rax = 0xFFFFFFFF; break;
            }
            if (!is_user_range(reinterpret_cast<const void *>(arg2), arg3)) {
                r->rax = 0xFFFFFFFF; break;
            }
            uint8_t *buf = reinterpret_cast<uint8_t *>(arg2);
            uint32_t buf_len = arg3;
            fs::VNode *dir = fs::VFS::resolve(path);
            if (!dir || dir->type != fs::NodeType::DIRECTORY) {
                r->rax = 0xFFFFFFFF; break;
            }
            uint32_t total = 0;
            for (fs::VNode *c = dir->children; c; c = c->next) {
                uint32_t name_len = 0;
                while (c->name[name_len]) name_len++;
                if (total + name_len + 2 > buf_len) break;
                lib::memcpy(buf + total, c->name, name_len);
                total += name_len;
                buf[total++] = (c->type == fs::NodeType::DIRECTORY) ? '/' : '\n';
            }
            if (total < buf_len) buf[total] = '\0';
            r->rax = total;
            break;
        }
        case SyscallNum::WRITE_FILE: {
            char path[256];
            if (copy_string_from_user(path, reinterpret_cast<const char *>(arg1),
                                      sizeof(path)) < 0) {
                r->rax = 0xFFFFFFFF; break;
            }
            if (!is_user_range(reinterpret_cast<const void *>(arg2), arg3)) {
                r->rax = 0xFFFFFFFF; break;
            }
            r->rax = (fs::VFS::write_file(path,
                      reinterpret_cast<const uint8_t *>(arg2), arg3) == 0) ? 0 : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::MKDIR: {
            char path[256];
            if (copy_string_from_user(path, reinterpret_cast<const char *>(arg1),
                                      sizeof(path)) < 0) {
                r->rax = 0xFFFFFFFF; break;
            }
            r->rax = (fs::VFS::mkdir(path) == 0) ? 0 : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::UNLINK: {
            char path[256];
            if (copy_string_from_user(path, reinterpret_cast<const char *>(arg1),
                                      sizeof(path)) < 0) {
                r->rax = 0xFFFFFFFF; break;
            }
            r->rax = (fs::VFS::unlink(path) == 0) ? 0 : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::RMDIR: {
            char path[256];
            if (copy_string_from_user(path, reinterpret_cast<const char *>(arg1),
                                      sizeof(path)) < 0) {
                r->rax = 0xFFFFFFFF; break;
            }
            r->rax = (fs::VFS::rmdir(path) == 0) ? 0 : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::RENAME: {
            char old_path[256], new_path[256];
            if (copy_string_from_user(old_path, reinterpret_cast<const char *>(arg1),
                                      sizeof(old_path)) < 0) {
                r->rax = 0xFFFFFFFF; break;
            }
            if (copy_string_from_user(new_path, reinterpret_cast<const char *>(arg2),
                                      sizeof(new_path)) < 0) {
                r->rax = 0xFFFFFFFF; break;
            }
            r->rax = (fs::VFS::rename(old_path, new_path) == 0) ? 0 : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::STAT: {
            char path[256];
            if (copy_string_from_user(path, reinterpret_cast<const char *>(arg1),
                                      sizeof(path)) < 0) {
                r->rax = 0xFFFFFFFF; break;
            }
            if (!is_user_range(reinterpret_cast<const void *>(arg2), sizeof(fs::FileStat))) {
                r->rax = 0xFFFFFFFF; break;
            }
            r->rax = (fs::VFS::stat(path, reinterpret_cast<fs::FileStat *>(arg2)) == 0) ? 0 : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::GETCHAR: {
            if (Scheduler::current && Scheduler::current->stdin_fd >= 0) {
                uint8_t c;
                int n = fs::VFS::pipe_read(Scheduler::current->stdin_fd, &c, 1);
                r->rax = (n > 0) ? static_cast<uint32_t>(c) : 0xFFFFFFFF;
            } else {
                char c = drivers::PS2Keyboard::getchar();
                r->rax = static_cast<uint32_t>(static_cast<unsigned char>(c));
            }
            break;
        }
        case SyscallNum::PIPE_CREATE: {
            if (!is_user_range(reinterpret_cast<const void *>(arg1), 2 * sizeof(int))) {
                r->rax = 0xFFFFFFFF; break;
            }
            r->rax = (fs::VFS::pipe_create(reinterpret_cast<int *>(arg1)) == 0) ? 0 : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::PIPE_READ: {
            int fd = static_cast<int>(arg1);
            if (!is_user_range(reinterpret_cast<const void *>(arg2), arg3)) {
                r->rax = 0xFFFFFFFF; break;
            }
            int result = fs::VFS::pipe_read(fd, reinterpret_cast<uint8_t *>(arg2), arg3);
            r->rax = (result >= 0) ? static_cast<uint32_t>(result) : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::PIPE_WRITE: {
            int fd = static_cast<int>(arg1);
            if (!is_user_range(reinterpret_cast<const void *>(arg2), arg3)) {
                r->rax = 0xFFFFFFFF; break;
            }
            int result = fs::VFS::pipe_write(fd, reinterpret_cast<const uint8_t *>(arg2), arg3);
            r->rax = (result >= 0) ? static_cast<uint32_t>(result) : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::PIPE_CLOSE: {
            fs::VFS::pipe_close(static_cast<int>(arg1));
            r->rax = 0;
            break;
        }
        case SyscallNum::WRITE_FD: {
            int fd = static_cast<int>(arg1);
            if (!is_user_range(reinterpret_cast<const void *>(arg2), arg3)) {
                r->rax = 0xFFFFFFFF; break;
            }
            int result = fs::VFS::write_fd(fd, reinterpret_cast<const uint8_t *>(arg2), arg3);
            r->rax = (result >= 0) ? static_cast<uint32_t>(result) : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::CLEAR_SCREEN: {
            drivers::VGA::clear();
            r->rax = 0;
            break;
        }
        case SyscallNum::SYSTEM_INFO: {
            if (arg2 > 0 && !is_user_range(reinterpret_cast<const void *>(arg1), arg2)) {
                r->rax = 0xFFFFFFFF; break;
            }
            r->rax = drivers::PITTimer::get_ticks();
            if (arg1 && arg2 > 0) {
                uint8_t *buf = reinterpret_cast<uint8_t *>(arg1);
                char kernel_buf[64];
                uint64_t i = 0;
                const char *ver = "Elitra OS v0.2 x86-64";
                while (ver[i] && i < sizeof(kernel_buf) - 1) {
                    kernel_buf[i] = ver[i]; i++;
                }
                kernel_buf[i] = '\0';
                copy_to_user(buf, kernel_buf, arg2 < (i + 1) ? arg2 : (i + 1));
            }
            break;
        }
        case SyscallNum::OPEN_WRITE: {
            char path[256];
            if (copy_string_from_user(path, reinterpret_cast<const char *>(arg1),
                                      sizeof(path)) < 0) {
                r->rax = 0xFFFFFFFF; break;
            }
            int fd = fs::VFS::open_write(path);
            r->rax = (fd >= 0) ? static_cast<uint32_t>(fd) : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::GETCWD: {
            if (!is_user_range(reinterpret_cast<const void *>(arg1), arg2)) {
                r->rax = 0xFFFFFFFF; break;
            }
            if (Scheduler::current && arg2 > 0) {
                char *buf = reinterpret_cast<char *>(arg1);
                uint32_t i = 0;
                while (Scheduler::current->cwd[i] && i < arg2 - 1) {
                    buf[i] = Scheduler::current->cwd[i];
                    i++;
                }
                buf[i] = '\0';
                r->rax = 0;
            } else {
                r->rax = 0xFFFFFFFF;
            }
            break;
        }
        case SyscallNum::CHDIR: {
            char path[256];
            if (copy_string_from_user(path, reinterpret_cast<const char *>(arg1),
                                      sizeof(path)) < 0) {
                r->rax = 0xFFFFFFFF; break;
            }
            if (Scheduler::current) {
                fs::VNode *node = fs::VFS::resolve(path);
                if (node && node->type == fs::NodeType::DIRECTORY) {
                    lib::strncpy(Scheduler::current->cwd, path, sizeof(Scheduler::current->cwd) - 1);
                    r->rax = 0;
                } else {
                    r->rax = 0xFFFFFFFF;
                }
            } else {
                r->rax = 0xFFFFFFFF;
            }
            break;
        }
        case SyscallNum::FORK: {
            int pid = Scheduler::fork(r);
            r->rax = (pid >= 0) ? static_cast<uint32_t>(pid) : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::EXECVE: {
            char path[256];
            if (copy_string_from_user(path, reinterpret_cast<const char *>(arg1),
                                       sizeof(path)) < 0) {
                r->rax = 0xFFFFFFFF; break;
            }
            int argc = static_cast<int>(arg2);
            const char **argv = reinterpret_cast<const char **>(arg3);

            /* Validate argv in user space */
            if (argc > 0 && argv) {
                if (!is_user_range(argv, (argc + 1) * sizeof(char *))) {
                    r->rax = 0xFFFFFFFF; break;
                }
                /* Validate each argv pointer */
                for (int i = 0; i < argc; i++) {
                    if (!is_user_range(reinterpret_cast<const void *>(argv[i]), 1)) {
                        r->rax = 0xFFFFFFFF; break;
                    }
                }
            }

            int result = Scheduler::execve(r, path, argc, argv);
            r->rax = (result == 0) ? 0 : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::WAITPID: {
            int pid = static_cast<int>(arg1);
            int status = 0;
            int result = Scheduler::waitpid(pid, &status);
            if (result >= 0) {
                if (arg2 && is_user_range(reinterpret_cast<void *>(arg2), sizeof(int))) {
                    copy_to_user(reinterpret_cast<void *>(arg2), &status, sizeof(int));
                }
                r->rax = static_cast<uint32_t>(result);
            } else {
                r->rax = 0xFFFFFFFF;
            }
            break;
        }
        case SyscallNum::GETPID: {
            r->rax = static_cast<uint32_t>(Scheduler::get_pid());
            break;
        }
        case SyscallNum::GETPPID: {
            r->rax = static_cast<uint32_t>(Scheduler::get_ppid());
            break;
        }
        case SyscallNum::KILL: {
            int pid = static_cast<int>(arg1);
            int sig = static_cast<int>(arg2);
            r->rax = (Scheduler::kill(pid, sig) == 0) ? 0 : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::SIGACTION: {
            int sig = static_cast<int>(arg1);
            uint64_t handler = arg2;
            int result = Scheduler::sigaction(sig, handler,
                (arg3 ? reinterpret_cast<uint64_t *>(arg3) : nullptr));
            r->rax = (result == 0) ? 0 : 0xFFFFFFFF;
            break;
        }
        case SyscallNum::SIGRETURN: {
            Scheduler::sigreturn(r);
            break;
        }
        case SyscallNum::REBOOT: {
            drivers::VGA::clear();
            drivers::VGA::writestring("Rebooting...\n");
            if (drivers::acpi::is_available()) {
                drivers::acpi::reboot();
            } else {
                arch::x86::outb(0x64, 0xFE);
            }
            r->rax = 0;
            break;
        }
        case SyscallNum::POWEROFF: {
            drivers::VGA::clear();
            drivers::VGA::writestring("Power off...\n");
            if (drivers::acpi::is_available()) {
                drivers::acpi::poweroff();
            } else {
                arch::x86::outw(0x604, 0x2000);
                arch::x86::outw(0xB004, 0x2000);
            }
            r->rax = 0;
            break;
        }
        case SyscallNum::GETTIME: {
            if (!is_user_range(reinterpret_cast<const void *>(arg1), sizeof(drivers::RTCInfo))) {
                r->rax = 0xFFFFFFFF; break;
            }
            drivers::RTCInfo info = drivers::CMOSRTC::read_time();
            if (copy_to_user(reinterpret_cast<void *>(arg1), &info, sizeof(info)) == 0) {
                r->rax = 0;
            } else {
                r->rax = 0xFFFFFFFF;
            }
            break;
        }
        default:
            drivers::VGA::printf("Unknown syscall: %d\n", num);
            break;
    }
}

#ifndef ELITRA_SYSCALL_HPP
#define ELITRA_SYSCALL_HPP

#include <cstdint>
#include "isr.hpp"

namespace kernel {

enum class SyscallNum {
    WRITE      = 0,
    EXIT       = 1,
    SLEEP      = 2,
    YIELD      = 3,
    OPEN       = 4,
    READ       = 5,
    CLOSE      = 6,
    READDIR    = 7,
    WRITE_FILE = 8,
    MKDIR      = 9,
    UNLINK     = 10,
    RMDIR      = 11,
    RENAME     = 12,
    STAT       = 13,
    GETCHAR    = 14,
    PIPE_CREATE = 15,
    PIPE_READ  = 16,
    PIPE_WRITE = 17,
    PIPE_CLOSE   = 18,
    WRITE_FD     = 19,
    CLEAR_SCREEN = 20,
    SYSTEM_INFO  = 21,
    REBOOT       = 22,
    POWEROFF     = 23,
    OPEN_WRITE   = 24,
    GETCWD       = 25,
    CHDIR        = 26,
    GETTIME      = 27,
    FORK         = 28,
    EXECVE       = 29,
    WAITPID      = 30,
    GETPID       = 31,
    KILL         = 32,
    SIGACTION    = 33,
    SIGRETURN    = 34,
    GETPPID      = 35,
};

class Syscall {
public:
    static void init();
    static void handler(arch::x86::Registers *r);
};

}

#endif

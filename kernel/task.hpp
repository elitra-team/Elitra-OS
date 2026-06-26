#ifndef ELITRA_TASK_HPP
#define ELITRA_TASK_HPP

#include <cstdint>
#include "isr.hpp"

namespace kernel {

enum class TaskState {
    READY,
    RUNNING,
    BLOCKED,
    WAITING,
    EXITED
};

struct TaskContext {
    uint64_t rsp;   // pointer to ISR frame on kernel stack
    uint64_t cr3;   // page table physical address
};

// Signal constants
static const int NSIG        = 32;
static const int SIGKILL     = 9;
static const int SIGSEGV     = 11;
static const int SIGTERM     = 15;
static const int SIGCHLD     = 17;

enum class SigAction : uint32_t {
    DEFAULT = 0,
    IGNORE  = 1,
    HANDLER = 2
};

struct SigHandler {
    uint64_t handler_addr;
};

struct Task;
struct TaskNode {
    Task    *task;
    TaskNode *next;
};

struct Task {
    uint32_t    id;
    uint32_t    ppid;
    TaskState   state;
    TaskContext ctx;
    uint64_t   *kstack;
    uint64_t   *ustack;
    uint32_t    pages;
    int         stdin_fd;
    int         stdout_fd;
    int         stderr_fd;
    uint32_t    exit_code;
    uint8_t    *fpu_state;
    char        cwd[128];
    Task       *next;

    Task    *parent;
    Task    *child_head;
    Task    *child_tail;
    Task    *sibling_next;

    SigHandler sig_handlers[NSIG];
    uint32_t   sig_pending;
    uint32_t   sig_blocked;

    bool     has_child_exit;
};

class Scheduler {
public:
    static void init();
    static int  create(void (*entry)(), int stdin_fd = -1, int stdout_fd = -1);
    static int  create_elf(uint64_t entry, int argc = 0, const char **argv = nullptr, int stdin_fd = -1, int stdout_fd = -1);
    static int  create_init(uint64_t entry);
    static int  current_tid();
    static void wait_tid(int tid);
    static void yield();
    static void exit(uint32_t code = 0);
    static void preempt(arch::x86::Registers *r);

    static int  fork(arch::x86::Registers *r);
    static int  execve(arch::x86::Registers *r, const char *path, int argc, const char **argv);
    static int  waitpid(int pid, int *status);
    static int  get_pid();
    static int  get_ppid();

    static int  kill(int pid, int sig);
    static int  sigaction(int sig, uint64_t handler, uint64_t *old_handler);
    static void deliver_signals(arch::x86::Registers *r);
    static int  sigreturn(arch::x86::Registers *r);

    static void enqueue(Task *t);
    static Task *dequeue();

public:
    static Task *current;
    static const uint64_t USTACK_VADDR  = 0xC0000000;
    static const uint32_t USTACK_PAGES  = 16;
    static const uint32_t USTACK_SIZE   = USTACK_PAGES * 4096;

private:
    static const uint32_t STACK_SIZE = 4096;
    static const uint32_t MAX_TASKS  = 64;

    static Task tasks[MAX_TASKS];
    static Task *ready_head;
    static Task *ready_tail;
    static uint32_t next_id;

    static void add_child(Task *parent, Task *child);
    static void remove_child(Task *parent, Task *child);
    static void build_user_frame(uint64_t *sp_top, uint64_t entry,
                                 uint64_t user_rsp, int argc,
                                 const uint64_t *argv);
};

extern "C" void yield_handler_c(arch::x86::Registers *r);
extern "C" void task_resume(TaskContext *ctx);

}

#endif

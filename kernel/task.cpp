#include "task.hpp"
#include "paging.hpp"
#include "pmm.hpp"
#include "vga.hpp"
#include "ns16550.hpp"
#include "port.hpp"
#include "lib.hpp"
#include "tss.hpp"
#include "fpu.hpp"
#include "vfs.hpp"
#include "elf.hpp"
#include "access_user.hpp"

using namespace kernel;

extern "C" void context_switch(uint64_t *old_rsp, uint64_t new_rsp);
extern "C" void yield_task(void);
extern "C" void task_resume_exit(TaskContext *ctx);

static bool sched_has_fpu = false;

Task Scheduler::tasks[MAX_TASKS];
Task *Scheduler::current = nullptr;
Task *Scheduler::ready_head = nullptr;
Task *Scheduler::ready_tail = nullptr;
uint32_t Scheduler::next_id = 0;

static void fpu_save_current() {
    if (sched_has_fpu && Scheduler::current && Scheduler::current->fpu_state) {
        arch::x86::fpu_save(Scheduler::current->fpu_state);
    }
}

static void fpu_restore_current() {
    if (sched_has_fpu && Scheduler::current && Scheduler::current->fpu_state) {
        arch::x86::fpu_restore(Scheduler::current->fpu_state);
    }
}

void Scheduler::init() {
    lib::memset(tasks, 0, sizeof(tasks));
    sched_has_fpu = arch::x86::has_fxsr();

    Task *idle = &tasks[next_id++];
    idle->id = 0;
    idle->state = TaskState::RUNNING;
    idle->kstack = nullptr;
    idle->ctx.rsp = 0;
    current = idle;
    drivers::VGA::writestring_color("Scheduler initialized\n",
                                         static_cast<uint8_t>(drivers::VGAColor::GREEN));
}

void Scheduler::build_user_frame(uint64_t *sp_top, uint64_t entry,
                                  uint64_t user_rsp, int argc,
                                  const uint64_t *argv_ptr) {
    uint64_t *sp = sp_top;
    *--sp = 0x23;                /* ss */
    *--sp = user_rsp;            /* user rsp */
    *--sp = 0x202;               /* rflags */
    *--sp = 0x1B;                /* cs */
    *--sp = entry;               /* rip */
    *--sp = 0;                   /* err_code */
    *--sp = 0;                   /* int_no */
    *--sp = (uint64_t)argc;      /* rax */
    *--sp = 0;                   /* rcx */
    *--sp = 0;                   /* rdx */
    *--sp = (uint64_t)argv_ptr;  /* rbx */
    *--sp = 0;                   /* rsp (dummy) */
    *--sp = 0;                   /* rbp */
    *--sp = 0;                   /* rsi */
    *--sp = 0;                   /* rdi */
    *--sp = 0;                   /* r8 */
    *--sp = 0;                   /* r9 */
    *--sp = 0;                   /* r10 */
    *--sp = 0;                   /* r11 */
    *--sp = 0;                   /* r12 */
    *--sp = 0;                   /* r13 */
    *--sp = 0;                   /* r14 */
    *--sp = 0;                   /* r15 */
}

void Scheduler::enqueue(Task *t) {
    t->next = nullptr;
    if (!ready_head) {
        ready_head = ready_tail = t;
    } else {
        ready_tail->next = t;
        ready_tail = t;
    }
}

Task *Scheduler::dequeue() {
    if (!ready_head) return nullptr;
    Task *t = ready_head;
    ready_head = t->next;
    if (!ready_head) ready_tail = nullptr;
    t->next = nullptr;
    return t;
}

void Scheduler::yield() {
    yield_task();
}

int Scheduler::current_tid() {
    return current ? current->id : -1;
}

int Scheduler::get_pid() {
    return current ? static_cast<int>(current->id) : -1;
}

int Scheduler::get_ppid() {
    return current ? static_cast<int>(current->ppid) : -1;
}

void Scheduler::wait_tid(int tid) {
    if (tid < 0 || static_cast<uint32_t>(tid) >= next_id) return;
    while (tasks[tid].state != TaskState::EXITED) {
        yield_task();
    }
}

void Scheduler::add_child(Task *parent, Task *child) {
    child->parent = parent;
    child->sibling_next = nullptr;
    if (!parent->child_head) {
        parent->child_head = child;
        parent->child_tail = child;
    } else {
        parent->child_tail->sibling_next = child;
        parent->child_tail = child;
    }
}

void Scheduler::remove_child(Task *parent, Task *child) {
    Task *prev = nullptr;
    Task *curr = parent->child_head;
    while (curr) {
        if (curr == child) {
            if (prev) prev->sibling_next = child->sibling_next;
            else parent->child_head = child->sibling_next;
            if (!child->sibling_next) parent->child_tail = prev;
            child->parent = nullptr;
            child->sibling_next = nullptr;
            return;
        }
        prev = curr;
        curr = curr->sibling_next;
    }
}

static void setup_task_common(Task *t, uint64_t id, uint64_t *kstack,
                               Task *parent) {
    t->id = id;
    t->state = TaskState::READY;
    t->kstack = kstack;
    t->pages = 1;
    t->stdin_fd = -1;
    t->stdout_fd = -1;
    t->stderr_fd = -1;
    t->exit_code = 0;
    t->cwd[0] = '/';
    t->cwd[1] = '\0';
    t->parent = parent;
    t->child_head = nullptr;
    t->child_tail = nullptr;
    t->sibling_next = nullptr;
    t->has_child_exit = false;
    t->sig_pending = 0;
    t->sig_blocked = 0;
    lib::memset(t->sig_handlers, 0, sizeof(t->sig_handlers));

    if (sched_has_fpu) {
        t->fpu_state = reinterpret_cast<uint8_t *>(mm::PMM::alloc_frame());
        if (t->fpu_state) lib::memset(t->fpu_state, 0, mm::Paging::PAGE_SIZE);
    }
}

static int allocate_user_stack(uint64_t *kstack_ptr, uint64_t *ustack_out,
                                uint64_t *user_rsp_out) {
    uint64_t ustack_base = Scheduler::USTACK_VADDR
        - (Scheduler::USTACK_PAGES - 1) * mm::Paging::PAGE_SIZE;
    uint64_t allocated = 0;
    for (uint64_t i = 0; i < Scheduler::USTACK_PAGES; i++) {
        uint64_t *frame = reinterpret_cast<uint64_t *>(mm::PMM::alloc_frame());
        if (!frame) {
            for (uint64_t j = 0; j < allocated; j++) {
                uint64_t *f = reinterpret_cast<uint64_t *>(
                    mm::Paging::get_phys(ustack_base + j * mm::Paging::PAGE_SIZE));
                if (f) mm::PMM::free_frame(f);
                mm::Paging::unmap_page(ustack_base + j * mm::Paging::PAGE_SIZE);
            }
            mm::PMM::free_frame(kstack_ptr);
            return -1;
        }
        lib::memset(frame, 0, mm::Paging::PAGE_SIZE);
        mm::Paging::map_page(ustack_base + i * mm::Paging::PAGE_SIZE,
                             reinterpret_cast<uint64_t>(frame),
                             mm::Paging::PAGE_PRESENT | mm::Paging::PAGE_WRITE | mm::Paging::PAGE_USER);
        allocated++;
    }
    *ustack_out = ustack_base;
    *user_rsp_out = Scheduler::USTACK_VADDR + mm::Paging::PAGE_SIZE;
    return 0;
}

int Scheduler::create_elf(uint64_t entry, int argc, const char **argv,
                           int stdin_fd, int stdout_fd) {
    if (next_id >= MAX_TASKS) return -1;

    Task *t = &tasks[next_id];
    uint64_t id = next_id++;

    uint64_t *kstack = reinterpret_cast<uint64_t *>(mm::PMM::alloc_frame());
    if (!kstack) return -1;
    lib::memset(kstack, 0, mm::Paging::PAGE_SIZE);

    setup_task_common(t, id, kstack, nullptr);
    t->stdin_fd = stdin_fd;
    t->stdout_fd = stdout_fd;

    uint64_t *sp_top = reinterpret_cast<uint64_t *>(
        reinterpret_cast<uintptr_t>(kstack) + mm::Paging::PAGE_SIZE);

    uint64_t ustack_base = 0;
    uint64_t user_rsp = 0;
    if (allocate_user_stack(kstack, &ustack_base, &user_rsp) != 0) return -1;
    t->ustack = reinterpret_cast<uint64_t *>(ustack_base);

    uint64_t argv_ptr = 0;
    if (argc > 0 && argv) {
        uint64_t str_size = 0;
        for (int i = 0; i < argc; i++)
            str_size += lib::strlen(argv[i]) + 1;
        uint64_t argv_size = (argc + 1) * sizeof(char *);
        uint64_t total = str_size + argv_size;
        if (total <= USTACK_SIZE - 16) {
            char *str_area = reinterpret_cast<char *>(ustack_base);
            uint64_t *argv_arr = reinterpret_cast<uint64_t *>(ustack_base + str_size);
            for (int i = 0; i < argc; i++) {
                argv_arr[i] = reinterpret_cast<uint64_t>(str_area);
                size_t len = lib::strlen(argv[i]);
                lib::memcpy(str_area, argv[i], len);
                str_area[len] = '\0';
                str_area += len + 1;
            }
            argv_arr[argc] = 0;
            argv_ptr = reinterpret_cast<uint64_t>(argv_arr);
        }
    }

    uint64_t *sp = sp_top;
    *--sp = 0x23;                /* ss */
    *--sp = user_rsp;            /* user rsp */
    *--sp = 0x202;               /* rflags */
    *--sp = 0x1B;                /* cs */
    *--sp = entry;               /* rip */
    *--sp = 0;                   /* err_code */
    *--sp = 0;                   /* int_no */
    *--sp = (uint64_t)argc;      /* rax */
    *--sp = 0;                   /* rcx */
    *--sp = 0;                   /* rdx */
    *--sp = argv_ptr;            /* rbx */
    *--sp = 0;                   /* rsp (dummy) */
    *--sp = 0;                   /* rbp */
    *--sp = 0;                   /* rsi */
    *--sp = 0;                   /* rdi */
    *--sp = 0;                   /* r8 */
    *--sp = 0;                   /* r9 */
    *--sp = 0;                   /* r10 */
    *--sp = 0;                   /* r11 */
    *--sp = 0;                   /* r12 */
    *--sp = 0;                   /* r13 */
    *--sp = 0;                   /* r14 */
    *--sp = 0;                   /* r15 */

    lib::memset(&t->ctx, 0, sizeof(TaskContext));
    t->ctx.rsp = reinterpret_cast<uint64_t>(sp);

    uint64_t *task_pml4 = mm::Paging::clone_kernel_dir();
    t->ctx.cr3 = task_pml4 ? reinterpret_cast<uint64_t>(task_pml4) : 0;

    arch::x86::TSS::set_kernel_stack(reinterpret_cast<uint64_t>(sp_top));
    enqueue(t);
    return static_cast<int>(id);
}

int Scheduler::create_init(uint64_t entry) {
    if (next_id >= MAX_TASKS) return -1;

    Task *t = &tasks[next_id];
    uint64_t id = next_id++;

    uint64_t *kstack = reinterpret_cast<uint64_t *>(mm::PMM::alloc_frame());
    if (!kstack) return -1;
    lib::memset(kstack, 0, mm::Paging::PAGE_SIZE);

    setup_task_common(t, id, kstack, nullptr);
    t->stdin_fd = -1;
    t->stdout_fd = -1;
    t->stderr_fd = -1;

    uint64_t *sp_top = reinterpret_cast<uint64_t *>(
        reinterpret_cast<uintptr_t>(kstack) + mm::Paging::PAGE_SIZE);

    uint64_t ustack_base = 0;
    uint64_t user_rsp = 0;
    if (allocate_user_stack(kstack, &ustack_base, &user_rsp) != 0) {
        mm::PMM::free_frame(kstack);
        return -1;
    }
    t->ustack = reinterpret_cast<uint64_t *>(ustack_base);

    uint64_t *sp = sp_top;
    *--sp = 0x23;
    *--sp = user_rsp;
    *--sp = 0x202;
    *--sp = 0x1B;
    *--sp = entry;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;

    lib::memset(&t->ctx, 0, sizeof(TaskContext));
    t->ctx.rsp = reinterpret_cast<uint64_t>(sp);

    uint64_t *task_pml4 = mm::Paging::clone_kernel_dir();
    t->ctx.cr3 = task_pml4 ? reinterpret_cast<uint64_t>(task_pml4) : 0;

    arch::x86::TSS::set_kernel_stack(reinterpret_cast<uint64_t>(sp_top));
    enqueue(t);
    return static_cast<int>(id);
}

int Scheduler::create(void (*entry)(), int stdin_fd, int stdout_fd) {
    if (next_id >= MAX_TASKS) return -1;

    Task *t = &tasks[next_id];
    uint64_t id = next_id++;

    uint64_t *kstack = reinterpret_cast<uint64_t *>(mm::PMM::alloc_frame());
    if (!kstack) return -1;
    lib::memset(kstack, 0, mm::Paging::PAGE_SIZE);

    uint64_t *sp_top = reinterpret_cast<uint64_t *>(
        reinterpret_cast<uintptr_t>(kstack) + mm::Paging::PAGE_SIZE);

    uint64_t *sp = sp_top;
    *--sp = 0x10;                /* ss */
    *--sp = 0;                   /* rsp (dummy) */
    *--sp = 0x202;               /* rflags */
    *--sp = 0x08;                /* cs */
    *--sp = reinterpret_cast<uint64_t>(entry);  /* rip */
    *--sp = 0;                   /* err_code */
    *--sp = 0;                   /* int_no */
    *--sp = 0;                   /* rax */
    *--sp = 0;                   /* rcx */
    *--sp = 0;                   /* rdx */
    *--sp = 0;                   /* rbx */
    *--sp = 0;                   /* rsp (dummy) */
    *--sp = 0;                   /* rbp */
    *--sp = 0;                   /* rsi */
    *--sp = 0;                   /* rdi */
    *--sp = 0;                   /* r8 */
    *--sp = 0;                   /* r9 */
    *--sp = 0;                   /* r10 */
    *--sp = 0;                   /* r11 */
    *--sp = 0;                   /* r12 */
    *--sp = 0;                   /* r13 */
    *--sp = 0;                   /* r14 */
    *--sp = 0;                   /* r15 */

    setup_task_common(t, id, kstack, nullptr);
    t->stdin_fd = stdin_fd;
    t->stdout_fd = stdout_fd;

    lib::memset(&t->ctx, 0, sizeof(TaskContext));
    t->ctx.rsp = reinterpret_cast<uint64_t>(sp);
    t->ctx.cr3 = reinterpret_cast<uint64_t>(mm::Paging::page_directory());

    enqueue(t);
    return static_cast<int>(id);
}

// ─── FORK ───────────────────────────────────────────────────────

int Scheduler::fork(arch::x86::Registers *r) {
    if (next_id >= MAX_TASKS) return -1;

    Task *parent = current;
    Task *child = &tasks[next_id];
    uint64_t child_id = next_id++;

    uint64_t *child_kstack = reinterpret_cast<uint64_t *>(mm::PMM::alloc_frame());
    if (!child_kstack) {
        next_id--;
        return -1;
    }
    lib::memset(child_kstack, 0, mm::Paging::PAGE_SIZE);

    setup_task_common(child, child_id, child_kstack, parent);

    /* Copy kernel stack contents */
    uintptr_t parent_kstack_start = reinterpret_cast<uintptr_t>(parent->kstack);
    uintptr_t parent_r_offset = reinterpret_cast<uintptr_t>(r) - parent_kstack_start;
    uintptr_t child_r_addr = reinterpret_cast<uintptr_t>(child_kstack) + parent_r_offset;
    arch::x86::Registers *child_r = reinterpret_cast<arch::x86::Registers *>(child_r_addr);
    lib::memcpy(child_kstack, parent->kstack, mm::Paging::PAGE_SIZE);

    child_r->rax = 0;  /* child returns 0 */

    /* Clone page table */
    uint64_t *child_pml4 = mm::Paging::clone_kernel_dir();
    if (!child_pml4) {
        mm::PMM::free_frame(child_kstack);
        next_id--;
        return -1;
    }

    /* Copy user pages (simple copy, no COW for now) */
    mm::Paging::copy_user_pages(reinterpret_cast<uint64_t *>(parent->ctx.cr3),
                                 child_pml4);

    /* Copy task properties */
    child->stdin_fd = parent->stdin_fd;
    child->stdout_fd = parent->stdout_fd;
    child->stderr_fd = parent->stderr_fd;
    child->ustack = parent->ustack;
    child->pages = parent->pages;
    lib::strncpy(child->cwd, parent->cwd, sizeof(child->cwd) - 1);
    child->ppid = parent->id;

    /* Copy signal handlers */
    lib::memcpy(child->sig_handlers, parent->sig_handlers, sizeof(child->sig_handlers));
    child->sig_blocked = parent->sig_blocked;

    /* Set up context for scheduler */
    lib::memset(&child->ctx, 0, sizeof(TaskContext));
    child->ctx.rsp = child_r_addr;
    child->ctx.cr3 = reinterpret_cast<uint64_t>(child_pml4);

    /* Register as child */
    add_child(parent, child);

    arch::x86::TSS::set_kernel_stack(
        reinterpret_cast<uint64_t>(parent->kstack) + mm::Paging::PAGE_SIZE);
    enqueue(child);

    return static_cast<int>(child_id);
}

// ─── EXECVE ─────────────────────────────────────────────────────

int Scheduler::execve(arch::x86::Registers *r, const char *path,
                       int argc, const char **argv) {
    fs::VNode *node = fs::VFS::resolve(path);
    if (!node || node->type != fs::NodeType::FILE) {
        char elf_path[64];
        const char prefix[] = "/bin/";
        const char suffix[] = ".elf";
        size_t plen = lib::strlen(prefix);
        size_t slen = lib::strlen(suffix);
        size_t nlen = lib::strlen(path);
        if (plen + nlen + slen + 1 <= sizeof(elf_path)) {
            lib::memcpy(elf_path, prefix, plen);
            lib::memcpy(elf_path + plen, path, nlen);
            lib::memcpy(elf_path + plen + nlen, suffix, slen);
            elf_path[plen + nlen + slen] = '\0';
            node = fs::VFS::resolve(elf_path);
        }
        if (!node || node->type != fs::NodeType::FILE) return -1;
    }

    uint64_t entry;
    if (loader::load_elf(node->data, node->size, &entry) != 0) return -1;

    mm::Paging::free_user_pages(current->ctx.cr3);

    uint64_t *new_pml4 = mm::Paging::clone_kernel_dir();
    if (!new_pml4) return -1;

    uint64_t old_cr3 = 0;
    __asm__ volatile ("mov %%cr3, %0" : "=r"(old_cr3));
    __asm__ volatile ("mov %0, %%cr3" : : "r"(new_pml4) : "memory");

    uint64_t entry2;
    int load_ok = loader::load_elf(node->data, node->size, &entry2);
    if (load_ok != 0) {
        __asm__ volatile ("mov %0, %%cr3" : : "r"(old_cr3) : "memory");
        return -1;
    }

    uint64_t ustack_base = USTACK_VADDR - (USTACK_PAGES - 1) * mm::Paging::PAGE_SIZE;
    uint64_t user_rsp = USTACK_VADDR + mm::Paging::PAGE_SIZE;
    for (uint64_t i = 0; i < USTACK_PAGES; i++) {
        uint64_t *frame = reinterpret_cast<uint64_t *>(mm::PMM::alloc_frame());
        if (!frame) {
            __asm__ volatile ("mov %0, %%cr3" : : "r"(old_cr3) : "memory");
            return -1;
        }
        lib::memset(frame, 0, mm::Paging::PAGE_SIZE);
        mm::Paging::map_page(ustack_base + i * mm::Paging::PAGE_SIZE,
                             reinterpret_cast<uint64_t>(frame),
                             mm::Paging::PAGE_PRESENT | mm::Paging::PAGE_WRITE | mm::Paging::PAGE_USER);
    }

    uint64_t argv_ptr = 0;
    if (argc > 0 && argv) {
        uint64_t str_size = 0;
        for (int i = 0; i < argc; i++)
            str_size += lib::strlen(argv[i]) + 1;
        uint64_t argv_size = (argc + 1) * sizeof(char *);
        uint64_t total = str_size + argv_size;
        if (total <= USTACK_SIZE - 16) {
            char *str_area = reinterpret_cast<char *>(ustack_base);
            uint64_t *argv_arr = reinterpret_cast<uint64_t *>(ustack_base + str_size);
            for (int i = 0; i < argc; i++) {
                argv_arr[i] = reinterpret_cast<uint64_t>(str_area);
                size_t len = lib::strlen(argv[i]);
                lib::memcpy(str_area, argv[i], len);
                str_area[len] = '\0';
                str_area += len + 1;
            }
            argv_arr[argc] = 0;
            argv_ptr = reinterpret_cast<uint64_t>(argv_arr);
        }
    }

    current->ctx.cr3 = reinterpret_cast<uint64_t>(new_pml4);
    __asm__ volatile ("mov %0, %%cr3" : : "r"(new_pml4) : "memory");

    uint64_t *sp_top = reinterpret_cast<uint64_t *>(
        reinterpret_cast<uintptr_t>(current->kstack) + mm::Paging::PAGE_SIZE);
    uint64_t *sp = sp_top;
    *--sp = 0x23;
    *--sp = user_rsp;
    *--sp = 0x202;
    *--sp = 0x1B;
    *--sp = entry2;
    *--sp = 0;
    *--sp = 0;
    *--sp = (uint64_t)argc;
    *--sp = 0;
    *--sp = 0;
    *--sp = argv_ptr;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;
    *--sp = 0;

    current->ctx.rsp = reinterpret_cast<uint64_t>(sp);
    current->ustack = reinterpret_cast<uint64_t *>(ustack_base);
    current->pages = USTACK_PAGES;

    for (int i = 0; i < NSIG; i++) {
        if (current->sig_handlers[i].handler_addr != 1) {
            current->sig_handlers[i].handler_addr = 0;
        }
    }
    current->sig_pending = 0;

    r->rip = entry2;
    r->user_rsp = user_rsp;
    r->rax = 0;
    r->cs = 0x1B;
    r->ss = 0x23;
    r->rflags = 0x202;

    return 0;
}

// ─── WAITPID ────────────────────────────────────────────────────

int Scheduler::waitpid(int pid, int *status) {
    if (!current) return -1;

    Task *child = nullptr;

    /* Specific child? */
    if (pid > 0) {
        if (static_cast<uint32_t>(pid) >= next_id) return -1;
        child = &tasks[static_cast<uint32_t>(pid)];
        if (child->parent != current) return -1;
    } else if (pid == -1) {
        /* Wait for any child */
        child = current->child_head;
        while (child) {
            if (child->state == TaskState::EXITED) break;
            if (child->state != TaskState::EXITED) {
                child = child->sibling_next;
                continue;
            }
            break;
        }
        if (!child) child = current->child_head;
        if (!child) return -1;
    } else {
        return -1;
    }

    /* Block until child exits */
    while (child->state != TaskState::EXITED) {
        current->state = TaskState::WAITING;
        yield_task();
        current->state = TaskState::RUNNING;
    }

    if (status) {
        *status = static_cast<int>(child->exit_code);
    }

    /* Remove child from parent's list */
    remove_child(current, child);

    if (child->ctx.cr3) {
        mm::Paging::free_user_pages(child->ctx.cr3);
    }
    if (child->kstack) {
        mm::PMM::free_frame(child->kstack);
        child->kstack = nullptr;
    }
    if (child->fpu_state) {
        mm::PMM::free_frame(child->fpu_state);
        child->fpu_state = nullptr;
    }

    return static_cast<int>(child->exit_code);
}

// ─── SIGNALS ────────────────────────────────────────────────────

int Scheduler::kill(int pid, int sig) {
    if (sig < 0 || sig >= NSIG) return -1;
    if (pid < 0 || static_cast<uint32_t>(pid) >= next_id) return -1;

    Task *t = &tasks[static_cast<uint32_t>(pid)];
    if (t->state == TaskState::EXITED) return -1;

    if (sig == SIGKILL) {
        /* Immediate termination */
        t->exit_code = static_cast<uint32_t>(0x100 + SIGKILL);
        t->state = TaskState::EXITED;
        t->has_child_exit = true;

        /* Notify parent */
        if (t->parent) {
            t->parent->has_child_exit = true;
        }
        return 0;
    }

    t->sig_pending |= (1 << sig);
    return 0;
}

int Scheduler::sigaction(int sig, uint64_t handler, uint64_t *old_handler) {
    if (sig < 0 || sig >= NSIG || !current) return -1;
    if (sig == SIGKILL) return -1;

    if (old_handler) {
        *old_handler = current->sig_handlers[sig].handler_addr;
    }
    current->sig_handlers[sig].handler_addr = handler;
    return 0;
}

int Scheduler::sigreturn(arch::x86::Registers *r) {
    if (!current) return -1;

    uint64_t *user_sp = reinterpret_cast<uint64_t *>(r->user_rsp);

    uint64_t sigframe[7];
    if (copy_from_user(sigframe, user_sp, sizeof(sigframe)) != 0)
        return -1;

    r->rip = sigframe[1];
    r->cs = sigframe[2];
    r->rflags = sigframe[3];
    r->user_rsp = sigframe[4];
    r->ss = sigframe[5];
    r->rax = sigframe[0];

    if (sigframe[0] > 0 && static_cast<uint64_t>(sigframe[0]) < NSIG) {
        current->sig_pending &= ~(1 << sigframe[0]);
    }

    return 0;
}

void Scheduler::deliver_signals(arch::x86::Registers *r) {
    if (!current || !current->sig_pending) return;

    for (int sig = 1; sig < NSIG; sig++) {
        if (!(current->sig_pending & (1 << sig))) continue;
        if (current->sig_blocked & (1 << sig)) continue;

        uint64_t handler = current->sig_handlers[sig].handler_addr;

        if (handler == 1) {
            /* Ignore */
            current->sig_pending &= ~(1 << sig);
            continue;
        }

        if (handler == 0) {
            /* Default action */
            if (sig == SIGKILL || sig == SIGSEGV || sig == SIGTERM) {
                current->exit_code = static_cast<uint32_t>(0x100 + sig);
                current->state = TaskState::EXITED;
                if (current->parent)
                    current->parent->has_child_exit = true;
                return;
            }
            current->sig_pending &= ~(1 << sig);
            continue;
        }

        /* Custom handler: push sigframe on user stack, call handler */
        uint64_t old_usp = r->user_rsp;
        uint64_t new_usp = old_usp - sizeof(uint64_t) * 7;
        if (new_usp < 0x1000) {
            /* Stack underflow, kill instead */
            current->exit_code = static_cast<uint32_t>(0x100 + sig);
            current->state = TaskState::EXITED;
            if (current->parent)
                current->parent->has_child_exit = true;
            return;
        }

        uint64_t sigframe[7];
        sigframe[0] = static_cast<uint64_t>(sig);
        sigframe[1] = r->rip;
        sigframe[2] = r->cs;
        sigframe[3] = r->rflags;
        sigframe[4] = old_usp;
        sigframe[5] = r->ss;
        sigframe[6] = 0;
        if (copy_to_user(reinterpret_cast<void *>(new_usp), sigframe, sizeof(sigframe)) != 0) {
            current->exit_code = static_cast<uint32_t>(0x100 + sig);
            current->state = TaskState::EXITED;
            if (current->parent)
                current->parent->has_child_exit = true;
            return;
        }

        current->sig_pending &= ~(1 << sig);
        r->user_rsp = new_usp;
        r->rip = handler;
        r->rdi = sig;
        return;
    }
}

// ─── EXIT ────────────────────────────────────────────────────────

void Scheduler::exit(uint32_t code) {
    if (!current || current->id == 0) return;

    fpu_save_current();
    current->exit_code = code;
    current->state = TaskState::EXITED;

    /* Notify parent */
    if (current->parent) {
        current->parent->has_child_exit = true;
    }

    /* Reparent children to idle */
    Task *child = current->child_head;
    while (child) {
        child->parent = &tasks[0];
        child = child->sibling_next;
    }
    current->child_head = nullptr;
    current->child_tail = nullptr;

    Task *next = dequeue();
    if (!next) {
        next = &tasks[0];
        if (next->ctx.rsp == 0) {
            drivers::NS16550::write("exit: idle has no context, halting\n");
            arch::x86::disable_interrupts();
            for (;;) __asm__ volatile ("hlt");
        }
    }
    current = next;
    current->state = TaskState::RUNNING;

    if (next->kstack) {
        uint64_t kst = reinterpret_cast<uint64_t>(next->kstack) + mm::Paging::PAGE_SIZE;
        arch::x86::TSS::set_kernel_stack(kst);
    }

    task_resume_exit(&next->ctx);
}

// ─── YIELD / PREEMPT ────────────────────────────────────────────

void Scheduler::preempt(arch::x86::Registers *r) {
    if (!current) return;

    Scheduler::deliver_signals(r);

    fpu_save_current();
    Task *prev = current;

    if (prev->id == 0) {
        prev->ctx.rsp = reinterpret_cast<uint64_t>(r);

        Task *next = dequeue();
        if (next) {
            current = next;
            next->state = TaskState::RUNNING;
            if (next->kstack) {
                uint64_t kst = reinterpret_cast<uint64_t>(next->kstack) + mm::Paging::PAGE_SIZE;
                arch::x86::TSS::set_kernel_stack(kst);
            }
            fpu_restore_current();
            task_resume(&next->ctx);
        }
        fpu_restore_current();
        return;
    }

    prev->ctx.rsp = reinterpret_cast<uint64_t>(r);

    prev->state = TaskState::READY;
    enqueue(prev);

    Task *next = dequeue();
    if (!next) {
        current = prev;
        prev->state = TaskState::RUNNING;
        fpu_restore_current();
        return;
    }

    current = next;
    next->state = TaskState::RUNNING;
    if (next->kstack) {
        uint64_t kstack_top = reinterpret_cast<uint64_t>(next->kstack) + mm::Paging::PAGE_SIZE;
        arch::x86::TSS::set_kernel_stack(kstack_top);
    }
    fpu_restore_current();
    task_resume(&next->ctx);
}

extern "C" void yield_handler_c(arch::x86::Registers *r) {
    if (!Scheduler::current) return;

    Scheduler::deliver_signals(r);

    fpu_save_current();
    Task *prev = Scheduler::current;

    if (prev->id == 0) {
        prev->ctx.rsp = reinterpret_cast<uint64_t>(r);
        Task *next = Scheduler::dequeue();
        if (next) {
            Scheduler::current = next;
            next->state = TaskState::RUNNING;
            if (next->kstack) {
                uint64_t kst = reinterpret_cast<uint64_t>(next->kstack) + mm::Paging::PAGE_SIZE;
                arch::x86::TSS::set_kernel_stack(kst);
            }
            fpu_restore_current();
            task_resume(&next->ctx);
        }
        fpu_restore_current();
        return;
    }

    prev->ctx.rsp = reinterpret_cast<uint64_t>(r);

    prev->state = TaskState::READY;
    Scheduler::enqueue(prev);

    Task *next = Scheduler::dequeue();
    Scheduler::current = next;
    next->state = TaskState::RUNNING;

    if (next->kstack) {
        uint64_t kst = reinterpret_cast<uint64_t>(next->kstack) + mm::Paging::PAGE_SIZE;
        arch::x86::TSS::set_kernel_stack(kst);
    }
    fpu_restore_current();
    task_resume(&next->ctx);
}

extern "C" void task_resume_exit(TaskContext *ctx) {
    fpu_restore_current();
    task_resume(ctx);
}

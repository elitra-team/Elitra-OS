#include "paging.hpp"
#include "pmm.hpp"
#include "vga.hpp"
#include "ns16550.hpp"
#include "isr.hpp"
#include "lib.hpp"
#include "heap.hpp"
#include "task.hpp"

using namespace mm;

uint64_t *Paging::pml4 = nullptr;
uint64_t  Paging::heap_phys = 0;

static const uint64_t PTE_MASK = 0x000FFFFFFFFFF000ULL;
static const uint64_t FLAGS_MASK = 0xFFFULL;
static const uint64_t PS_2MB = 0x80;

uint64_t pml4_i(uint64_t v) { return (v >> 39) & 0x1FF; }
uint64_t pdp_i(uint64_t v)  { return (v >> 30) & 0x1FF; }
uint64_t pd_i(uint64_t v)   { return (v >> 21) & 0x1FF; }
uint64_t pt_i(uint64_t v)   { return (v >> 12) & 0x1FF; }

void Paging::init() {
    __asm__ volatile("mov %%cr3, %0" : "=r"(pml4));

    drivers::NS16550::printf("paging: PML4=0x%lx\n", (uint64_t)pml4);

    arch::x86::ISR::register_handler(14, page_fault_handler);

    heap_phys = 0;
    alloc_heap_pages();

    drivers::VGA::writestring_color("Paging: 4-level paging active\n", 0x02);
    drivers::NS16550::write("paging: init done\n");
}

void Paging::enable() {}

static uint64_t *table_alloc() {
    uint64_t *f = reinterpret_cast<uint64_t *>(PMM::alloc_frame());
    if (f) lib::memset(f, 0, 4096);
    return f;
}

void Paging::map_page(uint64_t virt, uint64_t phys, uint64_t flags) {
    uint64_t i4 = pml4_i(virt), i3 = pdp_i(virt), i2 = pd_i(virt), i1 = pt_i(virt);
    uint64_t *t4 = pml4;
    if (!(t4[i4] & 1)) { t4[i4] = (uint64_t)table_alloc() | 3; }
    uint64_t *t3 = (uint64_t *)(t4[i4] & PTE_MASK);
    if (!(t3[i3] & 1)) { t3[i3] = (uint64_t)table_alloc() | 3; }
    uint64_t *t2 = (uint64_t *)(t3[i3] & PTE_MASK);
    if (!(t2[i2] & 1)) { t2[i2] = (uint64_t)table_alloc() | 3; }
    uint64_t *t1 = (uint64_t *)(t2[i2] & PTE_MASK);
    t1[i1] = (phys & PTE_MASK) | (flags & FLAGS_MASK) | 1;
    __asm__ volatile("invlpg %0" : : "m"(*(volatile char *)virt));
}

void Paging::unmap_page(uint64_t virt) {
    uint64_t i4 = pml4_i(virt), i3 = pdp_i(virt), i2 = pd_i(virt), i1 = pt_i(virt);
    uint64_t *t4 = pml4;
    if (!(t4[i4] & 1)) return;
    uint64_t *t3 = (uint64_t *)(t4[i4] & PTE_MASK);
    if (!(t3[i3] & 1)) return;
    uint64_t *t2 = (uint64_t *)(t3[i3] & PTE_MASK);
    if (!(t2[i2] & 1)) return;
    if (t2[i2] & PS_2MB) { t2[i2] = 0; __asm__ volatile("invlpg %0" : : "m"(*(volatile char *)virt)); return; }
    uint64_t *t1 = (uint64_t *)(t2[i2] & PTE_MASK);
    t1[i1] = 0;
    __asm__ volatile("invlpg %0" : : "m"(*(volatile char *)virt));
}

uint64_t Paging::get_phys(uint64_t virt) {
    uint64_t i4 = pml4_i(virt), i3 = pdp_i(virt), i2 = pd_i(virt), i1 = pt_i(virt);
    uint64_t *t4 = pml4;
    if (!(t4[i4] & 1)) return ~0ULL;
    uint64_t *t3 = (uint64_t *)(t4[i4] & PTE_MASK);
    if (!(t3[i3] & 1)) return ~0ULL;
    uint64_t *t2 = (uint64_t *)(t3[i3] & PTE_MASK);
    if (!(t2[i2] & 1)) return ~0ULL;
    if (t2[i2] & PS_2MB) return (t2[i2] & PTE_MASK) | (virt & 0x1FFFFF);
    uint64_t *t1 = (uint64_t *)(t2[i2] & PTE_MASK);
    if (!(t1[i1] & 1)) return ~0ULL;
    return (t1[i1] & PTE_MASK) | (virt & 0xFFF);
}

uint64_t *Paging::page_directory() { return pml4; }

uint64_t *Paging::clone_kernel_dir() {
    uint64_t *np = table_alloc();
    if (!np) return nullptr;
    for (int i = 256; i < 512; i++) np[i] = pml4[i];
    return np;
}

void Paging::copy_user_pages(uint64_t *src, uint64_t *dst) {
    for (int i4 = 0; i4 < 256; i4++) {
        if (!(src[i4] & 1)) continue;
        uint64_t *s3 = (uint64_t *)(src[i4] & PTE_MASK);
        if (!(dst[i4] & 1)) { dst[i4] = (uint64_t)table_alloc() | 7; }
        uint64_t *d3 = (uint64_t *)(dst[i4] & PTE_MASK);
        for (int i3 = 0; i3 < 512; i3++) {
            if (!(s3[i3] & 1)) continue;
            uint64_t *s2 = (uint64_t *)(s3[i3] & PTE_MASK);
            if (!(d3[i3] & 1)) { d3[i3] = (uint64_t)table_alloc() | 7; }
            uint64_t *d2 = (uint64_t *)(d3[i3] & PTE_MASK);
            for (int i2 = 0; i2 < 512; i2++) {
                if (!(s2[i2] & 1)) continue;
                if (s2[i2] & PS_2MB) {
                    uint64_t *pt = table_alloc();
                    if (!pt) return;
                    uint64_t base = s2[i2] & PTE_MASK;
                    uint64_t fl = (s2[i2] & FLAGS_MASK) & ~PS_2MB;
                    for (int i1 = 0; i1 < 512; i1++)
                        pt[i1] = (base + i1 * 4096) | fl | 1;
                    d2[i2] = (uint64_t)pt | (fl & ~PS_2MB) | 1;
                }
                uint64_t *s1 = (uint64_t *)(s2[i2] & PTE_MASK);
                if (!(d2[i2] & 1)) { d2[i2] = (uint64_t)table_alloc() | 7; }
                uint64_t *d1 = (uint64_t *)(d2[i2] & PTE_MASK);
                for (int i1 = 0; i1 < 512; i1++) {
                    d1[i1] = s1[i1];
                    if (d1[i1] & 1) d1[i1] |= 4;
                }
            }
        }
    }
}

void Paging::free_user_pages(uint64_t pa) {
    uint64_t *t4 = (uint64_t *)pa;
    for (int i4 = 0; i4 < 256; i4++) {
        if (!(t4[i4] & 1)) continue;
        uint64_t *t3 = (uint64_t *)(t4[i4] & PTE_MASK);
        for (int i3 = 0; i3 < 512; i3++) {
            if (!(t3[i3] & 1)) continue;
            uint64_t *t2 = (uint64_t *)(t3[i3] & PTE_MASK);
            for (int i2 = 0; i2 < 512; i2++) {
                if (!(t2[i2] & 1)) continue;
                if (t2[i2] & PS_2MB) continue;
                uint64_t *t1 = (uint64_t *)(t2[i2] & PTE_MASK);
                for (int i1 = 0; i1 < 512; i1++)
                    if (t1[i1] & 1) PMM::free_frame((void *)(t1[i1] & PTE_MASK));
                PMM::free_frame((void *)(t2[i2] & PTE_MASK));
            }
            PMM::free_frame((void *)(t3[i3] & PTE_MASK));
        }
        PMM::free_frame((void *)(t4[i4] & PTE_MASK));
    }
    PMM::free_frame((void *)pa);
}

uint64_t Paging::pd_to_phys(uint64_t *pd) {
    return (uint64_t)pd;
}

void Paging::page_fault_handler(arch::x86::Registers *r) {
    uint64_t addr;
    __asm__ volatile("mov %%cr2, %0" : "=r"(addr));
    drivers::VGA::printf("\nPAGE FAULT at 0x%lx RIP=0x%lx RSP=0x%lx\n", addr, r->rip, r->rsp);
    drivers::VGA::printf("  %s %s %s%s\n",
        (r->err_code & 1) ? "protection" : "not-present",
        (r->err_code & 2) ? "write" : "read",
        (r->err_code & 4) ? "user" : "supervisor",
        (r->err_code & 8) ? " reserved" : "");
    if (kernel::Scheduler::current) {
        drivers::NS16550::printf("pagefault: kill task %d\n", kernel::Scheduler::current->id);
        kernel::Scheduler::exit(11);
    }
    __asm__ volatile("cli; hlt");
}

void Paging::identity_map(uint64_t s, uint64_t e) { (void)s; (void)e; }

void Paging::alloc_heap_pages() {
    for (uint64_t off = 0; off < HEAP_INITIAL; off += 4096) {
        uint64_t *p = (uint64_t *)PMM::alloc_frame();
        if (!p) { drivers::NS16550::write("heap OOM\n"); break; }
        uint64_t phys = (uint64_t)p;
        map_page(HEAP_VADDR + off, phys, 3);
        if (!heap_phys) heap_phys = phys;
    }
    drivers::NS16550::printf("heap: mapped %d KB at 0x%lx\n", HEAP_INITIAL / 1024, (uint64_t)HEAP_VADDR);
}

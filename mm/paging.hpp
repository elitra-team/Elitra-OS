#ifndef ELITRA_PAGING_HPP
#define ELITRA_PAGING_HPP

#include <cstdint>
#include "isr.hpp"

namespace mm {

class Paging {
public:
    static void init();
    static void map_page(uint64_t virt, uint64_t phys, uint64_t flags);
    static void unmap_page(uint64_t virt);
    static uint64_t get_phys(uint64_t virt);
    static void enable();

    static const uint64_t PAGE_SIZE    = 4096;
    static const uint64_t PAGE_PRESENT = 0x1;
    static const uint64_t PAGE_WRITE   = 0x2;
    static const uint64_t PAGE_USER    = 0x4;

    static const uint64_t HEAP_VADDR   = 0x40000000;
    static const uint64_t HEAP_INITIAL = 0x400000;
    static const uint64_t HEAP_MAX     = 0x10000000;

    static uint64_t *page_directory();
    static uint64_t *clone_kernel_dir();

    static void copy_user_pages(uint64_t *src_pml4, uint64_t *dst_pml4);
    static void free_user_pages(uint64_t pml4_paddr);
    static uint64_t pd_to_phys(uint64_t *pd);

private:
    static uint64_t *pml4;
    static uint64_t  heap_phys;

    static void page_fault_handler(arch::x86::Registers *r);
    static void identity_map(uint64_t start, uint64_t end);
    static void alloc_heap_pages();

    static uint64_t *walk_pml4(uint64_t *table, uint64_t virt, bool alloc);
};

}

#endif

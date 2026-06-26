#include "pmm.hpp"
#include "vga.hpp"
#include "ns16550.hpp"

extern "C" {
void  krust_pmm_init(void *bitmap, uint32_t bitmap_size, uint32_t total_frames);
void  krust_pmm_mark_used(void *bitmap, uint32_t start, uint32_t end);
void  krust_pmm_free_frames(void *bitmap, uint32_t start_frame, uint32_t count);
uint32_t krust_pmm_alloc_frame(void);
void  krust_pmm_free_frame(uint32_t frame);
void *krust_pmm_get_bitmap(void);
uint32_t krust_pmm_get_total_frames(void);
}

using namespace mm;

uint32_t *PMM::bitmap = nullptr;
uint32_t PMM::total_frames_count = 0;
uint32_t PMM::used_frames_count = 0;

void PMM::init(uint32_t mem_upper_kb, uint64_t placement_addr) {
    total_frames_count = (1024 + mem_upper_kb) / 4;
    drivers::NS16550::printf("pmm: frames=%d\n", total_frames_count);

    uint32_t bitmap_size = (total_frames_count + 7) / 8;
    bitmap = reinterpret_cast<uint32_t *>((placement_addr + 0xFFF) & ~0xFFF);
    drivers::NS16550::printf("pmm: bitmap=0x%lx size=%d\n", (uint64_t)(uintptr_t)bitmap, bitmap_size);

    krust_pmm_init(bitmap, bitmap_size, total_frames_count);
    used_frames_count = total_frames_count;
    drivers::NS16550::printf("pmm: memset ok size=%d\n", bitmap_size);

    uintptr_t bitmap_end = reinterpret_cast<uintptr_t>(bitmap) + bitmap_size;
    uint32_t first_free = (bitmap_end + 0xFFF) / 0x1000;
    drivers::NS16550::printf("pmm: first_free=%d\n", first_free);

    uint32_t free_count = total_frames_count - first_free;
    krust_pmm_free_frames(bitmap, first_free, free_count);
    used_frames_count -= free_count;

    drivers::VGA::printf("PMM: %d frames (%d KB), %d free\n",
                             total_frames_count, total_frames_count * 4, free_frames());
}

void *PMM::alloc_frame() {
    uint32_t frame = krust_pmm_alloc_frame();
    if (frame == 0xFFFFFFFF)
        return nullptr;
    used_frames_count++;
    return reinterpret_cast<void *>(frame * 0x1000);
}

void PMM::free_frame(void *addr) {
    uintptr_t frame = reinterpret_cast<uintptr_t>(addr) / 0x1000;
    if (frame >= total_frames_count)
        return;
    krust_pmm_free_frame(frame);
    used_frames_count--;
}

uint32_t PMM::free_frames() {
    return total_frames_count - used_frames_count;
}

uint32_t PMM::used_frames() {
    return used_frames_count;
}

uint32_t PMM::total_frames() {
    return total_frames_count;
}

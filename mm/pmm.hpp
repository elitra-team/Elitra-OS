#ifndef ELITRA_PMM_HPP
#define ELITRA_PMM_HPP

#include <cstdint>
#include <cstddef>

namespace mm {

class PMM {
public:
    static void init(uint32_t mem_upper_kb, uint64_t placement_addr);
    static void *alloc_frame();
    static void free_frame(void *addr);
    static uint32_t free_frames();
    static uint32_t used_frames();
    static uint32_t total_frames();

private:
    static const uint32_t FRAME_SIZE = 4096;

    static uint32_t *bitmap;
    static uint32_t total_frames_count;
    static uint32_t used_frames_count;

    static void set_bit(uint32_t frame);
    static void clear_bit(uint32_t frame);
    static bool test_bit(uint32_t frame);
    static uint32_t find_first_free();
};

}

#endif

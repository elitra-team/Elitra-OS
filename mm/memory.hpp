#ifndef ELITRA_MEMORY_HPP
#define ELITRA_MEMORY_HPP

#include <cstdint>
#include <cstddef>

namespace mm {

void init(uint32_t magic, uint32_t addr);
void *kmalloc(size_t size);
void *kmalloc_aligned(size_t size, uint32_t align);
void info(uint32_t *total_kb, uint32_t *free_kb);

}

#endif

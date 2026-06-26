#ifndef ELITRA_HEAP_HPP
#define ELITRA_HEAP_HPP

#include <cstdint>
#include <cstddef>

namespace mm {

class Heap {
public:
    static void init();
    static void *alloc(size_t size);
    static void free(void *ptr);
    static void *realloc(void *ptr, size_t size);

private:
    struct Block {
        size_t   size;
        bool     free;
        Block   *next;
    } __attribute__((packed));

    static const uintptr_t HEAP_START = 0x40000000;
    static const size_t    HEAP_SIZE  = 0x10000000;

    static Block *head;

    static Block *find_block(Block **prev, size_t size);
    static void split_block(Block *block, size_t size);
    static void merge_adjacent();
};

void *malloc(size_t size);
void free(void *ptr);
void *realloc(void *ptr, size_t size);

}

#endif

#include "heap.hpp"
#include "paging.hpp"
#include "lib.hpp"
#include "vga.hpp"
#include "spinlock.hpp"

using namespace mm;

Heap::Block *Heap::head = nullptr;

void Heap::init() {
    head = reinterpret_cast<Block *>(HEAP_START);
    head->size = HEAP_SIZE - sizeof(Block);
    head->free = true;
    head->next = nullptr;

    drivers::VGA::printf("Heap: 0x%x - 0x%x (%d MB)\n",
                             HEAP_START, HEAP_START + HEAP_SIZE, HEAP_SIZE / 0x100000);
}

Heap::Block *Heap::find_block(Block **prev, size_t size) {
    Block *current = head;
    *prev = nullptr;

    while (current) {
        if (current->free && current->size >= size)
            return current;
        *prev = current;
        current = current->next;
    }
    return nullptr;
}

void Heap::split_block(Block *block, size_t size) {
    if (block->size < size + sizeof(Block) + 16)
        return;

    Block *new_block = reinterpret_cast<Block *>(
        reinterpret_cast<uintptr_t>(block) + sizeof(Block) + size);
    new_block->size = block->size - size - sizeof(Block);
    new_block->free = true;
    new_block->next = block->next;

    block->size = size;
    block->next = new_block;
}

void Heap::merge_adjacent() {
    Block *current = head;
    while (current && current->next) {
        if (current->free && current->next->free) {
            current->size += sizeof(Block) + current->next->size;
            current->next = current->next->next;
        } else {
            current = current->next;
        }
    }
}

void *Heap::alloc(size_t size) {
    kernel::IrqLock lock;
    if (!head) return nullptr;

    if (size == 0) size = 1;
    size = (size + 3) & ~3;

    Block *prev;
    Block *block = find_block(&prev, size);
    if (!block)
        return nullptr;

    split_block(block, size);
    block->free = false;
    return reinterpret_cast<void *>(reinterpret_cast<uintptr_t>(block) + sizeof(Block));
}

void Heap::free(void *ptr) {
    if (!ptr) return;

    kernel::IrqLock lock;
    Block *block = reinterpret_cast<Block *>(
        reinterpret_cast<uintptr_t>(ptr) - sizeof(Block));
    block->free = true;
    merge_adjacent();
}

void *Heap::realloc(void *ptr, size_t size) {
    if (!ptr) return alloc(size);
    if (size == 0) { free(ptr); return nullptr; }

    Block *block = reinterpret_cast<Block *>(
        reinterpret_cast<uintptr_t>(ptr) - sizeof(Block));

    if (block->size >= size) {
        split_block(block, size);
        return ptr;
    }

    void *new_ptr = alloc(size);
    if (new_ptr) {
        lib::memcpy(new_ptr, ptr, block->size);
        free(ptr);
    }
    return new_ptr;
}

void *mm::malloc(size_t size) {
    return Heap::alloc(size);
}

void mm::free(void *ptr) {
    Heap::free(ptr);
}

void *mm::realloc(void *ptr, size_t size) {
    return Heap::realloc(ptr, size);
}

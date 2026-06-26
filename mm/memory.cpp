#include "memory.hpp"
#include "pmm.hpp"
#include "vga.hpp"
#include "ns16550.hpp"
#include "lib.hpp"

using namespace mm;

extern uint32_t _kernel_end;

static uint64_t placement_addr;
static uint32_t total_memory_kb;
static uint32_t used_memory_kb;

struct MultibootInfo {
    uint32_t flags;
    uint32_t mem_lower;
    uint32_t mem_upper;
    uint32_t boot_device;
    uint32_t cmdline;
    uint32_t mods_count;
    uint32_t mods_addr;
    uint32_t syms[4];
    uint32_t mmap_length;
    uint32_t mmap_addr;
    uint32_t drives_length;
    uint32_t drives_addr;
    uint32_t config_table;
    uint32_t boot_loader_name;
    uint32_t apm_table;
    uint32_t vbe_control_info;
    uint32_t vbe_mode_info;
    uint16_t vbe_mode;
    uint16_t vbe_interface_seg;
    uint16_t vbe_interface_off;
    uint16_t vbe_interface_len;
    uint64_t framebuffer_addr;
    uint32_t framebuffer_pitch;
    uint32_t framebuffer_width;
    uint32_t framebuffer_height;
    uint8_t  framebuffer_bpp;
    uint8_t  framebuffer_type;
    uint8_t  color_info[6];
} __attribute__((packed));

void mm::init(uint32_t magic, uint32_t addr) {
    placement_addr = reinterpret_cast<uint64_t>(&_kernel_end);

    uint32_t mem_upper_kb;
    if (magic != 0x2BADB002) {
        drivers::VGA::writestring_color("[WARN] Not booted by GRUB, assuming 32MB RAM\n",
                                             static_cast<uint8_t>(drivers::VGAColor::BROWN));
        mem_upper_kb = 32 * 1024 - 1024;
    } else {
        auto *mbi = reinterpret_cast<MultibootInfo *>(addr);
        mem_upper_kb = (mbi->flags & (1 << 0)) ? mbi->mem_upper : (32 * 1024 - 1024);
    }
    drivers::NS16550::write("step: mm detected memory\n");

    total_memory_kb = mem_upper_kb + 1024;
    used_memory_kb = (placement_addr - 0x100000 + 1023) / 1024;
    drivers::NS16550::write("step: mm calculated\n");

    PMM::init(mem_upper_kb, placement_addr);
    drivers::NS16550::write("step: after pmm init\n");

    drivers::NS16550::write("step: before display printf\n");
    drivers::VGA::printf("Memory: %d KB total\n", total_memory_kb);
    drivers::NS16550::write("step: mm printf done\n");
}

void *mm::kmalloc(size_t size) {
    uint64_t addr = placement_addr;
    placement_addr += size;
    placement_addr = (placement_addr + 3) & ~3;
    used_memory_kb = (placement_addr - 0x100000 + 1023) / 1024;
    return reinterpret_cast<void *>(addr);
}

void *mm::kmalloc_aligned(size_t size, uint32_t align) {
    uint64_t addr = placement_addr;
    if (addr & (align - 1))
        addr = (addr + align - 1) & ~(align - 1);
    placement_addr = addr + size;
    used_memory_kb = (placement_addr - 0x100000 + 1023) / 1024;
    return reinterpret_cast<void *>(addr);
}

void mm::info(uint32_t *total_kb, uint32_t *free_kb) {
    *total_kb = total_memory_kb;
    *free_kb = (used_memory_kb < total_memory_kb) ? total_memory_kb - used_memory_kb : 0;
}

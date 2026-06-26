#include "elf.hpp"
#include "paging.hpp"
#include "pmm.hpp"
#include "lib.hpp"
#include "vga.hpp"

using namespace loader;

int loader::load_elf(const uint8_t *data, uint32_t size, uint64_t *entry_out) {
    if (size < sizeof(Elf32Header)) return -1;

    const Elf32Header *hdr = reinterpret_cast<const Elf32Header *>(data);

    if (hdr->e_ident[EI_MAG0] != ELFMAG0 ||
        hdr->e_ident[EI_MAG1] != ELFMAG1 ||
        hdr->e_ident[EI_MAG2] != ELFMAG2 ||
        hdr->e_ident[EI_MAG3] != ELFMAG3) {
        drivers::VGA::writestring("ELF: bad magic\n");
        return -1;
    }

    if (hdr->e_ident[EI_CLASS] != ELFCLASS32) {
        drivers::VGA::writestring("ELF: not 32-bit\n");
        return -1;
    }
    if (hdr->e_ident[EI_DATA] != ELFDATA2LSB) {
        drivers::VGA::writestring("ELF: not little-endian\n");
        return -1;
    }
    if (hdr->e_type != ET_EXEC) {
        drivers::VGA::writestring("ELF: not executable\n");
        return -1;
    }
    if (hdr->e_machine != EM_386) {
        drivers::VGA::writestring("ELF: not i386\n");
        return -1;
    }

    uint32_t phoff = hdr->e_phoff;
    uint32_t phnum = hdr->e_phnum;
    uint32_t phentsize = hdr->e_phentsize;

    for (uint32_t i = 0; i < phnum; i++) {
        if (phoff + (i + 1) * phentsize > size) {
            drivers::VGA::printf("ELF: program header %d out of bounds\n", i);
            return -1;
        }

        const Elf32ProgramHeader *ph = reinterpret_cast<const Elf32ProgramHeader *>(
            data + phoff + i * phentsize);

        if (ph->p_type != PT_LOAD) continue;

        uint32_t page_start = ph->p_vaddr & ~0xFFF;
        uint32_t page_end = (ph->p_vaddr + ph->p_memsz + 0xFFF) & ~0xFFF;

        for (uint32_t vaddr = page_start; vaddr < page_end; vaddr += mm::Paging::PAGE_SIZE) {
            void *phys = mm::PMM::alloc_frame();
            if (!phys) {
                drivers::VGA::printf("ELF: OOM at vaddr 0x%x\n", vaddr);
                return -1;
            }
            lib::memset(phys, 0, mm::Paging::PAGE_SIZE);
            mm::Paging::map_page(vaddr, reinterpret_cast<uint64_t>(phys),
                                 mm::Paging::PAGE_PRESENT | mm::Paging::PAGE_WRITE | mm::Paging::PAGE_USER);
        }

        if (ph->p_offset + ph->p_filesz <= size) {
            lib::memcpy(reinterpret_cast<void *>(ph->p_vaddr),
                        data + ph->p_offset, ph->p_filesz);
        }

        if (ph->p_memsz > ph->p_filesz) {
            lib::memset(reinterpret_cast<void *>(ph->p_vaddr + ph->p_filesz),
                        0, ph->p_memsz - ph->p_filesz);
        }
    }

    *entry_out = hdr->e_entry;
    return 0;
}

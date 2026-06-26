#ifndef ELITRA_ELF_HPP
#define ELITRA_ELF_HPP

#include <cstdint>

namespace loader {

#define EI_NIDENT 16

struct Elf32Header {
    uint8_t  e_ident[EI_NIDENT];
    uint16_t e_type;
    uint16_t e_machine;
    uint32_t e_version;
    uint32_t e_entry;
    uint32_t e_phoff;
    uint32_t e_shoff;
    uint32_t e_flags;
    uint16_t e_ehsize;
    uint16_t e_phentsize;
    uint16_t e_phnum;
    uint16_t e_shentsize;
    uint16_t e_shnum;
    uint16_t e_shstrndx;
};

struct Elf32ProgramHeader {
    uint32_t p_type;
    uint32_t p_offset;
    uint32_t p_vaddr;
    uint32_t p_paddr;
    uint32_t p_filesz;
    uint32_t p_memsz;
    uint32_t p_flags;
    uint32_t p_align;
};

enum Elf32Ident {
    EI_MAG0       = 0,
    EI_MAG1       = 1,
    EI_MAG2       = 2,
    EI_MAG3       = 3,
    EI_CLASS      = 4,
    EI_DATA       = 5,
    EI_VERSION    = 6,
    EI_OSABI      = 7,
    EI_ABIVERSION = 8,
};

#define ELFMAG0   0x7F
#define ELFMAG1  'E'
#define ELFMAG2  'L'
#define ELFMAG3  'F'
#define ELFCLASS32 1
#define ELFDATA2LSB 1
#define ET_EXEC  2
#define EM_386   3
#define PT_LOAD  1

int load_elf(const uint8_t *data, uint32_t size, uint64_t *entry_out);

}

#endif

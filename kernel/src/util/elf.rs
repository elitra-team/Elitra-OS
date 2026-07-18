use core::ptr;

const EI_NIDENT: usize = 16;
const ELFMAG0: u8 = 0x7F;
const ELFMAG1: u8 = b'E';
const ELFMAG2: u8 = b'L';
const ELFMAG3: u8 = b'F';
const ELFCLASS32: u8 = 1;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const EM_386: u16 = 3;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const EI_CLASS: usize = 4;
const EI_DATA: usize = 5;

const PAGE_PRESENT: u64 = 0x1;
const PAGE_WRITE: u64 = 0x2;
const PAGE_USER: u64 = 0x4;

// ASLR: randomize base address for PIE executables (ET_DYN)
const ASLR_BASE: u64 = 0x200000; // 2MB minimum
const ASLR_BITS: u64 = 8; // 256 possible base positions

#[repr(C)]
pub struct Elf32Header {
    pub e_ident: [u8; EI_NIDENT],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u32,
    pub e_phoff: u32,
    pub e_shoff: u32,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

#[repr(C)]
pub struct Elf32ProgramHeader {
    pub p_type: u32,
    pub p_offset: u32,
    pub p_vaddr: u32,
    pub p_paddr: u32,
    pub p_filesz: u32,
    pub p_memsz: u32,
    pub p_flags: u32,
    pub p_align: u32,
}

#[repr(C)]
pub struct Elf64Header {
    pub e_ident: [u8; EI_NIDENT],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

#[repr(C)]
pub struct Elf64ProgramHeader {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

extern "C" {
    fn krust_pmm_alloc_frame() -> usize;
    fn krust_pmm_free_frame(frame: usize);
    fn krust_paging_map_page(virt: u64, phys: u64, flags: u64) -> bool;
    fn krust_paging_unmap_page(virt: u64);
    fn krust_page_size() -> u64;
}

unsafe fn generate_aslr_base() -> u64 {
    let random = crate::util::rdrand::random_u64();
    // Align to page boundary and apply ASLR
    let offset = (random % (1 << ASLR_BITS)) << 12;
    ASLR_BASE + offset
}

unsafe fn memset(s: *mut u8, c: i32, n: usize) {
    for i in 0..n {
        ptr::write_volatile(s.add(i), c as u8);
    }
}

unsafe fn memcpy(dst: *mut u8, src: *const u8, n: usize) {
    for i in 0..n {
        ptr::write_volatile(dst.add(i), ptr::read_volatile(src.add(i)));
    }
}

/// Free all pages that were mapped for a failed ELF load (tracked by base vaddr).
unsafe fn elf_cleanup_pages(start: u64, end: u64, page_size: u64) {
    let mut addr = start;
    while addr < end {
        let phys = crate::paging::krust_paging_get_phys(addr);
        if phys != !0u64 {
            krust_paging_unmap_page(addr);
            krust_pmm_free_frame((phys / page_size) as usize);
        }
        addr += page_size;
    }
}

unsafe fn load_elf32(data: *const u8, size: u32, entry_out: *mut u64, aslr_base: u64) -> i32 {
    let hdr = &*(data as *const Elf32Header);
    let phoff = hdr.e_phoff as u64;
    let phnum = hdr.e_phnum as u64;
    let phentsize = hdr.e_phentsize as u64;
    let page_size = krust_page_size();
    let page_mask = page_size - 1;

    // Track mapping range for cleanup on failure
    let mut map_start: u64 = !0u64;
    let mut map_end: u64 = 0;

    for i in 0..phnum {
        if phoff + (i + 1) * phentsize > size as u64 {
            elf_cleanup_pages(map_start, map_end, page_size);
            return -1;
        }

        let ph = &*((data as u64 + phoff + i * phentsize) as *const Elf32ProgramHeader);

        if ph.p_type != PT_LOAD {
            continue;
        }

        let vaddr = (ph.p_vaddr as u64 + aslr_base) & !page_mask;
        let vaddr_end = ((ph.p_vaddr as u64 + aslr_base) + ph.p_memsz as u64 + page_mask) & !page_mask;

        if vaddr < map_start { map_start = vaddr; }
        if vaddr_end > map_end { map_end = vaddr_end; }

        let mut vaddr = vaddr;
        while vaddr < vaddr_end {
            let phys = krust_pmm_alloc_frame();
            if phys == !0 {
                elf_cleanup_pages(map_start, map_end, page_size);
                return -1;
            }
            memset((phys * page_size as usize) as *mut u8, 0, page_size as usize);
            if !krust_paging_map_page(vaddr, (phys as u64) * page_size, PAGE_PRESENT | PAGE_WRITE | PAGE_USER) {
                krust_pmm_free_frame(phys);
                elf_cleanup_pages(map_start, map_end, page_size);
                return -1;
            }
            vaddr += page_size;
        }

        if ph.p_offset as u64 + ph.p_filesz as u64 <= size as u64 {
            memcpy(
                (ph.p_vaddr as u64 + aslr_base) as *mut u8,
                data.add(ph.p_offset as usize),
                ph.p_filesz as usize,
            );
        }

        if ph.p_memsz > ph.p_filesz {
            memset(
                ((ph.p_vaddr as u64 + aslr_base) + ph.p_filesz as u64) as *mut u8,
                0,
                (ph.p_memsz - ph.p_filesz) as usize,
            );
        }
    }

    *entry_out = hdr.e_entry as u64 + aslr_base;
    0
}

unsafe fn load_elf64(data: *const u8, size: u32, entry_out: *mut u64, aslr_base: u64) -> i32 {
    if (size as usize) < core::mem::size_of::<Elf64Header>() {
        return -1;
    }

    let hdr = &*(data as *const Elf64Header);
    let phoff = hdr.e_phoff;
    let phnum = hdr.e_phnum as u64;
    let phentsize = hdr.e_phentsize as u64;
    let page_size = krust_page_size();
    let page_mask = page_size - 1;

    let mut map_start: u64 = !0u64;
    let mut map_end: u64 = 0;

    for i in 0..phnum {
        if phoff + (i + 1) * phentsize > size as u64 {
            elf_cleanup_pages(map_start, map_end, page_size);
            return -1;
        }

        let ph = &*((data as u64 + phoff + i * phentsize) as *const Elf64ProgramHeader);

        if ph.p_type != PT_LOAD {
            continue;
        }

        let page_start = (ph.p_vaddr + aslr_base) & !page_mask;
        let page_end = (ph.p_vaddr + aslr_base + ph.p_memsz + page_mask) & !page_mask;

        if page_start < map_start { map_start = page_start; }
        if page_end > map_end { map_end = page_end; }

        let mut vaddr = page_start;
        while vaddr < page_end {
            let phys = krust_pmm_alloc_frame();
            if phys == !0 {
                elf_cleanup_pages(map_start, map_end, page_size);
                return -1;
            }
            memset((phys * page_size as usize) as *mut u8, 0, page_size as usize);
            if !krust_paging_map_page(vaddr, (phys as u64) * page_size, PAGE_PRESENT | PAGE_WRITE | PAGE_USER) {
                krust_pmm_free_frame(phys);
                elf_cleanup_pages(map_start, map_end, page_size);
                return -1;
            }
            vaddr += page_size;
        }

        if ph.p_offset + ph.p_filesz <= size as u64 {
            memcpy(
                (ph.p_vaddr + aslr_base) as *mut u8,
                data.add(ph.p_offset as usize),
                ph.p_filesz as usize,
            );
        }

        if ph.p_memsz > ph.p_filesz {
            memset(
                (ph.p_vaddr + aslr_base + ph.p_filesz) as *mut u8,
                0,
                (ph.p_memsz - ph.p_filesz) as usize,
            );
        }
    }

    *entry_out = hdr.e_entry + aslr_base;
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_elf_load(data: *const u8, size: u32, entry: *mut u64) -> i32 {
    if (size as usize) < EI_NIDENT {
        return -1;
    }

    if ptr::read_volatile(data.add(0)) != ELFMAG0
        || ptr::read_volatile(data.add(1)) != ELFMAG1
        || ptr::read_volatile(data.add(2)) != ELFMAG2
        || ptr::read_volatile(data.add(3)) != ELFMAG3
    {
        return -1;
    }
    if ptr::read_volatile(data.add(EI_DATA)) != ELFDATA2LSB {
        return -1;
    }

    let class = ptr::read_volatile(data.add(EI_CLASS));
    if class == ELFCLASS32 {
        if (size as usize) < core::mem::size_of::<Elf32Header>() {
            return -1;
        }
        let hdr = &*(data as *const Elf32Header);
        if hdr.e_type != ET_EXEC {
            return -1;
        }
        if hdr.e_machine != EM_386 {
            return -1;
        }
        return load_elf32(data, size, entry, 0);
    }

    if class == ELFCLASS64 {
        if (size as usize) < core::mem::size_of::<Elf64Header>() {
            return -1;
        }
        let hdr = &*(data as *const Elf64Header);
        if hdr.e_type != ET_EXEC && hdr.e_type != ET_DYN {
            return -1;
        }
        if hdr.e_machine != EM_X86_64 {
            return -1;
        }
        // ASLR only for PIE (ET_DYN), not for fixed-address (ET_EXEC)
        let aslr_base = if hdr.e_type == ET_DYN { generate_aslr_base() } else { 0 };
        return load_elf64(data, size, entry, aslr_base);
    }

    -1
}

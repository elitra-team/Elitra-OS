use core::ptr;

extern "C" {
    fn krust_malloc(size: u32) -> *mut u8;
    fn krust_free(ptr: *mut u8);
    fn krust_memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8;
    fn krust_memset(s: *mut u8, c: i32, n: usize) -> *mut u8;
    fn krust_memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32;
    fn krust_strlen(s: *const u8) -> usize;
    fn krust_strcmp(s1: *const u8, s2: *const u8) -> i32;
    fn krust_strncpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8;
}

unsafe fn read_le16(p: *const u8) -> u16 {
    (ptr::read_volatile(p) as u16) | ((ptr::read_volatile(p.add(1)) as u16) << 8)
}

unsafe fn read_le32(p: *const u8) -> u32 {
    (ptr::read_volatile(p) as u32)
        | ((ptr::read_volatile(p.add(1)) as u32) << 8)
        | ((ptr::read_volatile(p.add(2)) as u32) << 16)
        | ((ptr::read_volatile(p.add(3)) as u32) << 24)
}

unsafe fn write_le16(p: *mut u8, v: u16) {
    ptr::write_volatile(p, v as u8);
    ptr::write_volatile(p.add(1), (v >> 8) as u8);
}

unsafe fn write_le32(p: *mut u8, v: u32) {
    ptr::write_volatile(p, v as u8);
    ptr::write_volatile(p.add(1), (v >> 8) as u8);
    ptr::write_volatile(p.add(2), (v >> 16) as u8);
    ptr::write_volatile(p.add(3), (v >> 24) as u8);
}

const EXT2_MAGIC: u16 = 0xEF53;
const EXT2_S_IFMT: u16 = 0xF000;
const EXT2_S_IFDIR: u16 = 0x4000;
const EXT2_S_IFREG: u16 = 0x8000;
const EXT2_FT_DIR: u8 = 2;
const EXT2_FT_REG: u8 = 1;

const BGDT_SIZE: usize = 32;

#[repr(C)]
pub struct Ext2Superblock {
    pub inodes_count: u32,
    pub blocks_count: u32,
    pub r_blocks_count: u32,
    pub free_blocks_count: u32,
    pub free_inodes_count: u32,
    pub first_data_block: u32,
    pub log_block_size: u32,
    pub log_frag_size: u32,
    pub blocks_per_group: u32,
    pub frags_per_group: u32,
    pub inodes_per_group: u32,
    pub mtime: u32,
    pub wtime: u32,
    pub mnt_count: u16,
    pub max_mnt_count: u16,
    pub magic: u16,
    pub state: u16,
    pub errors: u16,
    pub minor_rev_level: u16,
    pub lastcheck: u32,
    pub checkinterval: u32,
    pub creator_os: u32,
    pub rev_level: u32,
    pub def_resuid: u16,
    pub def_resgid: u16,
}

#[repr(C)]
pub struct Ext2BlockGroupDescriptor {
    pub block_bitmap: u32,
    pub inode_bitmap: u32,
    pub inode_table: u32,
    pub free_blocks_count: u16,
    pub free_inodes_count: u16,
    pub used_dirs_count: u16,
    pub padding: u16,
}

#[repr(C)]
pub struct Ext2Inode {
    pub mode: u16,
    pub uid: u16,
    pub size: u32,
    pub atime: u32,
    pub ctime: u32,
    pub mtime: u32,
    pub dtime: u32,
    pub gid: u16,
    pub links_count: u16,
    pub blocks: u32,
    pub flags: u32,
    pub osd1: u32,
    pub block: [u32; 15],
    pub generation: u32,
    pub file_acl: u32,
    pub dir_acl: u32,
    pub faddr: u32,
    pub osd2: [u32; 3],
}

#[repr(C)]
pub struct Ext2DirEntry {
    pub inode: u32,
    pub rec_len: u16,
    pub name_len: u8,
    pub file_type: u8,
}

#[repr(C)]
pub struct Ext2Instance {
    pub image: *mut u8,
    pub image_size: u32,
    pub block_size: u32,
    pub inodes_per_group: u32,
    pub blocks_per_group: u32,
    pub inode_size: u32,
    pub inode_table_blocks: u32,
    pub first_data_block: u32,
    pub bgdt_block: u32,
    pub mounted: bool,
    pub mount_point: [u8; 128],
}

unsafe fn read_block<'a>(fs: *const Ext2Instance, block_num: u32) -> *const u8 {
    let offset = (block_num as u64) * (*fs).block_size as u64;
    if offset + (*fs).block_size as u64 > (*fs).image_size as u64 {
        return ptr::null();
    }
    (*fs).image.add(offset as usize)
}

unsafe fn read_block_mut(fs: *mut Ext2Instance, block_num: u32) -> *mut u8 {
    let offset = (block_num as u64) * (*fs).block_size as u64;
    if offset + (*fs).block_size as u64 > (*fs).image_size as u64 {
        return ptr::null_mut();
    }
    (*fs).image.add(offset as usize)
}

unsafe fn find_bg(fs: *const Ext2Instance, inode_num: u32, bg: *mut Ext2BlockGroupDescriptor) -> i32 {
    let bg_id = (inode_num - 1) / (*fs).inodes_per_group;
    let bgdt_block = (*fs).bgdt_block;
    let bg_per_block = (*fs).block_size as usize / BGDT_SIZE;
    let block_off = bg_id as usize / bg_per_block;
    let entry_off = bg_id as usize % bg_per_block;

    let data = read_block(fs, bgdt_block + block_off as u32);
    if data.is_null() {
        return -1;
    }

    ptr::copy_nonoverlapping(
        data.add(entry_off * BGDT_SIZE),
        bg as *mut u8,
        BGDT_SIZE,
    );
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_ext2_read_inode(
    fs: *const Ext2Instance,
    inode_num: u32,
    inode_buf: *mut u8,
) -> i32 {
    let mut bg: Ext2BlockGroupDescriptor = core::mem::zeroed();
    if find_bg(fs, inode_num, &mut bg) < 0 {
        return -1;
    }

    let inode_index = (inode_num - 1) % (*fs).inodes_per_group;
    let inodes_per_block = (*fs).block_size / (*fs).inode_size;
    let block_off = inode_index / inodes_per_block;
    let entry_off = (inode_index % inodes_per_block) * (*fs).inode_size;

    let data = read_block(fs, bg.inode_table + block_off);
    if data.is_null() {
        return -1;
    }

    ptr::copy_nonoverlapping(data.add(entry_off as usize), inode_buf, (*fs).inode_size as usize);
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_ext2_read_block_data(
    fs: *const Ext2Instance,
    inode_num: u32,
    block_index: u32,
    out: *mut u8,
) -> i32 {
    let mut inode_buf = [0u8; 256];
    if krust_ext2_read_inode(fs, inode_num, inode_buf.as_mut_ptr()) < 0 {
        return -1;
    }

    if block_index < 12 {
        let b = read_le32(inode_buf.as_ptr().add(40 + block_index as usize * 4));
        if b == 0 {
            return -1;
        }
        let data = read_block(fs, b);
        if data.is_null() {
            return -1;
        }
        krust_memcpy(out, data, (*fs).block_size as usize);
        return 0;
    }

    let mut bi = block_index - 12;
    let ptrs_per_block = (*fs).block_size / 4;

    // Single indirect (12)
    if bi < ptrs_per_block {
        let indirect_ptr = read_le32(inode_buf.as_ptr().add(40 + 12 * 4));
        if indirect_ptr == 0 {
            return -1;
        }
        let indirect = read_block(fs, indirect_ptr);
        if indirect.is_null() {
            return -1;
        }
        let b = read_le32(indirect.add(bi as usize * 4));
        if b == 0 {
            return -1;
        }
        let data = read_block(fs, b);
        if data.is_null() {
            return -1;
        }
        krust_memcpy(out, data, (*fs).block_size as usize);
        return 0;
    }

    bi -= ptrs_per_block;

    // Double indirect (13)
    if bi < ptrs_per_block * ptrs_per_block {
        let dindirect_ptr = read_le32(inode_buf.as_ptr().add(40 + 13 * 4));
        if dindirect_ptr == 0 {
            return -1;
        }
        let dindirect = read_block(fs, dindirect_ptr);
        if dindirect.is_null() {
            return -1;
        }
        let idx1 = bi / ptrs_per_block;
        let idx2 = bi % ptrs_per_block;
        let b1 = read_le32(dindirect.add(idx1 as usize * 4));
        if b1 == 0 {
            return -1;
        }
        let indirect = read_block(fs, b1);
        if indirect.is_null() {
            return -1;
        }
        let b = read_le32(indirect.add(idx2 as usize * 4));
        if b == 0 {
            return -1;
        }
        let data = read_block(fs, b);
        if data.is_null() {
            return -1;
        }
        krust_memcpy(out, data, (*fs).block_size as usize);
        return 0;
    }

    -1
}

unsafe fn resolve_block_num(fs: *const Ext2Instance, inode_buf: *const u8, block_index: u32) -> u32 {
    if block_index < 12 {
        return read_le32(inode_buf.add(40 + block_index as usize * 4));
    }

    let mut bi = block_index - 12;
    let ptrs_per_block = (*fs).block_size / 4;

    if bi < ptrs_per_block {
        let indirect_ptr = read_le32(inode_buf.add(40 + 12 * 4));
        if indirect_ptr == 0 {
            return 0;
        }
        let indirect = read_block(fs, indirect_ptr);
        if indirect.is_null() {
            return 0;
        }
        return read_le32(indirect.add(bi as usize * 4));
    }

    bi -= ptrs_per_block;

    if bi < ptrs_per_block * ptrs_per_block {
        let dindirect_ptr = read_le32(inode_buf.add(40 + 13 * 4));
        if dindirect_ptr == 0 {
            return 0;
        }
        let dindirect = read_block(fs, dindirect_ptr);
        if dindirect.is_null() {
            return 0;
        }
        let idx1 = bi / ptrs_per_block;
        let idx2 = bi % ptrs_per_block;
        let b1 = read_le32(dindirect.add(idx1 as usize * 4));
        if b1 == 0 {
            return 0;
        }
        let indirect = read_block(fs, b1);
        if indirect.is_null() {
            return 0;
        }
        return read_le32(indirect.add(idx2 as usize * 4));
    }

    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_ext2_read_file(
    fs: *const Ext2Instance,
    inode_num: u32,
    buf: *mut u8,
    max_size: u32,
) -> i32 {
    let mut inode_buf = [0u8; 256];
    if krust_ext2_read_inode(fs, inode_num, inode_buf.as_mut_ptr()) < 0 {
        return -1;
    }

    let mode = read_le16(inode_buf.as_ptr());
    if (mode & EXT2_S_IFMT) != EXT2_S_IFREG {
        return -1;
    }

    let file_size = read_le32(inode_buf.as_ptr().add(4));
    let copy_size = if file_size < max_size { file_size } else { max_size };
    if copy_size == 0 {
        return 0;
    }

    let block_size = (*fs).block_size;
    let num_blocks = (copy_size + block_size - 1) / block_size;
    let block_buf = krust_malloc(block_size);
    if block_buf.is_null() {
        return -1;
    }

    let mut remaining = copy_size;
    let mut offset = 0u32;

    for bi in 0..num_blocks {
        if krust_ext2_read_block_data(fs, inode_num, bi, block_buf) < 0 {
            break;
        }
        let chunk = if remaining < block_size { remaining } else { block_size };
        krust_memcpy(buf.add(offset as usize), block_buf, chunk as usize);
        offset += chunk;
        remaining -= chunk;
        if remaining == 0 {
            break;
        }
    }

    krust_free(block_buf);

    if offset < copy_size {
        -1
    } else {
        copy_size as i32
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ext2_find_in_dir(
    fs: *const Ext2Instance,
    dir_inode_num: u32,
    name: *const u8,
) -> u32 {
    let mut inode_buf = [0u8; 256];
    if krust_ext2_read_inode(fs, dir_inode_num, inode_buf.as_mut_ptr()) < 0 {
        return 0;
    }

    let mode = read_le16(inode_buf.as_ptr());
    if (mode & EXT2_S_IFMT) != EXT2_S_IFDIR {
        return 0;
    }

    let dir_size = read_le32(inode_buf.as_ptr().add(4));
    let name_len = krust_strlen(name) as u32;
    let block_size = (*fs).block_size;
    let num_blocks = (dir_size + block_size - 1) / block_size;

    let block_buf = krust_malloc(block_size);
    if block_buf.is_null() {
        return 0;
    }

    let mut result = 0u32;

    for bi in 0..num_blocks {
        if krust_ext2_read_block_data(fs, dir_inode_num, bi, block_buf) < 0 {
            continue;
        }

        let mut offset = 0u32;
        while offset < block_size {
            let de_ino = read_le32(block_buf.add(offset as usize));
            let de_rec_len = read_le16(block_buf.add(offset as usize + 4));
            let de_name_len = ptr::read_volatile(block_buf.add(offset as usize + 6));

            if de_rec_len < 8 {
                break;
            }

            if de_ino != 0 && de_name_len as u32 == name_len {
                let mut match_found = true;
                for ci in 0..name_len as usize {
                    if ptr::read_volatile(block_buf.add(offset as usize + 8 + ci))
                        != ptr::read_volatile(name.add(ci))
                    {
                        match_found = false;
                        break;
                    }
                }
                if match_found {
                    result = de_ino;
                    break;
                }
            }

            if de_rec_len == 0 {
                break;
            }
            offset += de_rec_len as u32;
        }

        if result != 0 {
            break;
        }
    }

    krust_free(block_buf);
    result
}

#[no_mangle]
pub unsafe extern "C" fn krust_ext2_resolve_path(
    fs: *const Ext2Instance,
    path: *const u8,
    parent_inode_num: *mut u32,
    name_buf: *mut u8,
) -> i32 {
    if path.is_null() || parent_inode_num.is_null() || name_buf.is_null() {
        return -1;
    }

    let mut p = path;
    while ptr::read_volatile(p) == b'/' {
        p = p.add(1);
    }
    if ptr::read_volatile(p) == 0 {
        return -1;
    }

    let mut current_inode_num: u32 = 2;
    let mut inode_buf = [0u8; 256];

    if krust_ext2_read_inode(fs, current_inode_num, inode_buf.as_mut_ptr()) < 0 {
        return -1;
    }
    if (read_le16(inode_buf.as_ptr()) & EXT2_S_IFMT) != EXT2_S_IFDIR {
        return -1;
    }

    let block_size = (*fs).block_size;
    let block_buf = krust_malloc(block_size);
    if block_buf.is_null() {
        return -1;
    }

    let mut component = [0u8; 256];

    loop {
        let mut comp_len: usize = 0;
        while ptr::read_volatile(p) != 0 && ptr::read_volatile(p) != b'/' {
            if comp_len < 255 {
                component[comp_len] = ptr::read_volatile(p);
                comp_len += 1;
            }
            p = p.add(1);
        }
        component[comp_len] = 0;

        while ptr::read_volatile(p) == b'/' {
            p = p.add(1);
        }

        if ptr::read_volatile(p) == 0 {
            krust_strncpy(name_buf, component.as_ptr(), 255);
            *parent_inode_num = current_inode_num;
            krust_free(block_buf);
            return 0;
        }

        // Read current directory to find component
        if krust_ext2_read_inode(fs, current_inode_num, inode_buf.as_mut_ptr()) < 0 {
            krust_free(block_buf);
            return -1;
        }

        let dir_size = read_le32(inode_buf.as_ptr().add(4));
        let num_blocks = (dir_size + block_size - 1) / block_size;
        let mut found = false;

        for bi in 0..num_blocks {
            if krust_ext2_read_block_data(fs, current_inode_num, bi, block_buf) < 0 {
                continue;
            }

            let mut offset = 0u32;
            while offset < block_size {
                let de_ino = read_le32(block_buf.add(offset as usize));
                let de_rec_len = read_le16(block_buf.add(offset as usize + 4));
                let de_name_len = ptr::read_volatile(block_buf.add(offset as usize + 6));

                if de_rec_len < 8 {
                    break;
                }

                if de_ino != 0 && de_name_len as usize == comp_len {
                    let mut match_found = true;
                    for ci in 0..comp_len {
                        if ptr::read_volatile(block_buf.add(offset as usize + 8 + ci))
                            != component[ci]
                        {
                            match_found = false;
                            break;
                        }
                    }
                    if match_found {
                        current_inode_num = de_ino;
                        found = true;
                        break;
                    }
                }

                if de_rec_len == 0 {
                    break;
                }
                offset += de_rec_len as u32;
            }

            if found {
                break;
            }
        }

        if !found {
            krust_free(block_buf);
            return -1;
        }

        if krust_ext2_read_inode(fs, current_inode_num, inode_buf.as_mut_ptr()) < 0 {
            krust_free(block_buf);
            return -1;
        }
        if (read_le16(inode_buf.as_ptr()) & EXT2_S_IFMT) != EXT2_S_IFDIR {
            krust_free(block_buf);
            return -1;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_ext2_list_dir(
    fs: *const Ext2Instance,
    dir_inode_num: u32,
    callback: Option<unsafe extern "C" fn(u32, *const u8, u8)>,
) -> i32 {
    let mut inode_buf = [0u8; 256];
    if krust_ext2_read_inode(fs, dir_inode_num, inode_buf.as_mut_ptr()) < 0 {
        return -1;
    }

    let mode = read_le16(inode_buf.as_ptr());
    if (mode & EXT2_S_IFMT) != EXT2_S_IFDIR {
        return -1;
    }

    let dir_size = read_le32(inode_buf.as_ptr().add(4));
    let block_size = (*fs).block_size;
    let num_blocks = (dir_size + block_size - 1) / block_size;

    let block_buf = krust_malloc(block_size);
    if block_buf.is_null() {
        return -1;
    }

    for bi in 0..num_blocks {
        if krust_ext2_read_block_data(fs, dir_inode_num, bi, block_buf) < 0 {
            continue;
        }

        let mut offset = 0u32;
        while offset < block_size {
            let de_ino = read_le32(block_buf.add(offset as usize));
            let de_rec_len = read_le16(block_buf.add(offset as usize + 4));
            let de_name_len = ptr::read_volatile(block_buf.add(offset as usize + 6));
            let de_file_type = ptr::read_volatile(block_buf.add(offset as usize + 7));

            if de_rec_len < 8 {
                break;
            }

            if de_ino != 0 && de_name_len > 0 {
                if let Some(cb) = callback {
                    cb(
                        de_ino,
                        block_buf.add(offset as usize + 8),
                        de_file_type,
                    );
                }
            }

            if de_rec_len == 0 {
                break;
            }
            offset += de_rec_len as u32;
        }
    }

    krust_free(block_buf);
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_ext2_get_inode_size(
    fs: *const Ext2Instance,
    inode_num: u32,
) -> i32 {
    let mut inode_buf = [0u8; 256];
    if krust_ext2_read_inode(fs, inode_num, inode_buf.as_mut_ptr()) < 0 {
        return -1;
    }
    read_le32(inode_buf.as_ptr().add(4)) as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_ext2_get_inode_mode(
    fs: *const Ext2Instance,
    inode_num: u32,
) -> i32 {
    let mut inode_buf = [0u8; 256];
    if krust_ext2_read_inode(fs, inode_num, inode_buf.as_mut_ptr()) < 0 {
        return -1;
    }
    read_le16(inode_buf.as_ptr()) as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_ext2_init(
    instance: *mut Ext2Instance,
    data: *const u8,
    size: u32,
) -> i32 {
    if instance.is_null() || data.is_null() {
        return -1;
    }

    ptr::write_volatile(instance, core::mem::zeroed::<Ext2Instance>());

    if size < 2048 {
        return -1;
    }

    let sb_ptr = data.add(1024);

    let magic = read_le16(sb_ptr.add(56));
    if magic != EXT2_MAGIC {
        return -1;
    }

    let log_block_size = read_le32(sb_ptr.add(24));
    let block_size = 1024u32 << log_block_size;
    let blocks_per_group = read_le32(sb_ptr.add(40));
    let inodes_per_group = read_le32(sb_ptr.add(42));
    let first_data_block = read_le32(sb_ptr.add(20));
    let rev_level = read_le32(sb_ptr.add(76));

    let inode_size;
    if rev_level >= 1 {
        inode_size = read_le16(sb_ptr.add(128)) as u32;
    } else {
        inode_size = 128;
    }

    let inode_table_blocks = (inodes_per_group * inode_size + block_size - 1) / block_size;

    let bgdt_block;
    if block_size == 1024 {
        bgdt_block = 2;
    } else {
        bgdt_block = 1;
    }

    (*instance).image = data as *mut u8;
    (*instance).image_size = size;
    (*instance).block_size = block_size;
    (*instance).inodes_per_group = inodes_per_group;
    (*instance).blocks_per_group = blocks_per_group;
    (*instance).inode_size = inode_size;
    (*instance).inode_table_blocks = inode_table_blocks;
    (*instance).first_data_block = first_data_block;
    (*instance).bgdt_block = bgdt_block;
    (*instance).mounted = false;

    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_ext2_mount(
    instance: *mut Ext2Instance,
    mount_point: *const u8,
) -> i32 {
    if instance.is_null() {
        return -1;
    }

    if (*instance).image.is_null() {
        return -1;
    }

    krust_strncpy((*instance).mount_point.as_mut_ptr(), mount_point, 127);
    (*instance).mount_point[127] = 0;
    (*instance).mounted = true;

    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_ext2_read_block(
    instance: *const Ext2Instance,
    block_num: u32,
    buf: *mut u8,
) -> i32 {
    if instance.is_null() || buf.is_null() {
        return -1;
    }

    let data = read_block(instance, block_num);
    if data.is_null() {
        return -1;
    }

    krust_memcpy(buf, data, (*instance).block_size as usize);
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_ext2_get_root_inode(
    instance: *const Ext2Instance,
) -> u32 {
    if instance.is_null() {
        return 0;
    }
    2
}

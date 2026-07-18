use core::ptr;

unsafe fn copy_volatile(dst: *mut u8, src: *const u8, n: usize) {
    for i in 0..n {
        ptr::write_volatile(dst.add(i), ptr::read_volatile(src.add(i)));
    }
}



const ATTR_DIRECTORY: u8 = 0x10;
const ATTR_ARCHIVE: u8 = 0x20;
const ATTR_LFN: u8 = 0x0F;

const FAT32_EOC: u32 = 0x0FFFFFF8;
const FAT32_FREE: u32 = 0x00000000;
const FAT32_BAD: u32 = 0x0FFFFFF7;

const DIR_NAME: usize = 0;
const DIR_ATTR: usize = 11;
const DIR_CLUSTER_HI: usize = 20;
const DIR_CLUSTER_LO: usize = 26;
const DIR_SIZE: usize = 28;
const DIR_ENTRY_SIZE: usize = 32;

unsafe fn dir_set_attr(entry: *mut u8, attr: u8) { ptr::write_volatile(entry.add(DIR_ATTR), attr); }

unsafe fn dir_set_cluster(entry: *mut u8, cluster: u32) {
    write_le16(entry.add(DIR_CLUSTER_HI), (cluster >> 16) as u16);
    write_le16(entry.add(DIR_CLUSTER_LO), cluster as u16);
}

unsafe fn dir_set_size(entry: *mut u8, size: u32) { write_le32(entry.add(DIR_SIZE), size); }

unsafe fn dir_get_cluster(entry: *const u8) -> u32 {
    (read_le16(entry.add(DIR_CLUSTER_HI)) as u32) << 16 | read_le16(entry.add(DIR_CLUSTER_LO)) as u32
}

#[repr(C)]
pub struct Instance {
    pub image: *mut u8,
    pub image_size: usize,
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub num_fats: u8,
    pub sectors_per_fat: u32,
    pub root_cluster: u32,
    pub first_data_sector: u32,
    pub first_fat_sector: u32,
    pub total_clusters: u32,
    pub write_callback: Option<unsafe extern "C" fn(*mut Instance, u32, u32)>,
}

pub type WriteCallback = Option<unsafe extern "C" fn(*mut Instance, u32, u32)>;

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

unsafe fn sector_ptr(fs: *const Instance, sector: u32) -> *mut u8 {
    let bps = (*fs).bytes_per_sector as usize;
    (*fs).image.add(sector as usize * bps)
}

unsafe fn cluster_to_sector(fs: *const Instance, cluster: u32) -> u32 {
    (*fs).first_data_sector + (cluster - 2) * ((*fs).sectors_per_cluster as u32)
}

unsafe fn mark_dirty(fs: *mut Instance, sector: u32) {
    if let Some(cb) = (*fs).write_callback {
        cb(fs, sector * (*fs).bytes_per_sector as u32, (*fs).bytes_per_sector as u32);
    }
}

// --- Init ---

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_init(fs: *mut Instance, image: *const u8, image_size: usize) -> u8 {
    let bps = read_le16(image.add(11));
    let spc = ptr::read_volatile(image.add(13));
    let reserved = read_le16(image.add(14));
    let num_fats = ptr::read_volatile(image.add(16));
    let spf = read_le32(image.add(36));
    let root_cluster = read_le32(image.add(44));
    let s16 = read_le16(image.add(19));
    let s32 = read_le32(image.add(32));
    let total_sectors = if s16 != 0 { s16 as u32 } else { s32 };

    // Validate image_size is large enough for FAT and data area
    let first_fat = reserved as u32;
    let fat_area = num_fats as u32 * spf;
    let first_data = first_fat + fat_area;
    let min_sectors = first_data + 1; // at least one data sector
    if (min_sectors as usize) * bps as usize > image_size {
        return 0;
    }
    let total_clusters = if total_sectors > first_data {
        (total_sectors - first_data) / spc as u32
    } else {
        0
    };

    (*fs).image = image as *mut u8;
    (*fs).bytes_per_sector = bps;
    (*fs).sectors_per_cluster = spc;
    (*fs).reserved_sectors = reserved;
    (*fs).num_fats = num_fats;
    (*fs).sectors_per_fat = spf;
    (*fs).root_cluster = root_cluster;
    (*fs).first_data_sector = first_data;
    (*fs).first_fat_sector = first_fat;
    (*fs).total_clusters = total_clusters;
    1
}

// --- FAT operations ---

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_get_fat_entry(fs: *mut Instance, cluster: u32) -> u32 {
    let off = cluster * 4;
    let sector = (*fs).first_fat_sector + off / ((*fs).bytes_per_sector as u32);
    let ins = (off % (*fs).bytes_per_sector as u32) as usize;
    read_le32(sector_ptr(fs, sector).add(ins)) & 0x0FFFFFFF
}

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_set_fat_entry(fs: *mut Instance, cluster: u32, value: u32) -> u8 {
    let off = cluster * 4;
    let sector = (*fs).first_fat_sector + off / ((*fs).bytes_per_sector as u32);
    let ins = (off % (*fs).bytes_per_sector as u32) as usize;
    let p = sector_ptr(fs, sector).add(ins);
    let old = read_le32(p) & 0xF0000000;
    write_le32(p, old | (value & 0x0FFFFFFF));
    mark_dirty(fs, sector);
    1
}

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_alloc_cluster(fs: *mut Instance) -> u32 {
    for c in 2..(*fs).total_clusters {
        if krust_fat32_get_fat_entry(fs, c) == FAT32_FREE {
            krust_fat32_set_fat_entry(fs, c, FAT32_EOC);
            return c;
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_free_cluster(fs: *mut Instance, cluster: u32) -> u8 {
    krust_fat32_set_fat_entry(fs, cluster, FAT32_FREE)
}

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_free_chain(fs: *mut Instance, start: u32) -> u8 {
    let mut c = start;
    loop {
        let n = krust_fat32_get_fat_entry(fs, c);
        krust_fat32_set_fat_entry(fs, c, FAT32_FREE);
        if n >= FAT32_EOC || n < 2 { break; }
        c = n;
    }
    1
}

// --- Name matching ---

unsafe fn name_matches(entry: *const u8, name: *const u8) -> bool {
    let mut sfn = [0u8; 11];
    let mut si = 0u8;
    let mut ei = 0usize;

    loop {
        let c = ptr::read_volatile(name.add(si as usize));
        if c == 0 { break; }
        if c == b'.' { si += 1; break; }
        sfn[si as usize] = if c >= b'a' && c <= b'z' { c - 32 } else { c };
        si += 1;
    }
    while si < 8 { sfn[si as usize] = b' '; si += 1; }

    loop {
        let c = ptr::read_volatile(name.add(si as usize));
        if c == 0 { break; }
        sfn[8 + ei] = if c >= b'a' && c <= b'z' { c - 32 } else { c };
        ei += 1;
        si += 1;
    }
    while ei < 3 { sfn[8 + ei] = b' '; ei += 1; }

    for i in 0..11 {
        if ptr::read_volatile(entry.add(i)) != sfn[i] { return false; }
    }
    true
}

// --- Find directory entry ---

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_find_dir_entry(
    fs: *mut Instance, dir_cluster: u32, name: *const u8, out_entry: *mut u8,
) -> i32 {
    let bps = (*fs).bytes_per_sector as usize;
    let spc = (*fs).sectors_per_cluster as u32;
    let mut cluster = dir_cluster;
    loop {
        let base = cluster_to_sector(fs, cluster);
        for so in 0..spc {
            let sector = sector_ptr(fs, base + so);
            for eo in 0..(bps / DIR_ENTRY_SIZE) {
                let ep = sector.add(eo * DIR_ENTRY_SIZE);
                let fb = ptr::read_volatile(ep);
                if fb == 0 { return -1; }
                if fb == 0xE5 { continue; }
                if ptr::read_volatile(ep.add(DIR_ATTR)) == ATTR_LFN { continue; }
                if name_matches(ep, name) {
                    if !out_entry.is_null() { copy_volatile(out_entry, ep, DIR_ENTRY_SIZE); }
                    return 1;
                }
            }
        }
        let n = krust_fat32_get_fat_entry(fs, cluster);
        if n >= FAT32_EOC || n < 2 { break; }
        cluster = n;
    }
    -1
}

// --- Add directory entry ---

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_add_dir_entry(
    fs: *mut Instance, dir_cluster: u32, entry: *const u8,
) -> u8 {
    let bps = (*fs).bytes_per_sector as usize;
    let spc = (*fs).sectors_per_cluster as u32;
    let mut cluster = dir_cluster;
    loop {
        let base = cluster_to_sector(fs, cluster);
        for so in 0..spc {
            let sector = sector_ptr(fs, base + so);
            for eo in 0..(bps / DIR_ENTRY_SIZE) {
                let ep = sector.add(eo * DIR_ENTRY_SIZE);
                let fb = ptr::read_volatile(ep);
                if fb == 0 || fb == 0xE5 {
                    copy_volatile(ep, entry, DIR_ENTRY_SIZE);
                    mark_dirty(fs, base + so);
                    return 1;
                }
            }
        }
        let n = krust_fat32_get_fat_entry(fs, cluster);
        if n >= FAT32_EOC || n < 2 { break; }
        cluster = n;
    }
    let nc = krust_fat32_alloc_cluster(fs);
    if nc == 0 { return 0; }
    krust_fat32_set_fat_entry(fs, cluster, nc);
    let bps32 = (*fs).bytes_per_sector as u32;
    let ns = cluster_to_sector(fs, nc);
    let dst = sector_ptr(fs, ns);
    let csize = (bps32 * (*fs).sectors_per_cluster as u32) as usize;
    for i in 0..csize { ptr::write_volatile(dst.add(i), 0u8); }
    copy_volatile(dst, entry, DIR_ENTRY_SIZE);
    mark_dirty(fs, ns);
    1
}

// --- Remove directory entry ---

unsafe fn remove_entry(fs: *mut Instance, dir_cluster: u32, name: *const u8) -> i32 {
    let bps = (*fs).bytes_per_sector as usize;
    let spc = (*fs).sectors_per_cluster as u32;
    let mut cluster = dir_cluster;
    loop {
        let base = cluster_to_sector(fs, cluster);
        for so in 0..spc {
            let sector = sector_ptr(fs, base + so);
            for eo in 0..(bps / DIR_ENTRY_SIZE) {
                let ep = sector.add(eo * DIR_ENTRY_SIZE);
                let fb = ptr::read_volatile(ep);
                if fb == 0 { return -1; }
                if fb == 0xE5 { continue; }
                if ptr::read_volatile(ep.add(DIR_ATTR)) == ATTR_LFN { continue; }
                if name_matches(ep, name) {
                    ptr::write_volatile(ep, 0xE5u8);
                    mark_dirty(fs, base + so);
                    return 0;
                }
            }
        }
        let n = krust_fat32_get_fat_entry(fs, cluster);
        if n >= FAT32_EOC || n < 2 { break; }
        cluster = n;
    }
    -1
}

// --- Name to SFN ---

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_name_to_sfn(name: *const u8, sfn: *mut u8) {
    let mut ni = 0usize;
    let mut ei = 0usize;
    let mut si = 0usize;
    let mut dot = false;
    loop {
        let c = ptr::read_volatile(name.add(si));
        if c == 0 { break; }
        if c == b'.' && !dot { dot = true; si += 1; continue; }
        let u = if c >= b'a' && c <= b'z' { c - 32 } else { c };
        if !dot { if ni < 8 { ptr::write_volatile(sfn.add(ni), u); ni += 1; } }
        else { if ei < 3 { ptr::write_volatile(sfn.add(8 + ei), u); ei += 1; } }
        si += 1;
    }
    while ni < 8 { ptr::write_volatile(sfn.add(ni), b' '); ni += 1; }
    while ei < 3 { ptr::write_volatile(sfn.add(8 + ei), b' '); ei += 1; }
}

// --- Write file ---

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_write_file(
    fs: *mut Instance, dir_cluster: u32, name: *const u8, data: *const u8, size: u32,
) -> i32 {
    let bps32 = (*fs).bytes_per_sector as u32;
    let spc32 = (*fs).sectors_per_cluster as u32;
    let bpc = bps32 * spc32;

    let mut tmp = [0u8; DIR_ENTRY_SIZE];
    if krust_fat32_find_dir_entry(fs, dir_cluster, name, tmp.as_mut_ptr()) > 0 {
        let old = dir_get_cluster(tmp.as_ptr());
        if old >= 2 && old < FAT32_EOC { krust_fat32_free_chain(fs, old); }
    }

    if size == 0 {
        let mut entry = [0u8; DIR_ENTRY_SIZE];
        krust_fat32_name_to_sfn(name, entry.as_mut_ptr());
        dir_set_attr(entry.as_mut_ptr(), ATTR_ARCHIVE);
        return if krust_fat32_add_dir_entry(fs, dir_cluster, entry.as_ptr()) != 0 { 0 } else { -1 };
    }

    let num = (size + bpc - 1) / bpc;
    if num == 0 { return -1; }

    let mut first = 0u32;
    let mut prev = 0u32;
    for i in 0..num {
        let c = krust_fat32_alloc_cluster(fs);
        if c == 0 {
            if first != 0 { krust_fat32_free_chain(fs, first); }
            return -1;
        }
        if i == 0 { first = c; }
        if prev != 0 { krust_fat32_set_fat_entry(fs, prev, c); }
        prev = c;
    }

    let mut rem = size as usize;
    let mut c = first;
    let mut off = 0usize;
    while rem > 0 && c >= 2 && c < FAT32_EOC {
        let sn = cluster_to_sector(fs, c);
        let dst = sector_ptr(fs, sn);
        let chunk = if rem > bpc as usize { bpc as usize } else { rem };
        copy_volatile(dst, data.add(off), chunk);
        for s in 0..spc32 { mark_dirty(fs, sn + s); }
        off += chunk;
        rem -= chunk;
        if rem == 0 { break; }
        c = krust_fat32_get_fat_entry(fs, c);
    }

    let mut entry = [0u8; DIR_ENTRY_SIZE];
    krust_fat32_name_to_sfn(name, entry.as_mut_ptr());
    dir_set_attr(entry.as_mut_ptr(), ATTR_ARCHIVE);
    dir_set_cluster(entry.as_mut_ptr(), first);
    dir_set_size(entry.as_mut_ptr(), size);
    krust_fat32_add_dir_entry(fs, dir_cluster, entry.as_ptr());
    0
}

// --- Create dir ---

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_create_dir(
    fs: *mut Instance, dir_cluster: u32, name: *const u8,
) -> i32 {
    let bps32 = (*fs).bytes_per_sector as u32;
    let spc32 = (*fs).sectors_per_cluster as u32;
    let csize = (bps32 * spc32) as usize;

    let nc = krust_fat32_alloc_cluster(fs);
    if nc == 0 { return -1; }

    let sn = cluster_to_sector(fs, nc);
    let dst = sector_ptr(fs, sn);
    for i in 0..csize { ptr::write_volatile(dst.add(i), 0u8); }

    // "." entry
    let mut dot = [0u8; DIR_ENTRY_SIZE];
    ptr::write_volatile(dot.as_mut_ptr().add(0), b'.');
    for i in 1..11 { ptr::write_volatile(dot.as_mut_ptr().add(i), b' '); }
    dir_set_attr(dot.as_mut_ptr(), ATTR_DIRECTORY);
    dir_set_cluster(dot.as_mut_ptr(), nc);
    copy_volatile(dst, dot.as_ptr(), DIR_ENTRY_SIZE);

    // ".." entry
    let mut dotdot = [0u8; DIR_ENTRY_SIZE];
    ptr::write_volatile(dotdot.as_mut_ptr().add(0), b'.');
    ptr::write_volatile(dotdot.as_mut_ptr().add(1), b'.');
    for i in 2..11 { ptr::write_volatile(dotdot.as_mut_ptr().add(i), b' '); }
    dir_set_attr(dotdot.as_mut_ptr(), ATTR_DIRECTORY);
    let parent_cluster = if dir_cluster == (*fs).root_cluster { 0 } else { dir_cluster };
    dir_set_cluster(dotdot.as_mut_ptr(), parent_cluster);
    copy_volatile(dst.add(DIR_ENTRY_SIZE), dotdot.as_ptr(), DIR_ENTRY_SIZE);

    mark_dirty(fs, sn);

    // Parent entry
    let mut entry = [0u8; DIR_ENTRY_SIZE];
    krust_fat32_name_to_sfn(name, entry.as_mut_ptr());
    dir_set_attr(entry.as_mut_ptr(), ATTR_DIRECTORY);
    dir_set_cluster(entry.as_mut_ptr(), nc);
    krust_fat32_add_dir_entry(fs, dir_cluster, entry.as_ptr());
    0
}

// --- Delete file ---

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_delete_file(
    fs: *mut Instance, dir_cluster: u32, name: *const u8,
) -> i32 {
    let mut tmp = [0u8; DIR_ENTRY_SIZE];
    if krust_fat32_find_dir_entry(fs, dir_cluster, name, tmp.as_mut_ptr()) <= 0 { return -1; }
    let c = dir_get_cluster(tmp.as_ptr());
    if c >= 2 && c < FAT32_EOC { krust_fat32_free_chain(fs, c); }
    remove_entry(fs, dir_cluster, name)
}

// --- Delete dir ---

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_delete_dir(
    fs: *mut Instance, dir_cluster: u32, name: *const u8,
) -> i32 {
    let mut tmp = [0u8; DIR_ENTRY_SIZE];
    if krust_fat32_find_dir_entry(fs, dir_cluster, name, tmp.as_mut_ptr()) <= 0 { return -1; }
    let c = dir_get_cluster(tmp.as_ptr());
    if c >= 2 && c < FAT32_EOC { krust_fat32_free_chain(fs, c); }
    remove_entry(fs, dir_cluster, name)
}

// --- Rename ---

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_rename_entry(
    fs: *mut Instance, old_dir: u32, old_name: *const u8, new_dir: u32, new_name: *const u8,
) -> i32 {
    let mut entry = [0u8; DIR_ENTRY_SIZE];
    if krust_fat32_find_dir_entry(fs, old_dir, old_name, entry.as_mut_ptr()) <= 0 { return -1; }
    krust_fat32_name_to_sfn(new_name, entry.as_mut_ptr());
    if krust_fat32_add_dir_entry(fs, new_dir, entry.as_ptr()) == 0 { return -1; }
    remove_entry(fs, old_dir, old_name)
}

#[no_mangle]
pub unsafe extern "C" fn krust_fat32_mount(
    fs: *mut Instance, mount_point: *const u8,
) -> u8 {
    if fs.is_null() || mount_point.is_null() { return 0; }
    if crate::mount::krust_mount_mount(mount_point, 1, fs as *mut u8) == 0 { 1 } else { 0 }
}

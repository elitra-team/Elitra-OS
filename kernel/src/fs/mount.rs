use core::ptr;
use crate::klib::{krust_strlen, krust_strncpy, krust_strcmp, krust_strncmp, krust_memset};

const MAX_MOUNTS: usize = 16;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct MountInfo {
    pub used: bool,
    pub mount_point: [u8; 128],
    pub type_: u32,
    pub instance: *mut u8,
}

static mut MOUNTS: [MountInfo; MAX_MOUNTS] = [MountInfo {
    used: false,
    mount_point: [0u8; 128],
    type_: 0,
    instance: ptr::null_mut(),
}; MAX_MOUNTS];

#[no_mangle]
pub unsafe extern "C" fn krust_mount_init() {
    krust_memset(
        MOUNTS.as_mut_ptr() as *mut u8,
        0,
        core::mem::size_of::<MountInfo>() * MAX_MOUNTS,
    );
}

#[no_mangle]
pub unsafe extern "C" fn krust_mount_mount(
    mount_point: *const u8,
    type_: u32,
    instance: *mut u8,
) -> i32 {
    if mount_point.is_null() || *mount_point == 0 {
        return -1;
    }
    for i in 0..MAX_MOUNTS {
        if !MOUNTS[i].used {
            krust_strncpy(MOUNTS[i].mount_point.as_mut_ptr(), mount_point, 127);
            MOUNTS[i].type_ = type_;
            MOUNTS[i].instance = instance;
            MOUNTS[i].used = true;
            return 0;
        }
    }
    -1
}

#[no_mangle]
pub unsafe extern "C" fn krust_mount_umount(mount_point: *const u8) -> i32 {
    if mount_point.is_null() || *mount_point == 0 {
        return -1;
    }
    for i in 0..MAX_MOUNTS {
        if MOUNTS[i].used && krust_strcmp(MOUNTS[i].mount_point.as_ptr(), mount_point) == 0 {
            MOUNTS[i].used = false;
            MOUNTS[i].instance = ptr::null_mut();
            MOUNTS[i].type_ = 0;
            return 0;
        }
    }
    -1
}

#[no_mangle]
pub unsafe extern "C" fn krust_mount_find(mount_point: *const u8) -> *mut MountInfo {
    if mount_point.is_null() {
        return ptr::null_mut();
    }
    let mut best: *mut MountInfo = ptr::null_mut();
    let mut best_len: usize = 0;
    for i in 0..MAX_MOUNTS {
        if !MOUNTS[i].used {
            continue;
        }
        let len = krust_strlen(MOUNTS[i].mount_point.as_ptr());
        if krust_strncmp(MOUNTS[i].mount_point.as_ptr(), mount_point, len) == 0 {
            if *mount_point.add(len) == 0 || *mount_point.add(len) == b'/' {
                if len > best_len {
                    best_len = len;
                    best = &mut MOUNTS[i];
                }
            }
        }
    }
    best
}

#[no_mangle]
pub unsafe extern "C" fn krust_mount_count() -> i32 {
    let mut n: i32 = 0;
    for i in 0..MAX_MOUNTS {
        if MOUNTS[i].used {
            n += 1;
        }
    }
    n
}

#[no_mangle]
pub unsafe extern "C" fn krust_mount_get(index: i32) -> *mut MountInfo {
    if index < 0 || index >= MAX_MOUNTS as i32 {
        return ptr::null_mut();
    }
    &mut MOUNTS[index as usize]
}

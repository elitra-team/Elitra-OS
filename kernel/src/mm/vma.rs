use core::ptr;
use crate::scheduler::VMA;

extern "C" {
    fn krust_malloc(size: u32) -> *mut u8;
    fn krust_free(ptr: *mut u8);
}

#[no_mangle]
pub unsafe extern "C" fn krust_vmm_find(head: *mut VMA, addr: u64) -> *mut VMA {
    let mut v = head;
    while !v.is_null() {
        if addr >= (*v).start && addr < (*v).end {
            return v;
        }
        v = (*v).next;
    }
    ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn krust_vmm_add(
    head: *mut *mut VMA,
    start: u64,
    end: u64,
    flags: u64,
) -> *mut VMA {
    if start >= end {
        return ptr::null_mut();
    }

    let v = krust_malloc(core::mem::size_of::<VMA>() as u32) as *mut VMA;
    if v.is_null() {
        return ptr::null_mut();
    }
    (*v).start = start;
    (*v).end = end;
    (*v).flags = flags;
    (*v).next = ptr::null_mut();

    if (*head).is_null() {
        *head = v;
        return v;
    }

    let mut prev: *mut VMA = ptr::null_mut();
    let mut curr = *head;
    while !curr.is_null() && (*curr).start < start {
        prev = curr;
        curr = (*curr).next;
    }

    if !prev.is_null() {
        (*prev).next = v;
        (*v).next = curr;
    } else {
        (*v).next = *head;
        *head = v;
    }

    v
}

#[no_mangle]
pub unsafe extern "C" fn krust_vmm_remove(
    head: *mut *mut VMA,
    start: u64,
    end: u64,
) -> i32 {
    let mut count = 0i32;
    let mut prev: *mut VMA = ptr::null_mut();
    let mut curr = *head;
    while !curr.is_null() {
        if (*curr).start < end && (*curr).end > start {
            let next = (*curr).next;
            if (*curr).start >= start && (*curr).end <= end {
                if !prev.is_null() {
                    (*prev).next = next;
                } else {
                    *head = next;
                }
                krust_free(curr as *mut u8);
                curr = next;
                count += 1;
                continue;
            }
            if (*curr).start < start && (*curr).end > end {
                let new_v = krust_malloc(core::mem::size_of::<VMA>() as u32) as *mut VMA;
                if !new_v.is_null() {
                    (*new_v).start = end;
                    (*new_v).end = (*curr).end;
                    (*new_v).flags = (*curr).flags;
                    (*new_v).next = (*curr).next;
                    (*curr).end = start;
                    (*curr).next = new_v;
                }
                count += 1;
                break;
            }
            if (*curr).start < start && (*curr).end > start {
                (*curr).end = start;
                count += 1;
            } else if (*curr).start < end && (*curr).end > end {
                (*curr).start = end;
                count += 1;
            }
            prev = curr;
            curr = (*curr).next;
        } else {
            prev = curr;
            curr = (*curr).next;
        }
    }
    count
}

#[no_mangle]
pub unsafe extern "C" fn krust_vmm_free_all(head: *mut *mut VMA) {
    let mut curr = *head;
    while !curr.is_null() {
        let next = (*curr).next;
        krust_free(curr as *mut u8);
        curr = next;
    }
    *head = ptr::null_mut();
}

#[no_mangle]
pub unsafe extern "C" fn krust_vmm_has_overlap(
    head: *mut VMA,
    start: u64,
    end: u64,
) -> i32 {
    if start >= end {
        return 1;
    }
    let mut v = head;
    while !v.is_null() {
        if (*v).start < end && (*v).end > start {
            return 1;
        }
        v = (*v).next;
    }
    0
}

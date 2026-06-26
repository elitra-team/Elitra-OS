use core::ptr;

const PAGE_SIZE: usize = 4096;
const FRAMES_MAX: usize = 32768;

static mut BITMAP: *mut u8 = ptr::null_mut();
static mut BITMAP_SIZE: usize = 0;
static mut TOTAL_FRAMES: usize = 0;
static mut FIRST_FREE: usize = 0;

unsafe fn bitmap_test(bit: usize) -> bool {
    (ptr::read_volatile(BITMAP.add(bit / 8)) & (1 << (bit % 8))) != 0
}

unsafe fn bitmap_set(bit: usize) {
    let addr = BITMAP.add(bit / 8);
    ptr::write_volatile(addr, ptr::read_volatile(addr) | (1 << (bit % 8)));
}

unsafe fn bitmap_clear(bit: usize) {
    let addr = BITMAP.add(bit / 8);
    ptr::write_volatile(addr, ptr::read_volatile(addr) & !(1 << (bit % 8)));
}

unsafe fn find_first_free() -> usize {
    for i in FIRST_FREE..TOTAL_FRAMES {
        if !bitmap_test(i) { return i; }
    }
    !0
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_init(bitmap: *mut u8, bitmap_size: usize, total_frames: usize) {
    BITMAP = bitmap;
    BITMAP_SIZE = bitmap_size;
    TOTAL_FRAMES = total_frames;
    FIRST_FREE = 0;
    for i in 0..bitmap_size {
        ptr::write_volatile(BITMAP.add(i), 0xFF);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_mark_used(bitmap: *mut u8, start: usize, end: usize) {
    BITMAP = bitmap;
    for i in start..end {
        bitmap_set(i);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_free_frames(bitmap: *mut u8, start_frame: usize, count: usize) {
    BITMAP = bitmap;
    for i in start_frame..(start_frame + count) {
        bitmap_clear(i);
    }
    if start_frame < FIRST_FREE { FIRST_FREE = start_frame; }
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_alloc_frame() -> usize {
    let frame = find_first_free();
    if frame == !0 { return !0; }
    bitmap_set(frame);
    FIRST_FREE = frame + 1;
    frame
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_free_frame(frame: usize) {
    bitmap_clear(frame);
    if frame < FIRST_FREE { FIRST_FREE = frame; }
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_get_bitmap() -> *mut u8 { BITMAP }

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_get_total_frames() -> usize { TOTAL_FRAMES }

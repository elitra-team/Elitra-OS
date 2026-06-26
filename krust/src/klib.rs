use core::ptr;

#[no_mangle]
pub unsafe extern "C" fn krust_memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    for i in 0..n {
        ptr::write_volatile(dest.add(i), ptr::read_volatile(src.add(i)));
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn krust_memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if dest < src as *mut u8 {
        for i in 0..n {
            ptr::write_volatile(dest.add(i), ptr::read_volatile(src.add(i)));
        }
    } else {
        let mut i = n;
        while i > 0 {
            i -= 1;
            ptr::write_volatile(dest.add(i), ptr::read_volatile(src.add(i)));
        }
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn krust_memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    for i in 0..n {
        ptr::write_volatile(s.add(i), c as u8);
    }
    s
}

#[no_mangle]
pub unsafe extern "C" fn krust_memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let a = ptr::read_volatile(s1.add(i));
        let b = ptr::read_volatile(s2.add(i));
        if a != b { return a as i32 - b as i32; }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_strlen(s: *const u8) -> usize {
    let mut i = 0;
    while ptr::read_volatile(s.add(i)) != 0 { i += 1; }
    i
}

#[no_mangle]
pub unsafe extern "C" fn krust_strcmp(s1: *const u8, s2: *const u8) -> i32 {
    let mut i = 0;
    loop {
        let a = ptr::read_volatile(s1.add(i));
        let b = ptr::read_volatile(s2.add(i));
        if a != b { return a as i32 - b as i32; }
        if a == 0 { break; }
        i += 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_strncmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let a = ptr::read_volatile(s1.add(i));
        let b = ptr::read_volatile(s2.add(i));
        if a != b { return a as i32 - b as i32; }
        if a == 0 { break; }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_strcpy(dest: *mut u8, src: *const u8) -> *mut u8 {
    let mut i = 0;
    loop {
        let b = ptr::read_volatile(src.add(i));
        ptr::write_volatile(dest.add(i), b);
        if b == 0 { break; }
        i += 1;
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn krust_strncpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        let b = ptr::read_volatile(src.add(i));
        ptr::write_volatile(dest.add(i), b);
        if b == 0 { break; }
        i += 1;
    }
    while i < n {
        ptr::write_volatile(dest.add(i), 0);
        i += 1;
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn krust_strcat(dest: *mut u8, src: *const u8) -> *mut u8 {
    let mut i = 0;
    while ptr::read_volatile(dest.add(i)) != 0 { i += 1; }
    let mut j = 0;
    loop {
        let b = ptr::read_volatile(src.add(j));
        ptr::write_volatile(dest.add(i), b);
        if b == 0 { break; }
        i += 1;
        j += 1;
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn krust_strchr(s: *const u8, c: i32) -> *const u8 {
    let mut i = 0;
    loop {
        let b = ptr::read_volatile(s.add(i));
        if b as i32 == c { return s.add(i); }
        if b == 0 { return ptr::null(); }
        i += 1;
    }
}

fn digit_to_char(d: u32) -> u8 {
    if d < 10 { b'0' + d as u8 } else { b'a' + (d - 10) as u8 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_itoa(num: i32, buf: *mut u8) {
    krust_itoa_base(num, buf, 10);
}

#[no_mangle]
pub unsafe extern "C" fn krust_itoa_base(num: i32, buf: *mut u8, mut base: u32) {
    if base < 2 || base > 36 { base = 10; }
    if num == 0 {
        ptr::write_volatile(buf, b'0');
        ptr::write_volatile(buf.add(1), 0);
        return;
    }
    let mut pos = 0;
    let n = if num < 0 { -(num as i64) as u64 } else { num as u64 };
    let mut tmp = [0u8; 36];
    let mut tpos = 0;
    let mut x = n;
    while x > 0 {
        tmp[tpos] = digit_to_char((x % base as u64) as u32);
        tpos += 1;
        x /= base as u64;
    }
    if num < 0 {
        ptr::write_volatile(buf.add(pos), b'-');
        pos += 1;
    }
    while tpos > 0 {
        tpos -= 1;
        ptr::write_volatile(buf.add(pos), tmp[tpos]);
        pos += 1;
    }
    ptr::write_volatile(buf.add(pos), 0);
}

#[no_mangle]
pub unsafe extern "C" fn krust_uitoa(num: u32, buf: *mut u8) {
    krust_uitoa_base(num, buf, 10);
}

#[no_mangle]
pub unsafe extern "C" fn krust_uitoa_base(num: u32, buf: *mut u8, mut base: u32) {
    if base < 2 || base > 36 { base = 10; }
    if num == 0 {
        ptr::write_volatile(buf, b'0');
        ptr::write_volatile(buf.add(1), 0);
        return;
    }
    let mut pos = 0;
    let mut x = num as u64;
    let mut tmp = [0u8; 36];
    let mut tpos = 0;
    while x > 0 {
        tmp[tpos] = digit_to_char((x % base as u64) as u32);
        tpos += 1;
        x /= base as u64;
    }
    while tpos > 0 {
        tpos -= 1;
        ptr::write_volatile(buf.add(pos), tmp[tpos]);
        pos += 1;
    }
    ptr::write_volatile(buf.add(pos), 0);
}

#[no_mangle]
pub unsafe extern "C" fn krust_uitoa64_base(num: u64, buf: *mut u8, mut base: u32) {
    if base < 2 || base > 36 { base = 10; }
    if num == 0 {
        ptr::write_volatile(buf, b'0');
        ptr::write_volatile(buf.add(1), 0);
        return;
    }
    let mut pos = 0;
    let mut x = num;
    let mut tmp = [0u8; 68];
    let mut tpos = 0;
    while x > 0 {
        tmp[tpos] = digit_to_char((x % base as u64) as u32);
        tpos += 1;
        x /= base as u64;
    }
    while tpos > 0 {
        tpos -= 1;
        ptr::write_volatile(buf.add(pos), tmp[tpos]);
        pos += 1;
    }
    ptr::write_volatile(buf.add(pos), 0);
}

#[no_mangle]
pub unsafe extern "C" fn krust_atoi(s: *const u8) -> i32 {
    let mut i = 0;
    let mut neg = false;
    let mut b = ptr::read_volatile(s.add(i));
    while b == b' ' || b == b'\t' { i += 1; b = ptr::read_volatile(s.add(i)); }
    if b == b'-' { neg = true; i += 1; }
    else if b == b'+' { i += 1; }
    let mut n = 0i32;
    loop {
        b = ptr::read_volatile(s.add(i));
        if b < b'0' || b > b'9' { break; }
        n = n.wrapping_mul(10).wrapping_add((b - b'0') as i32);
        i += 1;
    }
    if neg { -n } else { n }
}

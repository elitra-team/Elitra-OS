use core::sync::atomic::{AtomicI32, Ordering};

const BUF_SIZE: usize = 64;

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct MousePacket {
    pub flags: u8,
    pub dx: i8,
    pub dy: i8,
}

static HEAD: AtomicI32 = AtomicI32::new(0);
static TAIL: AtomicI32 = AtomicI32::new(0);
static PACKET_BYTE: AtomicI32 = AtomicI32::new(0);
static mut BUF: [MousePacket; BUF_SIZE] = [MousePacket { flags: 0, dx: 0, dy: 0 }; BUF_SIZE];
static mut CURRENT_PACKET: [u8; 3] = [0; 3];

extern "C" {
    fn krust_isr_register_handler(vec: u8, handler: extern "C" fn(*mut core::ffi::c_void));
}

fn wait_input() {
    unsafe {
        for _ in 0..10000 {
            let status: u8;
            core::arch::asm!("in al, dx", out("al") status, in("dx") 0x64u16);
            if status & 0x02 == 0 { return; }
        }
    }
}

fn wait_output() {
    unsafe {
        for _ in 0..10000 {
            let status: u8;
            core::arch::asm!("in al, dx", out("al") status, in("dx") 0x64u16);
            if status & 0x01 != 0 { return; }
        }
    }
}

fn mouse_write(data: u8) {
    wait_input();
    unsafe { core::arch::asm!("out dx, al", in("dx") 0x64u16, in("al") 0xD4u8); }
    wait_input();
    unsafe { core::arch::asm!("out dx, al", in("dx") 0x60u16, in("al") data); }
}

fn mouse_read() -> u8 {
    wait_output();
    let val: u8;
    unsafe { core::arch::asm!("in al, dx", out("al") val, in("dx") 0x60u16); }
    val
}

extern "C" fn ps2mouse_callback(_r: *mut core::ffi::c_void) {
    let data: u8;
    unsafe { core::arch::asm!("in al, dx", out("al") data, in("dx") 0x60u16); }

    let pb = PACKET_BYTE.load(Ordering::SeqCst);
    match pb {
        0 => {
            if data & 0x08 == 0 { return; }
            unsafe { CURRENT_PACKET[0] = data; }
            PACKET_BYTE.store(1, Ordering::SeqCst);
        }
        1 => {
            unsafe { CURRENT_PACKET[1] = data; }
            PACKET_BYTE.store(2, Ordering::SeqCst);
        }
        2 => {
            unsafe { CURRENT_PACKET[2] = data; }
            PACKET_BYTE.store(0, Ordering::SeqCst);
            unsafe {
                let next = (HEAD.load(Ordering::SeqCst) + 1) % BUF_SIZE as i32;
                if next != TAIL.load(Ordering::SeqCst) {
                    BUF[HEAD.load(Ordering::SeqCst) as usize].flags = CURRENT_PACKET[0];
                    BUF[HEAD.load(Ordering::SeqCst) as usize].dx = CURRENT_PACKET[1] as i8;
                    BUF[HEAD.load(Ordering::SeqCst) as usize].dy = CURRENT_PACKET[2] as i8;
                    HEAD.store(next, Ordering::SeqCst);
                }
            }
        }
        _ => {}
    }
}

#[no_mangle]
pub extern "C" fn krust_ps2mouse_init() {
    wait_input();
    unsafe { core::arch::asm!("out dx, al", in("dx") 0x64u16, in("al") 0xA8u8); }
    wait_input();

    unsafe { core::arch::asm!("out dx, al", in("dx") 0x64u16, in("al") 0x20u8); }
    wait_output();
    let mut config: u8;
    unsafe { core::arch::asm!("in al, dx", out("al") config, in("dx") 0x60u16); }
    config |= 0x02;
    config &= !0x20;

    wait_input();
    unsafe { core::arch::asm!("out dx, al", in("dx") 0x64u16, in("al") 0x60u8); }
    wait_input();
    unsafe { core::arch::asm!("out dx, al", in("dx") 0x60u16, in("al") config); }

    mouse_write(0xF6);
    mouse_read();

    mouse_write(0xF4);
    mouse_read();

    mouse_write(0xF3);
    mouse_read();
    mouse_write(100);
    mouse_read();

    unsafe {
        krust_isr_register_handler(12, ps2mouse_callback);
    }
}

#[no_mangle]
pub extern "C" fn krust_ps2mouse_data_available() -> bool {
    HEAD.load(Ordering::SeqCst) != TAIL.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn krust_ps2mouse_read_packet(pkt: *mut MousePacket) -> bool {
    if HEAD.load(Ordering::SeqCst) == TAIL.load(Ordering::SeqCst) {
        return false;
    }
    unsafe {
        let t = TAIL.load(Ordering::SeqCst);
        (*pkt).flags = BUF[t as usize].flags;
        (*pkt).dx = BUF[t as usize].dx;
        (*pkt).dy = BUF[t as usize].dy;
        TAIL.store((t + 1) % BUF_SIZE as i32, Ordering::SeqCst);
    }
    true
}

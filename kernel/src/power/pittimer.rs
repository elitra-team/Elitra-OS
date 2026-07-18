use core::sync::atomic::{AtomicU32, Ordering};
use crate::scheduler::Registers;

static TICK_COUNT: AtomicU32 = AtomicU32::new(0);
pub static TICK_MS: AtomicU32 = AtomicU32::new(10);

extern "C" {
    fn krust_usb_poll();
    fn krust_ehci_poll();
    fn krust_irq_install_handler(irq: i32, handler: extern "C" fn(*mut Registers));
}

static mut USE_EHCI: bool = false;
static mut GUI_MOUSE_X: i32 = 400;
static mut GUI_MOUSE_Y: i32 = 300;

pub unsafe fn set_ehci(use_ehci: bool) {
    USE_EHCI = use_ehci;
}

extern "C" fn pittimer_callback(r: *mut Registers) {
    unsafe {
        TICK_COUNT.fetch_add(1, Ordering::SeqCst);
        let ticks = TICK_COUNT.load(Ordering::SeqCst);
        if ticks % 10 == 0 {
            if USE_EHCI {
                krust_ehci_poll();
            } else {
                krust_usb_poll();
            }
        }

        if crate::mouse_cursor::GUI_ACTIVE {
            while crate::ps2mouse::krust_ps2mouse_data_available() {
                let mut pkt = crate::ps2mouse::MousePacket { flags: 0, dx: 0, dy: 0 };
                if !crate::ps2mouse::krust_ps2mouse_read_packet(&mut pkt as *mut _) { break; }
                GUI_MOUSE_X += pkt.dx as i32;
                GUI_MOUSE_Y -= pkt.dy as i32;
                if GUI_MOUSE_X < 0 { GUI_MOUSE_X = 0; }
                if GUI_MOUSE_Y < 0 { GUI_MOUSE_Y = 0; }
                let info = crate::framebuffer::krust_framebuffer_info();
                let sw = info.width;
                let sh = info.height;
                if sw > 0 && GUI_MOUSE_X >= sw as i32 { GUI_MOUSE_X = sw as i32 - 1; }
                if sh > 0 && GUI_MOUSE_Y >= sh as i32 { GUI_MOUSE_Y = sh as i32 - 1; }
                let buttons = if pkt.flags & 0x01 != 0 { 1u8 } else { 0u8 };
                crate::gui::handle_mouse(GUI_MOUSE_X, GUI_MOUSE_Y, buttons);
            }
            if ticks % 3 == 0 {
                crate::gui::compositor_render();
            }
        } else {
            if ticks % 10 == 0 {
                crate::mouse_cursor::mouse_cursor_update();
            }
        }

        crate::scheduler::krust_sched_preempt(r);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_pittimer_init(frequency: u32) {
    krust_irq_install_handler(0, pittimer_callback);
    krust_pittimer_configure(frequency);
}

#[no_mangle]
pub extern "C" fn krust_pittimer_configure(frequency: u32) {
    let divisor = 1193182u32 / frequency;
    unsafe {
        core::arch::asm!("out dx, al", in("dx") 0x43u16, in("al") 0x36u8);
        core::arch::asm!("out dx, al", in("dx") 0x40u16, in("al") (divisor as u8));
        core::arch::asm!("out dx, al", in("dx") 0x40u16, in("al") ((divisor >> 8) as u8));
    }
    TICK_MS.store(1000 / frequency, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn krust_pittimer_get_ticks() -> u32 {
    TICK_COUNT.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn krust_pittimer_sleep(ms: u32) {
    let target = TICK_COUNT.load(Ordering::SeqCst) + (ms / TICK_MS.load(Ordering::SeqCst)) + 1;
    while TICK_COUNT.load(Ordering::SeqCst) < target {
        core::hint::spin_loop();
    }
}

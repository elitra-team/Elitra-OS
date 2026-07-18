#[repr(C)]
pub struct RTCInfo {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u16,
}

static mut BCD: bool = true;

fn read_register(reg: u8) -> u8 {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") 0x70u16, in("al") reg);
        core::arch::asm!("in al, dx", out("al") _, in("dx") 0x71u16);
        let val: u8;
        core::arch::asm!("in al, dx", out("al") val, in("dx") 0x71u16);
        val
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_cmos_rtc_init() {
    let status_b = read_register(0x0B);
    BCD = (status_b & 0x04) == 0;
}

#[no_mangle]
pub unsafe extern "C" fn krust_cmos_read_time() -> RTCInfo {
    let mut info: RTCInfo = unsafe { core::mem::zeroed() };

    while read_register(0x0A) & 0x80 != 0 {}

    info.second = read_register(0x00);
    info.minute = read_register(0x02);
    info.hour = read_register(0x04);
    info.day = read_register(0x07);
    info.month = read_register(0x08);
    info.year = read_register(0x09) as u16;

    let century = read_register(0x32);
    if century >= 0x10 || century > 0 {
        if BCD {
            info.year = ((century / 16 * 10 + century % 16) as u16) * 100
                + ((info.year / 16 * 10 + info.year % 16) as u16);
        } else {
            info.year = (century as u16) * 100 + info.year;
        }
    } else {
        if BCD {
            info.year = 2000 + (info.year / 16 * 10 + info.year % 16) as u16;
        } else {
            info.year = 2000 + info.year;
        }
    }

    if BCD {
        info.second = (info.second / 16 * 10) + (info.second % 16);
        info.minute = (info.minute / 16 * 10) + (info.minute % 16);
        info.hour = (info.hour / 16 * 10) + (info.hour % 16);
        info.day = (info.day / 16 * 10) + (info.day % 16);
        info.month = (info.month / 16 * 10) + (info.month % 16);
    }

    info
}

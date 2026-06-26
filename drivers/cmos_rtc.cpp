#include "cmos_rtc.hpp"
#include "port.hpp"
#include "lib.hpp"

using namespace drivers;

int CMOSRTC::bcd = 1;

uint8_t CMOSRTC::read_register(uint8_t reg) {
    arch::x86::outb(0x70, reg);
    arch::x86::io_wait();
    return arch::x86::inb(0x71);
}

void CMOSRTC::init() {
    // Check if RTC uses BCD or binary mode via status register B
    uint8_t status_b = read_register(0x0B);
    bcd = !(status_b & 0x04);
}

RTCInfo CMOSRTC::read_time() {
    RTCInfo info;
    lib::memset(&info, 0, sizeof(info));

    // Wait for RTC to not be updating (status register A bit 7)
    while (read_register(0x0A) & 0x80);

    info.second = read_register(0x00);
    info.minute = read_register(0x02);
    info.hour = read_register(0x04);
    info.day = read_register(0x07);
    info.month = read_register(0x08);
    info.year = read_register(0x09);

    // Read century if available
    uint8_t century = read_register(0x32);
    if (century >= 0x10 || century > 0) {
        if (bcd) {
            info.year = (century / 16 * 10 + century % 16) * 100 + (info.year / 16 * 10 + info.year % 16);
        } else {
            info.year = century * 100 + info.year;
        }
    } else {
        if (bcd) {
            info.year = 2000 + (info.year / 16 * 10 + info.year % 16);
        } else {
            info.year = 2000 + info.year;
        }
    }

    if (bcd) {
        info.second = (info.second / 16 * 10) + (info.second % 16);
        info.minute = (info.minute / 16 * 10) + (info.minute % 16);
        info.hour = (info.hour / 16 * 10) + (info.hour % 16);
        info.day = (info.day / 16 * 10) + (info.day % 16);
        info.month = (info.month / 16 * 10) + (info.month % 16);
    }

    return info;
}

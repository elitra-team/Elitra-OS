#ifndef ELITRA_CMOS_RTC_HPP
#define ELITRA_CMOS_RTC_HPP

#include <cstdint>

namespace drivers {

struct RTCInfo {
    uint8_t second;
    uint8_t minute;
    uint8_t hour;
    uint8_t day;
    uint8_t month;
    uint16_t year;
};

class CMOSRTC {
public:
    static void init();
    static RTCInfo read_time();

private:
    static int bcd;
    static uint8_t read_register(uint8_t reg);
};

}

#endif

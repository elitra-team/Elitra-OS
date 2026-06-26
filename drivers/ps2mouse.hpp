#ifndef ELITRA_PS2MOUSE_HPP
#define ELITRA_PS2MOUSE_HPP

#include <cstdint>
#include "isr.hpp"

namespace drivers {

#pragma pack(push, 1)
struct MousePacket {
    uint8_t flags;
    int8_t dx;
    int8_t dy;
};
#pragma pack(pop)

class PS2Mouse {
public:
    static void init();
    static bool data_available();
    static bool read_packet(MousePacket &pkt);

private:
    static const int BUF_SIZE = 64;

    static volatile MousePacket buf[BUF_SIZE];
    static volatile int head;
    static volatile int tail;
    static volatile int packet_byte;
    static volatile uint8_t current_packet[3];

    static void callback(arch::x86::Registers *r);
    static void mouse_write(uint8_t data);
    static uint8_t mouse_read();
    static void wait_input();
    static void wait_output();
};

}

#endif

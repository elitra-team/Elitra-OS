#include "ps2mouse.hpp"
#include "irq.hpp"
#include "port.hpp"
#include "lib.hpp"

using namespace drivers;

volatile MousePacket PS2Mouse::buf[BUF_SIZE];
volatile int PS2Mouse::head = 0;
volatile int PS2Mouse::tail = 0;
volatile int PS2Mouse::packet_byte = 0;
volatile uint8_t PS2Mouse::current_packet[3] = {0, 0, 0};

void PS2Mouse::wait_input() {
    for (int i = 0; i < 10000; i++) {
        if (!(arch::x86::inb(0x64) & 0x02)) return;
    }
}

void PS2Mouse::wait_output() {
    for (int i = 0; i < 10000; i++) {
        if (arch::x86::inb(0x64) & 0x01) return;
    }
}

void PS2Mouse::mouse_write(uint8_t data) {
    wait_input();
    arch::x86::outb(0x64, 0xD4);
    wait_input();
    arch::x86::outb(0x60, data);
}

uint8_t PS2Mouse::mouse_read() {
    wait_output();
    return arch::x86::inb(0x60);
}

void PS2Mouse::init() {
    // Enable the PS/2 mouse (second port)
    wait_input();
    arch::x86::outb(0x64, 0xA8); // enable mouse
    wait_input();

    // Read config byte
    arch::x86::outb(0x64, 0x20);
    wait_output();
    uint8_t config = arch::x86::inb(0x60);
    config |= 0x02;  // enable IRQ12
    config &= ~0x20; // disable mouse clock

    // Write config byte
    wait_input();
    arch::x86::outb(0x64, 0x60);
    wait_input();
    arch::x86::outb(0x60, config);

    // Set default settings
    mouse_write(0xF6);
    mouse_read(); // ack

    // Enable data reporting
    mouse_write(0xF4);
    mouse_read(); // ack

    // Set sample rate to 100 Hz
    mouse_write(0xF3);
    mouse_read(); // ack
    mouse_write(100);
    mouse_read(); // ack

    // Register IRQ handler (IRQ12 = int 44)
    arch::x86::IRQ::install_handler(12, callback);
}

void PS2Mouse::callback(arch::x86::Registers *r) {
    (void)r;
    uint8_t data = arch::x86::inb(0x60);

    switch (packet_byte) {
        case 0:
            // Wait for byte with bit 3 set (packet start marker)
            if (!(data & 0x08)) return;
            current_packet[0] = data;
            packet_byte = 1;
            break;
        case 1:
            current_packet[1] = data;
            packet_byte = 2;
            break;
        case 2:
            current_packet[2] = data;
            packet_byte = 0;

            // Store completed packet in ring buffer
            int next = (head + 1) % BUF_SIZE;
            if (next != tail) {
                buf[head].flags = current_packet[0];
                buf[head].dx = static_cast<int8_t>(current_packet[1]);
                buf[head].dy = static_cast<int8_t>(current_packet[2]);
                head = next;
            }
            break;
    }
}

bool PS2Mouse::data_available() {
    return head != tail;
}

bool PS2Mouse::read_packet(MousePacket &pkt) {
    if (head == tail) return false;
    pkt.flags = buf[tail].flags;
    pkt.dx = buf[tail].dx;
    pkt.dy = buf[tail].dy;
    tail = (tail + 1) % BUF_SIZE;
    return true;
}

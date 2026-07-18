use crate::klib::{uint8_t, uint16_t};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ArpHeader {
    pub hw_type: uint16_t,
    pub proto_type: uint16_t,
    pub hw_size: uint8_t,
    pub proto_size: uint8_t,
    pub opcode: uint16_t,
    pub sender_mac: [uint8_t; 6],
    pub sender_ip: [uint8_t; 4],
    pub target_mac: [uint8_t; 6],
    pub target_ip: [uint8_t; 4],
}

pub const OP_REQUEST: uint16_t = 1;
pub const OP_REPLY: uint16_t = 2;

pub fn parse_arp_packet(data: &[u8]) -> Option<(uint16_t, [uint8_t; 6], [uint8_t; 4])> {
    if data.len() < core::mem::size_of::<ArpHeader>() {
        return None;
    }

    let header = unsafe { &*(data.as_ptr() as *const ArpHeader) };
    let opcode = u16::from_be(header.opcode);
    let sender_mac = header.sender_mac;
    let sender_ip = header.sender_ip;

    Some((opcode, sender_mac, sender_ip))
}

pub fn build_arp_request(
    packet: &mut [u8],
    src_mac: [uint8_t; 6],
    src_ip: [uint8_t; 4],
    target_ip: [uint8_t; 4],
) {
    let header = unsafe { &mut *(packet.as_mut_ptr() as *mut ArpHeader) };
    header.hw_type = (1u16).to_be();
    header.proto_type = (0x0800u16).to_be();
    header.hw_size = 6;
    header.proto_size = 4;
    header.opcode = OP_REQUEST.to_be();
    header.sender_mac = src_mac;
    header.sender_ip = src_ip;
    header.target_mac = [0; 6];
    header.target_ip = target_ip;
}

pub fn build_arp_reply(
    packet: &mut [u8],
    src_mac: [uint8_t; 6],
    src_ip: [uint8_t; 4],
    target_mac: [uint8_t; 6],
    target_ip: [uint8_t; 4],
) {
    let header = unsafe { &mut *(packet.as_mut_ptr() as *mut ArpHeader) };
    header.hw_type = (1u16).to_be();
    header.proto_type = (0x0800u16).to_be();
    header.hw_size = 6;
    header.proto_size = 4;
    header.opcode = OP_REPLY.to_be();
    header.sender_mac = src_mac;
    header.sender_ip = src_ip;
    header.target_mac = target_mac;
    header.target_ip = target_ip;
}

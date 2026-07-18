use crate::klib::{uint8_t, uint16_t};

pub const ICMP_ECHO_REQUEST: u8 = 8;
pub const ICMP_ECHO_REPLY: u8 = 0;
pub const ICMP_DEST_UNREACHABLE: u8 = 3;
pub const ICMP_TIME_EXCEEDED: u8 = 11;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IcmpHeader {
    pub icmp_type: uint8_t,
    pub code: uint8_t,
    pub checksum: uint16_t,
    pub identifier: uint16_t,
    pub sequence: uint16_t,
}

fn icmp_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        let word = ((data[i] as u32) << 8) | (data[i + 1] as u32);
        sum = sum.wrapping_add(word);
        i += 2;
    }
    if i < data.len() {
        sum = sum.wrapping_add((data[i] as u32) << 8);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}

pub fn parse_icmp_packet(data: &[u8]) -> Option<(uint8_t, uint16_t, uint16_t, &[u8])> {
    if data.len() < core::mem::size_of::<IcmpHeader>() {
        return None;
    }

    let header = unsafe { &*(data.as_ptr() as *const IcmpHeader) };
    let icmp_type = header.icmp_type;
    let identifier = u16::from_be(header.identifier);
    let sequence = u16::from_be(header.sequence);
    let payload = &data[core::mem::size_of::<IcmpHeader>()..];

    Some((icmp_type, identifier, sequence, payload))
}

pub fn build_icmp_echo_request(
    packet: &mut [u8],
    id: uint16_t,
    seq: uint16_t,
    payload: &[u8],
) -> usize {
    let header_size = core::mem::size_of::<IcmpHeader>();
    let total = header_size + payload.len();

    if packet.len() < total {
        return 0;
    }

    let header = unsafe { &mut *(packet.as_mut_ptr() as *mut IcmpHeader) };
    header.icmp_type = ICMP_ECHO_REQUEST;
    header.code = 0;
    header.checksum = 0;
    header.identifier = id.to_be();
    header.sequence = seq.to_be();

    packet[header_size..header_size + payload.len()].copy_from_slice(payload);

    header.checksum = icmp_checksum(&packet[..total]).to_be();

    total
}

pub fn build_icmp_reply(
    packet: &mut [u8],
    id: uint16_t,
    seq: uint16_t,
    payload: &[u8],
) -> usize {
    let header_size = core::mem::size_of::<IcmpHeader>();
    let total = header_size + payload.len();

    if packet.len() < total {
        return 0;
    }

    let header = unsafe { &mut *(packet.as_mut_ptr() as *mut IcmpHeader) };
    header.icmp_type = ICMP_ECHO_REPLY;
    header.code = 0;
    header.checksum = 0;
    header.identifier = id.to_be();
    header.sequence = seq.to_be();

    packet[header_size..header_size + payload.len()].copy_from_slice(payload);

    header.checksum = icmp_checksum(&packet[..total]).to_be();

    total
}

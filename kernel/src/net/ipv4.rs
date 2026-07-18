use crate::klib::{uint8_t, uint16_t};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IPv4Header {
    pub version_ihl: uint8_t,
    pub dscp_ecn: uint8_t,
    pub total_length: uint16_t,
    pub identification: uint16_t,
    pub flags_fragment_offset: uint16_t,
    pub ttl: uint8_t,
    pub protocol: uint8_t,
    pub header_checksum: uint16_t,
    pub src_ip: [uint8_t; 4],
    pub dest_ip: [uint8_t; 4],
}

pub const PROTOCOL_TCP: uint8_t = 6;
pub const PROTOCOL_UDP: uint8_t = 17;
pub const PROTOCOL_ICMP: uint8_t = 1;

pub fn parse_ipv4_packet(data: &[u8]) -> Option<(uint8_t, &[u8])> {
    if data.len() < core::mem::size_of::<IPv4Header>() {
        return None;
    }

    let header = unsafe { &*(data.as_ptr() as *const IPv4Header) };
    let ihl = (header.version_ihl & 0x0F) as usize * 4;
    if data.len() < ihl {
        return None;
    }

    let protocol = header.protocol;
    let payload_start = ihl;
    let payload = &data[payload_start..];

    Some((protocol, payload))
}

pub fn build_ipv4_packet(
    packet: &mut [u8],
    src_ip: [uint8_t; 4],
    dest_ip: [uint8_t; 4],
    protocol: uint8_t,
    payload: &[u8],
) -> usize {
    let header_size = core::mem::size_of::<IPv4Header>();
    if packet.len() < header_size + payload.len() {
        return 0;
    }

    let header = unsafe { &mut *(packet.as_mut_ptr() as *mut IPv4Header) };
    header.version_ihl = 0x45;
    header.dscp_ecn = 0;
    header.total_length = ((header_size + payload.len()) as u16).to_be();
    header.identification = 0;
    header.flags_fragment_offset = 0;
    header.ttl = 64;
    header.protocol = protocol;
    header.src_ip = src_ip;
    header.dest_ip = dest_ip;

    // Compute header checksum
    let mut sum: u32 = 0;
    let header_words = unsafe {
        core::slice::from_raw_parts(
            packet.as_ptr() as *const u16,
            header_size / 2,
        )
    };
    for &word in header_words {
        sum += word as u32;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    header.header_checksum = !sum as u16;

    let payload_start = header_size;
    packet[payload_start..payload_start + payload.len()].copy_from_slice(payload);
    payload_start + payload.len()
}

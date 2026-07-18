use crate::klib::uint16_t;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UdpHeader {
    pub src_port: uint16_t,
    pub dest_port: uint16_t,
    pub length: uint16_t,
    pub checksum: uint16_t,
}

fn internet_checksum(data: &[u8]) -> u16 {
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

fn udp_checksum(src_ip: &[u8; 4], dst_ip: &[u8; 4], udp_data: &[u8]) -> u16 {
    let mut pseudo = [0u8; 12];
    pseudo[0..4].copy_from_slice(src_ip);
    pseudo[4..8].copy_from_slice(dst_ip);
    pseudo[8] = 0;
    pseudo[9] = 17;
    let len = (udp_data.len() as u16).to_be();
    pseudo[10] = (len >> 8) as u8;
    pseudo[11] = len as u8;

    let mut full = [0u8; 4096];
    let pseudo_len = pseudo.len();
    let data_len = udp_data.len();
    let total = pseudo_len + data_len;
    if total > full.len() {
        return 0;
    }
    full[..pseudo_len].copy_from_slice(&pseudo);
    full[pseudo_len..total].copy_from_slice(udp_data);

    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < total {
        let word = ((full[i] as u32) << 8) | (full[i + 1] as u32);
        sum = sum.wrapping_add(word);
        i += 2;
    }
    if i < total {
        sum = sum.wrapping_add((full[i] as u32) << 8);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    let csum = !sum as u16;
    if csum == 0 { 0xFFFF } else { csum }
}

pub fn parse_udp_packet(data: &[u8]) -> Option<(uint16_t, uint16_t, &[u8])> {
    if data.len() < core::mem::size_of::<UdpHeader>() {
        return None;
    }

    let header = unsafe { &*(data.as_ptr() as *const UdpHeader) };
    let src_port = u16::from_be(header.src_port);
    let dest_port = u16::from_be(header.dest_port);
    let length = u16::from_be(header.length) as usize;

    if data.len() < length {
        return None;
    }

    let payload = &data[core::mem::size_of::<UdpHeader>()..length];
    Some((src_port, dest_port, payload))
}

pub fn build_udp_packet(
    packet: &mut [u8],
    src_port: uint16_t,
    dest_port: uint16_t,
    payload: &[u8],
    src_ip: &[u8; 4],
    dst_ip: &[u8; 4],
) -> usize {
    let header_size = core::mem::size_of::<UdpHeader>();
    let total_length = header_size + payload.len();

    if packet.len() < total_length {
        return 0;
    }

    let header = unsafe { &mut *(packet.as_mut_ptr() as *mut UdpHeader) };
    header.src_port = src_port.to_be();
    header.dest_port = dest_port.to_be();
    header.length = (total_length as u16).to_be();
    header.checksum = 0;

    let payload_start = header_size;
    packet[payload_start..payload_start + payload.len()].copy_from_slice(payload);

    let csum = udp_checksum(src_ip, dst_ip, &packet[..total_length]);
    header.checksum = csum.to_be();

    total_length
}

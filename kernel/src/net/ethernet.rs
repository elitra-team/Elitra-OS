use crate::klib::{uint8_t, uint16_t};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct EthernetHeader {
    pub dest_mac: [uint8_t; 6],
    pub src_mac: [uint8_t; 6],
    pub ethertype: uint16_t,
}

pub const ETHERTYPE_IPV4: uint16_t = 0x0800;
pub const ETHERTYPE_ARP: uint16_t = 0x0806;

pub fn parse_ethernet_frame(data: &[u8]) -> Option<(uint16_t, &[u8])> {
    if data.len() < core::mem::size_of::<EthernetHeader>() {
        return None;
    }

    let header = unsafe { &*(data.as_ptr() as *const EthernetHeader) };
    let ethertype = u16::from_be(header.ethertype);
    let payload_start = core::mem::size_of::<EthernetHeader>();
    let payload = &data[payload_start..];

    Some((ethertype, payload))
}

pub fn build_ethernet_frame(
    frame: &mut [u8],
    src_mac: [uint8_t; 6],
    dest_mac: [uint8_t; 6],
    ethertype: uint16_t,
    payload: &[u8],
) -> usize {
    if frame.len() < core::mem::size_of::<EthernetHeader>() + payload.len() {
        return 0;
    }

    let header = unsafe { &mut *(frame.as_mut_ptr() as *mut EthernetHeader) };
    header.dest_mac = dest_mac;
    header.src_mac = src_mac;
    header.ethertype = ethertype.to_be();

    let payload_start = core::mem::size_of::<EthernetHeader>();
    frame[payload_start..payload_start + payload.len()].copy_from_slice(payload);
    payload_start + payload.len()
}

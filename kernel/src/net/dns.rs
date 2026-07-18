use super::e1000;
use super::udp;
use super::ipv4;
use super::ethernet;
use super::NetDevice;
use core::sync::atomic::{AtomicU16, Ordering};

const DNS_SERVER: [u8; 4] = [8, 8, 8, 8];
const DNS_PORT: u16 = 53;
const DNS_TIMEOUT_RETRIES: usize = 200;
const DNS_POLL_INTERVAL: usize = 10000;

static DNS_TXID: AtomicU16 = AtomicU16::new(1);

pub fn build_dns_query(domain: &[u8], buf: &mut [u8]) -> usize {
    if buf.len() < 512 { return 0; }
    let txid = DNS_TXID.fetch_add(1, Ordering::Relaxed);
    buf[0] = (txid >> 8) as u8;
    buf[1] = txid as u8;
    buf[2] = 0x01; buf[3] = 0x00;
    buf[4] = 0x00; buf[5] = 0x01;
    buf[6] = 0x00; buf[7] = 0x00;
    buf[8] = 0x00; buf[9] = 0x00;
    buf[10] = 0x00; buf[11] = 0x00;
    let mut pos = 12;
    let mut label_start = 0;
    let mut i = 0;
    while i <= domain.len() {
        if i == domain.len() || domain[i] == b'.' {
            let label_len = (i - label_start) as u8;
            buf[pos] = label_len;
            pos += 1;
            let mut j = label_start;
            while j < i { buf[pos] = domain[j]; pos += 1; j += 1; }
            label_start = i + 1;
        }
        i += 1;
    }
    buf[pos] = 0; pos += 1;
    buf[pos] = 0; buf[pos + 1] = 1;
    buf[pos + 2] = 0; buf[pos + 3] = 1;
    pos + 4
}

pub fn parse_dns_response(buf: &[u8]) -> Option<[u8; 4]> {
    if buf.len() < 12 { return None; }
    let answers = u16::from_be_bytes([buf[6], buf[7]]);
    if answers == 0 { return None; }
    let mut pos = 12;
    while pos < buf.len() && buf[pos] != 0 {
        let ll = buf[pos] as usize;
        if ll >= 0xC0 { pos += 2; break; }
        pos += 1 + ll;
    }
    if pos < buf.len() && buf[pos] == 0 { pos += 1; }
    pos += 4;
    for _ in 0..answers {
        if pos + 10 > buf.len() { return None; }
        if buf[pos] & 0xC0 == 0xC0 { pos += 2; } else {
            while pos < buf.len() && buf[pos] != 0 { pos += 1 + buf[pos] as usize; }
            pos += 1;
        }
        let rtype = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        let rdlen = u16::from_be_bytes([buf[pos + 8], buf[pos + 9]]) as usize;
        pos += 10;
        if rtype == 1 && rdlen == 4 && pos + 4 <= buf.len() {
            return Some([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
        }
        pos += rdlen;
    }
    None
}

pub fn dns_resolve(domain: &[u8]) -> Option<[u8; 4]> {
    let mut tx_buf = [0u8; 512];
    let query_len = build_dns_query(domain, &mut tx_buf);
    if query_len == 0 { return None; }
    let mut udp_packet = [0u8; 1500];
    let local_port = super::allocate_ephemeral_port();
    let my_ip = e1000::get_ip();
    let len = udp::build_udp_packet(
        &mut udp_packet, local_port, DNS_PORT,
        &tx_buf[..query_len], &my_ip, &DNS_SERVER,
    );
    if len == 0 { return None; }
    {
        let mut stack = super::NET_STACK.lock();
        let _ = stack.as_mut().unwrap().send_ipv4(DNS_SERVER, ipv4::PROTOCOL_UDP, &udp_packet[..len]);
    }
    let mut resp_buf = [0u8; 512];
    for _ in 0..DNS_TIMEOUT_RETRIES {
        for _ in 0..DNS_POLL_INTERVAL { core::hint::spin_loop(); }
        let mut stack = super::NET_STACK.lock();
        if let Some(ref mut s) = *stack {
            let _ = s.device().poll();
            let mut raw_buf = [0u8; 1500];
            while let Ok(pkt_len) = s.device().receive(&mut raw_buf) {
                if let Some((ethertype, payload)) = ethernet::parse_ethernet_frame(&raw_buf[..pkt_len]) {
                    if ethertype == ethernet::ETHERTYPE_IPV4 {
                        if let Some((proto, ip_payload)) = ipv4::parse_ipv4_packet(payload) {
                            if proto == ipv4::PROTOCOL_UDP {
                                if let Some((sp, dp, udp_payload)) = udp::parse_udp_packet(ip_payload) {
                                    if dp == local_port && sp == DNS_PORT {
                                        let n = core::cmp::min(udp_payload.len(), 512);
                                        resp_buf[..n].copy_from_slice(&udp_payload[..n]);
                                        return parse_dns_response(&resp_buf[..n]);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

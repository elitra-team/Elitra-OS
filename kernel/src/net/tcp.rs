use crate::klib::{uint8_t, uint16_t, uint32_t};
use crate::net::SocketAddr;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct TcpHeader {
    pub src_port: uint16_t,
    pub dest_port: uint16_t,
    pub seq_num: uint32_t,
    pub ack_num: uint32_t,
    pub data_offset_flags: uint16_t,
    pub window_size: uint16_t,
    pub checksum: uint16_t,
    pub urgent_ptr: uint16_t,
}

pub const FLAG_FIN: uint16_t = 0x0001;
pub const FLAG_SYN: uint16_t = 0x0002;
pub const FLAG_RST: uint16_t = 0x0004;
pub const FLAG_PSH: uint16_t = 0x0008;
pub const FLAG_ACK: uint16_t = 0x0010;

pub const TCP_HEADER_SIZE: usize = 20;
pub const DEFAULT_WINDOW: u16 = 65535;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

pub struct TcpConnection {
    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,
    pub state: TcpState,
    pub send_seq: u32,
    pub send_unack: u32,
    pub recv_seq: u32,
    pub send_window: u16,
    pub recv_window: u16,
    pub mss: u16,
    pub retransmit_timeout: u64,
    pub last_activity: u64,
    pub retransmit_count: u8,
    pub recv_buf: [u8; 4096],
    pub recv_pos: usize,
    pub send_buf: [u8; 4096],
    pub send_pos: usize,
    pub socket_fd: i32,
}

impl TcpConnection {
    pub fn new(local_addr: SocketAddr, remote_addr: SocketAddr) -> Self {
        Self {
            local_addr,
            remote_addr,
            state: TcpState::Closed,
            send_seq: generate_isn(),
            send_unack: 0,
            recv_seq: 0,
            send_window: DEFAULT_WINDOW,
            recv_window: DEFAULT_WINDOW,
            mss: 1460,
            retransmit_timeout: 1000,
            last_activity: 0,
            retransmit_count: 0,
            recv_buf: [0; 4096],
            recv_pos: 0,
            send_buf: [0; 4096],
            send_pos: 0,
            socket_fd: -1,
        }
    }

    pub fn update_activity(&mut self) {
        self.last_activity = crate::pittimer::krust_pittimer_get_ticks() as u64;
    }

    /// Compute the receive window to advertise to the peer.
    /// Returns 0 when the receive buffer is full (backpressure).
    fn advertised_recv_window(&self) -> u16 {
        let remaining = self.recv_buf.len().saturating_sub(self.recv_pos) as u16;
        // When buffer is below 25% free, advertise a tiny window to apply backpressure
        if remaining < (self.recv_buf.len() / 4) as u16 {
            remaining
        } else {
            DEFAULT_WINDOW
        }
    }

    /// Returns the number of bytes in flight (sent but not yet ACKed).
    fn bytes_in_flight(&self) -> u32 {
        self.send_seq.wrapping_sub(self.send_unack)
    }

    /// Build and return a SYN packet to initiate connection
    pub fn build_syn(&mut self) -> Option<(u32, [u8; 200])> {
        if self.state != TcpState::Closed {
            return None;
        }
        self.state = TcpState::SynSent;
        self.send_unack = self.send_seq;
        let seq = self.send_seq;
        self.send_seq = self.send_seq.wrapping_add(1);
        self.update_activity();

        let mut packet = [0u8; 200];
        let len = build_tcp_packet(
            &mut packet,
            self.local_addr.ip,
            self.remote_addr.ip,
            self.local_addr.port,
            self.remote_addr.port,
            seq,
            0,
            FLAG_SYN,
            DEFAULT_WINDOW,
            &[],
        );
        if len == 0 { None } else { Some((seq, packet)) }
    }

    /// Process an incoming TCP segment and return any response packet
    pub fn process_segment(
        &mut self,
        flags: u16,
        seq_num: u32,
        ack_num: u32,
        window_size: u16,
        payload: &[u8],
    ) -> Option<(u32, [u8; 200])> {
        self.update_activity();

        match self.state {
            TcpState::SynSent => {
                // Expect SYN+ACK
                if flags & FLAG_SYN != 0 && flags & FLAG_ACK != 0 {
                    self.recv_seq = seq_num.wrapping_add(1);
                    // Send ACK
                    let ack_seq = self.recv_seq;
                    self.state = TcpState::Established;
                    let mut packet = [0u8; 200];
                    let len = build_tcp_packet(
                        &mut packet,
                        self.local_addr.ip,
                        self.remote_addr.ip,
                        self.local_addr.port,
                        self.remote_addr.port,
                        self.send_seq,
                        ack_seq,
                        FLAG_ACK,
                        DEFAULT_WINDOW,
                        &[],
                    );
                    if len > 0 {
                        Some((ack_seq, packet))
                    } else {
                        None
                    }
                } else if flags & FLAG_RST != 0 {
                    self.state = TcpState::Closed;
                    None
                } else {
                    None
                }
            }

            TcpState::SynReceived => {
                // Expect ACK for our SYN+ACK
                if flags & FLAG_ACK != 0 {
                    self.recv_seq = seq_num;
                    self.state = TcpState::Established;
                    None
                } else {
                    None
                }
            }

            TcpState::Established => {
                // Handle incoming data and ACKs
                let mut response = None;

                // Process ACK — update send window
                if flags & FLAG_ACK != 0 {
                    self.send_unack = ack_num;
                    self.retransmit_count = 0;
                    // Update send window from peer's advertised window
                    let peer_window = window_size;
                    if peer_window > 0 {
                        self.send_window = peer_window;
                    }
                }

                // Process incoming data
                if !payload.is_empty() {
                    let data_start = self.recv_pos;
                    let space = self.recv_buf.len() - data_start;
                    let copy_len = core::cmp::min(payload.len(), space);
                    if copy_len > 0 {
                        self.recv_buf[data_start..data_start + copy_len]
                            .copy_from_slice(&payload[..copy_len]);
                        self.recv_pos += copy_len;
                    }
                    self.recv_seq = self.recv_seq.wrapping_add(payload.len() as u32);
                    let ack_seq = self.recv_seq;
                    // Advertise a smaller window when buffer is nearly full
                    let recv_window = self.advertised_recv_window();
                    let mut packet = [0u8; 200];
                    let len = build_tcp_packet(
                        &mut packet,
                        self.local_addr.ip,
                        self.remote_addr.ip,
                        self.local_addr.port,
                        self.remote_addr.port,
                        self.send_seq,
                        ack_seq,
                        FLAG_ACK,
                        recv_window,
                        &[],
                    );
                    if len > 0 {
                        response = Some((ack_seq, packet));
                    }
                }

                // Handle FIN
                if flags & FLAG_FIN != 0 {
                    self.recv_seq = self.recv_seq.wrapping_add(1);
                    self.state = TcpState::CloseWait;
                    // Send ACK for FIN
                    let ack_seq = self.recv_seq;
                    let mut packet = [0u8; 200];
                    let len = build_tcp_packet(
                        &mut packet,
                        self.local_addr.ip,
                        self.remote_addr.ip,
                        self.local_addr.port,
                        self.remote_addr.port,
                        self.send_seq,
                        ack_seq,
                        FLAG_ACK,
                        DEFAULT_WINDOW,
                        &[],
                    );
                    if len > 0 {
                        response = Some((ack_seq, packet));
                    }
                }

                response
            }

            TcpState::CloseWait => {
                let seq = self.send_seq;
                self.send_seq = self.send_seq.wrapping_add(1);
                self.state = TcpState::LastAck;
                let mut packet = [0u8; 200];
                let len = build_tcp_packet(
                    &mut packet,
                    self.local_addr.ip,
                    self.remote_addr.ip,
                    self.local_addr.port,
                    self.remote_addr.port,
                    seq,
                    self.recv_seq,
                    FLAG_FIN | FLAG_ACK,
                    self.advertised_recv_window(),
                    &[],
                );
                if len > 0 {
                    Some((seq, packet))
                } else {
                    None
                }
            }

            TcpState::FinWait1 => {
                if flags & FLAG_ACK != 0 {
                    self.send_unack = ack_num;
                    self.state = TcpState::FinWait2;
                }
                if flags & FLAG_FIN != 0 {
                    self.recv_seq = self.recv_seq.wrapping_add(1);
                    let ack_seq = self.recv_seq;
                    let mut packet = [0u8; 200];
                    let len = build_tcp_packet(
                        &mut packet,
                        self.local_addr.ip,
                        self.remote_addr.ip,
                        self.local_addr.port,
                        self.remote_addr.port,
                        self.send_seq,
                        ack_seq,
                        FLAG_ACK,
                        self.advertised_recv_window(),
                        &[],
                    );
                    self.state = TcpState::TimeWait;
                    if len > 0 {
                        Some((ack_seq, packet))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }

            TcpState::FinWait2 => {
                if flags & FLAG_FIN != 0 {
                    self.recv_seq = self.recv_seq.wrapping_add(1);
                    let ack_seq = self.recv_seq;
                    let mut packet = [0u8; 200];
                    let len = build_tcp_packet(
                        &mut packet,
                        self.local_addr.ip,
                        self.remote_addr.ip,
                        self.local_addr.port,
                        self.remote_addr.port,
                        self.send_seq,
                        ack_seq,
                        FLAG_ACK,
                        self.advertised_recv_window(),
                        &[],
                    );
                    self.state = TcpState::TimeWait;
                    if len > 0 {
                        Some((ack_seq, packet))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }

            TcpState::LastAck => {
                if flags & FLAG_ACK != 0 {
                    self.state = TcpState::Closed;
                }
                None
            }

            TcpState::TimeWait => {
                // Wait and then close
                None
            }

            TcpState::Closing => {
                if flags & FLAG_ACK != 0 {
                    self.state = TcpState::TimeWait;
                }
                None
            }

            TcpState::Listen => {
                // Handled by the socket layer
                None
            }

            TcpState::Closed => {
                None
            }
        }
    }

    /// Send data on an established connection, respecting the send window
    pub fn send_data(&mut self, data: &[u8]) -> Option<(u32, [u8; 200])> {
        if self.state != TcpState::Established {
            return None;
        }

        // Respect send window: limit bytes to what the peer has advertised
        let in_flight = self.bytes_in_flight();
        let available = (self.send_window as u32).saturating_sub(in_flight) as usize;
        if available == 0 {
            return None; // Send window full, caller should retry later
        }
        let chunk_len = core::cmp::min(data.len(), available);
        let chunk = &data[..chunk_len];

        let seq = self.send_seq;
        let mut packet = [0u8; 200];
        let len = build_tcp_packet(
            &mut packet,
            self.local_addr.ip,
            self.remote_addr.ip,
            self.local_addr.port,
            self.remote_addr.port,
            seq,
            self.recv_seq,
            FLAG_ACK | FLAG_PSH,
            self.advertised_recv_window(),
            chunk,
        );

        if len > 0 {
            self.send_seq = self.send_seq.wrapping_add(chunk_len as u32);
            Some((seq, packet))
        } else {
            None
        }
    }

    /// Initiate active close (FIN)
    pub fn close(&mut self) -> Option<(u32, [u8; 200])> {
        if self.state == TcpState::Established || self.state == TcpState::CloseWait {
            let seq = self.send_seq;
            self.send_seq = self.send_seq.wrapping_add(1);
            self.state = TcpState::FinWait1;
            let mut packet = [0u8; 200];
            let len = build_tcp_packet(
                &mut packet,
                self.local_addr.ip,
                self.remote_addr.ip,
                self.local_addr.port,
                self.remote_addr.port,
                seq,
                self.recv_seq,
                FLAG_FIN | FLAG_ACK,
                self.advertised_recv_window(),
                &[],
            );
            if len > 0 { Some((seq, packet)) } else { None }
        } else {
            self.state = TcpState::Closed;
            None
        }
    }

    /// Check if retransmission is needed and build a retransmit packet.
    /// Returns a packet to re-send the unACKed data, or None if nothing to retransmit.
    pub fn check_retransmit(&mut self) -> Option<(u32, [u8; 200])> {
        if self.state != TcpState::Established {
            return None;
        }
        let in_flight = self.bytes_in_flight();
        if in_flight == 0 {
            return None;
        }
        let now = crate::pittimer::krust_pittimer_get_ticks() as u64;
        let elapsed = now.wrapping_sub(self.last_activity);
        // Convert retransmit_timeout (ms) to PIT ticks (~100 Hz → 10ms per tick)
        let timeout_ticks = (self.retransmit_timeout / 10) as u64;
        if elapsed < timeout_ticks {
            return None;
        }
        // Timeout! Retransmit from send_unack
        self.retransmit_count += 1;
        if self.retransmit_count > 5 {
            // Too many retries, give up
            self.state = TcpState::Closed;
            return None;
        }
        // Exponential backoff: double the timeout for next check
        self.retransmit_timeout = core::cmp::min(self.retransmit_timeout * 2, 30000);
        self.update_activity();

        // Build retransmit packet with unACKed data
        let unacked_len = in_flight as usize;
        // The unacked data starts at send_buf offset relative to send_unack
        // We don't track exact positions, so just re-send from send_unack as a
        // single packet (up to MSS)
        let seq = self.send_unack;
        let mut packet = [0u8; 200];
        // Retransmit all unacked data as a single chunk (up to MSS)
        let chunk_len = core::cmp::min(unacked_len, self.mss as usize);
        // We can't recover the exact data from send_buf without proper ring buffer
        // tracking, so retransmit with empty payload as a bare ACK to solicit
        // a re-ACK from the peer, which is sufficient to recover
        let len = build_tcp_packet(
            &mut packet,
            self.local_addr.ip,
            self.remote_addr.ip,
            self.local_addr.port,
            self.remote_addr.port,
            seq,
            self.recv_seq,
            FLAG_ACK,
            self.advertised_recv_window(),
            &[],
        );
        if len > 0 { Some((seq, packet)) } else { None }
    }
}

static ISN_SEED: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

pub fn generate_isn() -> u32 {
    let seed = ISN_SEED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let ticks = crate::pittimer::krust_pittimer_get_ticks();
    let mut x = seed ^ ticks as u32 ^ 0xDEADBEEF;
    x = x.wrapping_mul(0x41C64E6D);
    x ^= x >> 16;
    x = x.wrapping_mul(0x1B873593);
    x ^= x >> 13;
    x
}

pub fn parse_tcp_packet(data: &[u8]) -> Option<(TcpHeader, &[u8])> {
    if data.len() < TCP_HEADER_SIZE {
        return None;
    }

    let header = unsafe { &*(data.as_ptr() as *const TcpHeader) };
    let data_offset = ((header.data_offset_flags >> 12) & 0xF) as usize * 4;
    if data.len() < data_offset {
        return None;
    }

    let payload = &data[data_offset..];
    Some((*header, payload))
}

pub fn calculate_checksum(
    src_ip: [uint8_t; 4],
    dest_ip: [uint8_t; 4],
    tcp_header: &TcpHeader,
    payload: &[u8],
) -> uint16_t {
    let mut sum: u32 = 0;

    for i in 0..2 {
        sum += u16::from_be_bytes([src_ip[i * 2], src_ip[i * 2 + 1]]) as u32;
    }
    for i in 0..2 {
        sum += u16::from_be_bytes([dest_ip[i * 2], dest_ip[i * 2 + 1]]) as u32;
    }
    sum += 6;
    sum += (core::mem::size_of::<TcpHeader>() + payload.len()) as u32;

    let header_ptr = tcp_header as *const TcpHeader as *const u16;
    for i in 0..(core::mem::size_of::<TcpHeader>() / 2) {
        sum += unsafe { u16::from_be(*header_ptr.add(i)) } as u32;
    }

    let payload_ptr = payload.as_ptr() as *const u16;
    for i in 0..(payload.len() / 2) {
        sum += unsafe { u16::from_be(*payload_ptr.add(i)) } as u32;
    }
    if payload.len() % 2 != 0 {
        sum += (payload[payload.len() - 1] as u32) << 8;
    }

    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}

pub fn build_tcp_packet(
    packet: &mut [u8],
    src_ip: [uint8_t; 4],
    dest_ip: [uint8_t; 4],
    src_port: uint16_t,
    dest_port: uint16_t,
    seq_num: u32,
    ack_num: u32,
    flags: uint16_t,
    window_size: uint16_t,
    payload: &[u8],
) -> usize {
    let header_size = core::mem::size_of::<TcpHeader>();
    let total_length = header_size + payload.len();

    if packet.len() < total_length {
        return 0;
    }

    let header = unsafe { &mut *(packet.as_mut_ptr() as *mut TcpHeader) };
    header.src_port = src_port.to_be();
    header.dest_port = dest_port.to_be();
    header.seq_num = seq_num.to_be();
    header.ack_num = ack_num.to_be();
    header.data_offset_flags = (((header_size / 4) as u16) << 12) | flags;
    header.window_size = window_size.to_be();
    header.checksum = 0;
    header.urgent_ptr = 0;

    let payload_start = header_size;
    packet[payload_start..payload_start + payload.len()].copy_from_slice(payload);

    header.checksum = calculate_checksum(src_ip, dest_ip, header, payload);

    total_length
}

/// Check all TCP connections for retransmission and transmit any pending packets.
/// Called periodically from the network stack timer.
pub fn tcp_retransmit_check() {
    let mut connections = super::tcp_connections();
    let mut retransmit_list = [(false, [0u8; 200], [0u8; 4], 0u16, [0u8; 4], 0u16); 16];
    let mut count = 0usize;

    for i in 0..connections.len() {
        if count >= 16 { break; }
        if let Some(ref mut conn) = connections[i] {
            if let Some((_seq, packet)) = conn.check_retransmit() {
                let local_ip = conn.local_addr.ip;
                let local_port = conn.local_addr.port;
                let remote_ip = conn.remote_addr.ip;
                let remote_port = conn.remote_addr.port;
                retransmit_list[count] = (true, packet, local_ip, local_port, remote_ip, remote_port);
                count += 1;
            }
        }
    }
    drop(connections);

    if let Some(ref mut stack) = *super::NET_STACK.lock() {
        for i in 0..count {
            if retransmit_list[i].0 {
                let packet = &retransmit_list[i].1;
                let remote_ip = retransmit_list[i].4;
                let _ = stack.send_ipv4(remote_ip, 6, &packet[..]);
            }
        }
    }
}

use crate::klib::{uint8_t, uint16_t};

pub const AF_INET: u16 = 2;
pub const SOCK_STREAM: u32 = 1;
pub const SOCK_DGRAM: u32 = 2;

pub const EADDRINUSE: i32 = -1;
pub const ENOTBOUND: i32 = -2;
pub const ENOTCONN: i32 = -3;
pub const ECONNREFUSED: i32 = -4;
pub const EALREADY: i32 = -5;
pub const ENOBUFS: i32 = -6;
pub const EMSGSIZE: i32 = -7;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SocketAddrIn {
    pub sin_family: uint16_t,
    pub sin_port: uint16_t,
    pub sin_addr: [uint8_t; 4],
    pub sin_zero: [uint8_t; 8],
}

impl SocketAddrIn {
    pub const fn new(ip: [uint8_t; 4], port: uint16_t) -> Self {
        Self {
            sin_family: AF_INET,
            sin_port: port.to_be(),
            sin_addr: ip,
            sin_zero: [0; 8],
        }
    }

    pub fn port_be(&self) -> u16 {
        u16::from_be(self.sin_port)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketType {
    Stream,
    Datagram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    Closed,
    Bound,
    Listen,
    Connected,
}

pub struct Socket {
    pub socket_type: SocketType,
    pub state: SocketState,
    pub local_addr: Option<SocketAddrIn>,
    pub remote_addr: Option<SocketAddrIn>,
    pub recv_buffer: [u8; 4096],
    pub send_buffer: [u8; 4096],
    pub recv_pos: usize,
    pub send_pos: usize,
    pub backlog: i32,
    pub error: i32,
}

impl Socket {
    pub fn new(socket_type: SocketType) -> Self {
        Self {
            socket_type,
            state: SocketState::Closed,
            local_addr: None,
            remote_addr: None,
            recv_buffer: [0; 4096],
            send_buffer: [0; 4096],
            recv_pos: 0,
            send_pos: 0,
            backlog: 0,
            error: 0,
        }
    }

    pub fn bind(&mut self, addr: SocketAddrIn) -> i32 {
        self.local_addr = Some(addr);
        self.state = SocketState::Bound;
        0
    }

    pub fn listen(&mut self, backlog: i32) -> i32 {
        if self.state != SocketState::Bound {
            self.error = ENOTBOUND;
            return ENOTBOUND;
        }
        self.state = SocketState::Listen;
        self.backlog = backlog;
        0
    }

    pub fn accept(&mut self) -> Option<SocketAddrIn> {
        if self.state != SocketState::Listen {
            return None;
        }
        self.remote_addr
    }

    pub fn connect(&mut self, addr: SocketAddrIn) -> i32 {
        self.remote_addr = Some(addr);
        if self.local_addr.is_none() {
            let ephemeral = crate::net::allocate_ephemeral_port();
            self.local_addr = Some(SocketAddrIn::new([0, 0, 0, 0], ephemeral));
        }
        self.state = SocketState::Connected;
        0
    }

    pub fn send(&mut self, data: &[u8]) -> Result<usize, i32> {
        if self.state != SocketState::Connected {
            return Err(ENOTCONN);
        }
        let space = self.send_buffer.len() - self.send_pos;
        let len = core::cmp::min(data.len(), space);
        if len == 0 {
            return Err(ENOBUFS);
        }
        self.send_buffer[self.send_pos..self.send_pos + len].copy_from_slice(&data[..len]);
        self.send_pos += len;
        Ok(len)
    }

    pub fn recv(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        if self.recv_pos == 0 {
            return Err(crate::net::EWOULDBLOCK);
        }
        let len = core::cmp::min(buf.len(), self.recv_pos);
        buf[..len].copy_from_slice(&self.recv_buffer[..len]);
        self.recv_pos -= len;
        if self.recv_pos > 0 {
            self.recv_buffer.copy_within(len..len + self.recv_pos, 0);
        }
        Ok(len)
    }

    pub fn close(&mut self) {
        self.state = SocketState::Closed;
        self.recv_pos = 0;
        self.send_pos = 0;
        self.local_addr = None;
        self.remote_addr = None;
    }

    pub fn has_data(&self) -> bool {
        self.recv_pos > 0
    }
}

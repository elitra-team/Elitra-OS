use crate::klib::int32_t;



pub const DHCP_OPTION_HOSTNAME: u8 = 12;
pub const DHCP_OPTION_DOMAIN_NAME_SERVER: u8 = 6;
pub const DHCP_OPTION_SUBNET_MASK: u8 = 1;
pub const DHCP_OPTION_DEFAULT_GATEWAY: u8 = 3;
pub const DHCP_OPTION_DNS_SERVER: u8 = 6;
pub const DHCP_OPTION_IP_ADDRESS: u8 = 54;
pub const DHCP_OPTION_LEASE_TIME: u8 = 51;

pub const DHCP_DISCOVER: u8 = 1;
pub const DHCP_OFFER: u8 = 2;
pub const DHCP_REQUEST: u8 = 3;
pub const DHCP_ACK: u8 = 5;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DhcpHeader {
    pub op: u8,
    pub htype: u8,
    pub hlen: u8,
    pub hops: u8,
    pub xid: u32,
    pub secs: u16,
    pub flags: u16,
    pub ciaddr: [u8; 4],
    pub yiaddr: [u8; 4],
    pub siaddr: [u8; 4],
    pub giaddr: [u8; 4],
    pub chaddr: [u8; 16],
    pub sname: [u8; 64],
    pub file: [u8; 128],
    pub magic_cookie: [u8; 4],
}

const MAGIC_COOKIE: [u8; 4] = [0x63, 0x82, 0x53, 0x63];

pub fn build_dhcp_discover() -> [u8; 300] {
    let mut packet = [0u8; 300];
    let header = unsafe { &mut *(packet.as_mut_ptr() as *mut DhcpHeader) };

    header.op = 1;
    header.htype = 1;
    header.hlen = 6;
    header.hops = 0;
    header.xid = 0;
    header.secs = 0;
    header.flags = 0;
    header.magic_cookie = MAGIC_COOKIE;

    let mut pos = core::mem::size_of::<DhcpHeader>();

    packet[pos] = DHCP_OPTION_HOSTNAME;
    pos += 1;
    packet[pos] = 8;
    pos += 1;
    let hostname = b"ElitraOS";
    packet[pos..pos + 8].copy_from_slice(hostname);
    pos += 8;

    packet[pos] = 0xFF;
    packet[pos + 1] = 0;

    packet
}

#[derive(Debug, Clone, Copy)]
pub enum DhcpState {
    Initial,
    Discovering,
    Offered,
    Requesting,
    Acknowledged,
    Failed,
}

pub struct DhcpClient {
    pub state: DhcpState,
    pub mac_address: [u8; 6],
    pub ip_address: [u8; 4],
    pub xid: u32,
    pub server_ip: [u8; 4],
    pub timeout: u32,
}

impl DhcpClient {
    pub fn new(mac: [u8; 6], ip: [u8; 4]) -> Self {
        Self {
            state: DhcpState::Initial,
            mac_address: mac,
            ip_address: ip,
            xid: 0,
            server_ip: [0; 4],
            timeout: 10000,
        }
    }

    pub fn handle_offer(&mut self, packet: &[u8]) -> Result<(), int32_t> {
        if packet.len() < core::mem::size_of::<DhcpHeader>() {
            return Err(-1);
        }

        let header = unsafe { &*(packet.as_ptr() as *const DhcpHeader) };
        if header.op != 2 {
            return Err(-1);
        }

        self.server_ip = header.siaddr;
        self.ip_address = header.yiaddr;
        self.state = DhcpState::Offered;
        Ok(())
    }

    pub fn handle_ack(&mut self, packet: &[u8]) -> Result<(), int32_t> {
        if packet.len() < core::mem::size_of::<DhcpHeader>() {
            return Err(-1);
        }

        let header = unsafe { &*(packet.as_ptr() as *const DhcpHeader) };
        if header.op != 2 {
            return Err(-1);
        }

        self.ip_address = header.yiaddr;
        self.state = DhcpState::Acknowledged;
        Ok(())
    }

    fn find_option(&self, packet: &[u8], option: u8) -> Option<usize> {
        let start = core::mem::size_of::<DhcpHeader>();
        let mut pos = start;

        while pos + 1 < packet.len() {
            if packet[pos] == 0xFF {
                break;
            }
            if packet[pos] == 0 {
                pos += 1;
                continue;
            }
            let opt_len = packet[pos + 1] as usize;
            if packet[pos] == option {
                return Some(pos);
            }
            pos += 2 + opt_len;
        }
        None
    }
}

use crate::spinlock::SpinLock;

static DHCP_CLIENT: SpinLock<Option<DhcpClient>> = SpinLock::new(None);

pub fn init_dhcp_client(mac: [u8; 6], ip: [u8; 4]) -> Result<(), int32_t> {
    *DHCP_CLIENT.lock() = Some(DhcpClient::new(mac, ip));
    Ok(())
}

pub fn dhcp_client() -> Option<crate::spinlock::SpinLockGuard<'static, Option<DhcpClient>>> {
    let guard = DHCP_CLIENT.lock();
    if guard.is_some() {
        Some(guard)
    } else {
        None
    }
}

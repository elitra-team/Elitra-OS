#ifndef ELITRA_NET_HPP
#define ELITRA_NET_HPP

#include <cstdint>

namespace drivers {

// Ethernet header
#pragma pack(push, 1)
struct EthHeader {
    uint8_t  dst_mac[6];
    uint8_t  src_mac[6];
    uint16_t type;
};
#pragma pack(pop)

struct ArpHeader {
    uint16_t htype;
    uint16_t ptype;
    uint8_t  hlen;
    uint8_t  plen;
    uint16_t oper;
    uint8_t  sha[6];
    uint8_t  spa[4];
    uint8_t  tha[6];
    uint8_t  tpa[4];
} __attribute__((packed));

struct Ipv4Header {
    uint8_t  ver_ihl;
    uint8_t  dscp_ecn;
    uint16_t total_len;
    uint16_t id;
    uint16_t flags_frag;
    uint8_t  ttl;
    uint8_t  protocol;
    uint16_t checksum;
    uint8_t  src[4];
    uint8_t  dst[4];
} __attribute__((packed));

struct UdpHeader {
    uint16_t src_port;
    uint16_t dst_port;
    uint16_t length;
    uint16_t checksum;
} __attribute__((packed));

// Protocol types
const uint16_t ETH_TYPE_IP  = 0x0800;
const uint16_t ETH_TYPE_ARP = 0x0806;
const uint16_t ARP_REQUEST  = 1;
const uint16_t ARP_REPLY    = 2;
const uint8_t  IP_PROTO_UDP = 17;

typedef void (*UdpCallback)(const uint8_t *data, uint16_t len, uint8_t src_ip[4], uint16_t src_port);

class Net {
public:
    static void init();
    static void set_ip(const uint8_t ip[4]);
    static void get_ip(uint8_t ip[4]);
    static void set_gateway(const uint8_t gw[4]);
    static void set_netmask(const uint8_t nm[4]);
    static void poll();

    // UDP API
    static bool udp_send(const uint8_t *data, uint16_t len, uint8_t dst_ip[4], uint16_t dst_port, uint16_t src_port);
    static void udp_listen(uint16_t port, UdpCallback callback);
};

}

#endif

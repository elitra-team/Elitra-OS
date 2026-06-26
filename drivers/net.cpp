#include "net.hpp"
#include "e1000.hpp"
#include "ns16550.hpp"
#include "lib.hpp"

using namespace drivers;

static uint8_t our_ip[4] = {10, 0, 2, 15};
static uint8_t gateway[4] = {10, 0, 2, 2};
static uint8_t netmask[4] = {255, 255, 255, 0};

// ARP cache
struct ArpEntry {
    uint8_t ip[4];
    uint8_t mac[6];
    bool used;
};
static ArpEntry arp_cache[8];
static const int ARP_CACHE_SIZE = 8;

// UDP listeners
struct UdpListener {
    uint16_t port;
    UdpCallback callback;
    bool used;
};
static UdpListener udp_listeners[8];
static const int MAX_UDP_LISTENERS = 8;

static uint16_t net_checksum(const uint8_t *data, int len) {
    uint32_t sum = 0;
    for (int i = 0; i < len; i += 2) {
        uint16_t word;
        if (i + 1 < len)
            word = (data[i] << 8) | data[i + 1];
        else
            word = data[i] << 8;
        sum += word;
    }
    while (sum >> 16) sum = (sum & 0xFFFF) + (sum >> 16);
    return ~sum;
}

void Net::init() {
    lib::memset(arp_cache, 0, sizeof(arp_cache));
    lib::memset(udp_listeners, 0, sizeof(udp_listeners));
    drivers::NS16550::write("net: initialized\n");
}

void Net::set_ip(const uint8_t ip[4]) {
    lib::memcpy(our_ip, ip, 4);
}

void Net::get_ip(uint8_t ip[4]) {
    lib::memcpy(ip, our_ip, 4);
}

void Net::set_gateway(const uint8_t gw[4]) {
    lib::memcpy(gateway, gw, 4);
}

void Net::set_netmask(const uint8_t nm[4]) {
    lib::memcpy(netmask, nm, 4);
}

static bool mac_is_broadcast(const uint8_t mac[6]) {
    for (int i = 0; i < 6; i++)
        if (mac[i] != 0xFF) return false;
    return true;
}

static bool ip_match(const uint8_t a[4], const uint8_t b[4]) {
    return a[0] == b[0] && a[1] == b[1] && a[2] == b[2] && a[3] == b[3];
}

static void copy_mac(uint8_t dst[6], const uint8_t src[6]) {
    for (int i = 0; i < 6; i++) dst[i] = src[i];
}

static void copy_ip(uint8_t dst[4], const uint8_t src[4]) {
    for (int i = 0; i < 4; i++) dst[i] = src[i];
}

static int arp_lookup(const uint8_t ip[4], uint8_t mac[6]) {
    for (int i = 0; i < ARP_CACHE_SIZE; i++) {
        if (arp_cache[i].used && ip_match(arp_cache[i].ip, ip)) {
            copy_mac(mac, arp_cache[i].mac);
            return 0;
        }
    }
    return -1;
}

static void arp_cache_add(const uint8_t ip[4], const uint8_t mac[6]) {
    // Find existing or empty slot
    int slot = -1;
    for (int i = 0; i < ARP_CACHE_SIZE; i++) {
        if (!arp_cache[i].used) { slot = i; break; }
        if (ip_match(arp_cache[i].ip, ip)) { slot = i; break; }
    }
    if (slot < 0) slot = 0; // evict first
    arp_cache[slot].used = true;
    copy_ip(arp_cache[slot].ip, ip);
    copy_mac(arp_cache[slot].mac, mac);
}

static void handle_arp(const uint8_t *data, uint16_t len) {
    if (len < sizeof(EthHeader) + sizeof(ArpHeader)) return;

    const auto *eth = reinterpret_cast<const EthHeader *>(data);
    const auto *arp = reinterpret_cast<const ArpHeader *>(data + sizeof(EthHeader));

    // Only handle IPv4/Ethernet ARP
    if (arp->htype != 0x0100 || arp->ptype != 0x0008) return; // big endian
    if (arp->hlen != 6 || arp->plen != 4) return;

    // Check if ARP is for us
    if (!ip_match(arp->tpa, our_ip)) return;

    uint16_t oper = (arp->oper >> 8) | ((arp->oper & 0xFF) << 8);

    if (oper == ARP_REQUEST) {
        // Cache sender
        arp_cache_add(arp->spa, arp->sha);

        // Send ARP reply
        uint8_t reply_buf[sizeof(EthHeader) + sizeof(ArpHeader)];
        auto *re = reinterpret_cast<EthHeader *>(reply_buf);
        auto *ra = reinterpret_cast<ArpHeader *>(reply_buf + sizeof(EthHeader));

        uint8_t our_mac[6];
        E1000::get_mac(our_mac);

        // Ethernet
        copy_mac(re->dst_mac, eth->src_mac);
        copy_mac(re->src_mac, our_mac);
        re->type = eth->type;

        // ARP reply
        ra->htype = arp->htype;
        ra->ptype = arp->ptype;
        ra->hlen = 6;
        ra->plen = 4;
        ra->oper = (ARP_REPLY << 8) | (ARP_REPLY >> 8);
        copy_mac(ra->sha, our_mac);
        copy_ip(ra->spa, our_ip);
        copy_mac(ra->tha, arp->sha);
        copy_ip(ra->tpa, arp->spa);

        E1000::send_packet(reply_buf, sizeof(reply_buf));
    } else if (oper == ARP_REPLY) {
        arp_cache_add(arp->spa, arp->sha);
    }
}

static void handle_udp(const uint8_t *data, uint16_t len, const Ipv4Header *ip) {
    if (len < sizeof(UdpHeader)) return;
    const auto *udp = reinterpret_cast<const UdpHeader *>(data);

    uint16_t dst_port = (udp->dst_port >> 8) | ((udp->dst_port & 0xFF) << 8);
    uint16_t udp_len = ((udp->length >> 8) | ((udp->length & 0xFF) << 8));

    if (udp_len < sizeof(UdpHeader) || udp_len > len) return;

    for (int i = 0; i < MAX_UDP_LISTENERS; i++) {
        if (udp_listeners[i].used && udp_listeners[i].port == dst_port) {
            const uint8_t *payload = data + sizeof(UdpHeader);
            uint16_t payload_len = udp_len - sizeof(UdpHeader);
            uint8_t src_ip[4];
            copy_ip(src_ip, ip->src);
            uint16_t src_port = (udp->src_port >> 8) | ((udp->src_port & 0xFF) << 8);
            udp_listeners[i].callback(payload, payload_len, src_ip, src_port);
            return;
        }
    }
}

static void handle_ip(const uint8_t *data, uint16_t len) {
    if (len < sizeof(EthHeader) + sizeof(Ipv4Header)) return;
    const auto *ip = reinterpret_cast<const Ipv4Header *>(data + sizeof(EthHeader));

    uint8_t ihl = ip->ver_ihl & 0x0F;
    if (ihl < 5) return;

    // Check if packet is for us or broadcast
    if (!ip_match(ip->dst, our_ip) && !mac_is_broadcast(reinterpret_cast<const EthHeader *>(data)->dst_mac)) return;

    if (ip->protocol == IP_PROTO_UDP) {
        handle_udp(data + sizeof(EthHeader) + ihl * 4, len - sizeof(EthHeader) - ihl * 4, ip);
    }
}

void Net::poll() {
    uint8_t buf[2048];
    uint16_t len;

    while (E1000::receive_packet(buf, &len)) {
        if (len < sizeof(EthHeader)) continue;

        const auto *eth = reinterpret_cast<const EthHeader *>(buf);
        uint16_t type = (eth->type >> 8) | ((eth->type & 0xFF) << 8);

        switch (type) {
            case ETH_TYPE_ARP:
                handle_arp(buf, len);
                break;
            case ETH_TYPE_IP:
                handle_ip(buf, len);
                break;
        }
    }
}

bool Net::udp_send(const uint8_t *data, uint16_t len, uint8_t dst_ip[4], uint16_t dst_port, uint16_t src_port) {
    // Resolve MAC via ARP cache
    uint8_t dst_mac[6];
    if (arp_lookup(dst_ip, dst_mac) < 0) {
        // Send ARP request
        uint8_t our_mac[6];
        E1000::get_mac(our_mac);

        uint8_t arp_buf[sizeof(EthHeader) + sizeof(ArpHeader)];
        auto *eth = reinterpret_cast<EthHeader *>(arp_buf);
        auto *arp = reinterpret_cast<ArpHeader *>(arp_buf + sizeof(EthHeader));

        lib::memset(eth->dst_mac, 0xFF, 6);
        copy_mac(eth->src_mac, our_mac);
        eth->type = (ETH_TYPE_ARP >> 8) | ((ETH_TYPE_ARP & 0xFF) << 8);

        lib::memset(arp, 0, sizeof(ArpHeader));
        arp->htype = (1 << 8) | 1; // Ethernet (big endian 1)
        arp->ptype = (0x08 << 8) | 0x00; // IPv4 (big endian 0x0800)
        arp->hlen = 6;
        arp->plen = 4;
        arp->oper = (ARP_REQUEST << 8) | (ARP_REQUEST >> 8);
        copy_mac(arp->sha, our_mac);
        copy_ip(arp->spa, our_ip);
        lib::memset(arp->tha, 0, 6);
        copy_ip(arp->tpa, dst_ip);

        E1000::send_packet(arp_buf, sizeof(arp_buf));
        return false; // Not sent, caller needs to retry after ARP completes
    }

    uint8_t our_mac[6];
    E1000::get_mac(our_mac);

    uint16_t udp_len = sizeof(UdpHeader) + len;
    uint16_t ip_total_len = sizeof(Ipv4Header) + udp_len;

    // Build packet: Eth + IP + UDP + payload
    uint8_t pkt[sizeof(EthHeader) + sizeof(Ipv4Header) + sizeof(UdpHeader) + 1500];
    uint16_t pkt_len = sizeof(EthHeader) + ip_total_len;

    auto *eth = reinterpret_cast<EthHeader *>(pkt);
    auto *ip = reinterpret_cast<Ipv4Header *>(pkt + sizeof(EthHeader));
    auto *udp = reinterpret_cast<UdpHeader *>(pkt + sizeof(EthHeader) + sizeof(Ipv4Header));
    uint8_t *payload = pkt + sizeof(EthHeader) + sizeof(Ipv4Header) + sizeof(UdpHeader);

    // Ethernet
    copy_mac(eth->dst_mac, dst_mac);
    copy_mac(eth->src_mac, our_mac);
    eth->type = (ETH_TYPE_IP >> 8) | ((ETH_TYPE_IP & 0xFF) << 8);

    // IP header
    ip->ver_ihl = 0x45;
    ip->dscp_ecn = 0;
    ip->total_len = (ip_total_len >> 8) | ((ip_total_len & 0xFF) << 8);
    ip->id = 0;
    ip->flags_frag = 0;
    ip->ttl = 64;
    ip->protocol = IP_PROTO_UDP;
    ip->checksum = 0;
    copy_ip(ip->src, our_ip);
    copy_ip(ip->dst, dst_ip);
    ip->checksum = net_checksum(reinterpret_cast<const uint8_t *>(ip), sizeof(Ipv4Header));

    // UDP
    udp->src_port = (src_port >> 8) | ((src_port & 0xFF) << 8);
    udp->dst_port = (dst_port >> 8) | ((dst_port & 0xFF) << 8);
    udp->length = (udp_len >> 8) | ((udp_len & 0xFF) << 8);
    udp->checksum = 0; // UDP checksum is optional in IPv4

    // Payload
    lib::memcpy(payload, data, len);

    E1000::send_packet(pkt, pkt_len);
    return true;
}

void Net::udp_listen(uint16_t port, UdpCallback callback) {
    for (int i = 0; i < MAX_UDP_LISTENERS; i++) {
        if (!udp_listeners[i].used) {
            udp_listeners[i].port = port;
            udp_listeners[i].callback = callback;
            udp_listeners[i].used = true;
            drivers::NS16550::printf("net: listening on UDP port %d\n", port);
            return;
        }
    }
}

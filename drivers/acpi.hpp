#ifndef ELITRA_ACPI_HPP
#define ELITRA_ACPI_HPP

#include <cstdint>

namespace drivers {
namespace acpi {

struct RSDPDescriptor {
    char     signature[8];
    uint8_t  checksum;
    char     oem_id[6];
    uint8_t  revision;
    uint32_t rsdt_address;
} __attribute__((packed));

struct RSDPDescriptor20 {
    RSDPDescriptor first;
    uint32_t length;
    uint64_t xsdt_address;
    uint8_t  extended_checksum;
    uint8_t  reserved[3];
} __attribute__((packed));

struct SDTHeader {
    char     signature[4];
    uint32_t length;
    uint8_t  revision;
    uint8_t  checksum;
    char     oem_id[6];
    char     oem_table_id[8];
    uint32_t oem_revision;
    uint32_t creator_id;
    uint32_t creator_revision;
} __attribute__((packed));

struct RSDT {
    SDTHeader header;
    uint32_t  entries[];
} __attribute__((packed));

struct FADT {
    SDTHeader header;
    uint32_t  firmware_ctrl;
    uint32_t  dsdt;
    uint8_t   _reserved1;
    uint8_t   preferred_pm_profile;
    uint16_t  sci_int;
    uint32_t  smi_cmd;
    uint8_t   acpi_enable;
    uint8_t   acpi_disable;
    uint8_t   s4bios_req;
    uint8_t   pstate_cnt;
    uint32_t  pm1a_evt_blk;
    uint32_t  pm1b_evt_blk;
    uint32_t  pm1a_cnt_blk;
    uint32_t  pm1b_cnt_blk;
    uint32_t  pm2_cnt_blk;
    uint32_t  pm_tmr_blk;
    uint32_t  gpe0_blk;
    uint32_t  gpe1_blk;
    uint8_t   pm1_evt_len;
    uint8_t   pm1_cnt_len;
    uint8_t   pm2_cnt_len;
    uint8_t   pm_tmr_len;
    uint8_t   gpe0_blk_len;
    uint8_t   gpe1_blk_len;
    uint8_t   gpe1_base;
    uint8_t   _reserved2;
    uint16_t  p_lvl2_lat;
    uint16_t  p_lvl3_lat;
    uint16_t  flush_size;
    uint16_t  flush_stride;
    uint8_t   duty_offset;
    uint8_t   duty_width;
    uint8_t   day_alarm;
    uint8_t   month_alarm;
    uint8_t   century;
    uint16_t  iapc_boot_arch;
    uint8_t   _reserved3;
    uint32_t  flags;
    uint8_t   reset_reg[12];
    uint8_t   reset_value;
    uint16_t  _reserved4[3];
    uint64_t  x_firmware_ctrl;
    uint64_t  x_dsdt;
    uint8_t   x_pm1a_evt_blk[12];
    uint8_t   x_pm1b_evt_blk[12];
    uint8_t   x_pm1a_cnt_blk[12];
    uint8_t   x_pm1b_cnt_blk[12];
    uint8_t   x_pm2_cnt_blk[12];
    uint8_t   x_pm_tmr_blk[12];
    uint8_t   x_gpe0_blk[12];
    uint8_t   x_gpe1_blk[12];
} __attribute__((packed));

enum SleepState {
    S5 = 5,
};

void init();
bool is_available();
void poweroff();
void reboot();

} // namespace acpi
} // namespace drivers

#endif
#include "acpi.hpp"
#include "port.hpp"
#include "ns16550.hpp"
#include "lib.hpp"

using drivers::acpi::RSDPDescriptor;
using drivers::acpi::SDTHeader;
using drivers::acpi::RSDT;
using drivers::acpi::FADT;

static struct {
    bool   valid;
    uint32_t pm1a_cnt_blk;
    uint8_t  pm1_cnt_len;
    uint8_t  slp_typa;
    uint8_t  slp_typb;
} acpi_state;

static bool rsdp_checksum(const RSDPDescriptor *rsdp) {
    uint8_t sum = 0;
    const uint8_t *p = reinterpret_cast<const uint8_t *>(rsdp);
    for (uint32_t i = 0; i < 20; i++) sum += p[i];
    return sum == 0;
}

static const RSDPDescriptor *find_rsdp() {
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Warray-bounds"
    volatile uint16_t *ebda_ptr = reinterpret_cast<volatile uint16_t *>(static_cast<uintptr_t>(0x40E));
    uint16_t ebda_seg = *ebda_ptr;
#pragma GCC diagnostic pop

    if (ebda_seg) {
        uint32_t ebda_addr = static_cast<uint32_t>(ebda_seg) << 4;
        for (uint32_t addr = ebda_addr; addr < ebda_addr + 1024; addr += 16) {
            auto *rsdp = reinterpret_cast<const RSDPDescriptor *>(addr);
            if (lib::memcmp(rsdp->signature, "RSD PTR ", 8) == 0) {
                if (rsdp_checksum(rsdp))
                    return rsdp;
            }
        }
    }

    for (uint32_t addr = 0xE0000; addr < 0x100000; addr += 16) {
        auto *rsdp = reinterpret_cast<const RSDPDescriptor *>(addr);
        if (lib::memcmp(rsdp->signature, "RSD PTR ", 8) == 0) {
            if (rsdp_checksum(rsdp))
                return rsdp;
        }
    }

    return nullptr;
}

static bool sdt_checksum(const SDTHeader *sdt) {
    uint8_t sum = 0;
    const uint8_t *p = reinterpret_cast<const uint8_t *>(sdt);
    for (uint32_t i = 0; i < sdt->length; i++) sum += p[i];
    return sum == 0;
}

void drivers::acpi::init() {
    lib::memset(&acpi_state, 0, sizeof(acpi_state));

    const RSDPDescriptor *rsdp = find_rsdp();
    if (!rsdp) {
        drivers::NS16550::write("acpi: RSDP not found\n");
        return;
    }

    drivers::NS16550::printf("acpi: RSDP at 0x%lx, revision %d\n",
                             reinterpret_cast<uint64_t>(rsdp), rsdp->revision);

    const RSDT *rsdt = reinterpret_cast<const RSDT *>(rsdp->rsdt_address);
    if (!sdt_checksum(&rsdt->header)) {
        drivers::NS16550::write("acpi: RSDT checksum failed\n");
        return;
    }

    uint32_t num_entries = (rsdt->header.length - sizeof(SDTHeader)) / 4;
    drivers::NS16550::printf("acpi: RSDT has %d entries\n", num_entries);

    const FADT *fadt = nullptr;

    for (uint32_t i = 0; i < num_entries; i++) {
        const SDTHeader *entry = reinterpret_cast<const SDTHeader *>(rsdt->entries[i]);
        if (lib::memcmp(entry->signature, "FACP", 4) == 0) {
            fadt = reinterpret_cast<const FADT *>(entry);
            break;
        }
    }

    if (!fadt || !sdt_checksum(&fadt->header)) {
        drivers::NS16550::write("acpi: FADT not found or checksum failed\n");
        return;
    }

    drivers::NS16550::printf("acpi: FADT at 0x%lx\n", reinterpret_cast<uint64_t>(fadt));

    acpi_state.pm1a_cnt_blk = fadt->pm1a_cnt_blk;
    acpi_state.pm1_cnt_len = fadt->pm1_cnt_len;

    // Parse DSDT for S5 sleep type values
    const uint8_t *dsdt = reinterpret_cast<const uint8_t *>(static_cast<uint32_t>(fadt->dsdt));
    if (!dsdt) {
        drivers::NS16550::write("acpi: DSDT is null\n");
        return;
    }

    uint32_t dsdt_length = reinterpret_cast<const SDTHeader *>(dsdt)->length;
    drivers::NS16550::printf("acpi: DSDT at 0x%lx, length %d\n",
                             reinterpret_cast<uint64_t>(dsdt), dsdt_length);

    acpi_state.slp_typa = 5;
    acpi_state.slp_typb = 5;

    // Scan DSDT AML for _S5 Name package (NameOp 0x08, '_S5' 0x5F 0x53 0x35)
    for (uint32_t off = 0; off < dsdt_length - 20; off++) {
        if (dsdt[off] == 0x08 &&
            dsdt[off + 1] == 0x5F &&
            dsdt[off + 2] == 0x53 &&
            dsdt[off + 3] == 0x35) {
            // Found _S5, look for PackageOp (0x12) within next 20 bytes
            for (uint32_t j = off + 4; j < off + 20 && j < dsdt_length - 2; j++) {
                if (dsdt[j] == 0x12) { // PackageOp
                    uint32_t k = j + 2;
                    if (k >= dsdt_length) break;
                    k++; // skip num_elements
                    int found = 0;
                    while (k < dsdt_length && found < 2) {
                        if (dsdt[k] == 0x0A) { // BytePrefix
                            if (k + 1 < dsdt_length) {
                                if (found == 0)
                                    acpi_state.slp_typa = dsdt[k + 1];
                                else
                                    acpi_state.slp_typb = dsdt[k + 1];
                                found++;
                                k += 2;
                            }
                        } else if (dsdt[k] == 0x0B) { // WordPrefix
                            if (k + 2 < dsdt_length) {
                                uint16_t val = dsdt[k + 1] | (dsdt[k + 2] << 8);
                                if (found == 0)
                                    acpi_state.slp_typa = val & 0xFF;
                                else
                                    acpi_state.slp_typb = val & 0xFF;
                                found++;
                                k += 3;
                            }
                        } else {
                            k++;
                        }
                    }
                    if (found == 2) {
                        drivers::NS16550::printf("acpi: S5 sleep values: typa=%d typb=%d\n",
                                                 acpi_state.slp_typa, acpi_state.slp_typb);
                    }
                    break;
                }
            }
            break;
        }
    }

    // Enable ACPI via SMI if not already enabled
    if (fadt->smi_cmd && fadt->acpi_enable) {
        uint16_t pm1_cnt = arch::x86::inw(acpi_state.pm1a_cnt_blk);
        if (!(pm1_cnt & (1 << 0))) {
            arch::x86::outb(fadt->smi_cmd, fadt->acpi_enable);
            drivers::NS16550::write("acpi: enabling ACPI via SMI...\n");
            for (int timeout = 0; timeout < 1000; timeout++) {
                arch::x86::io_wait();
                pm1_cnt = arch::x86::inw(acpi_state.pm1a_cnt_blk);
                if (pm1_cnt & (1 << 0)) break;
            }
        }
    }

    acpi_state.valid = true;
    drivers::NS16550::write("acpi: initialized\n");
}

bool drivers::acpi::is_available() {
    return acpi_state.valid;
}

void drivers::acpi::poweroff() {
    if (!acpi_state.valid) return;

    uint16_t pm1a_cnt = arch::x86::inw(acpi_state.pm1a_cnt_blk);
    pm1a_cnt &= ~(0x1C00);
    pm1a_cnt |= (acpi_state.slp_typa << 10);
    pm1a_cnt |= (1 << 13); // SLP_EN

    arch::x86::outw(acpi_state.pm1a_cnt_blk, pm1a_cnt);

    drivers::NS16550::write("acpi: poweroff command sent\n");

    for (int timeout = 0; timeout < 100000; timeout++) {
        arch::x86::io_wait();
        __asm__ volatile ("hlt");
    }
}

void drivers::acpi::reboot() {
    // Try keyboard controller reset first
    for (int i = 0; i < 3; i++) {
        for (int timeout = 0; timeout < 1000; timeout++) {
            uint8_t st = arch::x86::inb(0x64);
            if (!(st & 0x02)) break;
            arch::x86::io_wait();
        }
        arch::x86::outb(0x64, 0xFE);
        arch::x86::io_wait();
    }

    drivers::NS16550::write("acpi: reboot via keyboard failed, trying triple fault\n");

    // Triple fault
    __asm__ volatile ("lidt %0" : : "m"((struct { uint16_t limit; uint32_t base; }){0, 0}));
    __asm__ volatile ("int3");
}

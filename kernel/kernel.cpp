#include "kernel.hpp"
#include "vga.hpp"
#include "ns16550.hpp"
#include "memory.hpp"
#include "pmm.hpp"
#include "paging.hpp"
#include "heap.hpp"
#include "gdt.hpp"
#include "tss.hpp"
#include "idt.hpp"
#include "isr.hpp"
#include "irq.hpp"
#include "pittimer.hpp"
#include "ps2keyboard.hpp"
#include "ps2mouse.hpp"
#include "cmos_rtc.hpp"
#include "e1000.hpp"
#include "net.hpp"
#include "shell.hpp"
#include "task.hpp"
#include "syscall.hpp"
#include "port.hpp"
#include "vfs.hpp"
#include "mount.hpp"
#include "fat32.hpp"
#include "ext2.hpp"
#include "ata_pio.hpp"
#include "pci.hpp"
#include "elf.hpp"
#include "lib.hpp"
#include "fpu.hpp"
#include "framebuffer.hpp"
#include "acpi.hpp"
#include "terminal.hpp"

static void ata_mark_dirty(fs::fat32::Instance *fs, uint32_t off, uint32_t sz) {
    (void)fs;
    drivers::ata_pio::mark_dirty(off, sz);
}

extern uint8_t _binary_fat32_img_start[];
extern uint8_t _binary_fat32_img_end[];

static int mouse_dev_read(fs::VNode *node, uint8_t *buf, uint32_t size, uint32_t offset) {
    (void)node; (void)offset;
    if (size < sizeof(drivers::MousePacket)) return 0;
    drivers::MousePacket pkt;
    if (!drivers::PS2Mouse::read_packet(pkt)) return 0;
    lib::memcpy(buf, &pkt, sizeof(pkt));
    return sizeof(pkt);
}

static int mouse_dev_write(fs::VNode *node, const uint8_t *buf, uint32_t size, uint32_t offset) {
    (void)node; (void)buf; (void)size; (void)offset;
    return static_cast<int>(size);
}

static void init_initrd() {
    fs::VFS::create_dir("/bin");
    fs::VFS::create_dir("/home");
    fs::VFS::create_dir("/dev");
    /* device read/write function declarations */
    extern int dev_null_read(fs::VNode *, uint8_t *, uint32_t, uint32_t);
    extern int dev_null_write(fs::VNode *, const uint8_t *, uint32_t, uint32_t);
    extern int dev_zero_read(fs::VNode *, uint8_t *, uint32_t, uint32_t);
    extern int dev_zero_write(fs::VNode *, const uint8_t *, uint32_t, uint32_t);
    extern int dev_random_read(fs::VNode *, uint8_t *, uint32_t, uint32_t);
    extern int dev_random_write(fs::VNode *, const uint8_t *, uint32_t, uint32_t);
    fs::VFS::create_device("/dev/null", dev_null_read, dev_null_write);
    fs::VFS::create_device("/dev/zero", dev_zero_read, dev_zero_write);
    fs::VFS::create_device("/dev/random", dev_random_read, dev_random_write);
    fs::VFS::create_device("/dev/mouse", mouse_dev_read, mouse_dev_write);
    fs::VFS::create_dir("/dev/input");
    fs::VFS::create_dir("/tmp");

    const char *banner = "Welcome to Elitra OS!\n"
                         "This is a minimal x86-64 hobby operating system.\n"
                         "Features: preemptive multitasking, VFS, framebuffer console.\n";
    fs::VFS::create_file("/home/readme.txt",
                         reinterpret_cast<const uint8_t *>(banner),
                         lib::strlen(banner));

    const char *version_info = "Elitra OS v0.2.0\n"
                               "Architecture: x86-64\n"
                               "Kernel: Preemptive multitasking\n"
                               "FS: Virtual File System (ramfs)\n";
    fs::VFS::create_file("/etc/version",
                         reinterpret_cast<const uint8_t *>(version_info),
                         lib::strlen(version_info));

    const char *ls_help = "Elitra OS Shell\n"
                          "  ls [path]    - List directory contents\n"
                          "  cat <file>   - Display file contents\n"
                          "  vfsinfo      - Show VFS information\n";
    fs::VFS::create_file("/home/help.txt",
                         reinterpret_cast<const uint8_t *>(ls_help),
                         lib::strlen(ls_help));
}

extern "C" void kernel_main(uint32_t magic, uint32_t addr) {
    drivers::VGA::init();
    drivers::NS16550::init();

    drivers::NS16550::write("\n=== Elitra OS Boot ===\n");
    drivers::VGA::writestring_color("Elitra OS - Booting...\n",
                                         static_cast<uint8_t>(drivers::VGAColor::GREEN));
    drivers::NS16550::write("step: display init ok\n");

    mm::init(magic, addr);
    drivers::NS16550::write("step: mm init ok\n");

    drivers::VGA::writestring("Installing GDT... ");
    arch::x86::GDT::install();
    drivers::NS16550::write("step: gdt ok\n");

    drivers::VGA::writestring("Installing TSS... ");
    arch::x86::TSS::init();
    drivers::NS16550::write("step: tss ok\n");

    drivers::VGA::writestring("Installing IDT... ");
    arch::x86::IDT::install();
    drivers::NS16550::write("step: idt ok\n");

    drivers::VGA::writestring("Installing ISRs... ");
    arch::x86::ISR::install();
    drivers::NS16550::write("step: isr ok\n");

    drivers::VGA::writestring("Installing IRQs... ");
    arch::x86::IRQ::install();
    drivers::NS16550::write("step: irq ok\n");

    drivers::VGA::writestring("Initializing PIT... ");
    drivers::PITTimer::init(100);
    drivers::NS16550::write("step: pit ok\n");

    drivers::VGA::writestring("Initializing keyboard... ");
    drivers::PS2Keyboard::init();
    drivers::NS16550::write("step: kbd ok\n");

    drivers::VGA::writestring("Initializing RTC... ");
    drivers::CMOSRTC::init();
    drivers::NS16550::write("step: rtc ok\n");

    drivers::VGA::writestring("Initializing PS/2 mouse... ");
    drivers::PS2Mouse::init();
    drivers::NS16550::write("step: mouse ok\n");

    drivers::VGA::writestring("Enabling paging... ");
    mm::Paging::init();
    drivers::NS16550::write("step: paging ok\n");

    drivers::VGA::writestring("Initializing FPU... ");
    arch::x86::fpu_init();
    drivers::NS16550::write("step: fpu ok\n");

    // Parse framebuffer from multiboot info
    if (magic == 0x2BADB002) {
        struct __attribute__((packed)) {
            uint32_t flags;
            uint32_t _pad[11];
            uint32_t _vbe[9];
            uint64_t fb_addr;
            uint32_t fb_pitch;
            uint32_t fb_width;
            uint32_t fb_height;
            uint8_t  fb_bpp;
            uint8_t  fb_type;
        } *mbi = reinterpret_cast<decltype(mbi)>(addr);

        if (mbi->flags & (1 << 6)) {
            drivers::framebuffer::init(
                mbi->fb_addr,
                mbi->fb_width, mbi->fb_height,
                mbi->fb_pitch, mbi->fb_bpp);
            drivers::framebuffer::clear(drivers::framebuffer::COLOR_BLACK);
            drivers::NS16550::write("step: fb ok\n");
        }
    }

    drivers::VGA::writestring("Initializing heap... ");
    mm::Heap::init();
    drivers::NS16550::write("step: heap ok\n");

    drivers::VGA::writestring("Initializing PCI... ");
    arch::x86::PCI::install_driver(0x8086, 0x100E, drivers::E1000::probe);
    arch::x86::PCI::install_driver(0x8086, 0x100F, drivers::E1000::probe);
    arch::x86::PCI::install_driver(0x8086, 0x10D3, drivers::E1000::probe);
    arch::x86::PCI::init();

    drivers::VGA::writestring("Initializing e1000... ");
    drivers::E1000::init();
    drivers::NS16550::write("step: e1000 ok\n");

    drivers::VGA::writestring("Initializing networking... ");
    drivers::Net::init();
    drivers::NS16550::write("step: net ok\n");

    drivers::VGA::writestring("Initializing ATA... ");
    drivers::ata_pio::init();

    drivers::VGA::writestring("Initializing VFS... ");
    fs::VFS::init();
    fs::MountTable::init();
    init_initrd();

    fs::VFS::create_dir("/mnt");
    static fs::fat32::Instance fat_instance;
    fat_instance.write_callback = nullptr;

    bool fat32_ok = false;

    if (drivers::ata_pio::drive_count() > 0) {
        for (int d = 0; d < drivers::ata_pio::drive_count(); d++) {
            drivers::ata_pio::Partition parts[4];
            int np = drivers::ata_pio::find_partitions(d, parts, 4);
            if (np > 0) {
                drivers::NS16550::printf("ata: found FAT32 partition on drive %d, start_lba=%d, sectors=%d\n",
                                        d, parts[0].lba_start, parts[0].sector_count);
                uint32_t total = parts[0].sector_count;
                if (total > 4096) total = 4096; // limit to 2MB
                uint8_t *buf = reinterpret_cast<uint8_t *>(mm::malloc(total * 512));
                if (buf) {
                    bool ata_ok = true;
                    // ATA PIO read uses uint8_t count (max 256 sectors per call)
                    for (uint32_t chunk = 0; chunk < total && ata_ok; chunk += 256) {
                        uint32_t n = total - chunk;
                        if (n > 256) n = 256;
                        if (!drivers::ata_pio::read(d, parts[0].lba_start + chunk, n, buf + chunk * 512))
                            ata_ok = false;
                    }
                    if (ata_ok) {
                        drivers::ata_pio::mount_partition_buffer(d, parts[0].lba_start, total, buf);
                        if (fs::fat32::init(&fat_instance, buf, total * 512)) {
                            fat_instance.write_callback = ata_mark_dirty;
                            if (fs::fat32::mount(&fat_instance, "/mnt")) {
                                fs::MountTable::mount("/mnt", fs::FSType::FAT32, &fat_instance);
                                drivers::VGA::writestring_color("FAT32 mounted from ATA\n",
                                    static_cast<uint8_t>(drivers::VGAColor::GREEN));
                                fat32_ok = true;
                            }
                        }
                    }
                    if (!fat32_ok) mm::free(buf);
                }
                break;
            }
        }
    }

    if (!fat32_ok) {
        size_t fat32_size = _binary_fat32_img_end - _binary_fat32_img_start;
        if (fs::fat32::init(&fat_instance, _binary_fat32_img_start, fat32_size)) {
            if (fs::fat32::mount(&fat_instance, "/mnt")) {
                fs::MountTable::mount("/mnt", fs::FSType::FAT32, &fat_instance);
                drivers::VGA::writestring_color("FAT32 mounted (embedded)\n",
                    static_cast<uint8_t>(drivers::VGAColor::GREEN));
            } else {
                drivers::VGA::writestring_color("FAT32 mount failed\n",
                    static_cast<uint8_t>(drivers::VGAColor::BROWN));
            }
        } else {
            drivers::VGA::writestring_color("FAT32 mount failed\n",
                static_cast<uint8_t>(drivers::VGAColor::BROWN));
        }
    }

    // Try ext2 on ATA partitions (if present and FAT32 failed or additional partitions)
    fs::VFS::create_dir("/mnt_ext2");
    if (drivers::ata_pio::drive_count() > 1) {
        for (int d = 0; d < drivers::ata_pio::drive_count(); d++) {
            drivers::ata_pio::Partition parts[4];
            int np = drivers::ata_pio::find_partitions(d, parts, 4);
            if (np > 0) {
                for (int pi = 0; pi < np; pi++) {
                    uint32_t total = parts[pi].sector_count;
                    if (total > 4096) total = 4096;
                    uint8_t *buf = reinterpret_cast<uint8_t *>(mm::malloc(total * 512));
                    if (!buf) continue;
                    bool ext2_ok = false;
                    if (drivers::ata_pio::read(d, parts[pi].lba_start, total, buf)) {
                        static fs::ext2::Instance ext2_instance;
                        if (fs::ext2::init(&ext2_instance, buf, total * 512)) {
                            if (fs::ext2::mount(&ext2_instance, "/mnt_ext2")) {
                                fs::MountTable::mount("/mnt_ext2", fs::FSType::EXT2, &ext2_instance);
                                drivers::VGA::writestring_color("EXT2 mounted from ATA\n",
                                    static_cast<uint8_t>(drivers::VGAColor::GREEN));
                                ext2_ok = true;
                            }
                        }
                    }
                    if (!ext2_ok) mm::free(buf);
                    break;
                }
                break;
            }
        }
    }

    drivers::NS16550::write("step: vfs ok\n");

    drivers::VGA::writestring("Initializing ACPI... ");
    drivers::acpi::init();
    drivers::NS16550::write("step: acpi ok\n");

    drivers::VGA::writestring("Initializing scheduler... ");
    kernel::Scheduler::init();
    drivers::NS16550::write("step: sched ok\n");

    drivers::VGA::writestring("Installing syscalls... ");
    kernel::Syscall::init();
    drivers::NS16550::write("step: syscall ok\n");

    drivers::VGA::writestring("Enabling interrupts... ");
    arch::x86::enable_interrupts();
    drivers::NS16550::write("step: interrupts enabled\n");

    drivers::NS16550::write("Elitra OS boot complete\n");

    // Test pipe
    drivers::NS16550::write("-- Pipe test --\n");
    int pipe_fds[2];
    if (fs::VFS::pipe_create(pipe_fds) == 0) {
        const char *test_msg = "Hello from pipe!";
        size_t len = lib::strlen(test_msg);
        fs::VFS::pipe_write(pipe_fds[1],
            reinterpret_cast<const uint8_t *>(test_msg), len);
        fs::VFS::pipe_close(pipe_fds[1]);

        uint8_t buf[64];
        int n = fs::VFS::pipe_read(pipe_fds[0], buf, sizeof(buf) - 1);
        if (n > 0) {
            buf[n] = '\0';
            drivers::NS16550::printf("pipe test: read %d bytes: '%s'\n", n, buf);
        } else {
            drivers::NS16550::printf("pipe test: FAILED (read returned %d)\n", n);
        }
        fs::VFS::pipe_close(pipe_fds[0]);
    } else {
        drivers::NS16550::write("pipe test: FAILED (pipe_create)\n");
    }

    // Test output redirection to file via pipe
    drivers::NS16550::write("-- Redir test --\n");
    int redir_fds[2];
    if (fs::VFS::pipe_create(redir_fds) == 0) {
        const char *hello = "Hello from ELF program!";
        fs::VFS::pipe_write(redir_fds[1],
            reinterpret_cast<const uint8_t *>(hello), lib::strlen(hello));
        fs::VFS::pipe_close(redir_fds[1]);

        uint8_t rbuf[128];
        int rn = fs::VFS::pipe_read(redir_fds[0], rbuf, sizeof(rbuf) - 1);
        if (rn > 0) {
            rbuf[rn] = '\0';
            fs::VFS::write_file("/home/redir_test.txt", rbuf, rn);
            drivers::ata_pio::flush();
            fs::VNode *check = fs::VFS::resolve("/home/redir_test.txt");
            if (check) {
                drivers::NS16550::printf("redir test: wrote %d bytes (file size=%u)\n",
                    rn, check->size);
            } else {
                drivers::NS16550::printf("redir test: FAILED (file not found after write)\n");
            }
        } else {
            drivers::NS16550::printf("redir test: FAILED (read returned %d)\n", rn);
        }
        fs::VFS::pipe_close(redir_fds[0]);
    } else {
        drivers::NS16550::write("redir test: FAILED (pipe_create)\n");
    }

    drivers::Terminal::init();

    /* Spawn init process (userspace shell) */
    fs::VNode *init_node = fs::VFS::resolve("/mnt/bin/init.elf");
    if (!init_node || init_node->type != fs::NodeType::FILE) {
        init_node = fs::VFS::resolve("/bin/init.elf");
    }
    if (!init_node || init_node->type != fs::NodeType::FILE) {
        /* Fallback to built-in kernel shell */
        drivers::NS16550::write("init.elf not found, using kernel shell\n");
        kernel::Shell shell;
        shell.run();
    } else {
        uint64_t init_entry;
        if (loader::load_elf(init_node->data, init_node->size, &init_entry) == 0) {
            kernel::Scheduler::create_init(init_entry);
            drivers::NS16550::write("init: /bin/init.elf spawned\n");
            /* Yield to let init run; if init exits, return to kernel shell */
            kernel::Scheduler::yield();
            drivers::NS16550::write("init: returned, using kernel shell\n");
            kernel::Shell shell;
            shell.run();
        } else {
            drivers::NS16550::write("init.elf load failed, using kernel shell\n");
            kernel::Shell shell;
            shell.run();
        }
    }
}

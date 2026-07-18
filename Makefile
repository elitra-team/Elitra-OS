ASM      = nasm
LD       = ld
CARGO    = cargo +nightly
OBJCOPY  = objcopy

ASMFLAGS = -f elf64
LDFLAGS  = -m elf_x86_64 -T kernel/linker.ld -nostdlib -z noexecstack
CARGOFLAGS = -Z build-std=core --target x86_64-unknown-none --release

ASM_SRCS = arch/x86_64/boot.asm arch/x86_64/interrupt.asm arch/x86_64/switch.asm
TRAMPOLINE = arch/x86_64/trampoline.asm
TRAMP_BIN  = arch/x86_64/trampoline.bin
TRAMP_OBJ  = arch/x86_64/trampoline.o
ASM_OBJS = $(ASM_SRCS:.asm=.o)
KRUST    = kernel/target/x86_64-unknown-none/release/libelitra_kernel.a
TARGET   = elitra-kernel.bin

.PHONY: all clean run debug iso apps apps-clean kernel rust

all: kernel

kernel: $(TARGET)

$(TARGET): $(ASM_OBJS) $(TRAMP_OBJ) fat32-img.o $(KRUST)
	$(LD) $(LDFLAGS) -o $@ $(ASM_OBJS) $(TRAMP_OBJ) fat32-img.o $(KRUST)

$(KRUST): $(TRAMP_BIN)
	$(CARGO) build $(CARGOFLAGS) --manifest-path kernel/Cargo.toml

%.o: %.asm
	$(ASM) $(ASMFLAGS) -o $@ $<

$(TRAMP_BIN): $(TRAMPOLINE)
	$(ASM) -f bin -o $@ $<

$(TRAMP_OBJ): $(TRAMP_BIN)
	$(OBJCOPY) -I binary -O elf64-x86-64 -B i386:x86-64 --rename-section .data=.trampoline $< $@

fat32.img: apps
	dd if=/dev/zero of=fat32.img bs=1M count=2 2>/dev/null
	mkfs.fat -F 32 -n "ELITRA" fat32.img >/dev/null 2>&1
	echo "Elitra OS - FAT32 partition" | mcopy -i fat32.img - ::/README.txt 2>/dev/null || true
	mmd -i fat32.img ::/home 2>/dev/null || true
	mmd -i fat32.img ::/bin 2>/dev/null || true
	echo "Welcome to Elitra FAT32!" | mcopy -i fat32.img - ::/hello.txt
	echo "FAT32 test file in /home" | mcopy -i fat32.img - ::/home/test.txt
	for f in userspace/apps/*.elf; do \
		[ -f "$$f" ] && mcopy -i fat32.img "$$f" ::/bin/ 2>/dev/null || true; \
	done

fat32-img.o: fat32.img
	$(OBJCOPY) -I binary -O elf64-x86-64 -B i386:x86-64 fat32.img fat32-img.o

apps:
	$(MAKE) -C userspace

apps-clean:
	$(MAKE) -C userspace clean

clean:
	rm -f $(ASM_OBJS) $(TRAMP_BIN) $(TRAMP_OBJ) fat32.img fat32-img.o $(TARGET)
	$(CARGO) clean --manifest-path kernel/Cargo.toml 2>/dev/null || true
	$(MAKE) -C userspace clean

disk.img: fat32.img
	dd if=/dev/zero of=$@ bs=1M count=64 2>/dev/null
	parted -s $@ mklabel msdos
	parted -s $@ mkpart primary fat32 1 100% 2>/dev/null
	dd if=fat32.img of=$@ bs=512 seek=2048 count=4096 conv=notrunc 2>/dev/null

run: $(TARGET) disk.img
	qemu-system-x86_64 -machine q35 -kernel $(TARGET) -hda disk.img -display default -vga std -serial mon:stdio

debug: $(TARGET) disk.img
	qemu-system-x86_64 -machine q35 -kernel $(TARGET) -hda disk.img \
		-serial stdio -s -S -cpu max \
		-device ich9-usb-uhci1 -device usb-kbd -device usb-mouse

iso: $(TARGET)
	mkdir -p iso/boot/grub
	cp $(TARGET) iso/boot/
	echo 'set default=0' > iso/boot/grub/grub.cfg
	echo 'set timeout=5' >> iso/boot/grub/grub.cfg
	echo 'menuentry "Elitra OS" {' >> iso/boot/grub/grub.cfg
	echo '  multiboot2 /boot/elitra-kernel.bin' >> iso/boot/grub/grub.cfg
	echo '}' >> iso/boot/grub/grub.cfg
	grub-mkrescue -o elitra-os.iso iso
	rm -rf iso

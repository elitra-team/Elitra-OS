CXX      = g++
AS      = nasm
LD      = ld

CXXFLAGS = -m64 -ffreestanding -Wall -Wextra -O2 -nostdlib \
           -fno-stack-protector -fno-builtin -fno-rtti -fno-exceptions \
           -fno-use-cxa-atexit -nostartfiles -fno-pie -no-pie \
           -mno-red-zone \
           -mno-sse -mno-sse2 -mno-mmx -mno-3dnow -msoft-float \
           -MMD -MP \
           -Iarch/x86_64 -Ikernel -Idrivers -Imm -Ilib -Ifs -Iloader
ASFLAGS  = -f elf64
LDFLAGS  = -m elf_x86_64 -T linker.ld -nostdlib -z noexecstack

ARCH_CXX_SRC = $(wildcard arch/x86_64/*.cpp)
KERNEL_CXX_SRC = $(wildcard kernel/*.cpp)
DRIVER_CXX_SRC = $(wildcard drivers/*.cpp)
MM_CXX_SRC    = $(wildcard mm/*.cpp)
LIB_CXX_SRC   = $(wildcard lib/*.cpp)
FS_CXX_SRC    = $(wildcard fs/*.cpp)
LOADER_CXX_SRC = $(wildcard loader/*.cpp)

CXX_SRCS = $(ARCH_CXX_SRC) $(KERNEL_CXX_SRC) $(DRIVER_CXX_SRC) $(MM_CXX_SRC) $(LIB_CXX_SRC) $(FS_CXX_SRC) $(LOADER_CXX_SRC)
ASM_SRCS = arch/x86_64/boot.asm arch/x86_64/interrupt.asm arch/x86_64/switch.asm

CXX_OBJS = $(CXX_SRCS:.cpp=.o)
ASM_OBJS = $(ASM_SRCS:.asm=.o)
OBJS     = $(ASM_OBJS) $(CXX_OBJS) fat32-img.o

RUST_APPS = rust-rt/apps/hello.elf rust-rt/apps/cat.elf rust-rt/apps/ls.elf \
           rust-rt/apps/touch.elf rust-rt/apps/mkdir.elf rust-rt/apps/rm.elf \
           rust-rt/apps/rmdir.elf rust-rt/apps/mv.elf rust-rt/apps/cp.elf \
           rust-rt/apps/true.elf rust-rt/apps/false.elf rust-rt/apps/echo.elf \
           rust-rt/apps/yes.elf rust-rt/apps/seq.elf rust-rt/apps/sleep.elf \
           rust-rt/apps/clear.elf rust-rt/apps/uname.elf rust-rt/apps/basename.elf \
           rust-rt/apps/dirname.elf rust-rt/apps/pwd.elf rust-rt/apps/which.elf \
           rust-rt/apps/tee.elf rust-rt/apps/env.elf rust-rt/apps/init.elf

TARGET   = elitra-kernel.bin

.PHONY: all clean run debug iso fat32-img rust-apps krust

all: $(TARGET)

kernel: $(TARGET)

rust-apps: $(RUST_APPS)

RUST_BUILD_STAMP = rust-rt/.built

$(RUST_APPS): $(RUST_BUILD_STAMP)
	@true

$(RUST_BUILD_STAMP):
	$(MAKE) -C rust-rt || echo "Warning: rust-apps build failed (install nightly-2025-09-01 or fix target.json)"
	touch $@

KRUST_A = krust/target/x86_64-unknown-none/release/libelitra_kernel.a

$(KRUST_A):
	cargo +nightly build -Z build-std=core \
		--target x86_64-unknown-none --release --manifest-path krust/Cargo.toml \
		|| echo "Warning: krust build failed"

fat32.img: $(RUST_APPS)
	dd if=/dev/zero of=fat32.img bs=1M count=2 2>/dev/null
	mkfs.fat -F 32 -n "ELITRA" fat32.img >/dev/null 2>&1
	echo "Elitra OS - FAT32 partition" | mcopy -i fat32.img - ::/README.txt 2>/dev/null || true
	mmd -i fat32.img ::/home 2>/dev/null || true
	mmd -i fat32.img ::/bin 2>/dev/null || true
	echo "Welcome to Elitra FAT32!" | mcopy -i fat32.img - ::/hello.txt
	echo "FAT32 test file in /home" | mcopy -i fat32.img - ::/home/test.txt
	for f in $(RUST_APPS); do \
		if [ -f $$f ]; then mcopy -i fat32.img $$f ::/bin/ 2>/dev/null || true; fi; \
	done
	@echo "fat32.img ready"

fat32-img.o: fat32.img
	objcopy -I binary -O elf64-x86-64 -B i386:x86-64 fat32.img fat32-img.o

$(TARGET): $(OBJS) $(KRUST_A)
	$(LD) $(LDFLAGS) -o $@ $(OBJS) $(KRUST_A)

%.o: %.asm
	$(AS) $(ASFLAGS) -o $@ $<

%.o: %.cpp
	$(CXX) $(CXXFLAGS) -c -o $@ $<

-include $(CXX_OBJS:.o=.d)

krust: $(KRUST_A)

clean:
	rm -f $(OBJS) fat32.img fat32-img.o $(TARGET) rust-rt/.built
	$(MAKE) -C rust-rt clean
	cargo +nightly clean --manifest-path krust/Cargo.toml 2>/dev/null || true

disk.img: fat32.img
	dd if=/dev/zero of=$@ bs=1M count=64 2>/dev/null
	parted -s $@ mklabel msdos
	parted -s $@ mkpart primary fat32 1 100% 2>/dev/null
	dd if=fat32.img of=$@ bs=512 seek=2048 count=4096 conv=notrunc 2>/dev/null

run: $(TARGET) disk.img
	qemu-system-x86_64 -kernel $(TARGET) -hda disk.img -serial stdio -cpu max

debug: $(TARGET) disk.img
	qemu-system-x86_64 -kernel $(TARGET) -hda disk.img -serial stdio -s -S -cpu max

iso: $(TARGET)
	mkdir -p iso/boot/grub
	cp $(TARGET) iso/boot/
	echo 'set default=0' > iso/boot/grub/grub.cfg
	echo 'set timeout=5' >> iso/boot/grub/grub.cfg
	echo 'menuentry "Elitra OS" {' >> iso/boot/grub/grub.cfg
	echo '  multiboot /boot/elitra-kernel.bin' >> iso/boot/grub/grub.cfg
	echo '}' >> iso/boot/grub/grub.cfg
	grub-mkrescue -o elitra-os.iso iso
	rm -rf iso

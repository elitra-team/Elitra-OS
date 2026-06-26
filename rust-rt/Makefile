CARGO   = cargo +nightly

ZFLAGS = -Z build-std=core
CARGOFLAGS = $(ZFLAGS) --target x86_64-unknown-none --release

.PHONY: all clean

BINS = hello cat ls touch mkdir rm rmdir mv cp true false echo yes seq sleep clear uname basename dirname pwd which tee env init

all:
	$(CARGO) build $(CARGOFLAGS) $(addprefix --bin ,$(BINS))
	for bin in $(BINS); do cp target/x86_64-unknown-none/release/$$bin apps/$$bin.elf; done
	@echo "  Copied: $(addsuffix .elf,$(BINS))"

clean:
	$(CARGO) clean
	rm -f apps/*.elf

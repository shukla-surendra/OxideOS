NASM   := nasm
BINDIR := bin
CARGO  := cargo

SRCS := hello.asm counter.asm sysinfo.asm input.asm \
        fib.asm primes.asm countdown.asm spinner.asm filetest.asm
BINS := $(SRCS:%.asm=$(BINDIR)/%.bin)

# Rust programs (output = bin/<name>.elf)
RUST_BINS := $(BINDIR)/hello_rust.elf

.PHONY: all clean rust-programs

all: $(BINDIR) $(BINS) rust-programs

$(BINDIR):
	mkdir -p $(BINDIR)

$(BINDIR)/%.bin: %.asm
	$(NASM) -f bin -o $@ $<

rust-programs: $(BINDIR)
	$(CARGO) +nightly build --release -p hello-rust -p sh -p terminal 2>&1
	cp target/x86_64-unknown-none/release/hello_rust $(BINDIR)/hello_rust.elf
	cp target/x86_64-unknown-none/release/sh         $(BINDIR)/sh.elf
	cp target/x86_64-unknown-none/release/terminal   $(BINDIR)/terminal.elf

clean:
	rm -rf $(BINDIR)
	$(CARGO) clean

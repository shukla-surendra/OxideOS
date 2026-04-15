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
	$(CARGO) +nightly build --release -p hello-rust -p sh -p terminal -p coreutils -p wget -p edit -p nc 2>&1
	# coreutils rm and mv are built above with -p coreutils
	cp target/x86_64-unknown-none/release/hello_rust $(BINDIR)/hello_rust.elf
	cp target/x86_64-unknown-none/release/sh         $(BINDIR)/sh.elf
	cp target/x86_64-unknown-none/release/terminal   $(BINDIR)/terminal.elf
	cp target/x86_64-unknown-none/release/ls         $(BINDIR)/ls.elf
	cp target/x86_64-unknown-none/release/cat        $(BINDIR)/cat.elf
	cp target/x86_64-unknown-none/release/ps         $(BINDIR)/ps.elf
	cp target/x86_64-unknown-none/release/cp         $(BINDIR)/cp.elf
	cp target/x86_64-unknown-none/release/mkdir      $(BINDIR)/mkdir.elf
	cp target/x86_64-unknown-none/release/pwd        $(BINDIR)/pwd.elf
	cp target/x86_64-unknown-none/release/wget       $(BINDIR)/wget.elf
	cp target/x86_64-unknown-none/release/edit       $(BINDIR)/edit.elf
	cp target/x86_64-unknown-none/release/nc         $(BINDIR)/nc.elf
	cp target/x86_64-unknown-none/release/rm         $(BINDIR)/rm.elf
	cp target/x86_64-unknown-none/release/mv         $(BINDIR)/mv.elf

clean:
	rm -rf $(BINDIR)
	$(CARGO) clean

NASM   := nasm
BINDIR := bin
CARGO  := cargo

SRCS := hello.asm counter.asm sysinfo.asm input.asm \
        fib.asm primes.asm countdown.asm spinner.asm filetest.asm
BINS := $(SRCS:%.asm=$(BINDIR)/%.bin)

# Rust programs (output = bin/<name>.elf)
RUST_BINS := $(BINDIR)/hello_rust.elf

.PHONY: all clean rust-programs c-programs

all: $(BINDIR) $(BINS) c-programs rust-programs

$(BINDIR):
	mkdir -p $(BINDIR)

$(BINDIR)/%.bin: %.asm
	$(NASM) -f bin -o $@ $<

# ── C programs (gcc, no stdlib, Linux syscall ABI) ────────────────────────────
c-programs: $(BINDIR)
	x86_64-linux-gnu-gcc -static -nostdlib -nostartfiles \
	    -O2 -fno-stack-protector -fno-asynchronous-unwind-tables \
	    -o $(BINDIR)/hello_c.elf hello_c/hello_c.c

# ── Rust programs ─────────────────────────────────────────────────────────────
rust-programs: $(BINDIR)
	$(CARGO) +nightly build --release -p hello-rust -p sh -p terminal -p coreutils -p wget -p edit -p nc -p filemanager -p install 2>&1
	cp target/x86_64-unknown-none/release/hello_rust   $(BINDIR)/hello_rust.elf
	cp target/x86_64-unknown-none/release/sh           $(BINDIR)/sh.elf
	cp target/x86_64-unknown-none/release/terminal     $(BINDIR)/terminal.elf
	cp target/x86_64-unknown-none/release/ls           $(BINDIR)/ls.elf
	cp target/x86_64-unknown-none/release/cat          $(BINDIR)/cat.elf
	cp target/x86_64-unknown-none/release/ps           $(BINDIR)/ps.elf
	cp target/x86_64-unknown-none/release/cp           $(BINDIR)/cp.elf
	cp target/x86_64-unknown-none/release/mkdir        $(BINDIR)/mkdir.elf
	cp target/x86_64-unknown-none/release/pwd          $(BINDIR)/pwd.elf
	cp target/x86_64-unknown-none/release/wget         $(BINDIR)/wget.elf
	cp target/x86_64-unknown-none/release/edit         $(BINDIR)/edit.elf
	cp target/x86_64-unknown-none/release/nc           $(BINDIR)/nc.elf
	cp target/x86_64-unknown-none/release/rm           $(BINDIR)/rm.elf
	cp target/x86_64-unknown-none/release/mv           $(BINDIR)/mv.elf
	cp target/x86_64-unknown-none/release/filemanager  $(BINDIR)/filemanager.elf
	cp target/x86_64-unknown-none/release/echo         $(BINDIR)/echo.elf
	cp target/x86_64-unknown-none/release/grep         $(BINDIR)/grep.elf
	cp target/x86_64-unknown-none/release/wc           $(BINDIR)/wc.elf
	cp target/x86_64-unknown-none/release/head         $(BINDIR)/head.elf
	cp target/x86_64-unknown-none/release/tail         $(BINDIR)/tail.elf
	cp target/x86_64-unknown-none/release/sort         $(BINDIR)/sort.elf
	cp target/x86_64-unknown-none/release/sleep        $(BINDIR)/sleep.elf
	cp target/x86_64-unknown-none/release/kill         $(BINDIR)/kill.elf
	cp target/x86_64-unknown-none/release/touch        $(BINDIR)/touch.elf
	cp "target/x86_64-unknown-none/release/true"       $(BINDIR)/true.elf
	cp "target/x86_64-unknown-none/release/false"      $(BINDIR)/false.elf
	cp target/x86_64-unknown-none/release/install      $(BINDIR)/install.elf

clean:
	rm -rf $(BINDIR)
	$(CARGO) clean

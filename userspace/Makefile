NASM      := nasm
BINDIR    := bin
CARGO     := cargo
MUSL_GCC  := /home/surendra/musl-oxide/bin/musl-gcc

SRCS := hello.asm counter.asm sysinfo.asm input.asm \
        fib.asm primes.asm countdown.asm spinner.asm filetest.asm
BINS := $(SRCS:%.asm=$(BINDIR)/%.bin)

# Rust programs (output = bin/<name>.elf)
RUST_BINS := $(BINDIR)/hello_rust.elf

.PHONY: all clean rust-programs c-programs musl-programs

all: $(BINDIR) $(BINS) c-programs musl-programs rust-programs

$(BINDIR):
	mkdir -p $(BINDIR)

$(BINDIR)/%.bin: %.asm
	$(NASM) -f bin -o $@ $<

# ── C programs (gcc, no stdlib, Linux syscall ABI) ────────────────────────────
c-programs: $(BINDIR)
	x86_64-linux-gnu-gcc -static -nostdlib -nostartfiles \
	    -O2 -fno-stack-protector -fno-asynchronous-unwind-tables \
	    -o $(BINDIR)/hello_c.elf hello_c/hello_c.c

# ── musl-libc C programs (static, no syscall wrapper needed) ─────────────────
musl-programs: $(BINDIR)
	$(MUSL_GCC) -static -O2 -o $(BINDIR)/hello_musl.elf hello_musl/hello_musl.c
	$(MUSL_GCC) -static -O2 -o $(BINDIR)/musl_test.elf  hello_musl/musl_test.c

# ── Lua 5.4 interpreter (pre-built, copy from source tree) ───────────────────
lua: $(BINDIR)
	cp /home/surendra/lua-5.4.7/src/lua $(BINDIR)/lua.elf

# ── BusyBox 1.36.1 (pre-built, copy from source tree) ────────────────────────
busybox: $(BINDIR)
	cp /home/surendra/busybox-1.36.1/busybox $(BINDIR)/busybox.elf

# ── Rebuild BusyBox from source ───────────────────────────────────────────────
busybox-rebuild:
	cd /home/surendra/busybox-1.36.1 && \
	    CC=$(MUSL_GCC) LDFLAGS="-static" make -j$(shell nproc)
	cp /home/surendra/busybox-1.36.1/busybox $(BINDIR)/busybox.elf

# ── Bash 5.2 (pre-built, copy from build dir) ────────────────────────────────
bash: $(BINDIR)
	cp /tmp/bash-5.2/bash $(BINDIR)/bash.elf

# ── Rebuild Bash from source ─────────────────────────────────────────────────
bash-rebuild:
	test -f /tmp/bash-5.2/Makefile || ( \
	    cd /tmp && wget -q https://ftp.gnu.org/gnu/bash/bash-5.2.tar.gz && \
	    tar xzf bash-5.2.tar.gz && \
	    cd /tmp/bash-5.2 && CC=$(MUSL_GCC) CFLAGS="-static -O2 -fno-stack-protector" \
	        LDFLAGS="-static" ./configure --without-bash-malloc --disable-nls \
	        --without-readline --without-curses --disable-history --host=x86_64-linux-musl )
	cd /tmp/bash-5.2 && make LDFLAGS="-static -Wl,--allow-multiple-definition" -j$(shell nproc)
	cp /tmp/bash-5.2/bash $(BINDIR)/bash.elf

# ── Rust programs ─────────────────────────────────────────────────────────────
rust-programs: $(BINDIR)
	$(CARGO) +nightly build --release -p hello-rust -p sh -p terminal -p coreutils -p wget -p edit -p nc -p filemanager -p install -p sysmon 2>&1
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
	cp target/x86_64-unknown-none/release/sysmon       $(BINDIR)/sysmon.elf

clean:
	rm -rf $(BINDIR)
	$(CARGO) clean

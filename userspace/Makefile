NASM   := nasm
BINDIR := bin

SRCS := hello.asm counter.asm sysinfo.asm input.asm
BINS := $(SRCS:%.asm=$(BINDIR)/%.bin)

.PHONY: all clean

all: $(BINDIR) $(BINS)

$(BINDIR):
	mkdir -p $(BINDIR)

$(BINDIR)/%.bin: %.asm
	$(NASM) -f bin -o $@ $<

clean:
	rm -rf $(BINDIR)

/* hello_c.c — "Hello from C on OxideOS!"
 *
 * A no-stdlib C program that runs natively on OxideOS using the syscall
 * instruction (Linux x86-64 ABI). OxideOS syscall numbers now match Linux:
 *   write  = 1
 *   exit   = 60
 *
 * Compile:
 *   x86_64-linux-gnu-gcc -static -nostdlib -nostartfiles \
 *       -O2 -fno-stack-protector -fno-asynchronous-unwind-tables \
 *       -o hello_c.elf hello_c.c
 */

typedef long ssize_t;
typedef unsigned long size_t;

static ssize_t oxide_write(int fd, const void *buf, size_t n)
{
    ssize_t ret;
    __asm__ volatile(
        "syscall"
        : "=a"(ret)
        : "0"(1L), "D"((long)fd), "S"(buf), "d"((long)n)
        : "rcx", "r11", "memory"
    );
    return ret;
}

static void oxide_exit(int code)
{
    __asm__ volatile(
        "syscall"
        :
        : "a"(60L), "D"((long)code)
        : "rcx", "r11", "memory"
    );
    __builtin_unreachable();
}

void _start(void)
{
    const char msg[] =
        "=========================================\n"
        "  Hello from C on OxideOS!\n"
        "  This binary was compiled with gcc,\n"
        "  uses the Linux x86-64 syscall ABI,\n"
        "  and runs on a no_std Rust kernel.\n"
        "=========================================\n";

    oxide_write(1, msg, sizeof(msg) - 1);
    oxide_exit(0);
}

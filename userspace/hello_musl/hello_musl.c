#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
    printf("Hello from musl libc on OxideOS!\n");
    printf("argc = %d\n", argc);
    for (int i = 0; i < argc; i++)
        printf("  argv[%d] = %s\n", i, argv[i]);

    char *path = getenv("PATH");
    printf("PATH = %s\n", path ? path : "(not set)");

    char buf[64];
    if (getcwd(buf, sizeof(buf)))
        printf("cwd = %s\n", buf);

    return 0;
}

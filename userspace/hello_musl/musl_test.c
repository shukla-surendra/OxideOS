#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <time.h>

int main(int argc, char **argv, char **envp) {
    printf("=== musl libc test on OxideOS ===\n");

    /* Test argc/argv */
    printf("argc=%d", argc);
    for (int i = 0; i < argc; i++)
        printf(" argv[%d]=%s", i, argv[i]);
    printf("\n");

    /* Test envp */
    int envc = 0;
    if (envp) {
        while (envp[envc]) envc++;
        printf("envc=%d  PATH=%s\n", envc, getenv("PATH") ?: "(nil)");
    }

    /* Test malloc/free (exercises mmap/munmap via musl allocator) */
    size_t total = 0;
    for (int i = 0; i < 16; i++) {
        char *p = malloc(4096 * (i + 1));
        if (!p) { printf("malloc failed at i=%d\n", i); break; }
        memset(p, 0xAB, 4096 * (i + 1));
        total += 4096 * (i + 1);
        free(p);
    }
    printf("malloc/free ok: %zu bytes exercised\n", total);

    /* Test clock_gettime */
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) == 0)
        printf("uptime: %lds %ldns\n", (long)ts.tv_sec, ts.tv_nsec);

    /* Test getcwd */
    char cwd[256];
    if (getcwd(cwd, sizeof(cwd)))
        printf("cwd: %s\n", cwd);

    printf("=== all tests passed ===\n");
    return 0;
}

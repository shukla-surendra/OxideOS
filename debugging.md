```
hexdump -C target/x86_32-oxideos/debug/OxideOs | head -40
hexdump -C target/x86_32-oxideos/debug/OxideOs | head -2048
```
```
readelf -h target/x86_32-oxideos/debug/OxideOs
```

```
objdump -d target/x86_32-oxideos/debug/OxideOs

```

```
nm target/x86_32-oxideos/debug/OxideOs
```
# 🎨 VGA Text Mode Colors

In VGA text mode, each **cell** has 2 bytes:
- **Byte 0** → ASCII character
- **Byte 1** → Attribute (foreground + background colors)

The attribute byte format:

```
+----------------+----------------+
| Bits 7..4      | Bits 3..0      |
| Background     | Foreground     |
+----------------+----------------+
```

Formula:
```
attribute = (background << 4) | foreground
```

---

## Base 16 Colors

| Value | Color        | Value | Color           |
|-------|-------------|-------|----------------|
| 0     | Black       | 8     | Dark Gray      |
| 1     | Blue        | 9     | Light Blue     |
| 2     | Green       | 10    | Light Green    |
| 3     | Cyan        | 11    | Light Cyan     |
| 4     | Red         | 12    | Light Red      |
| 5     | Magenta     | 13    | Light Magenta  |
| 6     | Brown       | 14    | Yellow         |
| 7     | Light Gray  | 15    | White          |

---

## Examples

- **White on Black**
  ```
  foreground = 15 (White)
  background = 0  (Black)
  attribute  = (0 << 4) | 15 = 0x0F
  ```

- **Yellow on Blue**
  ```
  foreground = 14 (Yellow)
  background = 1  (Blue)
  attribute  = (1 << 4) | 14 = 0x1E
  ```

- **Light Red on Green**
  ```
  foreground = 12 (Light Red)
  background = 2  (Green)
  attribute  = (2 << 4) | 12 = 0x2C
  ```

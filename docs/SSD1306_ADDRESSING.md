# SSD1306 Display Addressing

## Page Layout

The 64 rows are divided into 8 horizontal bands called **pages** (0–7), each 8 pixels tall:

```
Page 0 → rows  0– 7
Page 1 → rows  8–15
Page 2 → rows 16–23
Page 3 → rows 24–31
Page 4 → rows 32–39
Page 5 → rows 40–47
Page 6 → rows 48–55
Page 7 → rows 56–63
```

Each page has 128 columns, so one page = 128 bytes. Total GDDRAM: 8 × 128 = **1024 bytes**.

## Each Byte is a Vertical Slice

A single byte controls one column within one page — 8 vertically stacked pixels.
Bit 0 is the **top** pixel of that page, bit 7 is the **bottom**:

```
Column 5, Page 2 (rows 16-23):

bit 0 → row 16  ●
bit 1 → row 17  ○
bit 2 → row 18  ●
bit 3 → row 19  ○
bit 4 → row 20  ○
bit 5 → row 21  ○
bit 6 → row 22  ○
bit 7 → row 23  ○

byte value = 0b00000101 = 0x05
```

## Addressing a Specific Pixel

To light pixel at **(x, y)**:
- Column = `x`
- Page = `y / 8`
- Bit position within the byte = `y % 8`

**Example:** pixel (5, 18) → column 5, page 2 (`18/8`), bit 2 (`18%8`)

## Horizontal Addressing Mode

In horizontal addressing mode (`0x20, 0x00`), bytes are written left-to-right across
columns, then wrap to the next page — like reading a book. This allows streaming the
entire framebuffer (1024 bytes) in one I2C transaction after setting the address window:

- Set column range: `0x21, 0x00, 0x7F` (columns 0–127)
- Set page range:   `0x22, 0x00, 0x07` (pages 0–7)
- Send 1025 bytes:  `[0x40, <1024 pixel bytes>]`

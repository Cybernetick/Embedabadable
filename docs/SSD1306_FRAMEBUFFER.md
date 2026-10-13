# SSD1306 Frame Buffer

A **frame buffer** is a 1024-byte copy of the whole screen kept in RAM. Each pixel is one
bit. You draw by setting or clearing bits in RAM, then send the buffer to the display.
The buffer uses exactly the same layout as the SSD1306's GDDRAM (see
`SSD1306_ADDRESSING.md`), so sending it is a straight copy.

## Why It's Needed

Writing straight into the display's memory, one page-aligned rectangle at a time, only
works while:

1. every element starts on a page boundary (y = 0, 8, 16, …),
2. no two elements share columns within the same page,
3. you never need to combine pixels.

Over I2C you **can't read the SSD1306's memory back**. Every byte written replaces all 8
vertical pixels in that column of that page. If an icon ends at y = 12 and the text below
starts at y = 13, they share page 1, and whichever is written second erases part of the
other.

In RAM you *can* read the current value before changing it, so overlapping elements
simply combine. As soon as the screen has several elements (clock, temperature and
humidity values, icons, separator lines), a frame buffer is the standard solution.

## Finding the Bit for Pixel (x, y)

The screen is 128 × 64: x from 0 to 127 (left → right), y from 0 to 63 (top → bottom).

```
page  = y / 8                 which 8-row band          (0..7)
index = page * 128 + x        which byte in the buffer   (0..1023)
bit   = y % 8                 which bit in that byte     (0 = top row of the page)
```

- **Pixel on:**  `buffer[index] |= 1 << bit`
- **Pixel off:** `buffer[index] &= !(1 << bit)`

Changing one bit leaves the other 7 pixels in that byte alone.

**Example:** the pixel at (5, 18) is page 2, index 2·128 + 5 = **261**, bit **2**.

```
buffer:  [  page 0: 0..127  ][  page 1: 128..255  ][  page 2: 256..383  ] ...
                                                       ↑ index 261 = column 5
                                                         bit 2 = row 18 (16 + 2)
```

## Drawing Text with `font8x8`

`font8x8` (`BASIC_FONTS.get(char) -> Option<[u8; 8]>`) only supplies the glyph's bits.
Its layout is **row by row**: byte `r` is row `r` of the glyph, and bit 0 is the leftmost
pixel.

To draw a character at any (x, y):

- for each of the 8 rows `r` in the glyph,
  - for each of the 8 bits `c` in that row's byte,
    - if the bit is 1, turn on pixel (x + c, y + r).

This loop also removes the orientation problem. You read the glyph row by row, as
`font8x8` stores it, and write pixel by pixel, so no transpose is needed. A transpose from
rows to SSD1306 columns is only needed for the fast path: copying a whole glyph into one
page when y is a multiple of 8.

**Scaling ×2:** each glyph pixel turns on a 2×2 block at (x + 2c, y + 2r). An 8×8 glyph
becomes 16×16, covering 2 pages. This is good for large clock digits.

**Icons** (thermometer, water drop, …) work the same way: store the bitmap in any
convenient format and draw it pixel by pixel.

Crate setup: `font8x8` has `std` on by default. Use `default-features = false` with
`features = ["unicode"]` for `no_std`. It has ASCII, Latin-1 (includes `°`), Greek,
box-drawing, block and Hiragana characters, but **no Cyrillic**.

## Drawing a Frame

1. **Clear what changes.** Either fill the whole buffer with zeros or clear only the area
   being redrawn (e.g. the rectangle with the seconds digits). Otherwise the old "8" stays
   under the new "3".
2. **Draw** text, icons and lines into the buffer. This is plain RAM work: no I2C, no lock.
3. **Send it to the display:** lock the bus, set column range 0–127 (`0x21`) and page range
   0–7 (`0x22`) as in `clear_screen`, send `0x40` followed by the 1024 bytes, unlock.

### The `0x40` prefix

- **Option A:** keep exactly 1024 bytes of pixels and send the prefix separately. esp-hal's
  `transaction_async` with two consecutive write operations, `[0x40]` then the buffer, sends
  them as one continuous I2C write.
- **Option B:** keep a 1025-byte array and treat index 0 as reserved. Remember the +1 offset
  in all pixel calculations.

### Only send changed pages

Keep track of which pages changed: 8 flags, or one `u8` used as a bitmask. Set the page
range to just those pages and send 128 bytes for each. A changed seconds digit then costs
1–2 pages, not the whole screen.

This matters because the **DS1307 supports only 100 kHz I2C**, so the shared bus can't go
to 400 kHz. A full 1025-byte frame takes about **90 ms** at 100 kHz.

## Where the Buffer Lives

1 KB is tiny next to the ESP32-S3's ~512 KB of RAM. The simplest owner is the drawing task
itself (a local array in `draw_clock_time`'s loop state). Only that task draws, so the
buffer needs **no mutex**. The only shared resource is still the I2C bus, locked just for
step 3. Don't hold the bus lock while drawing into the buffer, so `read_time` and other
sensor tasks aren't blocked.

## Testing

Pixel index and bit maths, set and clear, glyph drawing and dirty-page tracking don't touch
hardware. They can live in a host-testable crate next to the BCD helpers (`crates/`), and be
unit-tested with `cargo +stable test -p <crate> --target aarch64-apple-darwin`. For example,
a test can check that drawing `'1'` at (0, 0) sets the expected bits, without flashing the
board.

## Moving to `embedded-graphics` Later

The same buffer can become an `embedded-graphics` `DrawTarget` by implementing one
"set pixel (x, y) on or off" method, which is the bit operation above. That adds fonts in
several sizes, images and icons at any position, shapes and alignment helpers, while init,
addressing, the shared bus and the `0x40` writes all stay your own code.

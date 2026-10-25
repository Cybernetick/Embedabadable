use crate::fonts::{Font, FontBitOrder, FontOrientation};

static GLYPHS: [[u8; 8]; 128] = font8x8::legacy::BASIC_LEGACY;

pub const LARGE: Font = Font {
    width: 8,
    height: 8,
    advance: 8,
    orientation: FontOrientation::Horizontal,
    bit_order: FontBitOrder::LsbFirst,
    lookup: lookup_char,
};

fn lookup_char(c: char) -> &'static [u8] {
    if c.is_ascii(){
        &GLYPHS[c as usize]
    } else {
        &GLYPHS['?' as usize]
    }
}
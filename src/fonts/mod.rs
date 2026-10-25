pub mod small;
pub mod large;

pub struct Font {
    pub width: u8,
    pub height: u8,
    pub advance: u8,
    pub orientation: FontOrientation,
    pub bit_order: FontBitOrder,
    pub lookup: fn(char) -> &'static [u8],
}

impl Font {
    pub fn bytes_per_glyph(&self) -> usize {
        match self.orientation {
            FontOrientation::Horizontal => { self.height as usize}
            FontOrientation::Vertical => { self.width as usize }
        }
    }
}

pub enum FontOrientation {
    /// each byte is a row
    Horizontal,
    /// each byte is a column
    Vertical,
}

pub enum FontBitOrder {
    /// bit 7 is the leftmost pixel for horizontal fonts, or topmost pixel for vertical fonts
    MsbFirst,
    /// the bit 0 is the leftmost pixel for horizontal fonts, or topmost pixel for vertical fonts
    LsbFirst,
}
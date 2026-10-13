/// Converts a packed BCD byte (e.g. `0x59`) into its decimal value (`59`).
pub fn bcd2dec(bcd: u8) -> u8 {
    ((bcd & 0xF0) >> 4) * 10 + (bcd & 0x0F)
}

/// Converts a decimal value in `0..=99` (e.g. `59`) into a packed BCD byte (`0x59`).
pub fn dec2bcd(dec: u8) -> u8 {
    ((dec / 10) << 4) | (dec % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcd2dec_known_values() {
        assert_eq!(bcd2dec(0x00), 0);
        assert_eq!(bcd2dec(0x09), 9);
        assert_eq!(bcd2dec(0x10), 10);
        assert_eq!(bcd2dec(0x23), 23);
        assert_eq!(bcd2dec(0x59), 59);
        assert_eq!(bcd2dec(0x99), 99);
    }

    #[test]
    fn dec2bcd_known_values() {
        assert_eq!(dec2bcd(0), 0x00);
        assert_eq!(dec2bcd(9), 0x09);
        assert_eq!(dec2bcd(10), 0x10);
        assert_eq!(dec2bcd(23), 0x23);
        assert_eq!(dec2bcd(59), 0x59);
        assert_eq!(dec2bcd(99), 0x99);
    }

    #[test]
    fn round_trip_all_valid_values() {
        for n in 0..=99u8 {
            assert_eq!(bcd2dec(dec2bcd(n)), n, "round trip failed for {n}");
        }
    }
}

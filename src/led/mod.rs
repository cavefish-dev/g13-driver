#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    /// Parse `#RRGGBB` or `RRGGBB` (case-insensitive). `None` on any malformed input.
    pub fn from_hex(s: &str) -> Option<Color> {
        let h = s.strip_prefix('#').unwrap_or(s);
        if h.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&h[0..2], 16).ok()?;
        let g = u8::from_str_radix(&h[2..4], 16).ok()?;
        let b = u8::from_str_radix(&h[4..6], 16).ok()?;
        Some(Color(r, g, b))
    }

    pub fn to_hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(Color::from_hex("#FF3010"), Some(Color(0xFF, 0x30, 0x10)));
        assert_eq!(Color::from_hex("ff3010"), Some(Color(0xFF, 0x30, 0x10))); // no '#', lowercase
        assert_eq!(Color(0xFF, 0x30, 0x10).to_hex(), "#FF3010");
        assert_eq!(Color::from_hex("white"), None);   // malformed -> None
        assert_eq!(Color::from_hex("#FFF"), None);    // wrong length -> None
    }
}

use crate::protocol::MKey;

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BacklightConfig {
    pub default_color: Color,
    pub brightness: f32,
    pub mkey_indicator: bool,
    pub slot_colors: [Option<Color>; 3],
}

impl Default for BacklightConfig {
    fn default() -> Self {
        Self {
            default_color: Color(0xFF, 0xFF, 0xFF),
            brightness: 1.0,
            mkey_indicator: true,
            slot_colors: [None, None, None],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedState {
    pub rgb: (u8, u8, u8),
    pub mkeys: u8,
}

/// Map an active M-slot to `slot_colors` index (M1/M2/M3); MR has no slot.
fn slot_index(m: MKey) -> Option<usize> {
    match m {
        MKey::M1 => Some(0),
        MKey::M2 => Some(1),
        MKey::M3 => Some(2),
        MKey::MR => None,
    }
}

/// Resolve (active slot + config) into the hardware LED state.
pub fn resolve(active: MKey, cfg: &BacklightConfig) -> LedState {
    let base = slot_index(active)
        .and_then(|i| cfg.slot_colors[i])
        .unwrap_or(cfg.default_color);
    let scale = cfg.brightness.clamp(0.0, 1.0);
    let scaled = |c: u8| (c as f32 * scale).round() as u8;
    let rgb = (scaled(base.0), scaled(base.1), scaled(base.2));
    let mkeys = if cfg.mkey_indicator {
        match active {
            MKey::M1 => 1,
            MKey::M2 => 2,
            MKey::M3 => 4,
            MKey::MR => 0,
        }
    } else {
        0
    };
    LedState { rgb, mkeys }
}

/// 5-byte SET_REPORT payload for the keypad backlight color (wValue 0x0307).
pub fn color_packet(rgb: (u8, u8, u8)) -> [u8; 5] {
    [0x05, rgb.0, rgb.1, rgb.2, 0x00]
}

/// 5-byte SET_REPORT payload for the M-key indicator LEDs (wValue 0x0305).
pub fn mkey_packet(mask: u8) -> [u8; 5] {
    [0x05, mask, 0x00, 0x00, 0x00]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::MKey;

    #[test]
    fn hex_round_trip() {
        assert_eq!(Color::from_hex("#FF3010"), Some(Color(0xFF, 0x30, 0x10)));
        assert_eq!(Color::from_hex("ff3010"), Some(Color(0xFF, 0x30, 0x10))); // no '#', lowercase
        assert_eq!(Color(0xFF, 0x30, 0x10).to_hex(), "#FF3010");
        assert_eq!(Color::from_hex("white"), None);   // malformed -> None
        assert_eq!(Color::from_hex("#FFF"), None);    // wrong length -> None
    }

    fn cfg() -> BacklightConfig {
        BacklightConfig {
            default_color: Color(200, 200, 200),
            brightness: 1.0,
            mkey_indicator: true,
            slot_colors: [Some(Color(255, 0, 0)), None, Some(Color(0, 0, 255))],
        }
    }

    #[test]
    fn resolve_uses_slot_override() {
        let s = resolve(MKey::M1, &cfg());
        assert_eq!(s.rgb, (255, 0, 0));
        assert_eq!(s.mkeys, 1);
    }

    #[test]
    fn resolve_falls_back_to_default() {
        let s = resolve(MKey::M2, &cfg()); // M2 has no override
        assert_eq!(s.rgb, (200, 200, 200));
        assert_eq!(s.mkeys, 2);
    }

    #[test]
    fn resolve_scales_by_brightness() {
        let mut c = cfg();
        c.brightness = 0.5;
        let s = resolve(MKey::M1, &c); // (255,0,0) * 0.5
        assert_eq!(s.rgb, (128, 0, 0)); // 255*0.5 = 127.5 -> round -> 128
    }

    #[test]
    fn resolve_brightness_zero_is_off() {
        let mut c = cfg();
        c.brightness = 0.0;
        assert_eq!(resolve(MKey::M3, &c).rgb, (0, 0, 0));
    }

    #[test]
    fn resolve_indicator_off_clears_mkeys() {
        let mut c = cfg();
        c.mkey_indicator = false;
        assert_eq!(resolve(MKey::M3, &c).mkeys, 0);
    }

    #[test]
    fn resolve_mr_has_no_indicator_and_default_color() {
        let s = resolve(MKey::MR, &cfg());
        assert_eq!(s.mkeys, 0);
        assert_eq!(s.rgb, (200, 200, 200));
    }

    #[test]
    fn default_config_is_white_full_indicator() {
        let d = BacklightConfig::default();
        assert_eq!(d.default_color, Color(0xFF, 0xFF, 0xFF));
        assert_eq!(d.brightness, 1.0);
        assert!(d.mkey_indicator);
        assert_eq!(d.slot_colors, [None, None, None]);
    }

    #[test]
    fn packets_have_expected_layout() {
        assert_eq!(color_packet((0x11, 0x22, 0x33)), [0x05, 0x11, 0x22, 0x33, 0x00]);
        assert_eq!(mkey_packet(0b0000_0100), [0x05, 0x04, 0x00, 0x00, 0x00]);
    }
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use crate::config::ProfileSet;
use crate::protocol::MKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    /// Parse `#RRGGBB` or `RRGGBB` (case-insensitive). `None` on any malformed input.
    pub fn from_hex(s: &str) -> Option<Color> {
        let h = s.strip_prefix('#').unwrap_or(s);
        if h.len() != 6 || !h.is_ascii() {
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
/// Invariant: the M-key bitmask used below in `resolve` equals `1 << slot_index(m)`
/// for M1/M2/M3 (1, 2, 4 respectively) — keep this match and the one in `resolve`
/// in sync if M-key slots ever change.
fn slot_index(m: MKey) -> Option<usize> {
    match m {
        MKey::M1 => Some(0),
        MKey::M2 => Some(1),
        MKey::M3 => Some(2),
        MKey::MR => None,
    }
}

/// Resolve (active slot + dry-run + config) into the hardware LED state. While
/// `dry_run` is true, the MR indicator bit (8) also lights (gated by
/// `mkey_indicator`, like the active-slot bits) so the device always shows the
/// active/dry-run mode even though MR itself has no dedicated slot indicator.
pub fn resolve(active: MKey, dry_run: bool, cfg: &BacklightConfig) -> LedState {
    let base = slot_index(active)
        .and_then(|i| cfg.slot_colors[i])
        .unwrap_or(cfg.default_color);
    let scale = cfg.brightness.clamp(0.0, 1.0);
    let scaled = |c: u8| (c as f32 * scale).round() as u8;
    let rgb = (scaled(base.0), scaled(base.1), scaled(base.2));
    let mkeys = if cfg.mkey_indicator {
        let active_bit = match active {
            MKey::M1 => 1,
            MKey::M2 => 2,
            MKey::M3 => 4,
            MKey::MR => 0,
        };
        active_bit | if dry_run { 8 } else { 0 }
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

/// Poll the active profile + backlight config every ~150 ms and publish the
/// resolved LedState into the shared cell the USB reader consumes. Every change
/// source (device M-key, GUI edit, hot-reload) reconciles through this one loop.
pub fn spawn_poller(config: Arc<RwLock<ProfileSet>>, dry_run: Arc<AtomicBool>, desired: Arc<Mutex<LedState>>) {
    thread::spawn(move || loop {
        let state = config.read().unwrap().desired_led_state(dry_run.load(Ordering::Relaxed));
        *desired.lock().unwrap() = state;
        thread::sleep(Duration::from_millis(150));
    });
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

    #[test]
    fn from_hex_rejects_non_ascii_without_panicking() {
        // "€€" is 6 bytes (3 bytes/char) but not 6 chars; slicing by byte index
        // would land mid-codepoint and panic. Must return None instead.
        assert_eq!(Color::from_hex("€€"), None);
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
        let s = resolve(MKey::M1, false, &cfg());
        assert_eq!(s.rgb, (255, 0, 0));
        assert_eq!(s.mkeys, 1);
    }

    #[test]
    fn resolve_falls_back_to_default() {
        let s = resolve(MKey::M2, false, &cfg()); // M2 has no override
        assert_eq!(s.rgb, (200, 200, 200));
        assert_eq!(s.mkeys, 2);
    }

    #[test]
    fn resolve_scales_by_brightness() {
        let mut c = cfg();
        c.brightness = 0.5;
        let s = resolve(MKey::M1, false, &c); // (255,0,0) * 0.5
        assert_eq!(s.rgb, (128, 0, 0)); // 255*0.5 = 127.5 -> round -> 128
    }

    #[test]
    fn resolve_brightness_zero_is_off() {
        let mut c = cfg();
        c.brightness = 0.0;
        assert_eq!(resolve(MKey::M3, false, &c).rgb, (0, 0, 0));
    }

    #[test]
    fn resolve_indicator_off_clears_mkeys() {
        let mut c = cfg();
        c.mkey_indicator = false;
        assert_eq!(resolve(MKey::M3, false, &c).mkeys, 0);
    }

    #[test]
    fn resolve_mr_has_no_indicator_and_default_color() {
        let s = resolve(MKey::MR, false, &cfg());
        assert_eq!(s.mkeys, 0);
        assert_eq!(s.rgb, (200, 200, 200));
    }

    #[test]
    fn resolve_mr_lights_in_dry_run_when_indicator_on() {
        let cfg = cfg(); // existing helper: mkey_indicator = true
        assert_eq!(resolve(MKey::M1, true, &cfg).mkeys, 1 | 8); // M1 + MR
        assert_eq!(resolve(MKey::M1, false, &cfg).mkeys, 1);    // active only
    }

    #[test]
    fn resolve_dry_run_mr_gated_by_indicator() {
        let mut c = cfg();
        c.mkey_indicator = false;
        assert_eq!(resolve(MKey::M1, true, &c).mkeys, 0); // indicator off → nothing, even in dry-run
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

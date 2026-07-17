mod font;

use crate::config::ProfileSet;
use crate::protocol::{G13Event, MKey};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

pub const LCD_W: usize = 160;
pub const LCD_H: usize = 43;

/// A 160×43 1-bit framebuffer, row-major. Out-of-bounds writes are ignored so
/// rendering never panics.
pub struct Framebuffer {
    pixels: Vec<bool>,
}

impl Framebuffer {
    pub fn new() -> Self {
        Self { pixels: vec![false; LCD_W * LCD_H] }
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, on: bool) {
        if x < 0 || y < 0 || x as usize >= LCD_W || y as usize >= LCD_H {
            return;
        }
        self.pixels[y as usize * LCD_W + x as usize] = on;
    }

    pub fn get(&self, x: usize, y: usize) -> bool {
        if x >= LCD_W || y >= LCD_H {
            return false;
        }
        self.pixels[y * LCD_W + x]
    }

    pub fn draw_hline(&mut self, x: i32, y: i32, len: i32) {
        for i in 0..len {
            self.set_pixel(x + i, y, true);
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, on: bool) {
        for dy in 0..h {
            for dx in 0..w {
                self.set_pixel(x + dx, y + dy, on);
            }
        }
    }

    /// Pack into the 992-byte G13 LCD frame: 32-byte header (`[0]=0x03`) + 960
    /// bytes of 6-page column data. Pixel (x,y) -> bit y%8 of byte 32 + x + (y/8)*160.
    pub fn pack(&self) -> [u8; 992] {
        let mut frame = [0u8; 992];
        frame[0] = 0x03;
        for y in 0..LCD_H {
            for x in 0..LCD_W {
                if self.pixels[y * LCD_W + x] {
                    frame[32 + x + (y / 8) * LCD_W] |= 1 << (y % 8);
                }
            }
        }
        frame
    }

    /// Draw a 5-column glyph at (x,y); each set bit becomes a `scale`×`scale` block.
    pub fn draw_glyph(&mut self, x: i32, y: i32, glyph: &[u8; 5], scale: i32) {
        for (col, bits) in glyph.iter().enumerate() {
            for row in 0..8 {
                if bits & (1 << row) != 0 {
                    self.fill_rect(
                        x + col as i32 * scale,
                        y + row * scale,
                        scale, scale, true,
                    );
                }
            }
        }
    }

    pub fn draw_char(&mut self, x: i32, y: i32, ch: char, scale: i32) {
        self.draw_glyph(x, y, font::glyph(ch), scale);
    }

    /// Draw a string left-to-right; each cell is 6px wide (5 glyph + 1 gap) × scale.
    pub fn draw_text(&mut self, x: i32, y: i32, text: &str, scale: i32) {
        let mut cx = x;
        for ch in text.chars() {
            self.draw_char(cx, y, ch, scale);
            cx += 6 * scale;
        }
    }
}

/// Pixel width a string occupies: 6px (5 glyph + 1 gap) per char × scale.
pub fn text_width(text: &str, scale: i32) -> i32 {
    text.chars().count() as i32 * 6 * scale
}

/// On a discrete-button KeyDown, record what it fired (button name + bound combo
/// + label) from the profile active at press time. No-op for other events.
pub fn capture(
    event: &G13Event,
    profiles: &Arc<RwLock<ProfileSet>>,
    cell: &Arc<Mutex<Option<LastAction>>>,
) {
    let G13Event::KeyDown(key) = event else { return };
    let key = *key;
    let (combo, label) = {
        let set = profiles.read().unwrap();
        match set.active_profile() {
            Some(p) => (
                p.get_binding(key).map(str::to_string),
                p.label(key).map(str::to_string),
            ),
            None => (None, None),
        }
    };
    *cell.lock().unwrap() = Some(LastAction {
        button: format!("{key:?}"),
        combo,
        label,
    });
}

impl Default for Framebuffer {
    fn default() -> Self { Self::new() }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode { Active, DryRun }

#[derive(Clone, PartialEq, Debug)]
pub struct LastAction {
    pub button: String,
    pub combo: Option<String>,
    pub label: Option<String>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct LcdModel {
    pub mode: Mode,
    pub slot: MKey,
    pub profile_name: Option<String>,
    pub last: Option<LastAction>,
}

fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn slot_label(slot: MKey) -> &'static str {
    match slot { MKey::M1 => "M1", MKey::M2 => "M2", MKey::M3 => "M3", MKey::MR => "MR" }
}

/// Render the HUD: title + mode (y0), divider (y9), slot + profile name at 2×
/// (y12), last action (y30).
pub fn render(model: &LcdModel) -> Framebuffer {
    let mut fb = Framebuffer::new();

    // Title (left) + mode (right).
    fb.draw_text(0, 0, "G13 Driver", 1);
    let (mode_text, filled) = match model.mode {
        Mode::Active => ("ACTIVE", true),
        Mode::DryRun => ("DRY-RUN", false),
    };
    let box_x = LCD_W as i32 - text_width(mode_text, 1) - 9;
    if filled {
        fb.fill_rect(box_x, 1, 6, 6, true);
    } else {
        fb.draw_hline(box_x, 1, 6);
        fb.draw_hline(box_x, 6, 6);
        for dy in 1..7 { fb.set_pixel(box_x, dy, true); fb.set_pixel(box_x + 5, dy, true); }
    }
    fb.draw_text(box_x + 8, 0, mode_text, 1);

    // Divider.
    fb.draw_hline(0, 9, LCD_W as i32);

    // Slot + profile name (name at 2×). Slot label single height, baseline-ish aligned.
    fb.draw_text(0, 16, slot_label(model.slot), 1);
    let name = match &model.profile_name {
        Some(n) => truncate(n, 12),
        None => "(empty)".to_string(),
    };
    fb.draw_text(18, 12, &name, 2);

    // Last action: "BUTTON  combo  label" (blank when None).
    if let Some(a) = &model.last {
        let mut line = a.button.clone();
        match &a.combo {
            Some(c) => { line.push_str("  "); line.push_str(c); }
            None => line.push_str("  (unbound)"),
        }
        if let Some(l) = &a.label {
            line.push_str("  ");
            line.push_str(l);
        }
        fb.draw_text(0, 32, &truncate(&line, 26), 1);
    }

    fb
}

/// Rebuild the LCD frame from live state every ~150 ms and publish it. `dry_run`
/// drives Active/Dry-run; headless passes an always-false flag.
pub fn spawn_poller(
    profiles: Arc<RwLock<ProfileSet>>,
    dry_run: Arc<AtomicBool>,
    last: Arc<Mutex<Option<LastAction>>>,
    frame: Arc<Mutex<[u8; 992]>>,
) {
    thread::spawn(move || loop {
        let model = {
            let set = profiles.read().unwrap();
            LcdModel {
                mode: if dry_run.load(Ordering::Relaxed) { Mode::DryRun } else { Mode::Active },
                slot: set.active(),
                profile_name: set.active_name_stem().map(str::to_string),
                last: last.lock().unwrap().clone(),
            }
        };
        let packed = render(&model).pack();
        *frame.lock().unwrap() = packed;
        thread::sleep(Duration::from_millis(150));
    });
}

macro_rules! str_enum {
    ($name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub fn parse(s: &str) -> Option<Self> {
                match s.trim().to_ascii_lowercase().as_str() {
                    $($s => Some(Self::$variant),)+
                    _ => None,
                }
            }
            pub fn as_str(&self) -> &'static str {
                match self { $(Self::$variant => $s),+ }
            }
        }
    };
}
str_enum!(Line1Left { Name => "name", Version => "version" });
str_enum!(ModeDisplay { Label => "label", Icon => "icon", Off => "off" });
str_enum!(Line2Source { Filename => "filename", Display => "display" });
str_enum!(Line3Trigger { Last => "last", Held => "held" });

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LcdConfig {
    pub line1_left: Line1Left,
    pub line1_clock: bool,
    pub line1_mode: ModeDisplay,
    pub line2_source: Line2Source,
    pub line3_trigger: Line3Trigger,
    pub line3_mapping: bool,
    pub line3_label: bool,
}
impl Default for LcdConfig {
    fn default() -> Self {
        Self {
            line1_left: Line1Left::Name,
            line1_clock: false,
            line1_mode: ModeDisplay::Label,
            line2_source: Line2Source::Filename,
            line3_trigger: Line3Trigger::Last,
            line3_mapping: true,
            line3_label: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_pixel_is_bounds_safe() {
        let mut fb = Framebuffer::new();
        fb.set_pixel(-1, 0, true);      // no panic
        fb.set_pixel(0, 999, true);     // no panic
        fb.set_pixel(5, 6, true);
        assert!(fb.get(5, 6));
        assert!(!fb.get(6, 5));
    }

    #[test]
    fn pack_layout_is_992_bytes_with_header() {
        let fb = Framebuffer::new();
        let frame = fb.pack();
        assert_eq!(frame.len(), 992);
        assert_eq!(frame[0], 0x03);
        assert!(frame[1..32].iter().all(|&b| b == 0));
        assert!(frame[32..].iter().all(|&b| b == 0)); // blank fb -> zero body
    }

    #[test]
    fn pack_maps_pixel_to_correct_byte_and_bit() {
        let mut fb = Framebuffer::new();
        // (x=3, y=10): page = 10/8 = 1, bit = 10%8 = 2, byte = 32 + 3 + 1*160 = 195.
        fb.set_pixel(3, 10, true);
        let frame = fb.pack();
        assert_eq!(frame[195], 1 << 2);
        // (x=0, y=0): byte 32, bit 0.
        let mut fb2 = Framebuffer::new();
        fb2.set_pixel(0, 0, true);
        assert_eq!(fb2.pack()[32], 1 << 0);
        // (x=159, y=42): page 5, bit 2, byte 32 + 159 + 5*160 = 991.
        let mut fb3 = Framebuffer::new();
        fb3.set_pixel(159, 42, true);
        assert_eq!(fb3.pack()[991], 1 << 2);
    }

    #[test]
    fn draw_hline_and_fill_rect() {
        let mut fb = Framebuffer::new();
        fb.draw_hline(2, 4, 3); // (2,4),(3,4),(4,4)
        assert!(fb.get(2, 4) && fb.get(3, 4) && fb.get(4, 4));
        assert!(!fb.get(5, 4));
        fb.fill_rect(0, 0, 2, 2, true);
        assert!(fb.get(0, 0) && fb.get(1, 1));
        assert!(!fb.get(2, 2));
    }

    #[test]
    fn draw_glyph_places_bits_top_to_bottom() {
        let mut fb = Framebuffer::new();
        // Synthetic glyph: column 0 has row 0 and row 2 set; other columns empty.
        let g = [0b0000_0101u8, 0, 0, 0, 0];
        fb.draw_glyph(10, 20, &g, 1);
        assert!(fb.get(10, 20));       // col 0, row 0
        assert!(!fb.get(10, 21));      // row 1 clear
        assert!(fb.get(10, 22));       // col 0, row 2
        assert!(!fb.get(11, 20));      // col 1 empty
    }

    #[test]
    fn draw_glyph_scale_2_doubles_each_pixel() {
        let mut fb = Framebuffer::new();
        let g = [0b0000_0001u8, 0, 0, 0, 0]; // col 0, row 0 only
        fb.draw_glyph(0, 0, &g, 2);
        // one source pixel -> a 2x2 block
        assert!(fb.get(0, 0) && fb.get(1, 0) && fb.get(0, 1) && fb.get(1, 1));
        assert!(!fb.get(2, 0) && !fb.get(0, 2));
    }

    #[test]
    fn text_width_is_six_px_per_char_times_scale() {
        assert_eq!(text_width("AB", 1), 12);
        assert_eq!(text_width("AB", 2), 24);
        assert_eq!(text_width("", 1), 0);
    }

    #[test]
    fn draw_text_and_space_glyph_is_blank() {
        let mut fb = Framebuffer::new();
        fb.draw_text(0, 0, " ", 1); // space -> nothing set, no panic
        assert!((0..6).all(|x| (0..8).all(|y| !fb.get(x, y))));
        fb.draw_text(150, 0, "ABCDEFG", 1); // runs off the right edge -> no panic
    }

    fn any_pixel_in_row_band(fb: &Framebuffer, y0: usize, y1: usize) -> bool {
        (0..LCD_W).any(|x| (y0..y1).any(|y| fb.get(x, y)))
    }

    #[test]
    fn render_draws_divider_and_regions() {
        let m = LcdModel {
            mode: Mode::Active,
            slot: MKey::M2,
            profile_name: Some("Media".to_string()),
            last: Some(LastAction {
                button: "G12".to_string(),
                combo: Some("ctrl+c".to_string()),
                label: Some("Copy".to_string()),
            }),
        };
        let fb = render(&m);
        // title band (top ~8px) has content
        assert!(any_pixel_in_row_band(&fb, 0, 8));
        // divider row present around y=9
        assert!((0..LCD_W).filter(|&x| fb.get(x, 9)).count() > 20);
        // profile band (~y12..27) has content (2x name)
        assert!(any_pixel_in_row_band(&fb, 12, 27));
        // last-action band (~y30..40) has content
        assert!(any_pixel_in_row_band(&fb, 30, 40));
    }

    #[test]
    fn render_empty_slot_and_no_last_action() {
        let m = LcdModel {
            mode: Mode::DryRun,
            slot: MKey::M3,
            profile_name: None,
            last: None,
        };
        let fb = render(&m); // must not panic; renders "(empty)" and blank action line
        assert!(any_pixel_in_row_band(&fb, 12, 27)); // still shows slot + (empty)
        assert!(!any_pixel_in_row_band(&fb, 30, 40)); // no last action -> blank
    }

    #[test]
    fn truncate_hard_cuts() {
        assert_eq!(truncate("abcdef", 3), "abc");
        assert_eq!(truncate("ab", 5), "ab");
    }

    use crate::protocol::G13Key;

    // Build a single-M1-profile ProfileSet with G1->ctrl+c labelled "Copy".
    fn profiles_fixture() -> Arc<RwLock<crate::config::ProfileSet>> {
        let d = std::env::temp_dir().join("g13-lcd-capture");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("profiles/basic.toml"),
            "[keys]\nG1 = \"ctrl+c\"\n\n[labels]\nG1 = \"Copy\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n").unwrap();
        Arc::new(RwLock::new(crate::config::ProfileSet::load(&d.join("config.toml")).unwrap()))
    }

    #[test]
    fn capture_resolves_binding_and_label() {
        let p = profiles_fixture();
        let cell = Arc::new(Mutex::new(None));
        capture(&G13Event::KeyDown(G13Key::G1), &p, &cell);
        let got = cell.lock().unwrap().clone().unwrap();
        assert_eq!(got.button, "G1");
        assert_eq!(got.combo.as_deref(), Some("ctrl+c"));
        assert_eq!(got.label.as_deref(), Some("Copy"));
    }

    #[test]
    fn capture_unbound_key_has_no_combo() {
        let p = profiles_fixture();
        let cell = Arc::new(Mutex::new(None));
        capture(&G13Event::KeyDown(G13Key::G7), &p, &cell); // G7 unbound
        let got = cell.lock().unwrap().clone().unwrap();
        assert_eq!(got.button, "G7");
        assert_eq!(got.combo, None);
        assert_eq!(got.label, None);
    }

    #[test]
    fn capture_active_slot_empty_has_no_combo_or_label() {
        // Manifest mode (profiles_dir set) with no m1/m2/m3 assigned: active
        // defaults to M1, whose slot is empty, so active_profile() is None.
        let d = std::env::temp_dir().join("g13-lcd-capture-empty");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("config.toml"), "profiles_dir = \"profiles\"\n").unwrap();
        let p = Arc::new(RwLock::new(
            crate::config::ProfileSet::load(&d.join("config.toml")).unwrap(),
        ));
        assert!(p.read().unwrap().active_profile().is_none());

        let cell = Arc::new(Mutex::new(None));
        capture(&G13Event::KeyDown(G13Key::G1), &p, &cell);
        let got = cell.lock().unwrap().clone().unwrap();
        assert_eq!(got.button, "G1");
        assert_eq!(got.combo, None);
        assert_eq!(got.label, None);
    }

    #[test]
    fn capture_ignores_non_keydown() {
        let p = profiles_fixture();
        let cell = Arc::new(Mutex::new(None));
        capture(&G13Event::KeyUp(G13Key::G1), &p, &cell);
        capture(&G13Event::JoystickMove { x: 0, y: 0 }, &p, &cell);
        capture(&G13Event::MKeyDown(MKey::M2), &p, &cell);
        assert!(cell.lock().unwrap().is_none());
    }

    #[test]
    fn enum_parse_and_as_str_round_trip() {
        assert_eq!(Line1Left::parse("version"), Some(Line1Left::Version));
        assert_eq!(Line1Left::Version.as_str(), "version");
        assert_eq!(ModeDisplay::parse("off"), Some(ModeDisplay::Off));
        assert_eq!(Line2Source::parse("display"), Some(Line2Source::Display));
        assert_eq!(Line3Trigger::parse("held"), Some(Line3Trigger::Held));
        assert_eq!(Line1Left::parse("bogus"), None);
    }

    #[test]
    fn lcd_config_default() {
        let d = LcdConfig::default();
        assert_eq!(d.line1_left, Line1Left::Name);
        assert!(!d.line1_clock);
        assert_eq!(d.line1_mode, ModeDisplay::Label);
        assert_eq!(d.line2_source, Line2Source::Filename);
        assert_eq!(d.line3_trigger, Line3Trigger::Last);
        assert!(d.line3_mapping && d.line3_label);
    }
}

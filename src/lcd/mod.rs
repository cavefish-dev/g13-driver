mod font;

use crate::config::{JoystickDir, ProfileSet};
use crate::joystick::{HoldAction, JoystickMapper};
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum HeldId { Key(crate::protocol::G13Key), Dir(JoystickDir) }

/// Stateful tracker of the currently-held button/direction and the most recent
/// action fired, for the LCD's line-3 activity display. `on_event` drives a
/// `JoystickMapper` internally so joystick direction holds are derived the same
/// way the dispatcher derives key-hold transitions.
pub struct ActivityTracker {
    mapper: JoystickMapper,
    held: Vec<(HeldId, LastAction)>, // ordered, most-recent last
    last: Option<LastAction>,
}

impl ActivityTracker {
    pub fn new() -> Self {
        Self { mapper: JoystickMapper::new(), held: Vec::new(), last: None }
    }

    fn upsert(&mut self, id: HeldId, action: LastAction) {
        self.held.retain(|(hid, _)| *hid != id);
        self.held.push((id, action.clone()));
        self.last = Some(action);
    }

    fn remove(&mut self, id: HeldId) {
        self.held.retain(|(hid, _)| *hid != id);
    }

    pub fn on_event(&mut self, event: &G13Event, profiles: &Arc<RwLock<ProfileSet>>) {
        match event {
            G13Event::KeyDown(key) => {
                let key = *key;
                let (combo, label) = {
                    let set = profiles.read().unwrap();
                    match set.active_profile() {
                        Some(p) => (p.get_binding(key).map(str::to_string), p.label(key).map(str::to_string)),
                        None => (None, None),
                    }
                };
                self.upsert(HeldId::Key(key), LastAction { button: format!("{key:?}"), combo, label });
            }
            G13Event::KeyUp(key) => self.remove(HeldId::Key(*key)),
            G13Event::JoystickMove { x, y } => {
                let (cfg, deadzone) = {
                    let set = profiles.read().unwrap();
                    (set.active_profile().and_then(|p| p.joystick()).cloned(), set.joystick_deadzone())
                };
                let Some(jc) = cfg else { return };
                let actions = self.mapper.update(*x, *y, &jc, deadzone);
                for a in actions {
                    match a {
                        HoldAction::KeyDown { dir, key } => {
                            let label = {
                                let set = profiles.read().unwrap();
                                set.active_profile().and_then(|p| p.joystick_label(dir)).map(str::to_string)
                            };
                            self.upsert(HeldId::Dir(dir), LastAction {
                                button: format!("{dir:?}"), combo: Some(key), label,
                            });
                        }
                        HoldAction::KeyUp { dir, .. } => self.remove(HeldId::Dir(dir)),
                    }
                }
            }
            G13Event::MKeyDown(_) => { self.held.clear(); self.mapper = JoystickMapper::new(); }
            G13Event::MKeyUp(_) => {}
        }
    }

    pub fn current(&self, trigger: Line3Trigger) -> Option<LastAction> {
        match trigger {
            Line3Trigger::Last => self.last.clone(),
            Line3Trigger::Held => self.held.last().map(|(_, a)| a.clone()),
        }
    }
}

impl Default for ActivityTracker {
    fn default() -> Self { Self::new() }
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
    pub filename: Option<String>,
    pub display_name: Option<String>,
    pub last: Option<LastAction>,
    pub clock: Option<String>,
}

fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn slot_label(slot: MKey) -> &'static str {
    match slot { MKey::M1 => "M1", MKey::M2 => "M2", MKey::M3 => "M3", MKey::MR => "MR" }
}

/// Replace any non-printable-ASCII char with `*` so the 5×7 bitmap font (which
/// only has glyphs for `' '..='~'`) never gets asked to render something it
/// can't draw.
pub fn sanitize(s: &str) -> String {
    s.chars().map(|c| if (' '..='~').contains(&c) { c } else { '*' }).collect()
}

/// Current local time as `HH:MM`, via `GetLocalTime`. Non-Windows builds
/// (tests, cross-compiles) return an empty string.
#[cfg(windows)]
pub fn local_hh_mm() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut st: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    format!("{:02}:{:02}", st.wHour, st.wMinute)
}
#[cfg(not(windows))]
pub fn local_hh_mm() -> String { String::new() }

/// Render the HUD per `cfg`: line 1 title/clock/mode (y0), divider (y9), slot +
/// name at 2× (y12), last action (y32). Pure — the clock string is supplied by
/// the caller via `model.clock` rather than read here.
pub fn render(model: &LcdModel, cfg: &LcdConfig) -> Framebuffer {
    let mut fb = Framebuffer::new();

    // Line 1 left.
    let version = format!("v{}", env!("G13_VERSION"));
    let left = match cfg.line1_left { Line1Left::Name => "G13 Driver", Line1Left::Version => version.as_str() };
    // Left strings are one of two fixed short literals ("G13 Driver" / "v{VERSION}"),
    // so they never reach the right cluster and don't need width clamping.
    fb.draw_text(0, 0, left, 1);

    // Line 1 right cluster: [mode]. Build right-to-left.
    let mode_text = match model.mode { Mode::Active => "ACTIVE", Mode::DryRun => "DRY-RUN" };
    let filled = matches!(model.mode, Mode::Active);
    let mut x = LCD_W as i32;
    if cfg.line1_mode == ModeDisplay::Label {
        x -= text_width(mode_text, 1);
        fb.draw_text(x, 0, mode_text, 1);
        x -= 8; // box + gap
    } else if cfg.line1_mode == ModeDisplay::Icon {
        x -= 6;
    }
    if cfg.line1_mode != ModeDisplay::Off {
        let bx = x; // draw the box at bx..bx+6
        if filled { fb.fill_rect(bx, 1, 6, 6, true); }
        else {
            fb.draw_hline(bx, 1, 6); fb.draw_hline(bx, 6, 6);
            for dy in 1..7 { fb.set_pixel(bx, dy, true); fb.set_pixel(bx + 5, dy, true); }
        }
    }

    // Clock: centered on line 1.
    if cfg.line1_clock {
        if let Some(clk) = &model.clock {
            let cx = (LCD_W as i32 - text_width(clk, 1)) / 2;
            fb.draw_text(cx, 0, clk, 1);
        }
    }

    // Divider.
    fb.draw_hline(0, 9, LCD_W as i32);

    // Line 2: slot + name (2x), per source, sanitized.
    fb.draw_text(0, 16, slot_label(model.slot), 1);
    let raw_name = match cfg.line2_source {
        Line2Source::Filename => model.filename.clone(),
        Line2Source::Display => model.display_name.clone().or_else(|| model.filename.clone()),
    };
    let name = match raw_name { Some(n) => truncate(&sanitize(&n), 12), None => "(empty)".to_string() };
    fb.draw_text(18, 12, &name, 2);

    // Line 3: button [+ combo] [+ label] per flags.
    if let Some(a) = &model.last {
        let mut line = a.button.clone();
        if cfg.line3_mapping {
            match &a.combo {
                Some(c) => { line.push_str("  "); line.push_str(c); }
                None => line.push_str("  (unbound)"),
            }
        }
        if cfg.line3_label {
            if let Some(l) = &a.label { line.push_str("  "); line.push_str(l); }
        }
        fb.draw_text(0, 32, &sanitize(&truncate(&line, 26)), 1);
    }
    fb
}

/// Rebuild the LCD frame from live state every ~150 ms and publish it. `dry_run`
/// drives Active/Dry-run; headless passes an always-false flag.
pub fn spawn_poller(
    profiles: Arc<RwLock<ProfileSet>>,
    dry_run: Arc<AtomicBool>,
    tracker: Arc<Mutex<ActivityTracker>>,
    frame: Arc<Mutex<[u8; 992]>>,
) {
    thread::spawn(move || loop {
        // `profiles.read()` guard is dropped before `tracker.lock()` is taken, so we
        // never hold both simultaneously (see `on_event`, which takes `profiles.read()`
        // while holding the tracker lock — the reverse order would deadlock).
        let (cfg, mode, slot, filename, display_name, clock) = {
            let set = profiles.read().unwrap();
            let cfg = set.lcd_config();
            let clock = if cfg.line1_clock { Some(local_hh_mm()) } else { None };
            (
                cfg,
                if dry_run.load(Ordering::Relaxed) { Mode::DryRun } else { Mode::Active },
                set.active(),
                set.active_name_stem().map(str::to_string),
                set.active_profile().and_then(|p| p.meta_name()).map(str::to_string),
                clock,
            )
        };
        let last = tracker.lock().unwrap().current(cfg.line3_trigger);
        let model = LcdModel { mode, slot, filename, display_name, last, clock };
        *frame.lock().unwrap() = render(&model, &cfg).pack();
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

    fn model(last: Option<LastAction>) -> LcdModel {
        LcdModel { mode: Mode::Active, slot: MKey::M2,
            filename: Some("basic".into()), display_name: Some("My Set".into()),
            last, clock: None }
    }

    #[test]
    fn render_draws_divider_and_regions() {
        let mut m = model(Some(LastAction {
            button: "G12".to_string(),
            combo: Some("ctrl+c".to_string()),
            label: Some("Copy".to_string()),
        }));
        m.display_name = Some("Media".to_string());
        let fb = render(&m, &LcdConfig::default());
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
            filename: None,
            display_name: None,
            last: None,
            clock: None,
        };
        let fb = render(&m, &LcdConfig::default()); // must not panic; renders "(empty)" and blank action line
        assert!(any_pixel_in_row_band(&fb, 12, 27)); // still shows slot + (empty)
        assert!(!any_pixel_in_row_band(&fb, 30, 40)); // no last action -> blank
    }

    #[test]
    fn render_line1_version_and_clock() {
        let mut cfg = LcdConfig::default();
        cfg.line1_left = Line1Left::Version;
        cfg.line1_clock = true;
        let mut m = model(None); m.clock = Some("12:34".into());
        let fb = render(&m, &cfg); // must not panic; title band has content
        assert!((0..LCD_W).any(|x| (0..8).any(|y| fb.get(x, y))));
    }

    #[test]
    fn render_clock_is_centered() {
        let mut cfg = LcdConfig::default();
        cfg.line1_clock = true;
        cfg.line1_mode = ModeDisplay::Off; // isolate the clock
        let mut m = model(None); // existing test helper
        m.clock = Some("12:34".into());
        let fb = render(&m, &cfg);
        // "12:34" is 30px wide, centered at x = (160-30)/2 = 65..95 → lit pixels in the center band.
        assert!((65..95).any(|x| (0..8).any(|y| fb.get(x, y))));
        // ...and nothing in the old far-right clock spot (x ~130..160 on the title row).
        assert!(!(130..160).any(|x| (0..8).any(|y| fb.get(x, y))));
    }

    #[test]
    fn render_line2_display_and_sanitize() {
        let mut cfg = LcdConfig::default();
        cfg.line2_source = Line2Source::Display;
        let mut m = model(None); m.display_name = Some("Ünïcode".into());
        let fb = render(&m, &cfg); // non-ASCII replaced by '*', no panic
        assert!((0..LCD_W).any(|x| (12..27).any(|y| fb.get(x, y))));
    }

    #[test]
    fn render_line3_flags() {
        let last = Some(LastAction { button: "G1".into(), combo: Some("ctrl+c".into()), label: Some("Copy".into()) });
        let mut cfg = LcdConfig::default();
        cfg.line3_mapping = false; cfg.line3_label = false;
        let fb = render(&model(last), &cfg); // only the button shows; no panic
        assert!((0..LCD_W).any(|x| (30..40).any(|y| fb.get(x, y))));
    }

    #[test]
    fn sanitize_replaces_non_ascii() {
        assert_eq!(sanitize("aé1"), "a*1");
        assert_eq!(sanitize("ok"), "ok");
    }

    #[test]
    fn truncate_hard_cuts() {
        assert_eq!(truncate("abcdef", 3), "abc");
        assert_eq!(truncate("ab", 5), "ab");
    }

    use crate::protocol::G13Key;

    // A unique temp dir per call — fixtures must NOT share a fixed path, or tests
    // running in parallel race on remove/recreate (flaky "combo: None" on CI).
    fn unique_dir(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static CTR: AtomicU64 = AtomicU64::new(0);
        let n = CTR.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("g13-lcd-{tag}-{n}"));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    // Build a single-M1-profile ProfileSet with G1->ctrl+c labelled "Copy".
    fn profiles_fixture() -> Arc<RwLock<crate::config::ProfileSet>> {
        let d = unique_dir("capture");
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("profiles/basic.toml"),
            "[keys]\nG1 = \"ctrl+c\"\n\n[labels]\nG1 = \"Copy\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n").unwrap();
        Arc::new(RwLock::new(crate::config::ProfileSet::load(&d.join("config.toml")).unwrap()))
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

    // --- ActivityTracker ---

    fn joystick_profiles_fixture() -> Arc<RwLock<crate::config::ProfileSet>> {
        let d = unique_dir("tracker-joystick");
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("profiles/basic.toml"),
            "[joystick]\nup = \"w\"\n\n[joystick.labels]\nup = \"Fwd\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n").unwrap();
        Arc::new(RwLock::new(crate::config::ProfileSet::load(&d.join("config.toml")).unwrap()))
    }

    #[test]
    fn tracker_key_held_and_last() {
        let p = profiles_fixture(); // [keys] G1="ctrl+c" [labels] G1="Copy"
        let mut t = ActivityTracker::new();
        t.on_event(&G13Event::KeyDown(G13Key::G1), &p);
        let want = LastAction { button: "G1".into(), combo: Some("ctrl+c".into()), label: Some("Copy".into()) };
        assert_eq!(t.current(Line3Trigger::Held), Some(want.clone()));
        assert_eq!(t.current(Line3Trigger::Last), Some(want.clone()));
        t.on_event(&G13Event::KeyUp(G13Key::G1), &p);
        assert_eq!(t.current(Line3Trigger::Held), None);
        assert_eq!(t.current(Line3Trigger::Last), Some(want));
    }

    #[test]
    fn tracker_joystick_direction_held_and_released() {
        let p = joystick_profiles_fixture();
        let mut t = ActivityTracker::new();
        t.on_event(&G13Event::JoystickMove { x: 127, y: 0 }, &p);
        let want = LastAction { button: "Up".into(), combo: Some("w".into()), label: Some("Fwd".into()) };
        assert_eq!(t.current(Line3Trigger::Held), Some(want.clone()));
        assert_eq!(t.current(Line3Trigger::Last), Some(want));
        t.on_event(&G13Event::JoystickMove { x: 127, y: 127 }, &p); // center
        assert_eq!(t.current(Line3Trigger::Held), None);
    }

    #[test]
    fn tracker_mkey_down_clears_held_but_keeps_last() {
        let p = profiles_fixture();
        let mut t = ActivityTracker::new();
        t.on_event(&G13Event::KeyDown(G13Key::G1), &p);
        let want = LastAction { button: "G1".into(), combo: Some("ctrl+c".into()), label: Some("Copy".into()) };
        assert_eq!(t.current(Line3Trigger::Held), Some(want.clone()));
        t.on_event(&G13Event::MKeyDown(MKey::M2), &p);
        assert_eq!(t.current(Line3Trigger::Held), None);
        assert_eq!(t.current(Line3Trigger::Last), Some(want));
    }
}

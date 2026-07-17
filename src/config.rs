use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use crate::protocol::{G13Key, MKey};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub(crate) struct RawMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct RawConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<RawMeta>,
    #[serde(default)]
    pub keys: HashMap<String, String>,
    #[serde(default)]
    pub joystick: Option<RawJoystick>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub repeat: HashMap<String, bool>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawJoystick {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadzone: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub down: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<std::collections::HashMap<String, bool>>,
}

#[derive(Debug, Deserialize, Clone)]
struct RawAutoRepeat {
    #[serde(default = "default_delay_ms")]
    delay_ms: u64,
    #[serde(default = "default_interval_ms")]
    interval_ms: u64,
}

fn default_delay_ms() -> u64 { 400 }
fn default_interval_ms() -> u64 { 40 }

#[derive(Debug, Deserialize)]
struct RawManifestJoystick {
    #[serde(default = "default_global_deadzone")]
    deadzone: u16,
}
fn default_global_deadzone() -> u16 { 50 }

/// Global auto-repeat timing (from the manifest `[autorepeat]`; defaults when absent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoRepeat {
    pub delay_ms: u64,
    pub interval_ms: u64,
}

impl Default for AutoRepeat {
    fn default() -> Self { Self { delay_ms: 400, interval_ms: 40 } }
}

impl AutoRepeat {
    fn from_raw(r: RawAutoRepeat) -> Self {
        Self {
            delay_ms: r.delay_ms,
            interval_ms: r.interval_ms.max(1), // 0 would busy-spin the tick
        }
    }
}

/// Where a profile came from. Absent/unknown in the file => `User`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProfileSource {
    #[default]
    User,
    Github,
}

impl ProfileSource {
    pub(crate) fn parse(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("github") { Self::Github } else { Self::User }
    }
    fn as_str(self) -> Option<&'static str> {
        match self { Self::Github => Some("github"), Self::User => None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoystickDir { Up, Down, Left, Right }

impl JoystickDir {
    pub fn as_str(&self) -> &'static str {
        match self { Self::Up => "up", Self::Down => "down", Self::Left => "left", Self::Right => "right" }
    }
}

pub fn parse_joystick_dir(s: &str) -> Option<JoystickDir> {
    match s.to_ascii_lowercase().as_str() {
        "up" => Some(JoystickDir::Up),
        "down" => Some(JoystickDir::Down),
        "left" => Some(JoystickDir::Left),
        "right" => Some(JoystickDir::Right),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct JoystickConfig {
    pub up: Option<String>,
    pub down: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Profile {
    key_bindings: HashMap<G13Key, String>,
    joystick: Option<JoystickConfig>,
    repeat: HashMap<G13Key, bool>,
    labels: HashMap<G13Key, String>,
    joystick_labels: HashMap<JoystickDir, String>,
    joystick_repeat: HashMap<JoystickDir, bool>,
    meta_name: Option<String>,
    source: ProfileSource,
    modified: bool,
    origin: Option<String>,
}

impl Profile {
    pub fn load(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let raw: RawConfig = toml::from_str(&content)
            .with_context(|| format!("failed to parse config: {}", path.display()))?;
        Self::from_raw(raw)
    }

    pub(crate) fn from_raw(raw: RawConfig) -> Result<Self> {
        let mut key_bindings = HashMap::new();
        for (name, binding) in raw.keys {
            let key = parse_g13_key(&name)
                .with_context(|| format!("unknown G13 key: {}", name))?;
            key_bindings.insert(key, binding);
        }

        let mut repeat = HashMap::new();
        for (name, on) in raw.repeat {
            let key = parse_g13_key(&name)
                .with_context(|| format!("unknown G13 key in [repeat]: {}", name))?;
            repeat.insert(key, on);
        }

        let mut labels = HashMap::new();
        for (name, text) in raw.labels {
            let key = parse_g13_key(&name)
                .with_context(|| format!("unknown G13 key in [labels]: {}", name))?;
            let text = text.trim().to_string();
            if !text.is_empty() { labels.insert(key, text); }
        }

        let mut joystick_labels = HashMap::new();
        let mut joystick_repeat = HashMap::new();
        let joystick = match raw.joystick {
            Some(rj) => {
                if let Some(labels) = &rj.labels {
                    for (name, text) in labels {
                        let dir = parse_joystick_dir(name)
                            .with_context(|| format!("unknown joystick direction in [joystick.labels]: {name}"))?;
                        let text = text.trim().to_string();
                        if !text.is_empty() { joystick_labels.insert(dir, text); }
                    }
                }
                if let Some(rep) = &rj.repeat {
                    for (name, on) in rep {
                        let dir = parse_joystick_dir(name)
                            .with_context(|| format!("unknown joystick direction in [joystick.repeat]: {name}"))?;
                        joystick_repeat.insert(dir, *on);
                    }
                }
                Some(parse_joystick(rj)?)
            }
            None => None,
        };

        let (meta_name, source, modified, origin) = match raw.meta {
            Some(m) => (
                m.name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                m.source.as_deref().map(ProfileSource::parse).unwrap_or_default(),
                m.modified.unwrap_or(false),
                m.origin.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            ),
            None => (None, ProfileSource::User, false, None),
        };

        Ok(Self {
            key_bindings, joystick, repeat, labels, joystick_labels, joystick_repeat,
            meta_name, source, modified, origin,
        })
    }

    pub fn get_binding(&self, key: G13Key) -> Option<&str> {
        self.key_bindings.get(&key).map(|s| s.as_str())
    }

    pub fn joystick(&self) -> Option<&JoystickConfig> {
        self.joystick.as_ref()
    }

    pub fn set_joystick(&mut self, joystick: Option<JoystickConfig>) {
        self.joystick = joystick;
    }

    pub fn bindings(&self) -> &HashMap<G13Key, String> {
        &self.key_bindings
    }

    pub fn set_bindings(&mut self, bindings: HashMap<G13Key, String>) {
        self.key_bindings = bindings;
    }

    // Schema read-accessor; exercised by tests, kept as the symmetric pair to set_meta_name.
    #[allow(dead_code)]
    pub fn meta_name(&self) -> Option<&str> {
        self.meta_name.as_deref()
    }

    pub fn set_meta_name(&mut self, name: Option<String>) {
        self.meta_name = name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    }

    pub fn source(&self) -> ProfileSource { self.source }
    pub fn set_source(&mut self, source: ProfileSource) { self.source = source; }
    pub fn modified(&self) -> bool { self.modified }
    pub fn set_modified(&mut self, modified: bool) { self.modified = modified; }
    pub fn origin(&self) -> Option<&str> { self.origin.as_deref() }
    pub fn set_origin(&mut self, origin: Option<String>) {
        self.origin = origin.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    }

    pub fn repeats(&self, key: G13Key) -> bool {
        *self.repeat.get(&key).unwrap_or(&false)
    }

    pub fn set_repeat(&mut self, repeat: HashMap<G13Key, bool>) {
        self.repeat = repeat;
    }

    pub fn label(&self, key: G13Key) -> Option<&str> {
        self.labels.get(&key).map(|s| s.as_str())
    }

    pub fn set_labels(&mut self, labels: HashMap<G13Key, String>) {
        self.labels = labels.into_iter()
            .map(|(k, v)| (k, v.trim().to_string()))
            .filter(|(_, v)| !v.is_empty())
            .collect();
    }

    pub fn joystick_label(&self, dir: JoystickDir) -> Option<&str> {
        self.joystick_labels.get(&dir).map(String::as_str)
    }

    pub fn joystick_repeats(&self, dir: JoystickDir) -> bool {
        self.joystick_repeat.get(&dir).copied().unwrap_or(false)
    }

    pub fn set_joystick_labels(&mut self, m: HashMap<JoystickDir, String>) {
        self.joystick_labels = m;
    }
    pub fn set_joystick_repeat(&mut self, m: HashMap<JoystickDir, bool>) {
        self.joystick_repeat = m;
    }

    /// Serialize this profile back to TOML (keys + joystick + repeat). Comments in the
    /// original file are not preserved (the file becomes GUI-managed).
    pub fn to_toml(&self) -> Result<String> {
        let keys: HashMap<String, String> = self.key_bindings.iter()
            .map(|(k, v)| (format!("{k:?}"), v.clone())) // Debug of G13Key is "G1".."G22"
            .collect();
        let joystick = self.joystick.as_ref()
            .filter(|j| j.up.is_some() || j.down.is_some() || j.left.is_some() || j.right.is_some())
            .map(|j| RawJoystick {
                mode: None,
                deadzone: None,
                up: j.up.clone(),
                down: j.down.clone(),
                left: j.left.clone(),
                right: j.right.clone(),
                labels: {
                    let m: std::collections::HashMap<String, String> = self.joystick_labels.iter()
                        .filter(|(_, v)| !v.trim().is_empty())
                        .map(|(d, v)| (d.as_str().to_string(), v.clone()))
                        .collect();
                    if m.is_empty() { None } else { Some(m) }
                },
                repeat: {
                    let m: std::collections::HashMap<String, bool> = self.joystick_repeat.iter()
                        .filter(|(_, &v)| v)
                        .map(|(d, _)| (d.as_str().to_string(), true))
                        .collect();
                    if m.is_empty() { None } else { Some(m) }
                },
            });
        let repeat: HashMap<String, bool> = self.repeat.iter()
            .filter(|(_, &v)| v)
            .map(|(k, _)| (format!("{k:?}"), true)) // Debug of G13Key: "G1".."Stick"
            .collect();
        let labels: HashMap<String, String> = self.labels.iter()
            .filter(|(_, v)| !v.trim().is_empty())
            .map(|(k, v)| (format!("{k:?}"), v.clone()))
            .collect();
        let meta = {
            let name = self.meta_name.clone();
            let source = self.source.as_str().map(str::to_string);
            let modified = if self.modified { Some(true) } else { None };
            let origin = self.origin.clone();
            if name.is_some() || source.is_some() || modified.is_some() || origin.is_some() {
                Some(RawMeta { name, source, modified, origin })
            } else {
                None
            }
        };
        let raw = RawConfig { meta, keys, joystick, repeat, labels };
        toml::to_string(&raw).context("failed to serialize profile")
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            key_bindings: HashMap::new(),
            joystick: None,
            repeat: HashMap::new(),
            labels: HashMap::new(),
            joystick_labels: HashMap::new(),
            joystick_repeat: HashMap::new(),
            meta_name: None,
            source: ProfileSource::User,
            modified: false,
            origin: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawApp {
    #[serde(default)]
    start_active: bool,
}

#[derive(Debug, Deserialize)]
struct RawBacklight {
    #[serde(default)]
    default_color: Option<String>,
    #[serde(default)]
    brightness: Option<f32>,
    #[serde(default)]
    mkey_indicator: Option<bool>,
    #[serde(default)]
    m1_color: Option<String>,
    #[serde(default)]
    m2_color: Option<String>,
    #[serde(default)]
    m3_color: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawLcd {
    #[serde(default)]
    line1_left: Option<String>,
    #[serde(default)]
    line1_clock: Option<bool>,
    #[serde(default)]
    line1_mode: Option<String>,
    #[serde(default)]
    line2_source: Option<String>,
    #[serde(default)]
    line3_trigger: Option<String>,
    #[serde(default)]
    line3_mapping: Option<bool>,
    #[serde(default)]
    line3_label: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawManifest {
    profiles_dir: Option<String>,
    m1: Option<String>,
    m2: Option<String>,
    m3: Option<String>,
    #[serde(default)]
    autorepeat: Option<RawAutoRepeat>,
    #[serde(default)]
    app: Option<RawApp>,
    #[serde(default)]
    joystick: Option<RawManifestJoystick>,
    #[serde(default)]
    backlight: Option<RawBacklight>,
    #[serde(default)]
    lcd: Option<RawLcd>,
}

/// The loaded profiles plus which M-key is active. Replaces a bare `Profile`
/// as the shared state so both the dispatcher and the GUI see profiles + active.
#[derive(Debug, Clone)]
pub struct ProfileSet {
    profiles_dir: PathBuf,
    m1: Option<Profile>,
    m2: Option<Profile>,
    m3: Option<Profile>,
    m1_name: Option<String>,
    m2_name: Option<String>,
    m3_name: Option<String>,
    /// The selected M-slot. May point at an empty slot — `active_profile()` then
    /// returns `None` and the driver injects nothing.
    active: MKey,
    autorepeat: AutoRepeat,
    joystick_deadzone: u8,
    config_path: PathBuf,
    start_active: bool,
    backlight: crate::led::BacklightConfig,
    lcd: crate::lcd::LcdConfig,
}

impl ProfileSet {
    /// Load from the manifest at `config_path`. Manifest mode when any of
    /// `profiles_dir`, `m1`, `m2`, or `m3` is present; otherwise the file is
    /// itself the single M1 profile (legacy). Paths resolve relative to the
    /// config file's directory.
    pub fn load(config_path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("failed to read config: {}", config_path.display()))?;
        let raw: RawManifest = toml::from_str(&content)
            .with_context(|| format!("failed to parse config: {}", config_path.display()))?;
        let base = config_path.parent().unwrap_or_else(|| Path::new("."));
        let autorepeat = raw.autorepeat.map(AutoRepeat::from_raw).unwrap_or_default();
        let start_active = raw.app.as_ref().map(|a| a.start_active).unwrap_or(false);
        let joystick_deadzone = raw.joystick.map(|j| j.deadzone.min(127) as u8).unwrap_or(50);
        let backlight = raw.backlight.map(|b| {
            use crate::led::{BacklightConfig, Color};
            let d = BacklightConfig::default();
            // Present-but-unparseable colors fall back to the default and warn;
            // an absent field falls back silently.
            let parse = |s: Option<String>| s.and_then(|v| {
                let parsed = Color::from_hex(&v);
                if parsed.is_none() {
                    log::warn!("invalid [backlight] color {v:?}; using default");
                }
                parsed
            });
            BacklightConfig {
                default_color: parse(b.default_color).unwrap_or(d.default_color),
                brightness: b.brightness.unwrap_or(d.brightness).clamp(0.0, 1.0),
                mkey_indicator: b.mkey_indicator.unwrap_or(d.mkey_indicator),
                slot_colors: [parse(b.m1_color), parse(b.m2_color), parse(b.m3_color)],
            }
        }).unwrap_or_default();
        let lcd = raw.lcd.map(|l| {
            use crate::lcd::{LcdConfig, Line1Left, ModeDisplay, Line2Source, Line3Trigger};
            let d = LcdConfig::default();
            // Present-but-unparseable enum values fall back to the default and warn;
            // an absent field falls back silently.
            LcdConfig {
                line1_left: l.line1_left.map(|v| {
                    Line1Left::parse(&v).unwrap_or_else(|| {
                        log::warn!("invalid [lcd] line1_left {v:?}; using default");
                        d.line1_left
                    })
                }).unwrap_or(d.line1_left),
                line1_clock: l.line1_clock.unwrap_or(d.line1_clock),
                line1_mode: l.line1_mode.map(|v| {
                    ModeDisplay::parse(&v).unwrap_or_else(|| {
                        log::warn!("invalid [lcd] line1_mode {v:?}; using default");
                        d.line1_mode
                    })
                }).unwrap_or(d.line1_mode),
                line2_source: l.line2_source.map(|v| {
                    Line2Source::parse(&v).unwrap_or_else(|| {
                        log::warn!("invalid [lcd] line2_source {v:?}; using default");
                        d.line2_source
                    })
                }).unwrap_or(d.line2_source),
                line3_trigger: l.line3_trigger.map(|v| {
                    Line3Trigger::parse(&v).unwrap_or_else(|| {
                        log::warn!("invalid [lcd] line3_trigger {v:?}; using default");
                        d.line3_trigger
                    })
                }).unwrap_or(d.line3_trigger),
                line3_mapping: l.line3_mapping.unwrap_or(d.line3_mapping),
                line3_label: l.line3_label.unwrap_or(d.line3_label),
            }
        }).unwrap_or_default();

        // Manifest mode when ANY of profiles_dir/m1/m2/m3 is present.
        let is_manifest = raw.profiles_dir.is_some() || raw.m1.is_some()
            || raw.m2.is_some() || raw.m3.is_some();
        if is_manifest {
            let dir = base.join(raw.profiles_dir.as_deref().unwrap_or("profiles"));
            // Load a slot: None on missing name OR load failure. Keep the name even
            // when the file failed to load, so the UI can show the broken assignment.
            let load_opt = |name: &Option<String>| -> (Option<Profile>, Option<String>) {
                match name {
                    Some(n) => match Profile::load(&dir.join(n)) {
                        Ok(p) => (Some(p), Some(n.clone())),
                        Err(e) => { log::warn!("slot profile {n} not loaded: {e:#}"); (None, Some(n.clone())) }
                    },
                    None => (None, None),
                }
            };
            let (m1, m1_name) = load_opt(&raw.m1);
            let (m2, m2_name) = load_opt(&raw.m2);
            let (m3, m3_name) = load_opt(&raw.m3);
            Ok(Self {
                profiles_dir: dir,
                m1, m2, m3,
                m1_name, m2_name, m3_name,
                active: MKey::M1,
                autorepeat,
                joystick_deadzone,
                config_path: config_path.to_path_buf(),
                start_active,
                backlight,
                lcd,
            })
        } else {
            // Legacy: the file itself is a single M1 profile.
            let m1 = Profile::load(&config_path.to_path_buf())?;
            let name = config_path.file_name().and_then(|s| s.to_str()).map(String::from);
            Ok(Self {
                profiles_dir: base.to_path_buf(),
                m1: Some(m1), m2: None, m3: None,
                m1_name: name, m2_name: None, m3_name: None,
                active: MKey::M1,
                autorepeat,
                joystick_deadzone,
                config_path: config_path.to_path_buf(),
                start_active,
                backlight,
                lcd,
            })
        }
    }

    pub fn active(&self) -> MKey { self.active }

    pub fn autorepeat(&self) -> AutoRepeat { self.autorepeat }

    pub fn joystick_deadzone(&self) -> u8 { self.joystick_deadzone }

    pub fn set_joystick_deadzone(&mut self, deadzone: u8) {
        self.joystick_deadzone = deadzone.min(127);
    }

    pub fn set_backlight_default_color(&mut self, c: crate::led::Color) {
        self.backlight.default_color = c;
    }
    pub fn set_backlight_brightness(&mut self, b: f32) {
        self.backlight.brightness = b.clamp(0.0, 1.0);
    }
    pub fn set_backlight_mkey_indicator(&mut self, on: bool) {
        self.backlight.mkey_indicator = on;
    }
    pub fn set_backlight_slot_color(&mut self, slot: usize, c: Option<crate::led::Color>) {
        if slot < 3 {
            self.backlight.slot_colors[slot] = c;
        }
    }

    pub fn backlight_config(&self) -> crate::led::BacklightConfig { self.backlight }

    pub fn lcd_config(&self) -> crate::lcd::LcdConfig { self.lcd }

    pub fn set_lcd_line1_left(&mut self, v: crate::lcd::Line1Left) {
        self.lcd.line1_left = v;
    }
    pub fn set_lcd_line1_clock(&mut self, v: bool) {
        self.lcd.line1_clock = v;
    }
    pub fn set_lcd_line1_mode(&mut self, v: crate::lcd::ModeDisplay) {
        self.lcd.line1_mode = v;
    }
    pub fn set_lcd_line2_source(&mut self, v: crate::lcd::Line2Source) {
        self.lcd.line2_source = v;
    }
    pub fn set_lcd_line3_trigger(&mut self, v: crate::lcd::Line3Trigger) {
        self.lcd.line3_trigger = v;
    }
    pub fn set_lcd_line3_mapping(&mut self, v: bool) {
        self.lcd.line3_mapping = v;
    }
    pub fn set_lcd_line3_label(&mut self, v: bool) {
        self.lcd.line3_label = v;
    }

    /// The LED state the hardware should show for the current active slot + config.
    pub fn desired_led_state(&self) -> crate::led::LedState {
        crate::led::resolve(self.active, &self.backlight)
    }

    pub fn start_active(&self) -> bool { self.start_active }

    /// Set (`Some`) or clear (`None`) an M-slot in the manifest, preserving all other
    /// keys and comments. MR is a no-op.
    pub fn persist_slot(&self, key: MKey, filename: Option<&str>) -> Result<()> {
        use toml_edit::{DocumentMut, value as toml_value};
        let slot = match key {
            MKey::M1 => "m1",
            MKey::M2 => "m2",
            MKey::M3 => "m3",
            MKey::MR => return Ok(()),
        };
        let text = std::fs::read_to_string(&self.config_path)
            .with_context(|| format!("failed to read {}", self.config_path.display()))?;
        let mut doc = text.parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.config_path.display()))?;
        match filename {
            Some(name) => { doc[slot] = toml_value(name); }
            None => { doc.as_table_mut().remove(slot); }
        }
        std::fs::write(&self.config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", self.config_path.display()))?;
        Ok(())
    }

    /// Set `profiles_dir` in the manifest, preserving all other keys and comments.
    pub fn persist_profiles_dir(&self, dir: &Path) -> Result<()> {
        use toml_edit::{DocumentMut, value as toml_value};
        let text = std::fs::read_to_string(&self.config_path)
            .with_context(|| format!("failed to read {}", self.config_path.display()))?;
        let mut doc = text.parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.config_path.display()))?;
        doc["profiles_dir"] = toml_value(dir.display().to_string());
        std::fs::write(&self.config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", self.config_path.display()))?;
        Ok(())
    }

    /// Write `[joystick] deadzone` into the manifest, preserving every other key and
    /// comment (format-preserving via toml_edit). Best-effort; callers log on error.
    pub fn persist_joystick_deadzone(&self, deadzone: u8) -> Result<()> {
        use toml_edit::{DocumentMut, Item, Table, value as toml_value};
        let text = std::fs::read_to_string(&self.config_path)
            .with_context(|| format!("failed to read {}", self.config_path.display()))?;
        let mut doc = text.parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.config_path.display()))?;
        if !doc.as_table().contains_key("joystick") {
            doc.as_table_mut().insert("joystick", Item::Table(Table::new()));
        }
        doc["joystick"]["deadzone"] = toml_value(deadzone.min(127) as i64);
        std::fs::write(&self.config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", self.config_path.display()))?;
        Ok(())
    }

    /// Write the whole `[backlight]` table into the manifest, preserving every other
    /// key and comment (format-preserving via toml_edit). Best-effort; callers log on error.
    pub fn persist_backlight(&self) -> Result<()> {
        use toml_edit::{DocumentMut, Item, Table, value as toml_value};
        let text = std::fs::read_to_string(&self.config_path)
            .with_context(|| format!("failed to read {}", self.config_path.display()))?;
        let mut doc = text.parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.config_path.display()))?;
        if !doc.as_table().contains_key("backlight") {
            doc.as_table_mut().insert("backlight", Item::Table(Table::new()));
        }
        let b = &self.backlight;
        doc["backlight"]["default_color"] = toml_value(b.default_color.to_hex());
        doc["backlight"]["brightness"] = toml_value(b.brightness as f64);
        doc["backlight"]["mkey_indicator"] = toml_value(b.mkey_indicator);
        for (i, key) in ["m1_color", "m2_color", "m3_color"].iter().enumerate() {
            match b.slot_colors[i] {
                Some(c) => { doc["backlight"][*key] = toml_value(c.to_hex()); }
                None => {
                    if let Some(t) = doc["backlight"].as_table_mut() {
                        t.remove(*key);
                    }
                }
            }
        }
        std::fs::write(&self.config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", self.config_path.display()))?;
        Ok(())
    }

    /// Write the whole `[lcd]` table into the manifest, preserving every other
    /// key and comment (format-preserving via toml_edit). Best-effort; callers log on error.
    pub fn persist_lcd(&self) -> Result<()> {
        use toml_edit::{DocumentMut, Item, Table, value as toml_value};
        let text = std::fs::read_to_string(&self.config_path)
            .with_context(|| format!("failed to read {}", self.config_path.display()))?;
        let mut doc = text.parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.config_path.display()))?;
        if !doc.as_table().contains_key("lcd") {
            doc.as_table_mut().insert("lcd", Item::Table(Table::new()));
        }
        let c = &self.lcd;
        doc["lcd"]["line1_left"] = toml_value(c.line1_left.as_str());
        doc["lcd"]["line1_clock"] = toml_value(c.line1_clock);
        doc["lcd"]["line1_mode"] = toml_value(c.line1_mode.as_str());
        doc["lcd"]["line2_source"] = toml_value(c.line2_source.as_str());
        doc["lcd"]["line3_trigger"] = toml_value(c.line3_trigger.as_str());
        doc["lcd"]["line3_mapping"] = toml_value(c.line3_mapping);
        doc["lcd"]["line3_label"] = toml_value(c.line3_label);
        std::fs::write(&self.config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", self.config_path.display()))?;
        Ok(())
    }

    /// Write `[app] start_active` into the manifest, preserving every other key and
    /// comment (format-preserving via toml_edit). Best-effort; callers log on error.
    pub fn persist_start_active(&self, value: bool) -> Result<()> {
        use toml_edit::{DocumentMut, Item, Table, value as toml_value};
        let text = std::fs::read_to_string(&self.config_path)
            .with_context(|| format!("failed to read {}", self.config_path.display()))?;
        let mut doc = text.parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.config_path.display()))?;
        if !doc.as_table().contains_key("app") {
            doc.as_table_mut().insert("app", Item::Table(Table::new()));
        }
        doc["app"]["start_active"] = toml_value(value);
        std::fs::write(&self.config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", self.config_path.display()))?;
        Ok(())
    }

    pub fn active_profile(&self) -> Option<&Profile> {
        match self.active {
            MKey::M2 => self.m2.as_ref(),
            MKey::M3 => self.m3.as_ref(),
            _ => self.m1.as_ref(),
        }
    }

    /// Switch the active profile. Succeeds (returns true) for M1/M2/M3 whether the
    /// slot is populated or empty; a no-op (false) for MR.
    pub fn set_active(&mut self, k: MKey) -> bool {
        match k {
            MKey::MR => false,
            _ => { self.active = k; true }
        }
    }

    pub fn name(&self, k: MKey) -> Option<&str> {
        match k {
            MKey::M1 => self.m1_name.as_deref(),
            MKey::M2 => self.m2_name.as_deref(),
            MKey::M3 => self.m3_name.as_deref(),
            MKey::MR => None,
        }
    }

    pub fn active_name(&self) -> Option<&str> { self.name(self.active) }

    /// The active profile's filename without the `.toml` extension (for the LCD).
    pub fn active_name_stem(&self) -> Option<&str> {
        self.active_name().map(|n| n.trim_end_matches(".toml"))
    }

    pub fn profiles_dir(&self) -> &std::path::Path { &self.profiles_dir }

    /// The file path backing the active profile (profiles_dir + active filename;
    /// for a legacy single-profile config that resolves to the config file).
    ///
    /// Relies on the load invariant: in legacy mode `profiles_dir` is the config
    /// file's directory and the M1 name is the config filename, so this joins
    /// back to the config file itself.
    pub fn active_path(&self) -> PathBuf {
        let name = self.active_name().unwrap_or("config.toml");
        self.profiles_dir.join(name)
    }

    fn active_profile_mut(&mut self) -> Option<&mut Profile> {
        match self.active {
            MKey::M2 => self.m2.as_mut(),
            MKey::M3 => self.m3.as_mut(),
            _ => self.m1.as_mut(),
        }
    }

    /// Replace the active profile's key bindings, repeat flags, labels, and joystick
    /// and write the profile file. The watcher will reload the identical content.
    pub fn save_active_bindings(
        &mut self,
        bindings: HashMap<G13Key, String>,
        repeat: HashMap<G13Key, bool>,
        labels: HashMap<G13Key, String>,
        joystick: Option<JoystickConfig>,
        joystick_labels: HashMap<JoystickDir, String>,
        joystick_repeat: HashMap<JoystickDir, bool>,
    ) -> Result<()> {
        if self.active_name().is_none() || self.active_profile().is_none() {
            anyhow::bail!("no profile in the active slot");
        }
        let path = self.active_path();
        let profile = self.active_profile_mut().expect("checked above");
        profile.set_bindings(bindings);
        profile.set_repeat(repeat);
        profile.set_labels(labels);
        profile.set_joystick(joystick);
        profile.set_joystick_labels(joystick_labels);
        profile.set_joystick_repeat(joystick_repeat);
        if profile.source() == ProfileSource::Github {
            profile.set_modified(true);
        }
        let toml = profile.to_toml()?;
        std::fs::write(&path, toml)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}

fn parse_g13_key(s: &str) -> Option<G13Key> {
    match s.to_uppercase().as_str() {
        "G1"  => Some(G13Key::G1),  "G2"  => Some(G13Key::G2),
        "G3"  => Some(G13Key::G3),  "G4"  => Some(G13Key::G4),
        "G5"  => Some(G13Key::G5),  "G6"  => Some(G13Key::G6),
        "G7"  => Some(G13Key::G7),  "G8"  => Some(G13Key::G8),
        "G9"  => Some(G13Key::G9),  "G10" => Some(G13Key::G10),
        "G11" => Some(G13Key::G11), "G12" => Some(G13Key::G12),
        "G13" => Some(G13Key::G13), "G14" => Some(G13Key::G14),
        "G15" => Some(G13Key::G15), "G16" => Some(G13Key::G16),
        "G17" => Some(G13Key::G17), "G18" => Some(G13Key::G18),
        "G19" => Some(G13Key::G19), "G20" => Some(G13Key::G20),
        "G21" => Some(G13Key::G21), "G22" => Some(G13Key::G22),
        "BTN1"  => Some(G13Key::Btn1),
        "BTN2"  => Some(G13Key::Btn2),
        "STICK" => Some(G13Key::Stick),
        _ => None,
    }
}

fn parse_joystick(rj: RawJoystick) -> Result<JoystickConfig> {
    // `mode` and `deadzone` are legacy per-profile fields, now ignored: the mode is
    // always directions-only (WASD-style) and the deadzone is global (from the manifest).
    Ok(JoystickConfig {
        up: rj.up,
        down: rj.down,
        left: rj.left,
        right: rj.right,
    })
}

#[cfg(test)]
mod profileset_tests {
    use super::*;
    use crate::protocol::MKey;
    use crate::protocol::G13Key;
    use std::collections::HashMap;

    fn write(dir: &std::path::Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    // Build a temp dir under the OS temp with a unique suffix from the test name.
    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("g13-test-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        d
    }

    #[test]
    fn loads_manifest_and_switches() {
        let d = tmp("manifest");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"ctrl+c\"\n");
        write(&d.join("profiles"), "game.toml", "[keys]\nG1 = \"space\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\nm2 = \"game.toml\"\n");

        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.active(), MKey::M1);
        assert_eq!(set.active_profile().unwrap().get_binding(crate::protocol::G13Key::G1), Some("ctrl+c"));
        assert_eq!(set.name(MKey::M2), Some("game.toml"));

        assert!(set.set_active(MKey::M2));
        assert_eq!(set.active_profile().unwrap().get_binding(crate::protocol::G13Key::G1), Some("space"));

        // M3 empty -> now selectable, active_profile None.
        assert!(set.set_active(MKey::M3));
        assert!(set.active_profile().is_none());
        set.set_active(MKey::M2);
        // MR reserved -> no-op.
        assert!(!set.set_active(MKey::MR));
    }

    #[test]
    fn legacy_config_is_single_m1_profile() {
        let d = tmp("legacy");
        write(&d, "config.toml", "[keys]\nG1 = \"ctrl+c\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.active_profile().unwrap().get_binding(crate::protocol::G13Key::G1), Some("ctrl+c"));
        assert!(set.name(MKey::M2).is_none());
    }

    #[test]
    fn load_with_no_m1_is_ok_and_active_is_none() {
        let d = tmp("no-m1");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\n"); // no m1/m2/m3
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert!(set.active_profile().is_none());
        assert_eq!(set.active(), MKey::M1);
    }

    #[test]
    fn missing_m1_file_resolves_to_empty_not_error() {
        let d = tmp("m1-missing-file");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"nope.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap(); // was an error before
        assert!(set.active_profile().is_none());
    }

    #[test]
    fn set_active_allows_empty_slots() {
        let d = tmp("empty-active");
        write(&d.join("profiles"), "basic.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n");
        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert!(set.set_active(MKey::M2)); // empty, but selectable now
        assert_eq!(set.active(), MKey::M2);
        assert!(set.active_profile().is_none()); // empty active -> None
        assert!(!set.set_active(MKey::MR)); // MR still no-op
    }

    #[test]
    fn legacy_bare_keys_still_single_m1_profile() {
        let d = tmp("legacy-empty-rev");
        write(&d, "config.toml", "[keys]\nG1 = \"ctrl+c\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.active_profile().unwrap().get_binding(crate::protocol::G13Key::G1), Some("ctrl+c"));
    }

    #[test]
    fn profile_to_toml_round_trips() {
        // A profile with keys + joystick serializes and reloads identically.
        let src = "[keys]\nG1 = \"ctrl+c\"\nG5 = \"f5\"\n[joystick]\nmode = \"wasd\"\ndeadzone = 20\nup = \"w\"\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let p = Profile::from_raw(raw).unwrap();
        let toml = p.to_toml().unwrap();
        let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
        assert_eq!(reloaded.get_binding(G13Key::G1), Some("ctrl+c"));
        assert_eq!(reloaded.get_binding(G13Key::G5), Some("f5"));
        let j = reloaded.joystick().expect("joystick preserved");
        assert_eq!(j.up.as_deref(), Some("w"));
    }

    #[test]
    fn save_active_bindings_writes_labels() {
        let d = tmp("save-labels");
        write(&d.join("profiles"), "p.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"p.toml\"\n");
        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        let mut b = HashMap::new();
        b.insert(G13Key::G1, "ctrl+c".to_string());
        let mut labels = HashMap::new();
        labels.insert(G13Key::G1, "Copy".to_string());
        set.save_active_bindings(b, HashMap::new(), labels, None, HashMap::new(), HashMap::new()).unwrap();
        let text = std::fs::read_to_string(d.join("profiles/p.toml")).unwrap();
        assert!(text.contains("[labels]"));
        assert!(text.contains("Copy"));
        // reloads with the label intact
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(reloaded.active_profile().unwrap().label(G13Key::G1), Some("Copy"));
    }

    #[test]
    fn save_active_bindings_writes_joystick() {
        let d = tmp("save-joy");
        write(&d.join("profiles"), "p.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"p.toml\"\n");
        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        let joy = Some(JoystickConfig { up: Some("w".into()), down: Some("s".into()),
                                        left: Some("a".into()), right: Some("d".into()) });
        set.save_active_bindings(HashMap::new(), HashMap::new(), HashMap::new(), joy, HashMap::new(), HashMap::new()).unwrap();
        let text = std::fs::read_to_string(d.join("profiles/p.toml")).unwrap();
        assert!(text.contains("[joystick]"));
        assert!(text.contains("up = \"w\""));
    }

    #[test]
    fn save_active_bindings_writes_and_preserves_others() {
        let d = tmp("save");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"ctrl+c\"\n[joystick]\nup = \"w\"\n");
        write(&d.join("profiles"), "game.toml", "[keys]\nG1 = \"space\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\nm2 = \"game.toml\"\n");
        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();

        // Edit M1 (active): G1 -> ctrl+a, add G2 -> f1.
        let mut b = HashMap::new();
        b.insert(G13Key::G1, "ctrl+a".to_string());
        b.insert(G13Key::G2, "f1".to_string());
        set.save_active_bindings(b, HashMap::new(), HashMap::new(), None, HashMap::new(), HashMap::new()).unwrap();

        // Fresh load from disk reflects the change; passing None clears joystick; game untouched.
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(reloaded.active_profile().unwrap().get_binding(G13Key::G1), Some("ctrl+a"));
        assert_eq!(reloaded.active_profile().unwrap().get_binding(G13Key::G2), Some("f1"));
        assert!(reloaded.active_profile().unwrap().joystick().is_none(), "joystick cleared when None passed");
        // M2 file untouched.
        let game = std::fs::read_to_string(d.join("profiles/game.toml")).unwrap();
        assert!(game.contains("space"));
        // Manifest untouched.
        let manifest = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(manifest.contains("m1 = \"default.toml\""));
    }

    #[test]
    fn autorepeat_defaults_when_absent() {
        let d = tmp("ar-default");
        write(&d, "config.toml", "[keys]\nG1 = \"a\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.autorepeat(), AutoRepeat { delay_ms: 400, interval_ms: 40 });
    }

    #[test]
    fn autorepeat_parses_values() {
        let d = tmp("ar-parse");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml",
            "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\n[autorepeat]\ndelay_ms = 250\ninterval_ms = 33\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.autorepeat(), AutoRepeat { delay_ms: 250, interval_ms: 33 });
    }

    #[test]
    fn autorepeat_interval_zero_clamped_to_one() {
        let d = tmp("ar-clamp");
        write(&d, "config.toml", "[keys]\nG1 = \"a\"\n[autorepeat]\ninterval_ms = 0\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.autorepeat().interval_ms, 1);
    }

    #[test]
    fn start_active_defaults_false_when_absent() {
        let d = tmp("app-default");
        write(&d, "config.toml", "[keys]\nG1 = \"a\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert!(!set.start_active());
    }

    #[test]
    fn start_active_parses_true() {
        let d = tmp("app-true");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml",
            "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\n[app]\nstart_active = true\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert!(set.start_active());
    }

    #[test]
    fn persist_slot_sets_and_clears_preserving_others() {
        let d = tmp("persist-slot");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"a\"\n");
        write(&d.join("profiles"), "media.toml", "[keys]\nG1 = \"space\"\n");
        write(&d, "config.toml",
            "# manifest\nprofiles_dir = \"profiles\"\nm1 = \"default.toml\"\nm2 = \"media.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();

        // Set m2 -> default.toml, clear m3 (absent -> stays absent, no error).
        set.persist_slot(MKey::M2, Some("default.toml")).unwrap();
        set.persist_slot(MKey::M3, None).unwrap();

        let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(text.contains("m2 = \"default.toml\""));
        assert!(text.contains("# manifest"), "comment preserved");
        assert!(text.contains("m1 = \"default.toml\""), "m1 preserved");
        assert!(!text.contains("m3 ="), "m3 stays absent");
    }

    #[test]
    fn persist_slot_clear_removes_existing_key() {
        let d = tmp("persist-clear");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml",
            "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\nm2 = \"default.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        set.persist_slot(MKey::M2, None).unwrap();
        let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(!text.contains("m2 ="), "m2 removed");
        assert!(text.contains("m1 = \"default.toml\""));
    }

    #[test]
    fn persist_profiles_dir_updates_and_reloads() {
        let d = tmp("persist-dir");
        std::fs::create_dir_all(d.join("elsewhere")).unwrap();
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"a\"\n");
        write(&d.join("elsewhere"), "default.toml", "[keys]\nG1 = \"b\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        set.persist_profiles_dir(&d.join("elsewhere")).unwrap();

        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(reloaded.active_profile().unwrap().get_binding(crate::protocol::G13Key::G1), Some("b"));
    }

    #[test]
    fn saving_github_profile_marks_modified() {
        let d = tmp("flip-github");
        write(&d.join("profiles"), "g.toml",
            "[meta]\nname = \"G\"\nsource = \"github\"\n[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"g.toml\"\n");
        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        let mut b = HashMap::new();
        b.insert(G13Key::G1, "ctrl+c".to_string());
        set.save_active_bindings(b, HashMap::new(), HashMap::new(), None, HashMap::new(), HashMap::new()).unwrap();
        let text = std::fs::read_to_string(d.join("profiles/g.toml")).unwrap();
        assert!(text.contains("modified = true"), "github profile flips modified on save");
        assert!(text.contains("source = \"github\""), "source preserved");
    }

    #[test]
    fn saving_user_profile_stays_clean() {
        let d = tmp("flip-user");
        write(&d.join("profiles"), "u.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"u.toml\"\n");
        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        let mut b = HashMap::new();
        b.insert(G13Key::G1, "ctrl+c".to_string());
        set.save_active_bindings(b, HashMap::new(), HashMap::new(), None, HashMap::new(), HashMap::new()).unwrap();
        let text = std::fs::read_to_string(d.join("profiles/u.toml")).unwrap();
        assert!(!text.contains("modified"), "user profile stays clean");
        assert!(!text.contains("source"), "user profile stays clean");
    }

    #[test]
    fn global_deadzone_defaults_to_50() {
        let d = tmp("gdz-default");
        write(&d, "config.toml", "[keys]\nG1 = \"a\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.joystick_deadzone(), 50);
    }

    #[test]
    fn global_deadzone_parses_and_clamps() {
        let d = tmp("gdz-parse");
        write(&d.join("profiles"), "p.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml",
            "profiles_dir = \"profiles\"\nm1 = \"p.toml\"\n[joystick]\ndeadzone = 200\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.joystick_deadzone(), 127); // clamped
    }

    #[test]
    fn persist_joystick_deadzone_writes_and_reloads() {
        let d = tmp("gdz-persist");
        write(&d.join("profiles"), "p.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml", "# manifest\nprofiles_dir = \"profiles\"\nm1 = \"p.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        set.persist_joystick_deadzone(42).unwrap();
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(reloaded.joystick_deadzone(), 42);
        let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(text.contains("# manifest"), "comment preserved");
    }

    #[test]
    fn parses_backlight_section() {
        let d = std::env::temp_dir().join("g13-cfg-backlight");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n\
             [backlight]\ndefault_color = \"#102030\"\nbrightness = 0.5\n\
             mkey_indicator = false\nm1_color = \"#FF0000\"\n").unwrap();

        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        let b = set.backlight_config();
        assert_eq!(b.default_color, crate::led::Color(0x10, 0x20, 0x30));
        assert_eq!(b.brightness, 0.5);
        assert!(!b.mkey_indicator);
        assert_eq!(b.slot_colors[0], Some(crate::led::Color(0xFF, 0x00, 0x00)));
        assert_eq!(b.slot_colors[1], None);
    }

    #[test]
    fn missing_backlight_section_uses_defaults() {
        let d = std::env::temp_dir().join("g13-cfg-nobacklight");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n").unwrap();

        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.backlight_config(), crate::led::BacklightConfig::default());
        // active is M1 by default, default color white, indicator on -> mkeys = 1
        assert_eq!(set.desired_led_state(),
            crate::led::LedState { rgb: (255, 255, 255), mkeys: 1 });
    }

    #[test]
    fn lcd_section_parses() {
        let d = std::env::temp_dir().join("g13-cfg-lcd");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n\
             [lcd]\nline1_left = \"version\"\nline1_clock = true\nline1_mode = \"off\"\n\
             line2_source = \"display\"\nline3_trigger = \"held\"\nline3_mapping = false\n").unwrap();

        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        let l = set.lcd_config();
        assert_eq!(l.line1_left, crate::lcd::Line1Left::Version);
        assert!(l.line1_clock);
        assert_eq!(l.line1_mode, crate::lcd::ModeDisplay::Off);
        assert_eq!(l.line2_source, crate::lcd::Line2Source::Display);
        assert_eq!(l.line3_trigger, crate::lcd::Line3Trigger::Held);
        assert!(!l.line3_mapping);
        assert!(l.line3_label); // not set in the manifest -> default (true)
    }

    #[test]
    fn missing_lcd_section_uses_defaults() {
        let d = std::env::temp_dir().join("g13-cfg-nolcd");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n").unwrap();

        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.lcd_config(), crate::lcd::LcdConfig::default());
    }

    #[test]
    fn bad_lcd_enum_falls_back() {
        let d = std::env::temp_dir().join("g13-cfg-badlcd");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n\
             [lcd]\nline1_mode = \"bogus\"\n").unwrap();

        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.lcd_config().line1_mode, crate::lcd::ModeDisplay::Label);
    }

    #[test]
    fn persist_start_active_preserves_other_keys_and_reloads() {
        let d = tmp("app-persist");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml",
            "# my manifest\nprofiles_dir = \"profiles\"\nm1 = \"default.toml\"\nm2 = \"game.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        set.persist_start_active(true).unwrap();

        // Reloads as true; other keys + the comment survive.
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert!(reloaded.start_active());
        let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(text.contains("# my manifest"));
        assert!(text.contains("m2 = \"game.toml\""));
        assert!(text.contains("profiles_dir = \"profiles\""));
    }

    #[test]
    fn persist_backlight_round_trips() {
        use crate::led::Color;
        let d = std::env::temp_dir().join("g13-cfg-persistbl");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n").unwrap();

        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        set.set_backlight_default_color(Color(0x11, 0x22, 0x33));
        set.set_backlight_brightness(0.25);
        set.set_backlight_mkey_indicator(false);
        set.set_backlight_slot_color(0, Some(Color(0xAA, 0xBB, 0xCC)));
        set.set_backlight_slot_color(1, None);
        set.persist_backlight().unwrap();

        // Reload from disk and confirm the values survived.
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        let b = reloaded.backlight_config();
        assert_eq!(b.default_color, Color(0x11, 0x22, 0x33));
        assert_eq!(b.brightness, 0.25);
        assert!(!b.mkey_indicator);
        assert_eq!(b.slot_colors[0], Some(Color(0xAA, 0xBB, 0xCC)));
        assert_eq!(b.slot_colors[1], None);

        // Original manifest keys are preserved.
        let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(text.contains("m1 = \"basic.toml\""));
    }

    #[test]
    fn persist_backlight_removes_existing_slot_color_key() {
        let d = std::env::temp_dir().join("g13-cfg-persistbl-remove");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n\
             [backlight]\nm1_color = \"#FF0000\"\n").unwrap();

        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.backlight_config().slot_colors[0], Some(crate::led::Color(0xFF, 0x00, 0x00)));

        set.set_backlight_slot_color(0, None);
        set.persist_backlight().unwrap();

        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(reloaded.backlight_config().slot_colors[0], None);

        let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(!text.contains("m1_color"), "m1_color key removed from disk");
    }

    #[test]
    fn persist_lcd_round_trips() {
        use crate::lcd::{Line1Left, ModeDisplay, Line2Source, Line3Trigger};
        let d = std::env::temp_dir().join("g13-cfg-persistlcd");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n").unwrap();

        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        set.set_lcd_line1_left(Line1Left::Version);
        set.set_lcd_line1_clock(true);
        set.set_lcd_line1_mode(ModeDisplay::Off);
        set.set_lcd_line2_source(Line2Source::Display);
        set.set_lcd_line3_trigger(Line3Trigger::Held);
        set.set_lcd_line3_mapping(false);
        set.set_lcd_line3_label(false);
        set.persist_lcd().unwrap();

        // Reload from disk and confirm the values survived.
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        let l = reloaded.lcd_config();
        assert_eq!(l.line1_left, Line1Left::Version);
        assert!(l.line1_clock);
        assert_eq!(l.line1_mode, ModeDisplay::Off);
        assert_eq!(l.line2_source, Line2Source::Display);
        assert_eq!(l.line3_trigger, Line3Trigger::Held);
        assert!(!l.line3_mapping);
        assert!(!l.line3_label);

        // Original manifest keys are preserved, and the [lcd] table is present.
        let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(text.contains("m1 = \"basic.toml\""));
        assert!(text.contains("[lcd]"));
    }

    #[test]
    fn active_name_stem_strips_toml_extension() {
        let d = std::env::temp_dir().join("g13-cfg-stem");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n").unwrap();

        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.active_name(), Some("basic.toml"));   // raw filename unchanged
        assert_eq!(set.active_name_stem(), Some("basic"));   // stem drops .toml
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::G13Key;

    fn raw(pairs: &[(&str, &str)]) -> RawConfig {
        RawConfig {
            meta: None,
            keys: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            joystick: None,
            repeat: HashMap::new(),
            labels: HashMap::new(),
        }
    }

    #[test]
    fn no_joystick_section_is_none() {
        let config = Profile::from_raw(raw(&[("G1", "ctrl+c")])).unwrap();
        assert!(config.joystick().is_none());
    }

    #[test]
    fn parses_joystick_section() {
        let src = r#"
[keys]
G1 = "ctrl+c"

[joystick]
mode = "wasd"
deadzone = 30
up = "w"
down = "s"
left = "a"
right = "d"
"#;
        let raw: RawConfig = toml::from_str(src).unwrap();
        let config = Profile::from_raw(raw).unwrap();
        let j = config.joystick().expect("joystick config present");
        assert_eq!(j.up.as_deref(), Some("w"));
        assert_eq!(j.right.as_deref(), Some("d"));
    }

    #[test]
    fn joystick_parses_directions_only() {
        let src = "[joystick]\nup = \"w\"\nleft = \"a\"\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let p = Profile::from_raw(raw).unwrap();
        let j = p.joystick().unwrap();
        assert_eq!(j.up.as_deref(), Some("w"));
        assert_eq!(j.left.as_deref(), Some("a"));
        assert_eq!(j.down, None);
    }

    #[test]
    fn legacy_joystick_with_mode_and_deadzone_loads() {
        let src = "[joystick]\nmode = \"mouse\"\ndeadzone = 200\nup = \"w\"\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let p = Profile::from_raw(raw).unwrap(); // no error despite mouse + 200
        assert_eq!(p.joystick().unwrap().up.as_deref(), Some("w"));
    }

    #[test]
    fn to_toml_joystick_directions_only() {
        let src = "[joystick]\nmode = \"wasd\"\ndeadzone = 30\nup = \"w\"\n";
        let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
        let toml = p.to_toml().unwrap();
        assert!(toml.contains("up = \"w\""));
        assert!(!toml.contains("mode"));
        assert!(!toml.contains("deadzone"));
    }

    #[test]
    fn to_toml_omits_empty_joystick() {
        // A joystick with all directions None serializes no [joystick] table.
        let mut p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        p.set_joystick(Some(JoystickConfig { up: None, down: None, left: None, right: None }));
        assert!(!p.to_toml().unwrap().contains("[joystick]"));
    }

    #[test]
    fn loads_bindings_from_raw() {
        let config = Profile::from_raw(raw(&[("G1", "ctrl+c"), ("G2", "f5")])).unwrap();
        assert_eq!(config.get_binding(G13Key::G1), Some("ctrl+c"));
        assert_eq!(config.get_binding(G13Key::G2), Some("f5"));
    }

    #[test]
    fn unknown_g13_key_is_error() {
        assert!(Profile::from_raw(raw(&[("G99", "ctrl+c")])).is_err());
    }

    #[test]
    fn unmapped_key_returns_none() {
        let config = Profile::from_raw(raw(&[])).unwrap();
        assert_eq!(config.get_binding(G13Key::G5), None);
    }

    #[test]
    fn key_names_are_case_insensitive() {
        let config = Profile::from_raw(raw(&[("g1", "ctrl+c")])).unwrap();
        assert_eq!(config.get_binding(G13Key::G1), Some("ctrl+c"));
    }

    #[test]
    fn parses_thumb_button_names() {
        use crate::protocol::G13Key;
        let config = Profile::from_raw(raw(&[
            ("BTN1", "a"), ("btn2", "b"), ("Stick", "space"),
        ])).unwrap();
        assert_eq!(config.get_binding(G13Key::Btn1), Some("a"));
        assert_eq!(config.get_binding(G13Key::Btn2), Some("b"));
        assert_eq!(config.get_binding(G13Key::Stick), Some("space"));
    }

    #[test]
    fn thumb_binding_round_trips_through_toml() {
        use crate::protocol::G13Key;
        let p = Profile::from_raw(raw(&[("BTN1", "ctrl+c"), ("STICK", "enter")])).unwrap();
        let toml = p.to_toml().unwrap();
        let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
        assert_eq!(reloaded.get_binding(G13Key::Btn1), Some("ctrl+c"));
        assert_eq!(reloaded.get_binding(G13Key::Stick), Some("enter"));
    }

    #[test]
    fn parses_toml_content() {
        let src = r#"
[keys]
G1 = "ctrl+c"
G3 = "f5"
"#;
        let raw: RawConfig = toml::from_str(src).unwrap();
        let config = Profile::from_raw(raw).unwrap();
        assert_eq!(config.get_binding(G13Key::G1), Some("ctrl+c"));
        assert_eq!(config.get_binding(G13Key::G3), Some("f5"));
        assert_eq!(config.get_binding(G13Key::G2), None);
    }

    #[test]
    fn parses_repeat_flags() {
        let src = "[keys]\nG1 = \"a\"\nG2 = \"b\"\n[repeat]\nG2 = true\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let p = Profile::from_raw(raw).unwrap();
        assert!(!p.repeats(G13Key::G1));
        assert!(p.repeats(G13Key::G2));
    }

    #[test]
    fn repeat_defaults_false_when_absent() {
        let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        assert!(!p.repeats(G13Key::G1));
    }

    #[test]
    fn repeat_round_trips_through_toml() {
        let src = "[keys]\nG1 = \"a\"\nG2 = \"b\"\n[repeat]\nG2 = true\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let p = Profile::from_raw(raw).unwrap();
        let toml = p.to_toml().unwrap();
        let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
        assert!(reloaded.repeats(G13Key::G2));
        assert!(!reloaded.repeats(G13Key::G1));
    }

    #[test]
    fn to_toml_omits_disabled_repeat_flags() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert(G13Key::G1, false);
        let mut p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        p.set_repeat(map);
        let toml = p.to_toml().unwrap();
        assert!(!toml.contains("[repeat]"));
    }

    #[test]
    fn parses_meta_name() {
        let src = "[meta]\nname = \"My Profile\"\n[keys]\nG1 = \"a\"\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let p = Profile::from_raw(raw).unwrap();
        assert_eq!(p.meta_name(), Some("My Profile"));
    }

    #[test]
    fn meta_name_absent_is_none() {
        let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        assert_eq!(p.meta_name(), None);
    }

    #[test]
    fn empty_meta_name_is_none() {
        let src = "[meta]\nname = \"\"\n[keys]\nG1 = \"a\"\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let p = Profile::from_raw(raw).unwrap();
        assert_eq!(p.meta_name(), None);
    }

    #[test]
    fn to_toml_round_trips_meta_name() {
        let mut p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        p.set_meta_name(Some("Basic".to_string()));
        let toml = p.to_toml().unwrap();
        assert!(toml.contains("[meta]"));
        let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
        assert_eq!(reloaded.meta_name(), Some("Basic"));
    }

    #[test]
    fn parses_source_and_modified() {
        let src = "[meta]\nname = \"X\"\nsource = \"github\"\nmodified = true\n[keys]\nG1 = \"a\"\n";
        let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
        assert_eq!(p.source(), ProfileSource::Github);
        assert!(p.modified());
    }

    #[test]
    fn source_absent_is_user_and_modified_absent_is_false() {
        let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        assert_eq!(p.source(), ProfileSource::User);
        assert!(!p.modified());
    }

    #[test]
    fn garbage_source_is_user() {
        let src = "[meta]\nsource = \"nonsense\"\n[keys]\nG1 = \"a\"\n";
        let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
        assert_eq!(p.source(), ProfileSource::User);
    }

    #[test]
    fn to_toml_omits_source_and_modified_for_user_default() {
        let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        let toml = p.to_toml().unwrap();
        assert!(!toml.contains("source"));
        assert!(!toml.contains("modified"));
    }

    #[test]
    fn to_toml_round_trips_github_modified() {
        let mut p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        p.set_source(ProfileSource::Github);
        p.set_modified(true);
        let toml = p.to_toml().unwrap();
        assert!(toml.contains("source = \"github\""));
        assert!(toml.contains("modified = true"));
        let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
        assert_eq!(reloaded.source(), ProfileSource::Github);
        assert!(reloaded.modified());
    }

    #[test]
    fn github_unmodified_omits_modified_line() {
        let mut p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        p.set_source(ProfileSource::Github);
        let toml = p.to_toml().unwrap();
        assert!(toml.contains("source = \"github\""));
        assert!(!toml.contains("modified"));
    }

    #[test]
    fn parses_origin() {
        let src = "[meta]\nname = \"G\"\nsource = \"github\"\norigin = \"gaming.toml\"\n[keys]\nG1 = \"a\"\n";
        let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
        assert_eq!(p.origin(), Some("gaming.toml"));
    }

    #[test]
    fn origin_absent_is_none() {
        let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        assert_eq!(p.origin(), None);
    }

    #[test]
    fn to_toml_round_trips_origin() {
        let mut p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        p.set_source(ProfileSource::Github);
        p.set_origin(Some("gaming.toml".to_string()));
        let toml = p.to_toml().unwrap();
        assert!(toml.contains("origin = \"gaming.toml\""));
        let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
        assert_eq!(reloaded.origin(), Some("gaming.toml"));
    }

    #[test]
    fn user_profile_omits_origin() {
        let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        let toml = p.to_toml().unwrap();
        assert!(!toml.contains("origin"));
    }

    #[test]
    fn parses_labels() {
        let src = "[keys]\nG1 = \"ctrl+c\"\n[labels]\nG1 = \"Copy\"\n";
        let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
        assert_eq!(p.label(G13Key::G1), Some("Copy"));
        assert_eq!(p.label(G13Key::G2), None);
    }

    #[test]
    fn empty_label_is_omitted() {
        let src = "[keys]\nG1 = \"a\"\n[labels]\nG1 = \"  \"\n";
        let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
        assert_eq!(p.label(G13Key::G1), None);
    }

    #[test]
    fn unknown_key_in_labels_errors() {
        let src = "[labels]\nG99 = \"Nope\"\n";
        assert!(Profile::from_raw(toml::from_str(src).unwrap()).is_err());
    }

    #[test]
    fn label_without_binding_still_loads() {
        let src = "[labels]\nG1 = \"Copy\"\n"; // no [keys]
        let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
        assert_eq!(p.label(G13Key::G1), Some("Copy"));
        assert_eq!(p.get_binding(G13Key::G1), None);
    }

    #[test]
    fn to_toml_round_trips_labels() {
        use std::collections::HashMap;
        let mut p = Profile::from_raw(raw(&[("G1", "ctrl+c")])).unwrap();
        let mut labels = HashMap::new();
        labels.insert(G13Key::G1, "Copy".to_string());
        p.set_labels(labels);
        let toml = p.to_toml().unwrap();
        assert!(toml.contains("[labels]"));
        let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
        assert_eq!(reloaded.label(G13Key::G1), Some("Copy"));
    }

    #[test]
    fn to_toml_omits_empty_labels_table() {
        let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        assert!(!p.to_toml().unwrap().contains("[labels]"));
    }

    #[test]
    fn joystick_labels_and_repeat_parse() {
        let raw: RawConfig = toml::from_str(
            "[joystick]\nup = \"w\"\ndown = \"s\"\n\
             [joystick.labels]\nup = \"Forward\"\n\
             [joystick.repeat]\ndown = true\n").unwrap();
        let p = Profile::from_raw(raw).unwrap();
        assert_eq!(p.joystick_label(JoystickDir::Up), Some("Forward"));
        assert_eq!(p.joystick_label(JoystickDir::Down), None);
        assert!(p.joystick_repeats(JoystickDir::Down));
        assert!(!p.joystick_repeats(JoystickDir::Up));
    }

    #[test]
    fn joystick_without_labels_repeat_is_backward_compatible() {
        let raw: RawConfig = toml::from_str("[joystick]\nup = \"w\"\n").unwrap();
        let p = Profile::from_raw(raw).unwrap();
        assert_eq!(p.joystick_label(JoystickDir::Up), None);
        assert!(!p.joystick_repeats(JoystickDir::Up));
    }

    #[test]
    fn joystick_unknown_direction_key_is_error() {
        let raw: RawConfig = toml::from_str(
            "[joystick]\nup = \"w\"\n[joystick.labels]\ndiagonal = \"x\"\n").unwrap();
        assert!(Profile::from_raw(raw).is_err());
    }

    #[test]
    fn joystick_labels_repeat_round_trip_to_toml() {
        let raw: RawConfig = toml::from_str("[joystick]\nup = \"w\"\ndown = \"s\"\n").unwrap();
        let mut p = Profile::from_raw(raw).unwrap();
        let mut labels = HashMap::new();
        labels.insert(JoystickDir::Up, "Forward".to_string());
        let mut repeat = HashMap::new();
        repeat.insert(JoystickDir::Down, true);
        p.set_joystick_labels(labels);
        p.set_joystick_repeat(repeat);

        let toml = p.to_toml().unwrap();
        let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
        assert_eq!(reloaded.joystick_label(JoystickDir::Up), Some("Forward"));
        assert!(reloaded.joystick_repeats(JoystickDir::Down));
        assert!(toml.contains("[joystick.labels]"));
        assert!(toml.contains("[joystick.repeat]"));
    }
}

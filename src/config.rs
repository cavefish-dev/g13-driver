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
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_deadzone")]
    pub deadzone: u16,
    pub up: Option<String>,
    pub down: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
}

fn default_mode() -> String { "wasd".to_string() }
fn default_deadzone() -> u16 { 30 }

#[derive(Debug, Deserialize, Clone)]
struct RawAutoRepeat {
    #[serde(default = "default_delay_ms")]
    delay_ms: u64,
    #[serde(default = "default_interval_ms")]
    interval_ms: u64,
}

fn default_delay_ms() -> u64 { 400 }
fn default_interval_ms() -> u64 { 40 }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoystickMode {
    Wasd,
    Mouse,
}

#[derive(Debug, Clone)]
pub struct JoystickConfig {
    pub mode: JoystickMode,
    pub deadzone: u8,
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

        let joystick = match raw.joystick {
            Some(rj) => Some(parse_joystick(rj)?),
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

        Ok(Self { key_bindings, joystick, repeat, labels, meta_name, source, modified, origin })
    }

    pub fn get_binding(&self, key: G13Key) -> Option<&str> {
        self.key_bindings.get(&key).map(|s| s.as_str())
    }

    pub fn joystick(&self) -> Option<&JoystickConfig> {
        self.joystick.as_ref()
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

    /// Serialize this profile back to TOML (keys + joystick + repeat). Comments in the
    /// original file are not preserved (the file becomes GUI-managed).
    pub fn to_toml(&self) -> Result<String> {
        let keys: HashMap<String, String> = self.key_bindings.iter()
            .map(|(k, v)| (format!("{k:?}"), v.clone())) // Debug of G13Key is "G1".."G22"
            .collect();
        let joystick = self.joystick.as_ref().map(|j| RawJoystick {
            mode: match j.mode {
                JoystickMode::Wasd => "wasd".to_string(),
                JoystickMode::Mouse => "mouse".to_string(),
            },
            deadzone: j.deadzone as u16,
            up: j.up.clone(),
            down: j.down.clone(),
            left: j.left.clone(),
            right: j.right.clone(),
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
struct RawManifest {
    profiles_dir: Option<String>,
    m1: Option<String>,
    m2: Option<String>,
    m3: Option<String>,
    #[serde(default)]
    autorepeat: Option<RawAutoRepeat>,
    #[serde(default)]
    app: Option<RawApp>,
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
    config_path: PathBuf,
    start_active: bool,
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
                config_path: config_path.to_path_buf(),
                start_active,
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
                config_path: config_path.to_path_buf(),
                start_active,
            })
        }
    }

    pub fn active(&self) -> MKey { self.active }

    pub fn autorepeat(&self) -> AutoRepeat { self.autorepeat }

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

    /// Replace the active profile's key bindings, repeat flags, and labels (joystick untouched)
    /// and write the profile file. The watcher will reload the identical content.
    pub fn save_active_bindings(
        &mut self,
        bindings: HashMap<G13Key, String>,
        repeat: HashMap<G13Key, bool>,
        labels: HashMap<G13Key, String>,
    ) -> Result<()> {
        if self.active_name().is_none() || self.active_profile().is_none() {
            anyhow::bail!("no profile in the active slot");
        }
        let path = self.active_path();
        let profile = self.active_profile_mut().expect("checked above");
        profile.set_bindings(bindings);
        profile.set_repeat(repeat);
        profile.set_labels(labels);
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
    let mode = match rj.mode.to_lowercase().as_str() {
        "wasd" => JoystickMode::Wasd,
        "mouse" => JoystickMode::Mouse,
        other => anyhow::bail!("unknown joystick mode: {} (expected wasd or mouse)", other),
    };
    if rj.deadzone > 127 {
        anyhow::bail!("joystick deadzone {} out of range (0-127)", rj.deadzone);
    }
    Ok(JoystickConfig {
        mode,
        deadzone: rj.deadzone as u8,
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
        assert_eq!(j.deadzone, 20);
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
        set.save_active_bindings(b, HashMap::new(), labels).unwrap();
        let text = std::fs::read_to_string(d.join("profiles/p.toml")).unwrap();
        assert!(text.contains("[labels]"));
        assert!(text.contains("Copy"));
        // reloads with the label intact
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(reloaded.active_profile().unwrap().label(G13Key::G1), Some("Copy"));
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
        set.save_active_bindings(b, HashMap::new(), HashMap::new()).unwrap();

        // Fresh load from disk reflects the change; joystick preserved; game untouched.
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(reloaded.active_profile().unwrap().get_binding(G13Key::G1), Some("ctrl+a"));
        assert_eq!(reloaded.active_profile().unwrap().get_binding(G13Key::G2), Some("f1"));
        assert!(reloaded.active_profile().unwrap().joystick().is_some(), "joystick preserved");
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
        set.save_active_bindings(b, HashMap::new(), HashMap::new()).unwrap();
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
        set.save_active_bindings(b, HashMap::new(), HashMap::new()).unwrap();
        let text = std::fs::read_to_string(d.join("profiles/u.toml")).unwrap();
        assert!(!text.contains("modified"), "user profile stays clean");
        assert!(!text.contains("source"), "user profile stays clean");
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
        assert_eq!(j.mode, JoystickMode::Wasd);
        assert_eq!(j.deadzone, 30);
        assert_eq!(j.up.as_deref(), Some("w"));
        assert_eq!(j.right.as_deref(), Some("d"));
    }

    #[test]
    fn joystick_mode_defaults_to_wasd() {
        let src = r#"
[joystick]
deadzone = 10
"#;
        let raw: RawConfig = toml::from_str(src).unwrap();
        let config = Profile::from_raw(raw).unwrap();
        assert_eq!(config.joystick().unwrap().mode, JoystickMode::Wasd);
        assert_eq!(config.joystick().unwrap().deadzone, 10);
    }

    #[test]
    fn deadzone_default_is_30() {
        let src = "[joystick]\nup = \"w\"\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let config = Profile::from_raw(raw).unwrap();
        assert_eq!(config.joystick().unwrap().deadzone, 30);
    }

    #[test]
    fn deadzone_over_127_is_error() {
        let src = "[joystick]\ndeadzone = 200\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        assert!(Profile::from_raw(raw).is_err());
    }

    #[test]
    fn unknown_joystick_mode_is_error() {
        let src = "[joystick]\nmode = \"flight\"\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        assert!(Profile::from_raw(raw).is_err());
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
}

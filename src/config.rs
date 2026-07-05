use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use crate::protocol::{G13Key, MKey};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawConfig {
    #[serde(default)]
    pub keys: HashMap<String, String>,
    #[serde(default)]
    pub joystick: Option<RawJoystick>,
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

        let joystick = match raw.joystick {
            Some(rj) => Some(parse_joystick(rj)?),
            None => None,
        };

        Ok(Self { key_bindings, joystick })
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

    /// Serialize this profile back to TOML (keys + joystick). Comments in the
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
        let raw = RawConfig { keys, joystick };
        toml::to_string(&raw).context("failed to serialize profile")
    }
}

#[derive(Debug, Deserialize)]
struct RawManifest {
    profiles_dir: Option<String>,
    m1: Option<String>,
    m2: Option<String>,
    m3: Option<String>,
}

/// The loaded profiles plus which M-key is active. Replaces a bare `Profile`
/// as the shared state so both the dispatcher and the GUI see profiles + active.
#[derive(Debug, Clone)]
pub struct ProfileSet {
    profiles_dir: PathBuf,
    m1: Profile,
    m2: Option<Profile>,
    m3: Option<Profile>,
    m1_name: Option<String>,
    m2_name: Option<String>,
    m3_name: Option<String>,
    /// Invariant: always points at a populated slot (or M1). `set_active` refuses
    /// empty slots, so `active_profile()`/`active_name()` stay coherent.
    active: MKey,
}

impl ProfileSet {
    /// Load from the manifest at `config_path`. Manifest mode when a top-level
    /// `m1` is present; otherwise the file is itself the single M1 profile
    /// (legacy). Paths resolve relative to the config file's directory.
    pub fn load(config_path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("failed to read config: {}", config_path.display()))?;
        let raw: RawManifest = toml::from_str(&content)
            .with_context(|| format!("failed to parse config: {}", config_path.display()))?;
        let base = config_path.parent().unwrap_or_else(|| Path::new("."));

        if let Some(m1_name) = raw.m1 {
            // Manifest mode.
            let dir = base.join(raw.profiles_dir.as_deref().unwrap_or("profiles"));
            let m1 = Profile::load(&dir.join(&m1_name))
                .with_context(|| format!("failed to load M1 profile {m1_name}"))?;
            let load_opt = |name: &Option<String>| -> (Option<Profile>, Option<String>) {
                match name {
                    Some(n) => match Profile::load(&dir.join(n)) {
                        Ok(p) => (Some(p), Some(n.clone())),
                        Err(e) => { log::warn!("skipping profile {n}: {e:#}"); (None, None) }
                    },
                    None => (None, None),
                }
            };
            let (m2, m2_name) = load_opt(&raw.m2);
            let (m3, m3_name) = load_opt(&raw.m3);
            Ok(Self {
                profiles_dir: dir,
                m1, m2, m3,
                m1_name: Some(m1_name),
                m2_name, m3_name,
                active: MKey::M1,
            })
        } else {
            // Legacy: the config file is a single profile.
            let m1 = Profile::load(&config_path.to_path_buf())?;
            let name = config_path.file_name().and_then(|s| s.to_str()).map(String::from);
            Ok(Self {
                profiles_dir: base.to_path_buf(),
                m1, m2: None, m3: None,
                m1_name: name, m2_name: None, m3_name: None,
                active: MKey::M1,
            })
        }
    }

    pub fn active(&self) -> MKey { self.active }

    pub fn active_profile(&self) -> &Profile {
        match self.active {
            MKey::M2 => self.m2.as_ref().unwrap_or(&self.m1),
            MKey::M3 => self.m3.as_ref().unwrap_or(&self.m1),
            _ => &self.m1,
        }
    }

    /// Switch the active profile. No-op (returns false) for MR or an empty slot.
    pub fn set_active(&mut self, k: MKey) -> bool {
        let ok = match k {
            MKey::M1 => true,
            MKey::M2 => self.m2.is_some(),
            MKey::M3 => self.m3.is_some(),
            MKey::MR => false,
        };
        if ok { self.active = k; }
        ok
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

    /// All `.toml` files in the profiles folder (for the GUI browse list).
    pub fn available(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.profiles_dir) {
            for e in entries.flatten() {
                if let Some(n) = e.file_name().to_str() {
                    if n.ends_with(".toml") { names.push(n.to_string()); }
                }
            }
        }
        names
    }

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

    fn active_profile_mut(&mut self) -> &mut Profile {
        // Invariant: `active` points at a populated slot (or M1).
        if self.active == MKey::M2 {
            if let Some(p) = self.m2.as_mut() { return p; }
        } else if self.active == MKey::M3 {
            if let Some(p) = self.m3.as_mut() { return p; }
        }
        &mut self.m1
    }

    /// Replace the active profile's key bindings (joystick untouched) and write
    /// the profile file. The watcher will reload the identical content.
    pub fn save_active_bindings(&mut self, bindings: HashMap<G13Key, String>) -> Result<()> {
        let path = self.active_path();
        let profile = self.active_profile_mut();
        profile.set_bindings(bindings);
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
        assert_eq!(set.active_profile().get_binding(crate::protocol::G13Key::G1), Some("ctrl+c"));
        assert_eq!(set.name(MKey::M2), Some("game.toml"));

        assert!(set.set_active(MKey::M2));
        assert_eq!(set.active_profile().get_binding(crate::protocol::G13Key::G1), Some("space"));

        // M3 unbound -> no-op switch, stays on M2.
        assert!(!set.set_active(MKey::M3));
        assert_eq!(set.active(), MKey::M2);
        // MR reserved -> no-op.
        assert!(!set.set_active(MKey::MR));
    }

    #[test]
    fn legacy_config_is_single_m1_profile() {
        let d = tmp("legacy");
        write(&d, "config.toml", "[keys]\nG1 = \"ctrl+c\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.active_profile().get_binding(crate::protocol::G13Key::G1), Some("ctrl+c"));
        assert!(set.name(MKey::M2).is_none());
    }

    #[test]
    fn missing_m1_is_error() {
        let d = tmp("missing-m1");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"nope.toml\"\n");
        assert!(ProfileSet::load(&d.join("config.toml")).is_err());
    }

    #[test]
    fn available_lists_toml_files() {
        let d = tmp("available");
        write(&d.join("profiles"), "default.toml", "[keys]\n");
        write(&d.join("profiles"), "extra.toml", "[keys]\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        let mut avail = set.available();
        avail.sort();
        assert_eq!(avail, vec!["default.toml".to_string(), "extra.toml".to_string()]);
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
        set.save_active_bindings(b).unwrap();

        // Fresh load from disk reflects the change; joystick preserved; game untouched.
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(reloaded.active_profile().get_binding(G13Key::G1), Some("ctrl+a"));
        assert_eq!(reloaded.active_profile().get_binding(G13Key::G2), Some("f1"));
        assert!(reloaded.active_profile().joystick().is_some(), "joystick preserved");
        // M2 file untouched.
        let game = std::fs::read_to_string(d.join("profiles/game.toml")).unwrap();
        assert!(game.contains("space"));
        // Manifest untouched.
        let manifest = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(manifest.contains("m1 = \"default.toml\""));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::G13Key;

    fn raw(pairs: &[(&str, &str)]) -> RawConfig {
        RawConfig {
            keys: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            joystick: None,
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
}

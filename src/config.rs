use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::Deserialize;
use crate::protocol::G13Key;

#[derive(Debug, Deserialize, Clone)]
pub struct RawConfig {
    #[serde(default)]
    pub keys: HashMap<String, String>,
    #[serde(default)]
    pub joystick: Option<RawJoystick>,
}

#[derive(Debug, Deserialize, Clone)]
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
}

/// Temporary alias so existing consumers keep compiling while the profile
/// layer is introduced. Removed in the ProfileSet wiring task.
pub type Config = Profile;

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

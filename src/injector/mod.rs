pub mod key_map;
#[cfg(windows)]
pub mod windows;

use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCombo {
    pub modifiers: Vec<Modifier>,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Windows,
}

pub trait KeyInjector: Send + Sync {
    fn press(&self, combo: &KeyCombo) -> Result<()>;
    /// Press and hold a single key down (no release). For joystick hold-to-move.
    fn key_down(&self, key: &str) -> Result<()>;
    /// Release a single key previously held with `key_down`.
    fn key_up(&self, key: &str) -> Result<()>;
}

impl KeyCombo {
    pub fn parse(s: &str) -> Result<Self> {
        let lower = s.to_lowercase();
        let parts: Vec<&str> = lower.split('+').map(str::trim).collect();
        let mut modifiers = Vec::new();
        let mut key: Option<String> = None;

        for part in &parts {
            match *part {
                "ctrl" | "control" => modifiers.push(Modifier::Ctrl),
                "shift"            => modifiers.push(Modifier::Shift),
                "alt"              => modifiers.push(Modifier::Alt),
                "windows" | "win" | "super" => modifiers.push(Modifier::Windows),
                k => {
                    if key.is_some() {
                        bail!("multiple non-modifier keys in combo: {}", s);
                    }
                    key = Some(k.to_string());
                }
            }
        }

        let key = key.ok_or_else(|| anyhow::anyhow!("no key in combo: {}", s))?;
        Ok(Self { modifiers, key })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_key() {
        let c = KeyCombo::parse("f5").unwrap();
        assert_eq!(c.key, "f5");
        assert!(c.modifiers.is_empty());
    }

    #[test]
    fn parse_ctrl_c() {
        let c = KeyCombo::parse("ctrl+c").unwrap();
        assert_eq!(c.key, "c");
        assert_eq!(c.modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn parse_shift_ctrl_esc() {
        let c = KeyCombo::parse("shift+ctrl+esc").unwrap();
        assert_eq!(c.key, "esc");
        assert!(c.modifiers.contains(&Modifier::Ctrl));
        assert!(c.modifiers.contains(&Modifier::Shift));
    }

    #[test]
    fn parse_is_case_insensitive() {
        let c = KeyCombo::parse("CTRL+C").unwrap();
        assert_eq!(c.key, "c");
        assert_eq!(c.modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn parse_windows_key() {
        let c = KeyCombo::parse("windows+d").unwrap();
        assert_eq!(c.key, "d");
        assert_eq!(c.modifiers, vec![Modifier::Windows]);
    }

    #[test]
    fn parse_no_key_is_error() {
        assert!(KeyCombo::parse("ctrl+shift").is_err());
    }
}

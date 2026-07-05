pub mod key_map;
#[cfg(windows)]
pub mod windows;

use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCombo {
    pub modifiers: Vec<Modifier>,
    pub key: Option<String>,
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
        let mut modifiers = Vec::new();
        let mut key: Option<String> = None;

        for part in lower.split('+').map(str::trim) {
            if part.is_empty() {
                continue; // tolerate trailing/double '+'
            }
            match part {
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

        if key.is_none() && modifiers.is_empty() {
            bail!("empty combo: {}", s);
        }
        Ok(Self { modifiers, key })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_key() {
        let c = KeyCombo::parse("f5").unwrap();
        assert_eq!(c.key.as_deref(), Some("f5"));
        assert!(c.modifiers.is_empty());
    }

    #[test]
    fn parse_ctrl_c() {
        let c = KeyCombo::parse("ctrl+c").unwrap();
        assert_eq!(c.key.as_deref(), Some("c"));
        assert_eq!(c.modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn parse_shift_ctrl_esc() {
        let c = KeyCombo::parse("shift+ctrl+esc").unwrap();
        assert_eq!(c.key.as_deref(), Some("esc"));
        assert!(c.modifiers.contains(&Modifier::Ctrl));
        assert!(c.modifiers.contains(&Modifier::Shift));
    }

    #[test]
    fn parse_is_case_insensitive() {
        let c = KeyCombo::parse("CTRL+C").unwrap();
        assert_eq!(c.key.as_deref(), Some("c"));
        assert_eq!(c.modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn parse_windows_key() {
        let c = KeyCombo::parse("windows+d").unwrap();
        assert_eq!(c.key.as_deref(), Some("d"));
        assert_eq!(c.modifiers, vec![Modifier::Windows]);
    }

    #[test]
    fn parse_modifier_only_is_ok() {
        let c = KeyCombo::parse("ctrl+shift").unwrap();
        assert!(c.key.is_none());
        assert_eq!(c.modifiers, vec![Modifier::Ctrl, Modifier::Shift]);
        let c = KeyCombo::parse("shift").unwrap();
        assert!(c.key.is_none());
        assert_eq!(c.modifiers, vec![Modifier::Shift]);
    }

    #[test]
    fn parse_empty_is_error() {
        assert!(KeyCombo::parse("").is_err());
        assert!(KeyCombo::parse("+").is_err());
    }

    #[test]
    fn parse_two_keys_is_error() {
        assert!(KeyCombo::parse("a+b").is_err());
    }
}

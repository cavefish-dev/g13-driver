use std::collections::HashSet;
use crate::protocol::{G13Event, G13Key, MKey};

/// USB connection status shown in the monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    Connected,
    Disconnected(String),
}

/// Live snapshot of the G13's input, reconstructed from the G13Event stream.
/// Pure and platform-neutral so it can be unit-tested and rendered by the GUI.
/// Extend here (M-keys, joystick click) when the parser decodes bytes 6/7.
#[derive(Debug, Clone)]
pub struct DeviceState {
    pub pressed: HashSet<G13Key>,
    pub mkeys: HashSet<MKey>,
    pub joy_x: u8,
    pub joy_y: u8,
    pub connection: Connection,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            pressed: HashSet::new(),
            mkeys: HashSet::new(),
            joy_x: 127,
            joy_y: 127,
            connection: Connection::Disconnected("connecting".to_string()),
        }
    }
}

impl DeviceState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one event into the live state. KeyDown/KeyUp maintain the pressed
    /// set; JoystickMove updates the axes. Connection is set by the consumer,
    /// not by events.
    pub fn apply(&mut self, event: &G13Event) {
        match event {
            G13Event::KeyDown(k) => { self.pressed.insert(*k); }
            G13Event::KeyUp(k) => { self.pressed.remove(k); }
            G13Event::JoystickMove { x, y } => {
                self.joy_x = *x;
                self.joy_y = *y;
            }
            G13Event::MKeyDown(m) => { self.mkeys.insert(*m); }
            G13Event::MKeyUp(m) => { self.mkeys.remove(m); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{G13Event, G13Key};

    #[test]
    fn default_is_centered_and_empty() {
        let s = DeviceState::new();
        assert!(s.pressed.is_empty());
        assert_eq!(s.joy_x, 127);
        assert_eq!(s.joy_y, 127);
        assert_eq!(s.connection, Connection::Disconnected("connecting".to_string()));
    }

    #[test]
    fn key_down_inserts() {
        let mut s = DeviceState::new();
        s.apply(&G13Event::KeyDown(G13Key::G1));
        assert!(s.pressed.contains(&G13Key::G1));
    }

    #[test]
    fn key_up_removes() {
        let mut s = DeviceState::new();
        s.apply(&G13Event::KeyDown(G13Key::G1));
        s.apply(&G13Event::KeyUp(G13Key::G1));
        assert!(!s.pressed.contains(&G13Key::G1));
    }

    #[test]
    fn key_up_of_unpressed_is_noop() {
        let mut s = DeviceState::new();
        s.apply(&G13Event::KeyUp(G13Key::G5)); // never pressed
        assert!(s.pressed.is_empty());
    }

    #[test]
    fn multiple_keys_tracked() {
        let mut s = DeviceState::new();
        s.apply(&G13Event::KeyDown(G13Key::G1));
        s.apply(&G13Event::KeyDown(G13Key::G2));
        assert_eq!(s.pressed.len(), 2);
        assert!(s.pressed.contains(&G13Key::G1));
        assert!(s.pressed.contains(&G13Key::G2));
    }

    #[test]
    fn joystick_move_updates_axes() {
        let mut s = DeviceState::new();
        s.apply(&G13Event::JoystickMove { x: 10, y: 240 });
        assert_eq!(s.joy_x, 10);
        assert_eq!(s.joy_y, 240);
    }

    #[test]
    fn mkey_down_and_up_tracked() {
        use crate::protocol::MKey;
        let mut s = DeviceState::new();
        s.apply(&G13Event::MKeyDown(MKey::M2));
        assert!(s.mkeys.contains(&MKey::M2));
        s.apply(&G13Event::MKeyUp(MKey::M2));
        assert!(!s.mkeys.contains(&MKey::M2));
    }
}

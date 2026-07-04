use anyhow::Result;
use std::sync::{Arc, RwLock};
use crate::config::{Config, JoystickMode};
use crate::injector::{KeyCombo, KeyInjector};
use crate::joystick::{HoldAction, JoystickMapper};
use crate::protocol::{G13Event, G13Key};

pub struct Dispatcher {
    config: Arc<RwLock<Config>>,
    injector: Box<dyn KeyInjector>,
    joystick: JoystickMapper,
}

impl Dispatcher {
    pub fn new(config: Arc<RwLock<Config>>, injector: Box<dyn KeyInjector>) -> Self {
        Self { config, injector, joystick: JoystickMapper::new() }
    }

    pub fn handle(&mut self, event: G13Event) -> Result<()> {
        match event {
            G13Event::KeyDown(key) => self.handle_key(key)?,
            G13Event::KeyUp(_) => {}
            G13Event::JoystickMove { x, y } => self.handle_joystick(x, y),
            G13Event::MKeyDown(_) | G13Event::MKeyUp(_) => {} // handled in a later task
        }
        Ok(())
    }

    fn handle_key(&self, key: G13Key) -> Result<()> {
        let binding = {
            let cfg = self.config.read().unwrap();
            cfg.get_binding(key).map(str::to_owned)
        };
        match &binding {
            Some(b) => log::debug!("{key:?} -> {b}"),
            None => log::debug!("{key:?} -> (unmapped)"),
        }
        if let Some(binding) = binding {
            let combo = KeyCombo::parse(&binding)?;
            self.injector.press(&combo)?;
        }
        Ok(())
    }

    fn handle_joystick(&mut self, x: u8, y: u8) {
        // Read joystick config live so hot-reload takes effect. Clone so the
        // RwLock guard is released before we touch the injector.
        let cfg = {
            let guard = self.config.read().unwrap();
            guard.joystick()
                .filter(|j| j.mode == JoystickMode::Wasd)
                .cloned()
        };
        let actions = match &cfg {
            Some(jc) => self.joystick.update(x, y, jc),
            None => Vec::new(),
        };
        self.apply(actions);
    }

    fn apply(&self, actions: Vec<HoldAction>) {
        for action in actions {
            log::debug!("joystick {action:?}");
            let result = match &action {
                HoldAction::KeyDown(k) => self.injector.key_down(k),
                HoldAction::KeyUp(k) => self.injector.key_up(k),
            };
            if let Err(e) = result {
                log::warn!("joystick injection failed for {action:?}: {e:#}");
            }
        }
    }

    /// Release every currently-held joystick key. Call on shutdown / USB error
    /// so a deflected stick does not leave keys stuck down.
    pub fn release_held(&mut self) {
        let actions = self.joystick.release_all();
        self.apply(actions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, RawConfig};
    use crate::injector::Modifier;
    use crate::protocol::{G13Event, G13Key};
    use std::sync::{Arc, Mutex, RwLock};

    struct MockInjector {
        combos: Arc<Mutex<Vec<KeyCombo>>>,
        holds: Arc<Mutex<Vec<String>>>,
    }

    impl MockInjector {
        fn new() -> (Self, Arc<Mutex<Vec<KeyCombo>>>) {
            let combos = Arc::new(Mutex::new(Vec::new()));
            let holds = Arc::new(Mutex::new(Vec::new()));
            (Self { combos: combos.clone(), holds }, combos)
        }

        fn new_with_holds() -> (Self, Arc<Mutex<Vec<String>>>) {
            let combos = Arc::new(Mutex::new(Vec::new()));
            let holds = Arc::new(Mutex::new(Vec::new()));
            (Self { combos, holds: holds.clone() }, holds)
        }
    }

    impl KeyInjector for MockInjector {
        fn press(&self, combo: &KeyCombo) -> anyhow::Result<()> {
            self.combos.lock().unwrap().push(combo.clone());
            Ok(())
        }
        fn key_down(&self, key: &str) -> anyhow::Result<()> {
            self.holds.lock().unwrap().push(format!("down:{}", key));
            Ok(())
        }
        fn key_up(&self, key: &str) -> anyhow::Result<()> {
            self.holds.lock().unwrap().push(format!("up:{}", key));
            Ok(())
        }
    }

    fn make_config(pairs: &[(&str, &str)]) -> Arc<RwLock<Config>> {
        let keys = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Arc::new(RwLock::new(Config::from_raw(RawConfig { keys, joystick: None }).unwrap()))
    }

    fn config_with_joystick() -> Arc<RwLock<Config>> {
        let src = r#"
[keys]
[joystick]
mode = "wasd"
deadzone = 30
up = "w"
down = "s"
left = "a"
right = "d"
"#;
        let raw: RawConfig = toml::from_str(src).unwrap();
        Arc::new(RwLock::new(Config::from_raw(raw).unwrap()))
    }

    #[test]
    fn key_down_triggers_injection() {
        let config = make_config(&[("G1", "ctrl+c")]);
        let (injector, calls) = MockInjector::new();
        let mut d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].key, "c");
        assert_eq!(calls[0].modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn key_up_is_ignored() {
        let config = make_config(&[("G1", "ctrl+c")]);
        let (injector, calls) = MockInjector::new();
        let mut d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyUp(G13Key::G1)).unwrap();

        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn unmapped_key_does_nothing() {
        let config = make_config(&[]);
        let (injector, calls) = MockInjector::new();
        let mut d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G5)).unwrap();

        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn two_keys_dispatched_independently() {
        let config = make_config(&[("G1", "ctrl+c"), ("G2", "f5")]);
        let (injector, calls) = MockInjector::new();
        let mut d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.handle(G13Event::KeyDown(G13Key::G2)).unwrap();

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].key, "f5");
        assert!(calls[1].modifiers.is_empty());
    }

    #[test]
    fn joystick_move_left_holds_key() {
        let (injector, holds) = MockInjector::new_with_holds();
        let mut d = Dispatcher::new(config_with_joystick(), Box::new(injector));
        d.handle(G13Event::JoystickMove { x: 0, y: 127 }).unwrap();
        assert_eq!(*holds.lock().unwrap(), vec!["down:a".to_string()]);
    }

    #[test]
    fn joystick_return_to_center_releases_key() {
        let (injector, holds) = MockInjector::new_with_holds();
        let mut d = Dispatcher::new(config_with_joystick(), Box::new(injector));
        d.handle(G13Event::JoystickMove { x: 0, y: 127 }).unwrap();
        d.handle(G13Event::JoystickMove { x: 127, y: 127 }).unwrap();
        assert_eq!(*holds.lock().unwrap(), vec!["down:a".to_string(), "up:a".to_string()]);
    }

    #[test]
    fn joystick_ignored_when_no_config() {
        let (injector, holds) = MockInjector::new_with_holds();
        let config = make_config(&[("G1", "ctrl+c")]); // no [joystick]
        let mut d = Dispatcher::new(config, Box::new(injector));
        d.handle(G13Event::JoystickMove { x: 0, y: 127 }).unwrap();
        assert!(holds.lock().unwrap().is_empty());
    }

    #[test]
    fn release_held_lifts_keys() {
        let (injector, holds) = MockInjector::new_with_holds();
        let mut d = Dispatcher::new(config_with_joystick(), Box::new(injector));
        d.handle(G13Event::JoystickMove { x: 0, y: 0 }).unwrap(); // hold a + w
        holds.lock().unwrap().clear();
        d.release_held();
        let mut got = holds.lock().unwrap().clone();
        got.sort();
        assert_eq!(got, vec!["up:a".to_string(), "up:w".to_string()]);
    }
}

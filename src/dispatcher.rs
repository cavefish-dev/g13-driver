use anyhow::Result;
use std::sync::{Arc, RwLock};
use crate::config::Config;
use crate::injector::{KeyCombo, KeyInjector};
use crate::protocol::{G13Event, G13Key};

pub struct Dispatcher {
    config: Arc<RwLock<Config>>,
    injector: Box<dyn KeyInjector>,
}

impl Dispatcher {
    pub fn new(config: Arc<RwLock<Config>>, injector: Box<dyn KeyInjector>) -> Self {
        Self { config, injector }
    }

    pub fn handle(&self, event: G13Event) -> Result<()> {
        let G13Event::KeyDown(key) = event else { return Ok(()); };
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

    #[test]
    fn key_down_triggers_injection() {
        let config = make_config(&[("G1", "ctrl+c")]);
        let (injector, calls) = MockInjector::new();
        let d = Dispatcher::new(config, Box::new(injector));

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
        let d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyUp(G13Key::G1)).unwrap();

        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn unmapped_key_does_nothing() {
        let config = make_config(&[]);
        let (injector, calls) = MockInjector::new();
        let d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G5)).unwrap();

        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn two_keys_dispatched_independently() {
        let config = make_config(&[("G1", "ctrl+c"), ("G2", "f5")]);
        let (injector, calls) = MockInjector::new();
        let d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.handle(G13Event::KeyDown(G13Key::G2)).unwrap();

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].key, "f5");
        assert!(calls[1].modifiers.is_empty());
    }
}

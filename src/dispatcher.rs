use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use crate::config::{JoystickMode, ProfileSet};
use crate::injector::{KeyCombo, KeyInjector};
use crate::injector::key_map::tap_only_keys;
use crate::joystick::{HoldAction, JoystickMapper};
use crate::protocol::{G13Event, G13Key, MKey};

pub struct Dispatcher {
    profiles: Arc<RwLock<ProfileSet>>,
    injector: Box<dyn KeyInjector>,
    joystick: JoystickMapper,
    held_keys: HashMap<G13Key, KeyCombo>,
    tap_only: HashSet<String>,
}

impl Dispatcher {
    pub fn new(profiles: Arc<RwLock<ProfileSet>>, injector: Box<dyn KeyInjector>) -> Self {
        Self {
            profiles,
            injector,
            joystick: JoystickMapper::new(),
            held_keys: HashMap::new(),
            tap_only: tap_only_keys(),
        }
    }

    pub fn handle(&mut self, event: G13Event) -> Result<()> {
        match event {
            G13Event::KeyDown(key) => self.handle_key_down(key),
            G13Event::KeyUp(key) => self.handle_key_up(key),
            G13Event::JoystickMove { x, y } => self.handle_joystick(x, y),
            G13Event::MKeyDown(m) => self.handle_mkey(m),
            G13Event::MKeyUp(_) => {}
        }
        Ok(())
    }

    fn handle_key_down(&mut self, key: G13Key) {
        let binding = {
            let set = self.profiles.read().unwrap();
            set.active_profile().get_binding(key).map(str::to_owned)
        };
        let Some(binding) = binding else {
            log::debug!("{key:?} -> (unmapped)");
            return;
        };
        log::debug!("{key:?} -> {binding}");
        let combo = match KeyCombo::parse(&binding) {
            Ok(c) => c,
            Err(e) => { log::warn!("bad binding {binding:?}: {e:#}"); return; }
        };
        // Media keys tap; everything else holds.
        let is_media = combo.key.as_ref().is_some_and(|k| self.tap_only.contains(k));
        if is_media {
            if let Err(e) = self.injector.press(&combo) {
                log::warn!("injection failed: {e:#}");
            }
        } else {
            match self.injector.combo_down(&combo) {
                Ok(()) => { self.held_keys.insert(key, combo); }
                Err(e) => log::warn!("injection failed: {e:#}"),
            }
        }
    }

    fn handle_key_up(&mut self, key: G13Key) {
        if let Some(combo) = self.held_keys.remove(&key) {
            if let Err(e) = self.injector.combo_up(&combo) {
                log::warn!("injection failed: {e:#}");
            }
        }
    }

    fn handle_joystick(&mut self, x: u8, y: u8) {
        // Read the active profile's joystick config live; clone so the guard is
        // dropped before we touch the injector.
        let cfg = {
            let set = self.profiles.read().unwrap();
            set.active_profile().joystick()
                .filter(|j| j.mode == JoystickMode::Wasd)
                .cloned()
        };
        let actions = match &cfg {
            Some(jc) => self.joystick.update(x, y, jc),
            None => Vec::new(),
        };
        self.apply(actions);
    }

    /// Switch profile on M1/M2/M3. Release held joystick keys first (a new profile
    /// may rebind the stick); held G-keys stay down until their physical KeyUp.
    fn handle_mkey(&mut self, m: MKey) {
        if m == MKey::MR { return; }
        self.release_joystick();
        let mut set = self.profiles.write().unwrap();
        if set.set_active(m) {
            log::info!("profile -> {}", set.name(m).unwrap_or("?"));
        } else {
            log::warn!("no profile bound to {m:?}");
        }
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

    fn release_joystick(&mut self) {
        let actions = self.joystick.release_all();
        self.apply(actions);
    }

    /// Release everything held — the joystick and all held G-key combos. Call on
    /// Active->Dry-run, USB disconnect, and shutdown so nothing sticks.
    pub fn release_held(&mut self) {
        self.release_joystick();
        for (_key, combo) in self.held_keys.drain() {
            if let Err(e) = self.injector.combo_up(&combo) {
                log::warn!("injection failed on release: {e:#}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProfileSet;
    use crate::injector::Modifier;
    use crate::protocol::{G13Event, G13Key, MKey};
    use std::sync::{Arc, Mutex, RwLock};

    struct MockInjector {
        combos: Arc<Mutex<Vec<KeyCombo>>>,
        holds: Arc<Mutex<Vec<String>>>,
        combo_downs: Arc<Mutex<Vec<KeyCombo>>>,
        combo_ups: Arc<Mutex<Vec<KeyCombo>>>,
    }

    impl MockInjector {
        fn new() -> (Self, Arc<Mutex<Vec<KeyCombo>>>) {
            let combos = Arc::new(Mutex::new(Vec::new()));
            let holds = Arc::new(Mutex::new(Vec::new()));
            (Self {
                combos: combos.clone(),
                holds,
                combo_downs: Arc::new(Mutex::new(Vec::new())),
                combo_ups: Arc::new(Mutex::new(Vec::new())),
            }, combos)
        }

        fn new_with_holds() -> (Self, Arc<Mutex<Vec<String>>>) {
            let combos = Arc::new(Mutex::new(Vec::new()));
            let holds = Arc::new(Mutex::new(Vec::new()));
            (Self {
                combos,
                holds: holds.clone(),
                combo_downs: Arc::new(Mutex::new(Vec::new())),
                combo_ups: Arc::new(Mutex::new(Vec::new())),
            }, holds)
        }

        fn new_combos() -> (Self, Arc<Mutex<Vec<KeyCombo>>>, Arc<Mutex<Vec<KeyCombo>>>) {
            let combos = Arc::new(Mutex::new(Vec::new()));
            let holds = Arc::new(Mutex::new(Vec::new()));
            let combo_downs = Arc::new(Mutex::new(Vec::new()));
            let combo_ups = Arc::new(Mutex::new(Vec::new()));
            (
                Self { combos, holds, combo_downs: combo_downs.clone(), combo_ups: combo_ups.clone() },
                combo_downs,
                combo_ups,
            )
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
        fn combo_down(&self, combo: &KeyCombo) -> anyhow::Result<()> {
            self.combo_downs.lock().unwrap().push(combo.clone());
            Ok(())
        }
        fn combo_up(&self, combo: &KeyCombo) -> anyhow::Result<()> {
            self.combo_ups.lock().unwrap().push(combo.clone());
            Ok(())
        }
    }

    fn write(p: &std::path::Path, body: &str) { std::fs::write(p, body).unwrap(); }

    fn tmp(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static CTR: AtomicU64 = AtomicU64::new(0);
        let n = CTR.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("g13-disp-{tag}-{n}"));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn make_config(pairs: &[(&str, &str)]) -> Arc<RwLock<ProfileSet>> {
        let d = tmp("single");
        std::fs::create_dir_all(&d).unwrap();
        let mut body = String::from("[keys]\n");
        for (k, v) in pairs { body.push_str(&format!("{k} = \"{v}\"\n")); }
        write(&d.join("config.toml"), &body);
        Arc::new(RwLock::new(ProfileSet::load(&d.join("config.toml")).unwrap()))
    }

    fn config_with_joystick() -> Arc<RwLock<ProfileSet>> {
        let d = tmp("joy");
        std::fs::create_dir_all(&d).unwrap();
        let body = "[keys]\n[joystick]\nmode = \"wasd\"\ndeadzone = 30\nup = \"w\"\ndown = \"s\"\nleft = \"a\"\nright = \"d\"\n";
        write(&d.join("config.toml"), body);
        Arc::new(RwLock::new(ProfileSet::load(&d.join("config.toml")).unwrap()))
    }

    fn profiles_two() -> Arc<RwLock<ProfileSet>> {
        let d = tmp("two");
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        write(&d.join("profiles/default.toml"), "[keys]\nG1 = \"ctrl+c\"\n");
        write(&d.join("profiles/game.toml"), "[keys]\nG1 = \"space\"\n[joystick]\nup=\"w\"\n");
        write(&d.join("config.toml"), "profiles_dir=\"profiles\"\nm1=\"default.toml\"\nm2=\"game.toml\"\n");
        Arc::new(RwLock::new(ProfileSet::load(&d.join("config.toml")).unwrap()))
    }

    // --- Migrated tests (press -> combo_down) ---

    #[test]
    fn key_down_triggers_injection() {
        let config = make_config(&[("G1", "ctrl+c")]);
        let (injector, downs, _ups) = MockInjector::new_combos();
        let mut d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();

        let downs = downs.lock().unwrap();
        assert_eq!(downs.len(), 1);
        assert_eq!(downs[0].key.as_deref(), Some("c"));
        assert_eq!(downs[0].modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn two_keys_dispatched_independently() {
        let config = make_config(&[("G1", "ctrl+c"), ("G2", "f5")]);
        let (injector, downs, _ups) = MockInjector::new_combos();
        let mut d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.handle(G13Event::KeyDown(G13Key::G2)).unwrap();

        let downs = downs.lock().unwrap();
        assert_eq!(downs.len(), 2);
        assert_eq!(downs[1].key.as_deref(), Some("f5"));
        assert!(downs[1].modifiers.is_empty());
    }

    // --- Retained tests (unchanged behavior) ---

    #[test]
    fn unmapped_key_does_nothing() {
        let config = make_config(&[]);
        let (injector, calls) = MockInjector::new();
        let mut d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G5)).unwrap();

        assert!(calls.lock().unwrap().is_empty());
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

    #[test]
    fn mkey_switches_active_profile() {
        let (injector, downs, _ups) = MockInjector::new_combos();
        let mut d = Dispatcher::new(profiles_two(), Box::new(injector));
        d.handle(G13Event::MKeyDown(MKey::M2)).unwrap();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        assert_eq!(downs.lock().unwrap()[0].key.as_deref(), Some("space"));
    }

    #[test]
    fn mkey_switch_releases_held_joystick() {
        let (injector, holds) = MockInjector::new_with_holds();
        let mut d = Dispatcher::new(profiles_two(), Box::new(injector));
        // With M2 (has joystick up=w) active, hold up so a key is held.
        d.handle(G13Event::MKeyDown(MKey::M2)).unwrap();
        d.handle(G13Event::JoystickMove { x: 127, y: 0 }).unwrap(); // hold "w"
        holds.lock().unwrap().clear();
        // Switch back to M1 -> release_joystick fires before the switch.
        d.handle(G13Event::MKeyDown(MKey::M1)).unwrap();
        assert!(holds.lock().unwrap().iter().any(|s| s == "up:w"));
    }

    // --- New hold-means-hold tests ---

    #[test]
    fn gkey_holds_and_releases() {
        let (injector, downs, ups) = MockInjector::new_combos();
        let mut d = Dispatcher::new(make_config(&[("G1", "ctrl+c")]), Box::new(injector));
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        assert_eq!(downs.lock().unwrap().len(), 1);
        assert_eq!(downs.lock().unwrap()[0].key.as_deref(), Some("c"));
        assert!(ups.lock().unwrap().is_empty());
        d.handle(G13Event::KeyUp(G13Key::G1)).unwrap();
        assert_eq!(ups.lock().unwrap()[0].key.as_deref(), Some("c"));
    }

    #[test]
    fn gkey_modifier_only_holds() {
        let (injector, downs, ups) = MockInjector::new_combos();
        let mut d = Dispatcher::new(make_config(&[("G1", "shift")]), Box::new(injector));
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        assert!(downs.lock().unwrap()[0].key.is_none());
        assert_eq!(downs.lock().unwrap()[0].modifiers, vec![Modifier::Shift]);
        // The modifier-only combo is released on KeyUp too.
        d.handle(G13Event::KeyUp(G13Key::G1)).unwrap();
        assert!(ups.lock().unwrap()[0].key.is_none());
        assert_eq!(ups.lock().unwrap()[0].modifiers, vec![Modifier::Shift]);
    }

    #[test]
    fn media_key_taps_not_held() {
        let (injector, downs, ups) = MockInjector::new_combos();
        let calls = injector.combos.clone(); // press() recording
        let mut d = Dispatcher::new(make_config(&[("G1", "playpause")]), Box::new(injector));
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        assert!(downs.lock().unwrap().is_empty(), "media key should not be held");
        assert_eq!(calls.lock().unwrap().len(), 1, "media key should tap via press");
        d.handle(G13Event::KeyUp(G13Key::G1)).unwrap();
        assert!(ups.lock().unwrap().is_empty(), "no release for a tapped media key");
    }

    #[test]
    fn release_held_lifts_held_gkeys() {
        let (injector, _downs, ups) = MockInjector::new_combos();
        let mut d = Dispatcher::new(make_config(&[("G1", "w")]), Box::new(injector));
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.release_held();
        assert_eq!(ups.lock().unwrap()[0].key.as_deref(), Some("w"));
    }
}

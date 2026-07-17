use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use crate::config::ProfileSet;
use crate::injector::{KeyCombo, KeyInjector};
use crate::injector::key_map::tap_only_keys;
use crate::joystick::{HoldAction, JoystickMapper};
use crate::protocol::{G13Event, G13Key, MKey};

/// A currently-held key binding plus its auto-repeat schedule.
struct HeldKey {
    combo: KeyCombo,
    repeat: bool,        // snapshot of profile.repeats(key) at press time
    delay_ms: u64,       // snapshot of manifest timing at press time
    interval_ms: u64,
    next_repeat: Option<Instant>, // None until the first tick schedules it
}

/// A currently-held joystick direction plus its auto-repeat schedule.
struct JoyHeld {
    key: String,
    delay_ms: u64,
    interval_ms: u64,
    next_repeat: Option<Instant>,
}

pub struct Dispatcher {
    profiles: Arc<RwLock<ProfileSet>>,
    injector: Box<dyn KeyInjector>,
    joystick: JoystickMapper,
    held_keys: HashMap<G13Key, HeldKey>,
    joystick_held: HashMap<crate::config::JoystickDir, JoyHeld>,
    tap_only: HashSet<String>,
}

impl Dispatcher {
    pub fn new(profiles: Arc<RwLock<ProfileSet>>, injector: Box<dyn KeyInjector>) -> Self {
        Self {
            profiles,
            injector,
            joystick: JoystickMapper::new(),
            held_keys: HashMap::new(),
            joystick_held: HashMap::new(),
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
        let (binding, repeat, ar) = {
            let set = self.profiles.read().unwrap();
            match set.active_profile() {
                Some(p) => (p.get_binding(key).map(str::to_owned), p.repeats(key), set.autorepeat()),
                None => (None, false, set.autorepeat()),
            }
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
                Ok(()) => {
                    self.held_keys.insert(key, HeldKey {
                        combo,
                        repeat,
                        delay_ms: ar.delay_ms,
                        interval_ms: ar.interval_ms,
                        next_repeat: None,
                    });
                }
                Err(e) => log::warn!("injection failed: {e:#}"),
            }
        }
    }

    fn handle_key_up(&mut self, key: G13Key) {
        if let Some(held) = self.held_keys.remove(&key) {
            if let Err(e) = self.injector.combo_up(&held.combo) {
                log::warn!("injection failed: {e:#}");
            }
        }
    }

    /// Re-fire held, repeat-enabled keys whose interval has elapsed. Called
    /// periodically by the consumer loop with the current time. Collect first,
    /// inject second, so we don't borrow `held_keys` while calling the injector.
    pub fn tick(&mut self, now: Instant) {
        let mut to_fire: Vec<String> = Vec::new();
        for held in self.held_keys.values_mut() {
            if !held.repeat { continue; }
            let Some(key) = held.combo.key.as_deref() else { continue; };
            match held.next_repeat {
                None => {
                    held.next_repeat = Some(now + Duration::from_millis(held.delay_ms));
                }
                Some(mut due) => {
                    while now >= due {
                        to_fire.push(key.to_string());
                        due += Duration::from_millis(held.interval_ms);
                    }
                    held.next_repeat = Some(due);
                }
            }
        }
        for held in self.joystick_held.values_mut() {
            match held.next_repeat {
                None => held.next_repeat = Some(now + Duration::from_millis(held.delay_ms)),
                Some(mut due) => {
                    while now >= due {
                        to_fire.push(held.key.clone());
                        due += Duration::from_millis(held.interval_ms);
                    }
                    held.next_repeat = Some(due);
                }
            }
        }
        for key in to_fire {
            if let Err(e) = self.injector.key_down(&key) {
                log::warn!("auto-repeat injection failed: {e:#}");
            }
        }
    }

    fn handle_joystick(&mut self, x: u8, y: u8) {
        // Snapshot the active profile's joystick directions, the global deadzone, and the
        // autorepeat timing under a short read lock, then drop the guard before we touch
        // the injector.
        let (cfg, deadzone, ar) = {
            let set = self.profiles.read().unwrap();
            (set.active_profile().and_then(|p| p.joystick()).cloned(),
             set.joystick_deadzone(), set.autorepeat())
        };
        let actions = match &cfg {
            Some(jc) => self.joystick.update(x, y, jc, deadzone),
            None => Vec::new(),
        };
        for action in actions {
            match action {
                HoldAction::KeyDown { dir, key } => {
                    if let Err(e) = self.injector.key_down(&key) {
                        log::warn!("joystick injection failed: {e:#}");
                    }
                    let repeats = {
                        let set = self.profiles.read().unwrap();
                        set.active_profile().map(|p| p.joystick_repeats(dir)).unwrap_or(false)
                    };
                    if repeats {
                        self.joystick_held.insert(dir, JoyHeld {
                            key, delay_ms: ar.delay_ms, interval_ms: ar.interval_ms, next_repeat: None,
                        });
                    }
                }
                HoldAction::KeyUp { dir, key } => {
                    if let Err(e) = self.injector.key_up(&key) {
                        log::warn!("joystick injection failed: {e:#}");
                    }
                    self.joystick_held.remove(&dir);
                }
            }
        }
    }

    /// Switch profile on M1/M2/M3. Release held joystick keys first (a new profile
    /// may rebind the stick); held G-keys stay down until their physical KeyUp.
    fn handle_mkey(&mut self, m: MKey) {
        if m == MKey::MR { return; }
        self.release_joystick();
        let mut set = self.profiles.write().unwrap();
        if set.set_active(m) {
            log::info!("profile -> {}", set.name(m).unwrap_or("(none)"));
        }
    }

    fn apply(&self, actions: Vec<HoldAction>) {
        for action in actions {
            log::debug!("joystick {action:?}");
            let result = match &action {
                HoldAction::KeyDown { key, .. } => self.injector.key_down(key),
                HoldAction::KeyUp { key, .. } => self.injector.key_up(key),
            };
            if let Err(e) = result {
                log::warn!("joystick injection failed for {action:?}: {e:#}");
            }
        }
    }

    fn release_joystick(&mut self) {
        let actions = self.joystick.release_all();
        self.apply(actions);
        self.joystick_held.clear();
    }

    /// Release everything held — the joystick and all held G-key combos. Call on
    /// Active->Dry-run, USB disconnect, and shutdown so nothing sticks.
    pub fn release_held(&mut self) {
        self.release_joystick();
        for (_key, held) in self.held_keys.drain() {
            if let Err(e) = self.injector.combo_up(&held.combo) {
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
    use std::time::{Duration, Instant};

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
        let body = "[keys]\n[joystick]\nup = \"w\"\ndown = \"s\"\nleft = \"a\"\nright = \"d\"\n";
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

    fn make_config_repeat(keys: &str, repeat: &str, delay: u64, interval: u64) -> Arc<RwLock<ProfileSet>> {
        let d = tmp("rep");
        std::fs::create_dir_all(&d).unwrap();
        let body = format!(
            "[keys]\n{keys}\n[repeat]\n{repeat}\n[autorepeat]\ndelay_ms = {delay}\ninterval_ms = {interval}\n"
        );
        write(&d.join("config.toml"), &body);
        Arc::new(RwLock::new(ProfileSet::load(&d.join("config.toml")).unwrap()))
    }

    #[test]
    fn held_key_repeats_after_delay() {
        let (injector, holds) = MockInjector::new_with_holds();
        let config = make_config_repeat("G1 = \"a\"", "G1 = true", 100, 50);
        let mut d = Dispatcher::new(config, Box::new(injector));
        let t0 = Instant::now();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.tick(t0); // schedules first repeat at t0+100ms; no fire yet
        assert!(holds.lock().unwrap().is_empty());
        d.tick(t0 + Duration::from_millis(101)); // first repeat
        assert_eq!(*holds.lock().unwrap(), vec!["down:a".to_string()]);
        d.tick(t0 + Duration::from_millis(151)); // second repeat
        assert_eq!(*holds.lock().unwrap(),
            vec!["down:a".to_string(), "down:a".to_string()]);
    }

    #[test]
    fn disabled_key_never_repeats() {
        let (injector, holds) = MockInjector::new_with_holds();
        // G1 bound but only G2 is in [repeat].
        let config = make_config_repeat("G1 = \"a\"", "G2 = true", 100, 50);
        let mut d = Dispatcher::new(config, Box::new(injector));
        let t0 = Instant::now();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.tick(t0);
        d.tick(t0 + Duration::from_millis(500));
        assert!(holds.lock().unwrap().is_empty());
    }

    #[test]
    fn combo_repeat_fires_key_only() {
        let (injector, holds) = MockInjector::new_with_holds();
        let config = make_config_repeat("G1 = \"ctrl+c\"", "G1 = true", 100, 50);
        let mut d = Dispatcher::new(config, Box::new(injector));
        let t0 = Instant::now();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.tick(t0); // first tick anchors the schedule at t0+100ms; no fire yet
        d.tick(t0 + Duration::from_millis(101)); // first repeat
        assert_eq!(*holds.lock().unwrap(), vec!["down:c".to_string()]);
    }

    #[test]
    fn modifier_only_repeat_is_noop() {
        let (injector, holds) = MockInjector::new_with_holds();
        let config = make_config_repeat("G1 = \"shift\"", "G1 = true", 100, 50);
        let mut d = Dispatcher::new(config, Box::new(injector));
        let t0 = Instant::now();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.tick(t0 + Duration::from_millis(300));
        assert!(holds.lock().unwrap().is_empty(), "modifier-only has no key to repeat");
    }

    #[test]
    fn media_key_with_repeat_never_held_or_repeated() {
        let (injector, holds) = MockInjector::new_with_holds();
        let config = make_config_repeat("G1 = \"playpause\"", "G1 = true", 100, 50);
        let mut d = Dispatcher::new(config, Box::new(injector));
        let t0 = Instant::now();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.tick(t0 + Duration::from_millis(300));
        assert!(holds.lock().unwrap().is_empty()); // tapped via press(), never held
    }

    #[test]
    fn repeat_stops_after_key_up() {
        let (injector, holds) = MockInjector::new_with_holds();
        let config = make_config_repeat("G1 = \"a\"", "G1 = true", 100, 50);
        let mut d = Dispatcher::new(config, Box::new(injector));
        let t0 = Instant::now();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.tick(t0); // anchor
        d.tick(t0 + Duration::from_millis(101)); // one repeat
        assert_eq!(holds.lock().unwrap().len(), 1);
        d.handle(G13Event::KeyUp(G13Key::G1)).unwrap();
        holds.lock().unwrap().clear();
        d.tick(t0 + Duration::from_millis(300)); // released -> no more
        assert!(holds.lock().unwrap().is_empty());
    }

    #[test]
    fn joystick_repeat_refires_on_tick() {
        let d = tmp("joyrep");
        std::fs::create_dir_all(&d).unwrap();
        write(&d.join("config.toml"),
            "[keys]\n[joystick]\nup = \"w\"\ndown = \"s\"\n\
             [joystick.repeat]\nup = true\n\
             [autorepeat]\ndelay_ms = 0\ninterval_ms = 1\n");
        let config = Arc::new(RwLock::new(ProfileSet::load(&d.join("config.toml")).unwrap()));
        let (injector, holds) = MockInjector::new_with_holds();
        let mut disp = Dispatcher::new(config, Box::new(injector));

        // Full up -> key_down("w") once; repeat registered for Up.
        disp.handle(G13Event::JoystickMove { x: 127, y: 0 }).unwrap();
        assert_eq!(holds.lock().unwrap().clone(), vec!["down:w"]);

        // tick past the (zero) delay -> "w" re-fires.
        let t0 = Instant::now();
        disp.tick(t0);                              // schedules next_repeat
        disp.tick(t0 + Duration::from_millis(5));   // fires repeats
        assert!(holds.lock().unwrap().iter().filter(|k| *k == "down:w").count() >= 2,
            "up should auto-repeat: {:?}", holds.lock().unwrap());

        // Move to full down: Up releases (repeat entry cleared), Down has no repeat.
        holds.lock().unwrap().clear();
        disp.handle(G13Event::JoystickMove { x: 127, y: 255 }).unwrap();
        // Up releases (records "up:w"), Down presses ("down:s"); the mock records both.
        assert_eq!(holds.lock().unwrap().clone(), vec!["up:w", "down:s"]);
        disp.tick(Instant::now() + Duration::from_millis(50));
        assert_eq!(holds.lock().unwrap().iter().filter(|k| *k == "down:s").count(), 1,
            "down must not repeat; up must have stopped");
    }
}

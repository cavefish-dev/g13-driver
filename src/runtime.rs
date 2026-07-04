use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use anyhow::Result;
use crate::config::{JoystickMode, ProfileSet};
use crate::protocol::G13Event;
use crate::{dispatcher, injector, usb};

/// Load config and spawn the hot-reload watcher thread. Returns the shared handle.
pub fn load_config_and_watch(path: PathBuf) -> Result<Arc<RwLock<ProfileSet>>> {
    let set = ProfileSet::load(&path)?;
    let config = Arc::new(RwLock::new(set));
    let dir = config.read().unwrap().profiles_dir().to_path_buf();
    {
        let config = config.clone();
        let path = path.clone();
        thread::spawn(move || watch_config(config, path, dir));
    }
    Ok(config)
}

/// Open the G13 and spawn the USB reader thread. Returns the event channel.
/// Returns Err (does not exit) so callers — e.g. the GUI — can show the error.
pub fn spawn_usb_reader() -> Result<Receiver<G13Event>> {
    let (tx, rx) = mpsc::channel();
    let reader = usb::UsbReader::open()?;
    thread::spawn(move || {
        if let Err(e) = reader.run(tx) {
            log::error!("USB reader stopped: {e:#}");
        }
    });
    Ok(rx)
}

/// The console driver: consume events, inject, release held keys on exit.
pub fn run_headless(config: Arc<RwLock<ProfileSet>>, rx: Receiver<G13Event>) -> Result<()> {
    let injector = Box::new(injector::windows::WindowsInjector::new());
    let mut dispatcher = dispatcher::Dispatcher::new(config.clone(), injector);

    if let Some(j) = config.read().unwrap().active_profile().joystick() {
        if j.mode == JoystickMode::Mouse {
            log::warn!("joystick mouse mode is configured but not yet implemented; stick will be inert");
        }
    }

    log::info!("g13-driver running (headless) — press Ctrl+C to stop");

    for event in rx {
        if let Err(e) = dispatcher.handle(event) {
            log::warn!("dispatch error: {e:#}");
        }
    }

    dispatcher.release_held();
    Ok(())
}

fn watch_config(config: Arc<RwLock<ProfileSet>>, config_path: PathBuf, profiles_dir: PathBuf) {
    use notify::{Config as WatchConfig, Event, RecommendedWatcher, RecursiveMode, Watcher};

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = match RecommendedWatcher::new(tx, WatchConfig::default()) {
        Ok(w) => w,
        Err(e) => { log::error!("failed to create file watcher: {e}"); return; }
    };
    if let Err(e) = watcher.watch(&config_path, RecursiveMode::NonRecursive) {
        log::error!("failed to watch {}: {e}", config_path.display());
        return;
    }
    // Also watch the profiles directory recursively (may equal config dir for legacy configs).
    let _ = watcher.watch(&profiles_dir, RecursiveMode::Recursive);

    for result in rx {
        if result.is_ok() {
            let active = config.read().unwrap().active();
            match ProfileSet::load(&config_path) {
                Ok(mut new) => {
                    new.set_active(active);
                    *config.write().unwrap() = new;
                    log::info!("config reloaded");
                }
                Err(e) => log::warn!("config reload failed: {e:#}"),
            }
        }
    }
}

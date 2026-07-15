use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use anyhow::Result;
use crate::config::ProfileSet;
use crate::protocol::G13Event;
use crate::{dispatcher, injector, usb};

/// Reload the ProfileSet from disk and swap it under the write lock, preserving the
/// active M-key when its slot still resolves.
pub fn reload_now(config: &Arc<RwLock<ProfileSet>>, config_path: &Path) -> Result<()> {
    let active = config.read().unwrap().active();
    let mut new = ProfileSet::load(config_path)?;
    new.set_active(active); // no-op if that slot is now empty; stays on M1
    *config.write().unwrap() = new;
    Ok(())
}

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

/// The console driver: own a USB supervisor that reconnects on disconnect, consume
/// events, inject, release held keys on exit.
pub fn run_headless(config: Arc<RwLock<ProfileSet>>) -> Result<()> {
    let injector = Box::new(injector::windows::WindowsInjector::new());
    let mut dispatcher = dispatcher::Dispatcher::new(config.clone(), injector);

    // Supervisor: owns tx and keeps it alive across reconnects, so the dispatch
    // loop's channel never closes in normal operation. Reopens the G13 after a
    // disconnect or a failed open, retrying every 2s.
    let (tx, rx) = mpsc::channel::<G13Event>();
    thread::spawn(move || loop {
        match usb::UsbReader::open() {
            Ok(reader) => {
                log::info!("G13 connected");
                let _ = reader.run(tx.clone());
                log::warn!("G13 disconnected — retrying");
            }
            Err(e) => log::warn!("G13 open failed: {e:#}"),
        }
        thread::sleep(Duration::from_secs(2));
    });

    log::info!("g13-driver running (headless) — press Ctrl+C to stop");

    loop {
        match rx.recv_timeout(Duration::from_millis(15)) {
            Ok(event) => {
                if let Err(e) = dispatcher.handle(event) {
                    log::warn!("dispatch error: {e:#}");
                }
                dispatcher.tick(Instant::now());
            }
            Err(RecvTimeoutError::Timeout) => dispatcher.tick(Instant::now()),
            // The supervisor keeps tx alive, so this only fires if it died: exit safely.
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    dispatcher.release_held();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("g13-rt-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        d
    }

    #[test]
    fn reload_now_picks_up_disk_changes() {
        let d = tmp("reload");
        std::fs::write(d.join("profiles/default.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\n").unwrap();
        let cfg = std::sync::Arc::new(std::sync::RwLock::new(
            ProfileSet::load(&d.join("config.toml")).unwrap()));

        // Change the profile file on disk, then reload_now.
        std::fs::write(d.join("profiles/default.toml"), "[keys]\nG1 = \"z\"\n").unwrap();
        reload_now(&cfg, &d.join("config.toml")).unwrap();
        assert_eq!(
            cfg.read().unwrap().active_profile().unwrap().get_binding(crate::protocol::G13Key::G1),
            Some("z"));
    }
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

    let mut watched_dir = profiles_dir;
    for result in rx {
        if result.is_ok() {
            let active = config.read().unwrap().active();
            match ProfileSet::load(&config_path) {
                Ok(mut new) => {
                    new.set_active(active);
                    let new_dir = new.profiles_dir().to_path_buf();
                    *config.write().unwrap() = new;
                    if new_dir != watched_dir {
                        let _ = watcher.unwatch(&watched_dir);
                        let _ = watcher.watch(&new_dir, RecursiveMode::Recursive);
                        watched_dir = new_dir;
                        log::info!("watching profiles dir {}", watched_dir.display());
                    }
                    log::info!("config reloaded");
                }
                Err(e) => log::warn!("config reload failed: {e:#}"),
            }
        }
    }
}

#[cfg(not(windows))]
compile_error!("g13-driver v0.1 targets Windows only; Linux support is planned for v1.0");

mod config;
mod dispatcher;
mod injector;
mod protocol;
mod usb;

use anyhow::Result;
use config::Config;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, RwLock};
use std::thread;

fn main() -> Result<()> {
    env_logger::init();

    let config_path = PathBuf::from("config.toml");
    let config = Arc::new(RwLock::new(Config::load(&config_path)?));

    {
        let config = config.clone();
        let path = config_path.clone();
        thread::spawn(move || watch_config(config, path));
    }

    let (tx, rx) = mpsc::channel();
    let reader = usb::UsbReader::open()?;
    thread::spawn(move || {
        if let Err(e) = reader.run(tx) {
            log::error!("USB reader stopped: {e:#}");
        }
    });

    let injector = Box::new(injector::windows::WindowsInjector::new());
    let dispatcher = dispatcher::Dispatcher::new(config, injector);

    log::info!("g13-driver running — press Ctrl+C to stop");

    for event in rx {
        if let Err(e) = dispatcher.handle(event) {
            log::warn!("dispatch error: {e:#}");
        }
    }

    Ok(())
}

fn watch_config(config: Arc<RwLock<Config>>, path: PathBuf) {
    use notify::{Config as WatchConfig, Event, RecommendedWatcher, RecursiveMode, Watcher};

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = match RecommendedWatcher::new(tx, WatchConfig::default()) {
        Ok(w) => w,
        Err(e) => { log::error!("failed to create file watcher: {e}"); return; }
    };
    if let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive) {
        log::error!("failed to watch {}: {e}", path.display());
        return;
    }
    for result in rx {
        if result.is_ok() {
            match Config::load(&path) {
                Ok(new) => {
                    *config.write().unwrap() = new;
                    log::info!("config reloaded");
                }
                Err(e) => log::warn!("config reload failed: {e:#}"),
            }
        }
    }
}

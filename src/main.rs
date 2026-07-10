#[cfg(not(windows))]
compile_error!("g13-driver v0.1 targets Windows only; Linux support is planned for v1.0");

mod autostart;
mod config;
mod device_state;
mod dispatcher;
mod injector;
mod joystick;
mod monitor;
mod protocol;
mod runtime;
mod usb;

use anyhow::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    env_logger::init();

    let config = runtime::load_config_and_watch(PathBuf::from("config.toml"))?;

    if std::env::args().any(|a| a == "--headless") {
        let rx = runtime::spawn_usb_reader()?;
        runtime::run_headless(config, rx)
    } else {
        monitor::run(config)
    }
}

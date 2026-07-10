#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("g13-driver v0.1 targets Windows only; Linux support is planned for v1.0");

mod autostart;
mod config;
mod tray;
mod device_state;
mod dispatcher;
mod injector;
mod joystick;
mod monitor;
mod protocol;
mod runtime;
mod single_instance;
mod usb;

use anyhow::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");
    let minimized = args.iter().any(|a| a == "--minimized");

    #[cfg(windows)]
    if headless {
        // Reattach to the launching terminal so logs are visible under windows_subsystem=windows.
        unsafe { windows_sys::Win32::System::Console::AttachConsole(u32::MAX); } // ATTACH_PARENT_PROCESS
    }

    env_logger::init();

    let config = runtime::load_config_and_watch(PathBuf::from("config.toml"))?;

    if headless {
        let rx = runtime::spawn_usb_reader()?;
        return runtime::run_headless(config, rx);
    }

    // GUI: enforce single instance.
    #[cfg(windows)]
    {
        match single_instance::acquire() {
            single_instance::Acquired::Already => {
                single_instance::signal_existing();
                return Ok(());
            }
            single_instance::Acquired::First(guard) => {
                // Keep the guard alive for the whole GUI session.
                let _guard = guard;
                return monitor::run(config, minimized);
            }
        }
    }
    #[cfg(not(windows))]
    monitor::run(config, minimized)
}

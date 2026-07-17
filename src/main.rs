#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("g13-driver v0.1 targets Windows only; Linux support is planned for v1.0");

mod autostart;
mod catalog;
mod config;
mod device_state;
mod dispatcher;
mod g13_glyphs;
mod icon;
mod injector;
mod joystick;
mod lcd;
mod led;
mod monitor;
mod profiles;
mod protocol;
mod runtime;
mod single_instance;
mod tray;
mod update;
mod usb;

use anyhow::Result;

/// Resolve the config path independent of the current working directory:
/// (1) next to the executable, then (2) `config.toml` in the CWD (first found wins).
/// This lets the app be auto-started at login (where the CWD is not the repo).
fn resolve_config_path() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join("config.toml");
            if beside.exists() {
                return beside;
            }
        }
    }
    std::path::PathBuf::from("config.toml")
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");
    let minimized = args.iter().any(|a| a == "--minimized");
    let updated = args.iter().any(|a| a == "--updated");

    #[cfg(windows)]
    if headless {
        // Reattach to the launching terminal so logs are visible under windows_subsystem=windows.
        unsafe { windows_sys::Win32::System::Console::AttachConsole(u32::MAX); } // ATTACH_PARENT_PROCESS
    }

    env_logger::init();
    log::info!("g13-driver v{}", env!("G13_VERSION"));

    let config_path = resolve_config_path();
    let config = runtime::load_config_and_watch(config_path.clone())?;

    if headless {
        return runtime::run_headless(config);
    }

    // GUI: enforce single instance.
    #[cfg(windows)]
    {
        let acq = if updated {
            single_instance::acquire_retry(std::time::Duration::from_secs(10))
        } else {
            single_instance::acquire()
        };
        match acq {
            single_instance::Acquired::Already => {
                single_instance::signal_existing();
                return Ok(());
            }
            single_instance::Acquired::First(guard) => {
                let _guard = guard;
                return monitor::run(config, config_path, minimized);
            }
        }
    }
    #[cfg(not(windows))]
    monitor::run(config, config_path, minimized)
}

#[cfg(test)]
mod version_tests {
    #[test]
    fn binary_version_matches_version_txt() {
        let file = std::fs::read_to_string("version.txt").expect("version.txt at crate root");
        assert_eq!(env!("G13_VERSION"), file.trim());
        assert!(!env!("G13_VERSION").is_empty());
    }
}

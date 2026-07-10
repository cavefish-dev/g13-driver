//! Opt-in launch-at-login via the per-user registry Run key.
#![cfg(windows)]

use anyhow::Result;
use winreg::enums::*;
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "g13-driver";

/// The Run-key command: the quoted exe path plus the start-hidden flag.
pub fn run_command(exe: &str) -> String {
    format!("\"{exe}\" --minimized")
}

/// True if the Run-key value exists.
pub fn is_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(VALUE_NAME))
        .is_ok()
}

/// Register the current exe to launch (minimized) at login.
pub fn enable() -> Result<()> {
    let exe = std::env::current_exe()?;
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
    key.set_value(VALUE_NAME, &run_command(&exe.to_string_lossy()))?;
    Ok(())
}

/// Remove the Run-key value (no-op if already absent).
pub fn disable() -> Result<()> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)?;
    match key.delete_value(VALUE_NAME) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::run_command;

    #[test]
    fn run_command_quotes_exe_and_adds_minimized_flag() {
        assert_eq!(run_command(r"C:\Program Files\g13\g13-driver.exe"),
                   "\"C:\\Program Files\\g13\\g13-driver.exe\" --minimized");
    }
}

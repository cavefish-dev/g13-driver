g13-driver — open-source Logitech G13 driver

First run:
  1. Plug in the G13. Run Zadig once to install the WinUSB driver on the G13
     (see the project's docs/zadig-setup.md). This is a one-time step.
  2. Keep g13-driver.exe, config.toml and the profiles/ folder together in one
     folder — the app reads config.toml from next to the exe.
  3. Run g13-driver.exe. Close/minimize hides it to the tray; Quit from the tray
     to exit. Optional: enable "Launch at login" in Settings.

License: GPL-3.0-or-later (see LICENSE).
Project: https://github.com/cavefish-dev/g13-driver

Installer notes:
  - The installer and g13-driver.exe are currently unsigned. Windows SmartScreen
    may show an "unknown publisher" prompt the first time you run setup.exe (or
    the app itself) — click "More info" then "Run anyway" to proceed.
  - Setup.exe installs per-user (into your own profile, under
    %LOCALAPPDATA%\Programs\g13-driver) — no administrator rights are needed,
    and it updates in place on top of an existing install.
  - Prefer not to run an installer? The portable zip (just the exe + config +
    profiles, no install step) remains available on the releases page.

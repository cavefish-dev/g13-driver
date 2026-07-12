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

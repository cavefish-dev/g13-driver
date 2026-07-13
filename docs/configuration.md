# Configuration reference

The GUI (Bindings / Profiles / Settings tabs) is the easy way to configure g13-driver and covers
everything most people need. This document is the reference for hand-editing the config files.

## Where config lives

Next to `g13-driver.exe`:

- `config.toml` — the **manifest**: global settings + which profile file each M-key uses.
- `profiles/*.toml` — one file per profile (a full binding set).

The app looks for `config.toml` **next to the exe first, then in the current directory**. Files are
**hot-reloaded** — save and changes apply immediately (no restart).

> A GUI **Save** rewrites the profile file and does **not** preserve comments or key order (known
> limitation). If you keep comments, edit by hand.

## The manifest (`config.toml`)

```toml
profiles_dir = "profiles"   # folder holding the profile files
m1 = "default.toml"         # profile loaded on M1
m2 = "game.toml"            # profile loaded on M2 (optional)
m3 = "media.toml"           # profile loaded on M3 (optional)

[autorepeat]                # optional; global auto-repeat timing
delay_ms = 400              # wait before repeating starts
interval_ms = 40            # gap between repeats (min 1)

[app]                       # managed by the GUI
start_active = false        # resume Active (true) or Dry-run (false) on launch
```

(A legacy `config.toml` that is itself a bare `[keys]` profile still works as a single M1 profile.)

## A profile file (`profiles/<name>.toml`)

```toml
[keys]
G1  = "ctrl+c"
G2  = "w"
BTN1 = "space"
STICK = "enter"

[repeat]        # optional: which bindings auto-repeat while held
G2 = true

[joystick]      # optional: map the stick to WASD
mode = "wasd"
deadzone = 30   # 0-127; distance from center before a direction fires
up = "w"
down = "s"
left = "a"
right = "d"
```

### Bindable inputs

`G1`–`G22`, `BTN1`, `BTN2` (the two thumb buttons), `STICK` (the joystick click). Names are
case-insensitive.

### Binding syntax

A binding is optional **modifiers** + one **key**: `modifier+modifier+key`. Modifiers: `ctrl`,
`shift`, `alt`, `win` (aliases `control`, `windows`). Examples: `a`, `ctrl+c`, `ctrl+shift+z`,
`win+d`. A modifier **alone** is allowed (e.g. `shift`). Bindings are **hold-means-hold** (held
while the G-key is held); multimedia keys are the tap-only exception.

### Valid key names

- **Letters:** `a`–`z`
- **Digits:** `0`–`9`
- **Function keys:** `f1`–`f24`
- **Modifiers:** `ctrl` (`control`), `shift`, `alt`, `win` (`windows`)
- **Editing / navigation:** `enter` (`return`), `esc` (`escape`), `space`, `tab`, `backspace`,
  `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`
- **Locks / misc:** `capslock`, `printscreen`, `pause`, `numlock`, `scrolllock`
- **Media (tap-only):** `playpause`, `nexttrack` (`next`), `prevtrack` (`prev`), `mediastop`,
  `volup` (`volumeup`), `voldown` (`volumedown`), `mute`

An invalid key name is rejected (the GUI shows the binding as `bad`).

## Auto-repeat

Holding a bound key does **not** auto-repeat by default (a held key = one character). Tick **repeat**
on a binding (GUI) or add it to `[repeat]` to make it repeat while held, like a physical keyboard.
Timing is global in `[autorepeat]` (`delay_ms`, `interval_ms`). Joystick directions and modifier-only
bindings never repeat.

## Worked example

`profiles/game.toml` — WASD on the stick, jump on a thumb button, an auto-repeating attack:

```toml
[keys]
BTN1 = "space"     # jump
G1   = "e"         # interact
G2   = "r"         # reload / attack

[repeat]
G2 = true          # hold G2 to repeat "r"

[joystick]
mode = "wasd"
deadzone = 30
up = "w"
down = "s"
left = "a"
right = "d"
```

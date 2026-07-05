# Hold-means-hold G-keys + multimedia keys — design

- **Status:** approved (design)
- **Date:** 2026-07-05
- **Scope:** Change G-key bindings from a momentary **tap** to **hold-means-hold** (a G-key
  holds its bound key/combo down while the G-key is held, releasing on release), so a G-key
  behaves like a real key — enabling chording (e.g. G1 = Ctrl held), held movement keys, and
  modifier-only bindings. **Multimedia keys** (play/pause, volume, track) are added to the key
  map and are the **tap-only** exception (holding them makes no sense).

## Motivation

Today a G-key `KeyDown` injects a tap (down+up) and `KeyUp` is ignored, so a G-key can't act as
a held modifier or a held movement key. The joystick already holds keys (`key_down`/`key_up`);
this unifies G-keys with that model. Requested behavior: **all keys hold-means-hold, except
multimedia keys** (which tap).

## Components

### `src/injector/key_map.rs` — media keys + tap-only set (TDD)
- Add multimedia VKs: `playpause` (0xB3), `nexttrack`/`next` (0xB0), `prevtrack`/`prev`
  (0xB1), `mediastop` (0xB2), `volup`/`volumeup` (0xAF), `voldown`/`volumedown` (0xAE),
  `mute` (0xAD).
- `pub fn tap_only_keys() -> HashSet<String>` — the media key names. The dispatcher uses it to
  choose tap vs hold. Everything else holds.

### `src/injector/mod.rs` — parser: modifier-only combos (TDD)
- Change `KeyCombo.key` from `String` to `Option<String>`.
- `KeyCombo::parse`: collect modifiers + at most one non-modifier key; `key = None` when there
  is no non-modifier key. Error only if the combo is **empty** (no key, no modifier) or has
  **more than one** non-modifier key.
- Examples: `shift` → `{modifiers:[Shift], key:None}`; `ctrl+c` → `{[Ctrl], Some("c")}`;
  `w` → `{[], Some("w")}`; `ctrl+shift` → `{[Ctrl,Shift], None}`.

### `src/injector/mod.rs` + `windows.rs` — hold a combo (manual-verify SendInput)
- `KeyInjector` trait gains `combo_down(&self, &KeyCombo) -> Result<()>` and
  `combo_up(&self, &KeyCombo) -> Result<()>`. `press` (tap) stays for tap-only keys.
- `WindowsInjector::combo_down`: modifiers down (in order), then the key down (if `Some`) — all
  held. `combo_up`: key up (if `Some`), then modifiers up (reverse order). `press` handles a
  `None` key (tap the modifiers) for safety, though the tap path always has a media key.

### `src/dispatcher.rs` — G-keys held (TDD via MockInjector)
- New field `held_keys: HashMap<G13Key, KeyCombo>`.
- **KeyDown(g):** parse the binding. If its key is `Some(k)` and `k` is tap-only → `press()`
  (one tap, not tracked). Otherwise → `combo_down()` and record `held_keys[g] = combo`.
- **KeyUp(g):** if `held_keys.remove(&g)` returns a combo → `combo_up()` (release exactly what
  was pressed).
- **Release paths:** `release_held()` (called on Active→Dry-run, disconnect, shutdown) releases
  the joystick **and** all held G-key combos, then clears the map. **Profile switch**
  (`handle_mkey`) releases only the joystick (a new profile may rebind the stick); held G-keys
  stay down until their physical `KeyUp` (which releases the exact recorded combo). To keep this
  split, `handle_mkey` calls a joystick-only release; `release_held` releases both.

### `src/monitor/mod.rs` — editor validation + hints
- `combo_valid`: valid when `KeyCombo::parse` succeeds **and** (`key` is `None`, or `key` is in
  `build_key_map`). Media keys validate (now in the map).
- Hints line adds: "modifiers alone are allowed (e.g. `shift`, `ctrl+shift`)" and lists the
  media keys.

## Behavior notes & edge cases

- **Existing bindings change semantics:** `ctrl+c` etc. become hold-means-hold — a quick press
  is still one action; holding holds it. The shipped example profiles keep working.
- **No OS auto-repeat** for a held injected key: a held `w` is one char in a text field but a
  held *state* in games (correct for gaming; identical to the joystick).
- `ctrl+playpause` (modifier + media key): the key is tap-only, so the whole combo taps.
- **Stuck-key on Ctrl+C / force-kill** remains the known pre-existing limitation (no console
  signal handler) — held keys aren't released if the process is killed. Documented, unchanged.
- **Injection failure** on `combo_up` → logged `warn`; the held-map entry is removed regardless
  so it won't retry-stick. Same warn-and-continue policy as elsewhere.
- Two G-keys bound to the same key/modifier → both held; OS coalesces the repeated down/up.

## Testing

- **Unit (TDD):**
  - Parser: `shift` → `key:None`; `ctrl+c` → `Some("c")`; `w` → `Some("w")`; `ctrl+shift` →
    `None` + two modifiers; empty string and modifier-with-two-keys error.
  - key_map: media keys present with correct VKs; `tap_only_keys()` contains the media names and
    not letters/modifiers.
  - Dispatcher (MockInjector records `combo_down`/`combo_up`/`press`): normal binding →
    `combo_down` on KeyDown + `combo_up` on KeyUp; media binding → `press` (tap), not tracked, no
    release on KeyUp; `release_held` releases held G-keys + joystick; profile switch releases
    joystick but leaves held G-keys; a modifier-only `shift` binding holds via `combo_down`.
- **Manual-verify:** editor rendering/validation (headless exception). **Smoke test:** bind
  G1=`shift` (hold G1 + tap another G-key → Shift+key), G2=`w` (held), G3=`playpause` (taps);
  confirm hold-means-hold, media taps, and no stuck keys (physical release, Dry-run toggle,
  disconnect).

## Out of scope (future)
- A per-binding "force tap" override for non-media keys (not needed — hold-means-hold is the
  desired default).
- More media/system keys (brightness, launch keys) — the same tap-only mechanism extends to
  them trivially later.
- Ctrl+C / force-kill graceful release (a console control handler) — tracked separately.

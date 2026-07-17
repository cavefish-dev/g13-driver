# "g13" Icon + Help Tab — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Render "g13" from our console font into the tray (in the status color), window, and exe icons; add a Help tab.

**Tech Stack:** Rust, egui/eframe, `tray-icon`; `winres` + `ico` (build-deps) for the exe icon.

## Global Constraints
- **GNU toolchain only.** MinGW gcc at `C:\Strawberry\c\bin\gcc.exe`; `windres` at `C:\Strawberry\c\bin\windres.exe` (present — the exe-icon resource compiler). If `cargo`/`gcc`/`windres` not found, prepend `C:\Strawberry\c\bin` to PATH. Do NOT switch to the MSVC target.
- **TDD** for the pure `icon::render_g13_rgba`. Tray/window/exe/help are visual — no unit test (manual verify), like other GUI/build code.
- No panic on the runtime path.
- The exe icon is **deferrable**: if `winres`/`windres` can't be made to work, skip just that task (report BLOCKED) — the tray + window icons still ship.
- One focused commit per task.

## File Structure
- **Create** `src/g13_glyphs.rs` — the three `g`/`1`/`3` glyph constants (single source, `include!`'d by build.rs).
- **Create** `src/icon.rs` — `render_g13_rgba`.
- **Modify** `src/main.rs` — `mod g13_glyphs; mod icon;`.
- **Modify** `src/tray.rs` — tray icon = "g13" in status color.
- **Modify** `src/monitor/mod.rs` — window `with_icon`; `Tab::Help` + `render_help`.
- **Modify** `build.rs`, `Cargo.toml` — exe `.ico` via winres.
- **Modify** milestone.

---

## Task 1: `g13_glyphs` + `icon::render_g13_rgba`

**Files:** Create `src/g13_glyphs.rs`, `src/icon.rs`; Modify `src/main.rs`; Test `src/icon.rs`.

- [ ] **Step 1: Create the shared glyph data.** `src/g13_glyphs.rs` — copy the exact glcdfont rows for `g`, `1`, `3` from `src/lcd/font.rs`'s `FONT` array (indices: `1` = `0x31-0x20 = 17`, `3` = `0x33-0x20 = 19`, `g` = `0x67-0x20 = 71`). `1` is `[0x00, 0x42, 0x7F, 0x40, 0x00]` (anchor — verify against font.rs); read `g` and `3` out of font.rs:

```rust
//! The three glyphs for the "g13" app/tray/exe icon. Copied from src/lcd/font.rs
//! (glcdfont) so build.rs (which can't use crate code) and src/icon.rs share one source.
//! Each glyph is 5 columns; bit `b` of a column byte is row `b` (top→bottom).
pub const G13_GLYPHS: [[u8; 5]; 3] = [
    // 'g' (font.rs index 71)
    [/* copy from src/lcd/font.rs */],
    // '1' (index 17)
    [0x00, 0x42, 0x7F, 0x40, 0x00],
    // '3' (index 19)
    [/* copy from src/lcd/font.rs */],
];
```

- [ ] **Step 2: Write the failing test** (in `src/icon.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_dimensions_and_colors() {
        let fg = [255, 0, 0, 255];
        let bg = [0, 0, 0, 255];
        let (buf, w, h) = render_g13_rgba(2, fg, bg);
        // 3 chars * (5+1) - 1 = 17 cols; +padding both sides.
        assert_eq!(buf.len() as u32, w * h * 4);
        // A corner is padding → background.
        assert_eq!(&buf[0..4], &bg);
        // At least some foreground pixels were drawn (the glyphs).
        assert!(buf.chunks(4).any(|px| px == fg));
    }

    #[test]
    fn scale_grows_the_canvas() {
        let (_, w1, h1) = render_g13_rgba(1, [255;4], [0;4]);
        let (_, w2, h2) = render_g13_rgba(2, [255;4], [0;4]);
        assert!(w2 > w1 && h2 > h1);
    }
}
```

- [ ] **Step 3: Run → fail.** `cargo test icon::`

- [ ] **Step 4: Implement `src/icon.rs`:**

```rust
//! Renders "g13" (from the embedded console font) into an RGBA buffer for the
//! tray / window / exe icons.
use crate::g13_glyphs::G13_GLYPHS;

/// "g13" as RGBA: `fg` where a glyph bit is set, `bg` elsewhere. Each source pixel
/// is a `scale`×`scale` block; a uniform padding surrounds the text.
pub fn render_g13_rgba(scale: u32, fg: [u8; 4], bg: [u8; 4]) -> (Vec<u8>, u32, u32) {
    let scale = scale.max(1);
    let pad = 2 * scale;
    let cols = 3 * 6 - 1; // 3 glyphs of 5 cols + 1 gap between = 17
    let rows = 8u32;
    let w = cols * scale + pad * 2;
    let h = rows * scale + pad * 2;
    let mut buf = vec![0u8; (w * h * 4) as usize];
    for px in buf.chunks_mut(4) {
        px.copy_from_slice(&bg);
    }
    for (gi, glyph) in G13_GLYPHS.iter().enumerate() {
        let gx = pad + gi as u32 * 6 * scale; // 5 cols + 1 gap per glyph
        for (col, bits) in glyph.iter().enumerate() {
            for row in 0..rows {
                if bits & (1 << row) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let x = gx + col as u32 * scale + dx;
                            let y = pad + row * scale + dy;
                            let idx = ((y * w + x) * 4) as usize;
                            buf[idx..idx + 4].copy_from_slice(&fg);
                        }
                    }
                }
            }
        }
    }
    (buf, w, h)
}
```

Add `mod g13_glyphs;` and `mod icon;` to `src/main.rs` (alphabetical-ish; `g13_glyphs`/`icon` near the other mods).

- [ ] **Step 5: Run → pass + full `cargo test`.**
- [ ] **Step 6: Commit** `git commit -m "feat(icon): render g13 from the console font into an RGBA buffer"`

---

## Task 2: Tray + window icons

**Files:** Modify `src/tray.rs`, `src/monitor/mod.rs`.

**Note:** Visual — no unit test. Verify via `cargo build` + manual.

- [ ] **Step 1: Tray icon = "g13" in the status color.** In `src/tray.rs`, replace `icon_rgba`'s flat-fill body so it renders "g13" via `crate::icon::render_g13_rgba(scale, fg, bg)`, where `fg` is the state color (Problem red / Active green / Dry-run grey) and `bg` is near-black (e.g. `[24, 24, 24, 255]`). Pick a `scale` giving a ~32px-tall glyph (e.g. `scale = 3`). Keep the `(Vec<u8>, u32, u32)` return and the `make_icon`/`update` call path unchanged.

```rust
pub fn icon_rgba(state: IconState) -> (Vec<u8>, u32, u32) {
    let (r, g, b) = match state {
        IconState::Problem => (210, 70, 70),
        IconState::Active  => (95, 200, 130),
        IconState::DryRun  => (140, 140, 140),
    };
    crate::icon::render_g13_rgba(3, [r, g, b, 255], [24, 24, 24, 255])
}
```

- [ ] **Step 2: Window/title-bar icon.** In `src/monitor/mod.rs` `run()`, add `.with_icon(...)` to the `ViewportBuilder`:

```rust
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([660.0, 560.0])
            .with_resizable(false)
            .with_visible(!start_minimized)
            .with_icon(std::sync::Arc::new({
                let (rgba, w, h) = crate::icon::render_g13_rgba(4, [230, 230, 230, 255], [24, 24, 24, 255]);
                egui::IconData { rgba, width: w, height: h }
            })),
```

(Adjust the exact `IconData` construction to the eframe 0.31 API — `with_icon` takes `Arc<IconData>`; `IconData { rgba: Vec<u8>, width: u32, height: u32 }`.)

- [ ] **Step 3: Build + manual.** `cargo build` clean; `cargo test` green. `cargo run` → the tray icon shows "g13" (in status color, changing Active↔Dry-run) and the window title-bar/alt-tab shows "g13".

- [ ] **Step 4: Commit** `git commit -m "feat(gui): g13 tray icon (status color) + window icon"`

---

## Task 3: Exe / taskbar icon (build.rs + winres)

**Files:** Modify `Cargo.toml`, `build.rs`.

**Note:** DEFERRABLE. If `winres`/`windres` cannot be made to work after a real attempt, STOP and report BLOCKED with the error — do not fake it; the tray/window icons already shipped in Task 2.

- [ ] **Step 1: Add build-deps.** In `Cargo.toml`:

```toml
[build-dependencies]
winres = "0.1"
ico = "0.3"
```

- [ ] **Step 2: Generate + embed the .ico in `build.rs`.** Keep the existing version-stamp logic; add (guarded to the Windows target). `include!` the shared glyphs and render a couple of sizes:

```rust
    // ---- exe icon: render "g13" and embed as a .ico (Windows target only) ----
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/g13_glyphs.rs"));

        // Render "g13" into a `size`×`size` RGBA (glyph centered, max integer scale).
        fn render(size: u32) -> ico::IconImage {
            let cols = 3 * 6 - 1; // 17
            let rows = 8u32;
            let scale = ((size.saturating_sub(2)) / cols).max(1).min((size.saturating_sub(2)) / rows).max(1);
            let tw = cols * scale;
            let th = rows * scale;
            let ox = (size - tw) / 2;
            let oy = (size - th) / 2;
            let mut rgba = vec![0u8; (size * size * 4) as usize]; // transparent bg
            for (gi, glyph) in G13_GLYPHS.iter().enumerate() {
                let gx = ox + gi as u32 * 6 * scale;
                for (col, bits) in glyph.iter().enumerate() {
                    for row in 0..rows {
                        if bits & (1 << row) != 0 {
                            for dy in 0..scale {
                                for dx in 0..scale {
                                    let x = gx + col as u32 * scale + dx;
                                    let y = oy + row * scale + dy;
                                    let idx = ((y * size + x) * 4) as usize;
                                    rgba[idx..idx + 4].copy_from_slice(&[230, 230, 230, 255]);
                                }
                            }
                        }
                    }
                }
            }
            ico::IconImage::from_rgba_data(size, size, rgba)
        }

        let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
        for size in [16u32, 32, 48, 256] {
            icon_dir.add_entry(ico::IconDirEntry::encode(&render(size)).unwrap());
        }
        let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("g13.ico");
        icon_dir.write(std::fs::File::create(&out).unwrap()).unwrap();

        let mut res = winres::WindowsResource::new();
        res.set_icon(out.to_str().unwrap());
        if let Err(e) = res.compile() {
            println!("cargo:warning=exe icon embed failed (windres?): {e}");
        }
    }
    println!("cargo:rerun-if-changed=src/g13_glyphs.rs");
```

Verify `windres` is reachable (it's at `C:\Strawberry\c\bin`). If `res.compile()` errors, read the error: if it's a missing-`windres`, ensure `C:\Strawberry\c\bin` is on PATH for the build; if it fundamentally can't work, report BLOCKED (this task is deferrable).

- [ ] **Step 3: Build.** `cargo build --release` clean (watch for the `exe icon embed failed` warning — if present, investigate/report). `cargo test` green.

- [ ] **Step 4: Manual.** The built `target/release/g13-driver.exe` shows the "g13" icon in Explorer / taskbar.

- [ ] **Step 5: Commit** `git commit -m "build: embed g13 exe icon via winres"`

---

## Task 4: Help tab

**Files:** Modify `src/monitor/mod.rs`.

**Note:** GUI content — no unit test; verify via build + a glance.

- [ ] **Step 1: Add the tab.** Add `Help` to the `Tab` enum and a `(Tab::Help, "Help")` entry to the `TABS` list. Add the dispatch arm `Tab::Help => self.render_help(ui),`.

- [ ] **Step 2: `render_help`.** Add a method rendering a `ScrollArea` with headings + short paragraphs covering (per the spec): what the app is (Active vs Dry-run, MR toggles it, MR LED lights in dry-run); first-time setup (Zadig/WinUSB, `docs/zadig-setup.md`); a short walkthrough of each tab (Monitor, Profiles/M-keys, Bindings incl. combos/labels/repeat + joystick, Catalog, LCD config, Settings/backlight); tray + autostart; and the `config.toml` location. Use `egui::ScrollArea::vertical()` with `ui.heading(...)` / `ui.label(...)`; keep copy concise and accurate to the current features.

```rust
    fn render_help(&self, ui: &mut egui::Ui) {
        ui.heading("Help");
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label("g13-driver is an open-source replacement driver for the Logitech G13.");
            ui.add_space(6.0);
            ui.heading("Active vs Dry-run");
            ui.label("Active injects your key bindings; Dry-run pauses injection so you can \
                test safely. Press the MR key (or the tray/Settings toggle) to switch — the \
                LCD mode box and tray icon reflect it, and the MR LED lights in Dry-run.");
            // ... the remaining sections per the spec ...
        });
    }
```

Write out all sections (setup, per-tab walkthrough, tray/autostart, config path) with accurate, current wording.

- [ ] **Step 3: Build + manual.** `cargo build` clean; `cargo test` green. `cargo run` → the Help tab appears and scrolls.

- [ ] **Step 4: Commit** `git commit -m "feat(gui): add a Help tab with a usage guide"`

---

## Task 5: Milestone + smoke

**Files:** Move `milestones/open/icon-help-tab.md` → `milestones/ongoing/`.

- [ ] **Step 1:** Set `Status: ongoing`, check boxes, add a smoke checklist (tray shows g13 in status color + updates; window icon g13; exe icon g13 in Explorer/taskbar; Help tab renders/scrolls). `git mv` to `ongoing/`.
- [ ] **Step 2:** `cargo test && cargo build --release` — pass, clean.
- [ ] **Step 3: Commit** `git commit -m "docs: icon + Help-tab milestone to ongoing"`

---

## Self-Review
- Shared glyphs + font-based renderer → Task 1 (TDD). ✓
- Tray (status color) + window icons → Task 2. ✓
- Exe icon via winres/ico (deferrable, windres present) → Task 3. ✓
- Help tab → Task 4. ✓
- Milestone/smoke → Task 5. ✓
- Types: `render_g13_rgba(scale, fg, bg) -> (Vec<u8>, u32, u32)`, `G13_GLYPHS: [[u8;5];3]` consistent across icon.rs/tray.rs/monitor/build.rs.

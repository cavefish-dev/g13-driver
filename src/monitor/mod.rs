use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;
use anyhow::Result;
use eframe::egui;
use crate::config::ProfileSet;
use crate::device_state::{Connection, DeviceState};
use crate::dispatcher::Dispatcher;
use crate::injector::{KeyCombo, key_map::build_key_map, windows::WindowsInjector};
use crate::protocol::{G13Event, G13Key, MKey};
use crate::runtime;

/// A combo is valid for the editor only if it parses AND its key is a known key
/// (so `ctrl+zzz` is rejected here rather than silently failing at injection).
fn combo_valid(s: &str, valid_keys: &HashSet<String>) -> bool {
    KeyCombo::parse(s)
        .map(|c| valid_keys.contains(&c.key))
        .unwrap_or(false)
}

pub fn run(config: Arc<RwLock<ProfileSet>>) -> Result<()> {
    let state = Arc::new(Mutex::new(DeviceState::new()));
    let dry_run = Arc::new(AtomicBool::new(true)); // first launch = Dry-run

    // Fixed, non-resizable window sized to fit the content of every tab.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([660.0, 560.0])
            .with_resizable(false),
        ..Default::default()
    };

    eframe::run_native(
        "G13 Monitor",
        options,
        Box::new(move |cc| Ok(Box::new(MonitorApp::new(cc, config, state, dry_run)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;
    Ok(())
}

/// Which section of the window is shown. Monitor is live today; the rest are
/// UI-vision placeholders being prototyped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Monitor,
    Profiles,
    Bindings,
    Lcd,
    Settings,
}

const TABS: [(Tab, &str); 5] = [
    (Tab::Monitor, "Monitor"),
    (Tab::Profiles, "Profiles"),
    (Tab::Bindings, "Bindings"),
    (Tab::Lcd, "LCD"),
    (Tab::Settings, "Settings"),
];

pub struct MonitorApp {
    profiles: Arc<RwLock<ProfileSet>>,
    state: Arc<Mutex<DeviceState>>,
    dry_run: Arc<AtomicBool>,
    tab: Tab,
    edits: HashMap<G13Key, String>,
    edits_for: Option<String>,
    save_status: Option<String>,
}

impl MonitorApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        profiles: Arc<RwLock<ProfileSet>>,
        state: Arc<Mutex<DeviceState>>,
        dry_run: Arc<AtomicBool>,
    ) -> Self {
        let app = Self {
            profiles,
            state,
            dry_run,
            tab: Tab::Monitor,
            edits: HashMap::new(),
            edits_for: None,
            save_status: None,
        };
        app.start_consumer(cc.egui_ctx.clone());
        app
    }

    /// Attempt to open the G13 and spawn the consumer thread. Sets connection
    /// status accordingly. Called at startup and on Retry (only while
    /// disconnected, so no second reader races the first).
    fn start_consumer(&self, ctx: egui::Context) {
        match runtime::spawn_usb_reader() {
            Ok(rx) => {
                self.state.lock().unwrap().connection = Connection::Connected;
                let injector = Box::new(WindowsInjector::new());
                let dispatcher = Dispatcher::new(self.profiles.clone(), injector);
                let state = self.state.clone();
                let dry_run = self.dry_run.clone();
                std::thread::spawn(move || consumer_loop(rx, dispatcher, state, dry_run, ctx));
            }
            Err(e) => {
                self.state.lock().unwrap().connection = Connection::Disconnected(format!("{e:#}"));
            }
        }
    }
}

/// Drains the event stream: updates DeviceState for display always; injects via
/// the dispatcher only when Active. A 50ms recv timeout lets us notice a
/// Dry-run toggle promptly so an Active->Dry-run switch releases held keys even
/// with no new events. Exits when the channel closes (device unplugged).
fn consumer_loop(
    rx: Receiver<G13Event>,
    mut dispatcher: Dispatcher,
    state: Arc<Mutex<DeviceState>>,
    dry_run: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    let mut was_active = !dry_run.load(Ordering::Relaxed);
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                state.lock().unwrap().apply(&event);
                let active = !dry_run.load(Ordering::Relaxed);
                if was_active && !active {
                    dispatcher.release_held();
                }
                if active {
                    if let Err(e) = dispatcher.handle(event) {
                        log::warn!("dispatch error: {e:#}");
                    }
                }
                was_active = active;
                ctx.request_repaint();
            }
            Err(RecvTimeoutError::Timeout) => {
                let active = !dry_run.load(Ordering::Relaxed);
                if was_active && !active {
                    dispatcher.release_held();
                }
                was_active = active;
            }
            Err(RecvTimeoutError::Disconnected) => {
                dispatcher.release_held();
                state.lock().unwrap().connection =
                    Connection::Disconnected("device disconnected".to_string());
                ctx.request_repaint();
                return;
            }
        }
    }
}

// Physical G13 key arrangement: rows of 7, 7, 5, 3. Each row is centered when
// rendered, so the short rows sit under the wide ones and the whole block is
// centered in the window.
const ROWS: [&[G13Key]; 4] = [
    &[G13Key::G1, G13Key::G2, G13Key::G3, G13Key::G4, G13Key::G5, G13Key::G6, G13Key::G7],
    &[G13Key::G8, G13Key::G9, G13Key::G10, G13Key::G11, G13Key::G12, G13Key::G13, G13Key::G14],
    &[G13Key::G15, G13Key::G16, G13Key::G17, G13Key::G18, G13Key::G19],
    &[G13Key::G20, G13Key::G21, G13Key::G22],
];

impl eframe::App for MonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let snapshot = self.state.lock().unwrap().clone();

        egui::TopBottomPanel::top("hd").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("G13 Monitor");
                match &snapshot.connection {
                    Connection::Connected => ui.colored_label(egui::Color32::from_rgb(95, 214, 138), "● Connected"),
                    Connection::Disconnected(why) => ui.colored_label(egui::Color32::from_rgb(220, 90, 90), format!("○ {why}")),
                };
                if let Some(name) = self.profiles.read().unwrap().active_name() {
                    ui.separator();
                    ui.label(format!("Profile: {name}"));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = !self.dry_run.load(Ordering::Relaxed);
                    if ui.selectable_label(active, "Active").clicked() { active = true; }
                    else if ui.selectable_label(!active, "Dry-run").clicked() { active = false; }
                    self.dry_run.store(!active, Ordering::Relaxed);
                    ui.label("mode:");
                });
            });
        });

        egui::TopBottomPanel::bottom("ft").show(ctx, |ui| {
            let set = self.profiles.read().unwrap();
            let cfg = set.active_profile();
            let joy = cfg.joystick()
                .map(|j| format!("joystick: {:?}, deadzone {}", j.mode, j.deadzone))
                .unwrap_or_else(|| "joystick: disabled".to_string());
            ui.label(format!("config.toml · {joy}"));
            if let Connection::Disconnected(_) = &snapshot.connection {
                if ui.button("Retry connection").clicked() {
                    self.start_consumer(ctx.clone());
                }
            }
        });

        egui::SidePanel::left("nav").resizable(false).min_width(104.0).show(ctx, |ui| {
            ui.add_space(6.0);
            for (tab, label) in TABS {
                if ui.selectable_label(self.tab == tab, label).clicked() {
                    self.tab = tab;
                }
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.tab {
                Tab::Monitor => self.render_monitor(ui, &snapshot),
                Tab::Profiles => self.render_profiles(ui),
                Tab::Bindings => self.render_bindings(ui),
                Tab::Lcd => self.render_lcd(ui),
                Tab::Settings => self.render_settings(ui),
            }
        });
    }
}

impl MonitorApp {
    /// The live view: physical-layout key grid, with the joystick panel below it.
    fn render_monitor(&self, ui: &mut egui::Ui, snapshot: &DeviceState) {
        let set = self.profiles.read().unwrap();
        let cfg = set.active_profile();

        // Deterministic centering: a cell is exactly 62px wide (48 content + 8
        // inner margin + 6 outer margin) with inter-cell spacing zeroed, so a row
        // is `len * 62`. Center each row inside a fixed block (widest row = 7), and
        // center that block in the available width. Long bindings truncate rather
        // than stretching a cell (full bindings live on the Bindings tab).
        const CELL: f32 = 62.0;
        const BLOCK_W: f32 = 7.0 * CELL;
        let indent = ((ui.available_width() - BLOCK_W) * 0.5).max(0.0);

        ui.horizontal(|ui| {
            ui.add_space(indent);
            ui.vertical(|ui| {
                ui.set_width(BLOCK_W);

                for row in ROWS {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space((BLOCK_W - row.len() as f32 * CELL) * 0.5);
                        for &key in row {
                            let pressed = snapshot.pressed.contains(&key);
                            let binding = cfg.get_binding(key).unwrap_or("—");
                            let fill = if pressed { egui::Color32::from_rgb(20, 54, 31) } else { egui::Color32::from_gray(38) };
                            egui::Frame::new().fill(fill).inner_margin(4.0).outer_margin(3.0).corner_radius(4.0).show(ui, |ui| {
                                ui.set_width(48.0);
                                ui.vertical(|ui| {
                                    ui.strong(format!("{key:?}"));
                                    ui.add(egui::Label::new(egui::RichText::new(binding).small()).truncate());
                                });
                            });
                        }
                    });
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                // Joystick, centered under the grid.
                ui.horizontal(|ui| {
                    ui.add_space((BLOCK_W - 140.0) * 0.5);
                    ui.vertical(|ui| {
                        ui.label("JOYSTICK");
                        let (dz, up, down, left, right) = cfg.joystick()
                            .map(|j| (
                                j.deadzone,
                                j.up.clone().unwrap_or_default(),
                                j.down.clone().unwrap_or_default(),
                                j.left.clone().unwrap_or_default(),
                                j.right.clone().unwrap_or_default(),
                            ))
                            .unwrap_or((30, "w".into(), "s".into(), "a".into(), "d".into()));

                        let size = egui::vec2(140.0, 140.0);
                        let (resp, painter) = ui.allocate_painter(size, egui::Sense::hover());
                        let rect = resp.rect;
                        painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(90)), egui::StrokeKind::Inside);
                        let c = rect.center();
                        let dz_r = rect.width() * (dz as f32 / 255.0);
                        painter.circle_stroke(c, dz_r, egui::Stroke::new(1.0, egui::Color32::from_gray(70)));
                        let px = rect.left() + rect.width() * (snapshot.joy_x as f32 / 255.0);
                        let py = rect.top() + rect.height() * (snapshot.joy_y as f32 / 255.0);
                        painter.circle_filled(egui::pos2(px, py), 6.0, egui::Color32::from_rgb(127, 224, 160));

                        let hot = egui::Color32::from_rgb(127, 224, 160);
                        let dim = egui::Color32::from_gray(140);
                        let a_left = snapshot.joy_x < 127u8.saturating_sub(dz);
                        let a_right = snapshot.joy_x > 127u8.saturating_add(dz);
                        let a_up = snapshot.joy_y < 127u8.saturating_sub(dz);
                        let a_down = snapshot.joy_y > 127u8.saturating_add(dz);
                        ui.horizontal(|ui| {
                            ui.colored_label(if a_up { hot } else { dim }, format!("↑{up}"));
                            ui.colored_label(if a_down { hot } else { dim }, format!("↓{down}"));
                            ui.colored_label(if a_left { hot } else { dim }, format!("←{left}"));
                            ui.colored_label(if a_right { hot } else { dim }, format!("→{right}"));
                        });
                    });
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("M-keys:");
                    let hot = egui::Color32::from_rgb(127, 224, 160);
                    let dim = egui::Color32::from_gray(140);
                    for (m, label) in [(MKey::M1, "M1"), (MKey::M2, "M2"), (MKey::M3, "M3"), (MKey::MR, "MR")] {
                        let on = snapshot.mkeys.contains(&m);
                        ui.colored_label(if on { hot } else { dim }, label);
                    }
                });
            });
        });
    }

    // ---- UI-vision placeholders (not wired to real behavior yet) ----

    fn render_profiles(&self, ui: &mut egui::Ui) {
        ui.heading("Profiles");
        ui.label("M1/M2/M3 select the bound profile. Click a slot to switch (same as pressing the M-key).");
        ui.add_space(8.0);

        let (active, slots, available) = {
            let set = self.profiles.read().unwrap();
            let slots = [
                (MKey::M1, set.name(MKey::M1).map(String::from)),
                (MKey::M2, set.name(MKey::M2).map(String::from)),
                (MKey::M3, set.name(MKey::M3).map(String::from)),
            ];
            (set.active(), slots, set.available())
        };

        let mut switch_to: Option<MKey> = None;
        for (m, name) in &slots {
            let label = match name {
                Some(n) => format!("{m:?}  —  {n}"),
                None => format!("{m:?}  —  (unassigned)"),
            };
            let is_active = *m == active;
            if ui.add_enabled(name.is_some(), egui::SelectableLabel::new(is_active, label)).clicked() {
                switch_to = Some(*m);
            }
        }
        if let Some(m) = switch_to {
            self.profiles.write().unwrap().set_active(m);
        }

        ui.add_space(10.0);
        ui.separator();
        ui.label("Available in profiles/:");
        for f in &available {
            ui.weak(f);
        }
        ui.add_space(6.0);
        ui.weak("(assigning files to slots and editing bindings are planned)");
    }

    fn render_bindings(&mut self, ui: &mut egui::Ui) {
        // Which profile are we editing? Reload buffers when it changes.
        let active_name = self.profiles.read().unwrap().active_name().map(String::from);
        if self.edits_for != active_name {
            let set = self.profiles.read().unwrap();
            let profile = set.active_profile();
            let bound = profile.bindings();
            self.edits = ROWS.iter().flat_map(|row| row.iter())
                .map(|&k| (k, bound.get(&k).cloned().unwrap_or_default()))
                .collect();
            drop(set);
            self.edits_for = active_name.clone();
            self.save_status = None;
        }

        ui.heading("Bindings");
        match &active_name {
            Some(n) => ui.label(format!("Editing profile: {n}")),
            None => ui.label("No profile loaded"),
        };
        ui.weak("Combo = optional modifiers (ctrl / shift / alt / win) + one key.  \
                 Keys: a-z, 0-9, f1-f24, enter, esc, space, tab, arrows, home/end, \
                 pageup/pagedown, insert/delete.  Examples: ctrl+c, ctrl+shift+z, win+d.  \
                 Empty = unmapped.");
        ui.add_space(6.0);

        let green = egui::Color32::from_rgb(127, 224, 160);
        let red = egui::Color32::from_rgb(220, 90, 90);
        let dim = egui::Color32::from_gray(110);

        // Valid key names (built once per frame from the injector's key table).
        let valid_keys: HashSet<String> = build_key_map().into_keys().collect();

        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for row in ROWS {
                for &key in row {
                    let buf = self.edits.entry(key).or_default();
                    ui.horizontal(|ui| {
                        ui.monospace(format!("{key:?}"));
                        ui.add_space(6.0);
                        ui.add(egui::TextEdit::singleline(buf).desired_width(160.0));
                        // Compute validity AFTER the edit so the mark has no one-frame lag.
                        let (mark, color) = if buf.is_empty() {
                            ("—", dim)
                        } else if combo_valid(buf, &valid_keys) {
                            ("ok", green)
                        } else {
                            ("bad", red)
                        };
                        ui.colored_label(color, mark);
                    });
                }
            }
        });

        ui.add_space(8.0);
        let all_valid = self.edits.values().all(|b| b.is_empty() || combo_valid(b, &valid_keys));
        ui.horizontal(|ui| {
            if ui.add_enabled(all_valid, egui::Button::new("Save")).clicked() {
                let bindings: HashMap<G13Key, String> = self.edits.iter()
                    .filter(|(_, v)| !v.is_empty())
                    .map(|(k, v)| (*k, v.clone()))
                    .collect();
                match self.profiles.write().unwrap().save_active_bindings(bindings) {
                    Ok(()) => self.save_status = Some("saved".to_string()),
                    Err(e) => {
                        log::warn!("save failed: {e:#}");
                        self.save_status = Some(format!("save failed: {e:#}"));
                    }
                }
            }
            if ui.button("Revert").clicked() {
                self.edits_for = None; // forces a reload from the profile next frame
            }
            if let Some(s) = &self.save_status {
                ui.label(s);
            }
        });
        if !all_valid {
            ui.colored_label(red, "Fix the invalid (bad) combos before saving.");
        }
    }

    fn render_lcd(&self, ui: &mut egui::Ui) {
        ui.heading("LCD  (160 × 43)");
        ui.label("Preview and choose what shows on the G13's screen. Planned for v0.4.");
        ui.add_space(8.0);
        // 3x-scale preview of the monochrome 160x43 panel.
        let size = egui::vec2(160.0 * 3.0, 43.0 * 3.0);
        let (resp, painter) = ui.allocate_painter(size, egui::Sense::hover());
        let rect = resp.rect;
        painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(20, 30, 24));
        painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, egui::Color32::from_gray(90)), egui::StrokeKind::Inside);
        let green = egui::Color32::from_rgb(120, 230, 150);
        let o = rect.left_top();
        painter.text(o + egui::vec2(8.0, 6.0), egui::Align2::LEFT_TOP, "G13 Driver", egui::FontId::monospace(16.0), green);
        painter.text(o + egui::vec2(8.0, 34.0), egui::Align2::LEFT_TOP, "Profile: Default", egui::FontId::monospace(13.0), green);
        painter.text(o + egui::vec2(8.0, 58.0), egui::Align2::LEFT_TOP, "Active · 12:04", egui::FontId::monospace(13.0), green);
        ui.add_space(8.0);
        ui.weak("(placeholder — no LCD output yet; needs the display protocol)");
    }

    fn render_settings(&self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.add_space(8.0);
        let mut dry = self.dry_run.load(Ordering::Relaxed);
        ui.checkbox(&mut dry, "Start in Dry-run (safe)");
        self.dry_run.store(dry, Ordering::Relaxed);
        let mut f = false;
        ui.checkbox(&mut f, "Start minimized to tray");
        ui.checkbox(&mut f, "Launch at login");
        ui.add_space(6.0);
        ui.weak("(placeholder — only the Dry-run toggle is live; the rest are mockups)");
    }
}

#[cfg(test)]
mod tests {
    use super::ROWS;
    use std::collections::HashSet;

    #[test]
    fn rows_cover_all_22_keys_once() {
        let flat: Vec<_> = ROWS.iter().flat_map(|r| r.iter()).collect();
        assert_eq!(flat.len(), 22, "the physical layout must render all 22 G-keys");
        let unique: HashSet<_> = flat.iter().collect();
        // 22 unique keys out of 22 possible variants => every key exactly once.
        assert_eq!(unique.len(), 22, "no key may be duplicated or missing in ROWS");
    }
}

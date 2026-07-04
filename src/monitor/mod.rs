use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;
use anyhow::Result;
use eframe::egui;
use crate::config::Config;
use crate::device_state::{Connection, DeviceState};
use crate::dispatcher::Dispatcher;
use crate::injector::windows::WindowsInjector;
use crate::protocol::{G13Event, G13Key};
use crate::runtime;

pub fn run(config: Arc<RwLock<Config>>) -> Result<()> {
    let state = Arc::new(Mutex::new(DeviceState::new()));
    let dry_run = Arc::new(AtomicBool::new(true)); // first launch = Dry-run

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([720.0, 500.0]),
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

pub struct MonitorApp {
    config: Arc<RwLock<Config>>,
    state: Arc<Mutex<DeviceState>>,
    dry_run: Arc<AtomicBool>,
}

impl MonitorApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        config: Arc<RwLock<Config>>,
        state: Arc<Mutex<DeviceState>>,
        dry_run: Arc<AtomicBool>,
    ) -> Self {
        let app = Self { config, state, dry_run };
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
                let dispatcher = Dispatcher::new(self.config.clone(), injector);
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

// Physical G13 key arrangement: rows of 7, 7, 5, 3. The short rows are centered
// under the wide ones via a left pad of empty cells (`.0`); the right margin is
// naturally empty. Left pad 1 for the 5-row, 2 for the 3-row.
const ROWS: [(usize, &[G13Key]); 4] = [
    (0, &[G13Key::G1, G13Key::G2, G13Key::G3, G13Key::G4, G13Key::G5, G13Key::G6, G13Key::G7]),
    (0, &[G13Key::G8, G13Key::G9, G13Key::G10, G13Key::G11, G13Key::G12, G13Key::G13, G13Key::G14]),
    (1, &[G13Key::G15, G13Key::G16, G13Key::G17, G13Key::G18, G13Key::G19]),
    (2, &[G13Key::G20, G13Key::G21, G13Key::G22]),
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
            let cfg = self.config.read().unwrap();
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

        egui::CentralPanel::default().show(ctx, |ui| {
            let cfg = self.config.read().unwrap();
            ui.horizontal(|ui| {
                // Left: G-key grid in physical rows.
                ui.vertical(|ui| {
                    for (pad, row) in ROWS {
                        ui.horizontal(|ui| {
                            // Transparent spacer cells matching a key cell's footprint,
                            // to center the short rows under the wide ones.
                            for _ in 0..pad {
                                egui::Frame::new().inner_margin(4.0).show(ui, |ui| {
                                    ui.set_width(58.0);
                                    ui.vertical(|ui| {
                                        ui.strong(" ");
                                        ui.small(" ");
                                    });
                                });
                            }
                            for &key in row {
                                let pressed = snapshot.pressed.contains(&key);
                                let binding = cfg.get_binding(key).unwrap_or("—");
                                let fill = if pressed { egui::Color32::from_rgb(20, 54, 31) } else { egui::Color32::from_gray(38) };
                                egui::Frame::new().fill(fill).inner_margin(4.0).corner_radius(4.0).show(ui, |ui| {
                                    ui.set_width(58.0);
                                    ui.vertical(|ui| {
                                        ui.strong(format!("{key:?}"));
                                        ui.small(binding);
                                    });
                                });
                            }
                        });
                    }
                });

                ui.separator();

                // Right: joystick panel.
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
                    // deadzone circle (radius scaled from the 0..255 axis range)
                    let c = rect.center();
                    let dz_r = rect.width() * (dz as f32 / 255.0);
                    painter.circle_stroke(c, dz_r, egui::Stroke::new(1.0, egui::Color32::from_gray(70)));
                    // live position dot
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
        });
    }
}

#[cfg(test)]
mod tests {
    use super::ROWS;
    use std::collections::HashSet;

    #[test]
    fn rows_cover_all_22_keys_once() {
        let flat: Vec<_> = ROWS.iter().flat_map(|(_, r)| r.iter()).collect();
        assert_eq!(flat.len(), 22, "the physical layout must render all 22 G-keys");
        let unique: HashSet<_> = flat.iter().collect();
        // 22 unique keys out of 22 possible variants => every key exactly once.
        assert_eq!(unique.len(), 22, "no key may be duplicated or missing in ROWS");
    }
}

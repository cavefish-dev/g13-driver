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
                    if ui.selectable_label(!active, "Dry-run").clicked() { active = false; }
                    self.dry_run.store(!active, Ordering::Relaxed);
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let cfg = self.config.read().unwrap();
            ui.horizontal_wrapped(|ui| {
                for key in G13Key::ALL {
                    let pressed = snapshot.pressed.contains(&key);
                    let binding = cfg.get_binding(key).unwrap_or("—");
                    let text = format!("{key:?}\n{binding}");
                    let color = if pressed { egui::Color32::from_rgb(20, 54, 31) } else { egui::Color32::from_gray(38) };
                    egui::Frame::new().fill(color).inner_margin(4.0).show(ui, |ui| {
                        ui.set_width(58.0);
                        ui.label(text);
                    });
                }
            });
            ui.separator();
            ui.label(format!("joystick  x={}  y={}", snapshot.joy_x, snapshot.joy_y));
        });
    }
}

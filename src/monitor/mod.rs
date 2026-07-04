use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::AtomicBool;
use anyhow::Result;
use eframe::egui;
use crate::config::Config;
use crate::device_state::{Connection, DeviceState};

/// Launch the monitor window. Blocks on the eframe event loop (main thread).
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
        Box::new(move |_cc| Ok(Box::new(MonitorApp { config, state, dry_run }))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;
    Ok(())
}

pub struct MonitorApp {
    config: Arc<RwLock<Config>>,
    state: Arc<Mutex<DeviceState>>,
    dry_run: Arc<AtomicBool>,
}

impl eframe::App for MonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let connection = self.state.lock().unwrap().connection.clone();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("G13 Monitor");
            match &connection {
                Connection::Connected => ui.label("● Connected"),
                Connection::Disconnected(why) => ui.label(format!("○ Disconnected — {why}")),
            };
            ui.label("(skeleton — live monitor wired up in the next task)");
        });
    }
}

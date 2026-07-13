use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};
use anyhow::Result;
use eframe::egui;
use crate::config::ProfileSet;
use crate::device_state::{Connection, DeviceState};
use crate::dispatcher::Dispatcher;
use crate::injector::{KeyCombo, key_map::build_key_map, windows::WindowsInjector};
use crate::protocol::{G13Event, G13Key, MKey};

/// A combo is valid for the editor only if it parses AND its key is a known key
/// (so `ctrl+zzz` is rejected here rather than silently failing at injection).
fn combo_valid(s: &str, valid_keys: &HashSet<String>) -> bool {
    match KeyCombo::parse(s) {
        Ok(c) => match &c.key {
            Some(k) => valid_keys.contains(k),
            None => true, // modifier-only combo (e.g. "shift")
        },
        Err(_) => false,
    }
}

#[cfg(windows)]
fn find_main_window() -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    let title: Vec<u16> = "G13 Monitor\0".encode_utf16().collect();
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if hwnd.is_null() { return 0; }
    // Scope to our own process: an unrelated window titled "G13 Monitor" must
    // not receive our ShowWindow/WM_CLOSE.
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid); }
    if pid == unsafe { GetCurrentProcessId() } { hwnd as isize } else { 0 }
}

#[cfg(windows)]
fn show_main_window() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_SHOW};
    let hwnd = find_main_window();
    if hwnd != 0 {
        unsafe {
            ShowWindow(hwnd as _, SW_SHOW);
            SetForegroundWindow(hwnd as _);
        }
    }
}

#[cfg(windows)]
fn hide_main_window() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    let hwnd = find_main_window();
    if hwnd != 0 {
        unsafe { ShowWindow(hwnd as _, SW_HIDE); }
    }
}

#[cfg(windows)]
fn toggle_main_window() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsWindowVisible, SetForegroundWindow, ShowWindow, SW_HIDE, SW_SHOW,
    };
    let hwnd = find_main_window();
    if hwnd != 0 {
        unsafe {
            if IsWindowVisible(hwnd as _) != 0 {
                ShowWindow(hwnd as _, SW_HIDE);
            } else {
                ShowWindow(hwnd as _, SW_SHOW);
                SetForegroundWindow(hwnd as _);
            }
        }
    }
}

fn render_binding_row(
    ui: &mut egui::Ui,
    key: G13Key,
    edits: &mut HashMap<G13Key, String>,
    repeat_edits: &mut HashMap<G13Key, bool>,
    valid_keys: &HashSet<String>,
) {
    let green = egui::Color32::from_rgb(127, 224, 160);
    let red = egui::Color32::from_rgb(220, 90, 90);
    let dim = egui::Color32::from_gray(110);
    let buf = edits.entry(key).or_default();
    let rep = repeat_edits.entry(key).or_default();
    ui.horizontal(|ui| {
        ui.monospace(format!("{key:?}"));
        ui.add_space(6.0);
        ui.add(egui::TextEdit::singleline(buf).desired_width(160.0));
        let (mark, color) = if buf.is_empty() {
            ("—", dim)
        } else if combo_valid(buf, valid_keys) {
            ("ok", green)
        } else {
            ("bad", red)
        };
        ui.colored_label(color, mark);
        ui.add_space(6.0);
        ui.checkbox(rep, "repeat");
    });
}

pub fn run(config: Arc<RwLock<ProfileSet>>, config_path: std::path::PathBuf, start_minimized: bool) -> Result<()> {
    let state = Arc::new(Mutex::new(DeviceState::new()));
    let start_active = config.read().unwrap().start_active();
    let dry_run = Arc::new(AtomicBool::new(!start_active));

    // Fixed, non-resizable window sized to fit the content of every tab.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([660.0, 560.0])
            .with_resizable(false)
            .with_visible(!start_minimized),
        ..Default::default()
    };

    eframe::run_native(
        "G13 Monitor",
        options,
        Box::new(move |cc| Ok(Box::new(MonitorApp::new(cc, config, config_path, state, dry_run, start_minimized)))),
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

#[derive(Clone)]
enum PromptKind { New, Duplicate { src: String }, Rename { filename: String } }

#[derive(Clone)]
struct NamePrompt {
    kind: PromptKind,
    buffer: String,
}

/// Deferred library-list action, collected inside egui closures and applied
/// after the ScrollArea returns (see `render_profiles`).
enum Action {
    Assign(String),
    BeginDelete(String),
    Prompt(NamePrompt),
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
    /// Set by the tray "Quit" handler so the close interception in `update()`
    /// allows the close instead of hiding to the tray.
    quit: Arc<AtomicBool>,
    /// True while the window is shown. The tray/activation handlers run on the
    /// message-loop thread (even while hidden), so they own the visibility truth
    /// via this atomic; `update()` reads/refreshes it.
    window_visible: Arc<AtomicBool>,
    tab: Tab,
    edits: HashMap<G13Key, String>,
    repeat_edits: HashMap<G13Key, bool>,
    edits_for: Option<String>,
    save_status: Option<String>,
    #[cfg(windows)]
    tray: Option<crate::tray::TrayHandle>,
    last_persisted_active: bool,
    #[cfg(windows)]
    last_icon: Option<crate::tray::IconState>,
    update_status: std::sync::Arc<std::sync::Mutex<crate::update::UpdateStatus>>,
    config_path: std::path::PathBuf,
    name_prompt: Option<NamePrompt>,
    pending_delete: Option<String>,
    profiles_status: Option<String>,
}

impl MonitorApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        profiles: Arc<RwLock<ProfileSet>>,
        config_path: std::path::PathBuf,
        state: Arc<Mutex<DeviceState>>,
        dry_run: Arc<AtomicBool>,
        start_minimized: bool,
    ) -> Self {
        let last_persisted_active = !dry_run.load(Ordering::Relaxed);
        let quit = Arc::new(AtomicBool::new(false));
        let window_visible = Arc::new(AtomicBool::new(!start_minimized));
        #[cfg_attr(not(windows), allow(unused_mut))]
        let mut app = Self {
            profiles,
            state,
            dry_run,
            quit,
            window_visible,
            tab: Tab::Monitor,
            edits: HashMap::new(),
            repeat_edits: HashMap::new(),
            edits_for: None,
            save_status: None,
            #[cfg(windows)]
            tray: None,
            last_persisted_active,
            #[cfg(windows)]
            last_icon: None,
            update_status: std::sync::Arc::new(std::sync::Mutex::new(crate::update::UpdateStatus::Idle)),
            config_path,
            name_prompt: None,
            pending_delete: None,
            profiles_status: None,
        };

        // Windows: build the tray from the current state and start the
        // activation waiter. A tray-build failure logs and falls back to a
        // plain window (tray = None).
        //
        // Tray and activation events are handled by global event handlers /
        // a bridge thread rather than polled in `update()`: eframe's `update()`
        // does not run while the window is hidden (no repaints), but the tray
        // handlers run on the message-loop thread whenever a tray message
        // arrives — so they work even while hidden. They act on the window
        // directly through the egui `Context` (Clone + Send + Sync).
        #[cfg(windows)]
        {
            let active = !app.dry_run.load(Ordering::Relaxed);
            let st = crate::tray::icon_state(false, active); // not connected yet at startup
            let tray = crate::tray::TrayHandle::new(st, active, crate::autostart::is_enabled())
                .map_err(|e| log::warn!("tray unavailable: {e:#}"))
                .ok();

            if let Some(tray) = &tray {
                let ctx = cc.egui_ctx.clone();
                let ids = tray.ids();
                let (show_id, active_id, autostart_id, quit_id) =
                    (ids.show.clone(), ids.active.clone(), ids.autostart.clone(), ids.quit.clone());
                let dry_run = app.dry_run.clone();
                let window_visible = app.window_visible.clone();
                let quit = app.quit.clone();

                // Menu events (right-click menu items). These run on the
                // message-loop thread even while the window is hidden, so they
                // show/hide the OS window directly via Win32 — eframe pauses its
                // viewport-command processing while hidden, so Visible(true)
                // would never be applied.
                tray_icon::menu::MenuEvent::set_event_handler(Some(move |ev: tray_icon::menu::MenuEvent| {
                    if ev.id == show_id {
                        toggle_main_window();
                        use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
                        let visible = unsafe { IsWindowVisible(find_main_window() as _) != 0 };
                        window_visible.store(visible, Ordering::Relaxed);
                        ctx.request_repaint();
                    } else if ev.id == active_id {
                        let dry = dry_run.load(Ordering::Relaxed);
                        dry_run.store(!dry, Ordering::Relaxed);
                        ctx.request_repaint();
                    } else if ev.id == autostart_id {
                        if crate::autostart::is_enabled() {
                            if let Err(e) = crate::autostart::disable() {
                                log::warn!("autostart toggle failed: {e:#}");
                            }
                        } else if let Err(e) = crate::autostart::enable() {
                            log::warn!("autostart toggle failed: {e:#}");
                        }
                        ctx.request_repaint();
                    } else if ev.id == quit_id {
                        quit.store(true, std::sync::atomic::Ordering::Relaxed);
                        // Show the window first: eframe's update() (which honors the
                        // quit flag to allow the close) does not run while hidden, so
                        // we must un-hide before posting the close or Quit is ignored.
                        show_main_window();
                        use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
                        let hwnd = find_main_window();
                        if hwnd != 0 { unsafe { PostMessageW(hwnd as _, WM_CLOSE, 0, 0); } }
                        ctx.request_repaint();
                    }
                }));
            }

            // A second launch signals us to show the window. Bridge the
            // activation channel to the window on its own thread so it works
            // while hidden (update() would not run to drain it).
            let (tx, rx) = std::sync::mpsc::channel();
            crate::single_instance::spawn_activation_waiter(tx);
            let ctx = cc.egui_ctx.clone();
            let window_visible = app.window_visible.clone();
            std::thread::spawn(move || {
                while rx.recv().is_ok() {
                    show_main_window();
                    window_visible.store(true, Ordering::Relaxed);
                    ctx.request_repaint();
                }
            });

            app.tray = tray;
        }

        app.start_consumer(cc.egui_ctx.clone());
        spawn_update_check(app.update_status.clone(), cc.egui_ctx.clone(), false);
        app
    }

    /// Start the USB supervisor + the event consumer. Called ONCE at startup.
    ///
    /// The supervisor thread owns `tx` (kept alive across reconnects) and the
    /// shared state + ctx: it opens the G13, marks Connected, blocks in
    /// `reader.run` until disconnect, marks Disconnected, then retries every 2s.
    /// Because it holds `tx`, the consumer's channel never closes, so a
    /// disconnect/reconnect cycle does not tear down the consumer.
    fn start_consumer(&self, ctx: egui::Context) {
        let (tx, rx) = std::sync::mpsc::channel();

        // Supervisor: owns connection state and reconnects automatically.
        {
            let state = self.state.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || loop {
                match crate::usb::UsbReader::open() {
                    Ok(reader) => {
                        { state.lock().unwrap().connection = Connection::Connected; }
                        ctx.request_repaint();
                        let _ = reader.run(tx.clone()); // blocks until disconnect
                        {
                            state.lock().unwrap().connection =
                                Connection::Disconnected("device disconnected".to_string());
                        }
                        ctx.request_repaint();
                    }
                    Err(e) => {
                        { state.lock().unwrap().connection = Connection::Disconnected(format!("{e:#}")); }
                        ctx.request_repaint();
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(2));
            });
        }

        let injector = Box::new(WindowsInjector::new());
        let dispatcher = Dispatcher::new(self.profiles.clone(), injector);
        let state = self.state.clone();
        let dry_run = self.dry_run.clone();
        std::thread::spawn(move || consumer_loop(rx, dispatcher, state, dry_run, ctx));
    }
}

/// Drains the event stream: updates DeviceState for display always; injects via
/// the dispatcher only when Active. A short recv timeout lets us notice a
/// Dry-run toggle promptly so an Active->Dry-run switch releases held keys even
/// with no new events. Connection state is owned by the supervisor (which keeps
/// tx alive across reconnects), so the channel does not close on a mere
/// disconnect; the Disconnected arm here is only a safety exit if the supervisor
/// itself dies.
fn consumer_loop(
    rx: Receiver<G13Event>,
    mut dispatcher: Dispatcher,
    state: Arc<Mutex<DeviceState>>,
    dry_run: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    let mut was_active = !dry_run.load(Ordering::Relaxed);
    loop {
        match rx.recv_timeout(Duration::from_millis(15)) {
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
                dispatcher.tick(Instant::now());
                ctx.request_repaint();
            }
            Err(RecvTimeoutError::Timeout) => {
                let active = !dry_run.load(Ordering::Relaxed);
                if was_active && !active {
                    dispatcher.release_held();
                }
                was_active = active;
                dispatcher.tick(Instant::now());
            }
            // Only reachable if the supervisor thread died; connection state is
            // owned by the supervisor, so just release held keys and exit.
            Err(RecvTimeoutError::Disconnected) => {
                dispatcher.release_held();
                return;
            }
        }
    }
}

/// Run an update check on a background thread and store the result. A background
/// (manual = false) check that fails goes silently to Idle; a manual check surfaces
/// the error as Failed.
fn spawn_update_check(
    status: std::sync::Arc<std::sync::Mutex<crate::update::UpdateStatus>>,
    ctx: egui::Context,
    manual: bool,
) {
    std::thread::spawn(move || {
        *status.lock().unwrap() = crate::update::UpdateStatus::Checking;
        ctx.request_repaint();
        let next = match crate::update::check() {
            Ok(Some(u)) => crate::update::UpdateStatus::Available(u),
            Ok(None) => crate::update::UpdateStatus::UpToDate,
            Err(e) => {
                log::warn!("update check failed: {e:#}");
                if manual {
                    crate::update::UpdateStatus::Failed("couldn't check for updates".into())
                } else {
                    crate::update::UpdateStatus::Idle
                }
            }
        };
        *status.lock().unwrap() = next;
        ctx.request_repaint();
    });
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

/// The three bindable thumb inputs (byte 7): two side buttons + the joystick click.
const THUMB: [G13Key; 3] = [G13Key::Btn1, G13Key::Btn2, G13Key::Stick];

impl eframe::App for MonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // If update() is running, the window is (or is becoming) visible.
        // Keep the atomic accurate in case the OS showed us by other means.
        self.window_visible.store(true, Ordering::Relaxed);

        // Close (X) -> hide to tray instead of exiting, unless a tray "Quit"
        // requested the close (then let it through).
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.quit.load(Ordering::Relaxed) {
                // allow the close — do nothing
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                // Hide via Win32 (not ViewportCommand::Visible) so show and hide
                // use the same mechanism — otherwise eframe's visibility state and
                // the OS window drift out of sync after a Win32 show.
                #[cfg(windows)]
                hide_main_window();
                self.window_visible.store(false, Ordering::Relaxed);
            }
        }
        // Minimize -> hide to tray.
        if ctx.input(|i| i.viewport().minimized == Some(true)) {
            #[cfg(windows)]
            hide_main_window();
            self.window_visible.store(false, Ordering::Relaxed);
        }

        let snapshot = self.state.lock().unwrap().clone();

        // Persist a mode change (from any surface: header, Settings, tray).
        let active = !self.dry_run.load(Ordering::Relaxed);
        if active != self.last_persisted_active {
            self.last_persisted_active = active;
            if let Err(e) = self.profiles.read().unwrap().persist_start_active(active) {
                log::warn!("could not persist mode: {e:#}");
            }
        }

        // Sync the tray icon + checkmarks.
        #[cfg(windows)]
        if let Some(tray) = &mut self.tray {
            let connected = matches!(snapshot.connection, Connection::Connected);
            let st = crate::tray::icon_state(connected, active);
            if self.last_icon != Some(st) { tray.set_state(st); self.last_icon = Some(st); }
            tray.set_checks(active, crate::autostart::is_enabled());
        }

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
                ui.horizontal(|ui| {
                    ui.label("Thumb:");
                    let hot = egui::Color32::from_rgb(127, 224, 160);
                    let dim = egui::Color32::from_gray(140);
                    for (key, label) in [(G13Key::Btn1, "BTN1"), (G13Key::Btn2, "BTN2"), (G13Key::Stick, "STICK")] {
                        let on = snapshot.pressed.contains(&key);
                        ui.colored_label(if on { hot } else { dim }, label);
                    }
                });
            });
        });
    }

    // ---- UI-vision placeholders (not wired to real behavior yet) ----

    fn render_profiles(&mut self, ui: &mut egui::Ui) {
        ui.heading("Profiles");
        ui.label("Click a slot to make it active, then click a profile below to assign it to that slot.");
        ui.add_space(6.0);

        // Snapshot state under a short read lock.
        let (active, slot_names, dir) = {
            let set = self.profiles.read().unwrap();
            let names = [
                set.name(MKey::M1).map(String::from),
                set.name(MKey::M2).map(String::from),
                set.name(MKey::M3).map(String::from),
            ];
            (set.active(), names, set.profiles_dir().to_path_buf())
        };
        let entries = crate::profiles::list(&dir);
        let display_of = |filename: &str| -> String {
            entries.iter().find(|e| e.filename == filename)
                .map(|e| e.display_name.clone())
                .unwrap_or_else(|| filename.trim_end_matches(".toml").to_string())
        };

        // --- Slots ---
        let mkeys = [MKey::M1, MKey::M2, MKey::M3];
        let mut switch_to: Option<MKey> = None;
        for (i, m) in mkeys.iter().enumerate() {
            let label = match &slot_names[i] {
                Some(f) => format!("{m:?}  —  {}", display_of(f)),
                None => format!("{m:?}  —  (unassigned)"),
            };
            if ui.selectable_label(*m == active, label).clicked() {
                switch_to = Some(*m);
            }
        }
        if let Some(m) = switch_to {
            self.profiles.write().unwrap().set_active(m);
        }

        ui.add_space(10.0);
        ui.separator();

        // --- Folder bar ---
        ui.horizontal(|ui| {
            ui.label("Folder:");
            ui.monospace(elide_path(&dir, 48));
        });
        // Deferred folder-bar actions (avoid &mut self inside these closures).
        let mut do_change_folder = false;
        let mut do_open_folder = false;
        ui.horizontal(|ui| {
            if ui.button("Change folder…").clicked() {
                do_change_folder = true;
            }
            if ui.button("Open folder").clicked() {
                do_open_folder = true;
            }
            if ui.button("New").clicked() {
                self.name_prompt = Some(NamePrompt { kind: PromptKind::New, buffer: String::new() });
            }
        });
        if do_change_folder { self.change_folder(&dir); }
        if do_open_folder { open_folder(&dir); }

        ui.add_space(8.0);

        // --- Library list ---
        // Collect the intended action inside the closures, then apply it AFTER
        // the ScrollArea closure returns (where `self` is freely mutable). This
        // is the generalized deferred-action pattern; the nested egui closures
        // borrow `ui` and would conflict with `&mut self` method calls.
        let active_slot_file = slot_index(active).and_then(|i| slot_names[i].clone());
        let mut action: Option<Action> = None;
        egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
            for e in &entries {
                ui.horizontal(|ui| {
                    let is_active_file = active_slot_file.as_deref() == Some(e.filename.as_str());
                    if ui.selectable_label(is_active_file, &e.display_name).clicked() {
                        action = Some(Action::Assign(e.filename.clone()));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Delete").clicked() {
                            action = Some(Action::BeginDelete(e.filename.clone()));
                        }
                        if ui.small_button("Rename").clicked() {
                            action = Some(Action::Prompt(NamePrompt {
                                kind: PromptKind::Rename { filename: e.filename.clone() },
                                buffer: e.display_name.clone(),
                            }));
                        }
                        if ui.small_button("Duplicate").clicked() {
                            action = Some(Action::Prompt(NamePrompt {
                                kind: PromptKind::Duplicate { src: e.filename.clone() },
                                buffer: format!("Copy of {}", e.display_name),
                            }));
                        }
                    });
                });
            }
        });
        match action {
            Some(Action::Assign(f)) => self.assign_to_active(&f),
            Some(Action::BeginDelete(f)) => self.try_begin_delete(&f, &dir, &entries),
            Some(Action::Prompt(p)) => self.name_prompt = Some(p),
            None => {}
        }

        if let Some(s) = &self.profiles_status {
            ui.add_space(6.0);
            ui.weak(s);
        }

        self.render_name_prompt(ui.ctx(), &dir);
        self.render_delete_confirm(ui.ctx(), &dir, &entries);
    }

    fn assign_to_active(&mut self, filename: &str) {
        let active = self.profiles.read().unwrap().active();
        let persisted = self.profiles.read().unwrap().persist_slot(active, Some(filename));
        let res = persisted.and_then(|_| crate::runtime::reload_now(&self.profiles, &self.config_path));
        self.profiles_status = Some(match res {
            Ok(()) => format!("Assigned to {active:?}."),
            Err(e) => format!("Assign failed: {e}"),
        });
    }

    fn change_folder(&mut self, current: &std::path::Path) {
        #[cfg(windows)]
        {
            let picked = rfd::FileDialog::new().set_directory(current).pick_folder();
            let Some(new_dir) = picked else { return };
            let res = (|| -> anyhow::Result<crate::profiles::CopyReport> {
                let report = crate::profiles::copy_into(current, &new_dir)?;
                self.profiles.read().unwrap().persist_profiles_dir(&new_dir)?;
                crate::runtime::reload_now(&self.profiles, &self.config_path)?;
                Ok(report)
            })();
            self.profiles_status = Some(match res {
                Ok(r) => format!("Folder changed. Copied {} profile(s), skipped {}.", r.copied, r.skipped),
                Err(e) => format!("Change folder failed: {e}"),
            });
        }
        #[cfg(not(windows))]
        { let _ = current; }
    }

    fn try_begin_delete(&mut self, filename: &str, _dir: &std::path::Path, entries: &[crate::profiles::ProfileEntry]) {
        let set = self.profiles.read().unwrap();
        let slots = [set.name(MKey::M1), set.name(MKey::M2), set.name(MKey::M3)];
        match crate::profiles::deletion_plan(filename, slots, entries.len()) {
            Ok(_) => { drop(set); self.pending_delete = Some(filename.to_string()); }
            Err(reason) => { drop(set); self.profiles_status = Some(reason); }
        }
    }

    fn render_name_prompt(&mut self, ctx: &egui::Context, dir: &std::path::Path) {
        let Some(mut prompt) = self.name_prompt.take() else { return };
        let mut open = true;
        let mut submit = false;
        egui::Modal::new(egui::Id::new("name_prompt")).show(ctx, |ui| {
            ui.set_width(320.0);
            let title = match &prompt.kind {
                PromptKind::New => "New profile",
                PromptKind::Duplicate { .. } => "Duplicate profile",
                PromptKind::Rename { .. } => "Rename profile",
            };
            ui.heading(title);
            ui.add_space(4.0);
            let resp = ui.text_edit_singleline(&mut prompt.buffer);
            resp.request_focus();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let valid = !prompt.buffer.trim().is_empty();
                if ui.add_enabled(valid, egui::Button::new("OK")).clicked() { submit = true; }
                if ui.button("Cancel").clicked() { open = false; }
            });
        });
        if submit {
            let name = prompt.buffer.trim().to_string();
            let res: anyhow::Result<()> = (|| {
                match &prompt.kind {
                    PromptKind::New => { crate::profiles::create(dir, &name)?; }
                    PromptKind::Duplicate { src } => { crate::profiles::duplicate(dir, src, &name)?; }
                    PromptKind::Rename { filename } => { crate::profiles::rename(dir, filename, &name)?; }
                }
                crate::runtime::reload_now(&self.profiles, &self.config_path)
            })();
            self.profiles_status = Some(match res {
                Ok(()) => "Saved.".to_string(),
                Err(e) => format!("Failed: {e}"),
            });
            // fall through: prompt consumed (not re-stored)
        } else if open {
            self.name_prompt = Some(prompt); // keep showing until OK/Cancel
        }
    }

    fn render_delete_confirm(&mut self, ctx: &egui::Context, dir: &std::path::Path, entries: &[crate::profiles::ProfileEntry]) {
        let Some(filename) = self.pending_delete.clone() else { return };
        let display = entries.iter().find(|e| e.filename == filename)
            .map(|e| e.display_name.clone()).unwrap_or_else(|| filename.clone());
        let mut confirm = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("delete_confirm")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.heading("Delete profile");
            ui.label(format!("Delete profile '{display}'? This removes the file."));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() { confirm = true; }
                if ui.button("Cancel").clicked() { cancel = true; }
            });
        });
        if confirm {
            let res: anyhow::Result<()> = (|| {
                // Re-evaluate the plan against current state, then cascade unassigns.
                let (slots_owned, total) = {
                    let set = self.profiles.read().unwrap();
                    ([set.name(MKey::M1).map(String::from),
                      set.name(MKey::M2).map(String::from),
                      set.name(MKey::M3).map(String::from)], entries.len())
                };
                let slots = [slots_owned[0].as_deref(), slots_owned[1].as_deref(), slots_owned[2].as_deref()];
                let plan = crate::profiles::deletion_plan(&filename, slots, total)
                    .map_err(|e| anyhow::anyhow!(e))?;
                {
                    let set = self.profiles.read().unwrap();
                    for m in &plan.unassign { set.persist_slot(*m, None)?; }
                }
                crate::profiles::delete(dir, &filename)?;
                crate::runtime::reload_now(&self.profiles, &self.config_path)
            })();
            self.profiles_status = Some(match res {
                Ok(()) => "Deleted.".to_string(),
                Err(e) => format!("Delete failed: {e}"),
            });
            self.pending_delete = None;
        } else if cancel {
            self.pending_delete = None;
        }
    }

    fn render_bindings(&mut self, ui: &mut egui::Ui) {
        // Which profile are we editing? Reload buffers when it changes.
        let active_name = self.profiles.read().unwrap().active_name().map(String::from);
        if self.edits_for != active_name {
            let set = self.profiles.read().unwrap();
            let profile = set.active_profile();
            let bound = profile.bindings();
            self.edits = ROWS.iter().flat_map(|row| row.iter()).chain(THUMB.iter())
                .map(|&k| (k, bound.get(&k).cloned().unwrap_or_default()))
                .collect();
            self.repeat_edits = ROWS.iter().flat_map(|row| row.iter()).chain(THUMB.iter())
                .map(|&k| (k, profile.repeats(k)))
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
        ui.weak("Combo = optional modifiers (ctrl / shift / alt / win) + one key, held while \
                 the G-key is held. Examples: ctrl+c, ctrl+shift+z, win+d. Modifiers alone are \
                 allowed (e.g. shift, ctrl+shift). Keys: a-z, 0-9, f1-f24, enter, esc, space, \
                 tab, arrows, home/end, pageup/pagedown, insert/delete, and media: playpause, \
                 nexttrack, prevtrack, volup, voldown, mute (media keys tap). Empty = unmapped.");
        ui.weak("Tick 'repeat' to auto-repeat a key while held (like a keyboard). Repeat \
                 timing (delay/rate) is set in config.toml under [autorepeat].");
        ui.add_space(6.0);

        let red = egui::Color32::from_rgb(220, 90, 90);

        // Valid key names (built once per frame from the injector's key table).
        let valid_keys: HashSet<String> = build_key_map().into_keys().collect();

        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for &key in ROWS.iter().flat_map(|row| row.iter()) {
                render_binding_row(ui, key, &mut self.edits, &mut self.repeat_edits, &valid_keys);
            }
            ui.add_space(6.0);
            ui.separator();
            ui.label("Thumb buttons");
            for &key in THUMB.iter() {
                render_binding_row(ui, key, &mut self.edits, &mut self.repeat_edits, &valid_keys);
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
                let repeat: HashMap<G13Key, bool> = self.repeat_edits.iter()
                    .filter(|(_, &v)| v)
                    .map(|(k, &v)| (*k, v))
                    .collect();
                match self.profiles.write().unwrap().save_active_bindings(bindings, repeat) {
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
        if ui.checkbox(&mut dry, "Start in Dry-run (safe)").changed() {
            self.dry_run.store(dry, Ordering::Relaxed);
        }
        #[cfg(windows)]
        {
            let mut on = crate::autostart::is_enabled();
            if ui.checkbox(&mut on, "Launch at login").changed() {
                let r = if on { crate::autostart::enable() } else { crate::autostart::disable() };
                if let Err(e) = r {
                    log::warn!("autostart toggle failed: {e:#}");
                    ui.colored_label(egui::Color32::from_rgb(220, 90, 90), format!("failed: {e:#}"));
                }
            }
        }
        ui.add_space(10.0);
        ui.separator();
        ui.label(format!("Version: {}", env!("G13_VERSION")));
        let status = self.update_status.lock().unwrap().clone();
        match status {
            crate::update::UpdateStatus::Checking => { ui.weak("Checking for updates…"); }
            crate::update::UpdateStatus::UpToDate => { ui.weak("Up to date."); }
            crate::update::UpdateStatus::Installing => { ui.weak("Updating… the app will restart."); }
            crate::update::UpdateStatus::Failed(msg) => {
                ui.colored_label(egui::Color32::from_rgb(220, 90, 90), msg);
            }
            crate::update::UpdateStatus::Available(u) => {
                ui.colored_label(egui::Color32::from_rgb(95, 200, 130),
                    format!("Update available: v{}", u.version));
                #[cfg(windows)]
                if ui.button("Update now").clicked() {
                    let status = self.update_status.clone();
                    let upd = u.clone();
                    *status.lock().unwrap() = crate::update::UpdateStatus::Installing;
                    std::thread::spawn(move || {
                        if let Err(e) = crate::update::apply::install(&upd) {
                            log::warn!("update failed: {e:#}");
                            *status.lock().unwrap() =
                                crate::update::UpdateStatus::Failed(format!("update failed: {e:#}"));
                        }
                        // On success install() self-restarts and never returns here.
                    });
                }
            }
            crate::update::UpdateStatus::Idle => {}
        }
        if ui.button("Check for updates").clicked() {
            spawn_update_check(self.update_status.clone(), ui.ctx().clone(), true);
        }
        ui.add_space(6.0);
        ui.weak("Close or minimize hides to the tray; the driver keeps running. Quit from the tray to exit.");
    }
}

fn slot_index(m: MKey) -> Option<usize> {
    match m { MKey::M1 => Some(0), MKey::M2 => Some(1), MKey::M3 => Some(2), MKey::MR => None }
}

fn elide_path(p: &std::path::Path, max: usize) -> String {
    let s = p.display().to_string();
    if s.chars().count() <= max { s } else {
        let tail: String = s.chars().rev().take(max - 1).collect::<Vec<_>>().into_iter().rev().collect();
        format!("…{tail}")
    }
}

#[cfg(windows)]
fn open_folder(dir: &std::path::Path) {
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
}
#[cfg(not(windows))]
fn open_folder(_dir: &std::path::Path) {}

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

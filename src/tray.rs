//! System-tray icon + menu. Windows-only.
#![cfg(windows)]

use anyhow::Result;
use tray_icon::menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Effective tray state; problem outranks mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Problem,
    Active,
    DryRun,
}

pub fn icon_state(connected: bool, active: bool) -> IconState {
    if !connected {
        IconState::Problem
    } else if active {
        IconState::Active
    } else {
        IconState::DryRun
    }
}

/// A "g13" glyph icon in the state's colour (red / green / grey) on near-black.
pub fn icon_rgba(state: IconState) -> (Vec<u8>, u32, u32) {
    let (r, g, b) = match state {
        IconState::Problem => (210, 70, 70),
        IconState::Active  => (95, 200, 130),
        IconState::DryRun  => (140, 140, 140),
    };
    crate::icon::render_g13_rgba(3, [r, g, b, 255], [24, 24, 24, 255])
}

fn make_icon(state: IconState) -> Result<Icon> {
    let (rgba, w, h) = icon_rgba(state);
    Ok(Icon::from_rgba(rgba, w, h)?)
}

/// Menu item IDs shared with the consumer (Task 5 matches on these).
pub struct MenuIds {
    pub show: MenuId,
    pub active: MenuId,
    pub autostart: MenuId,
    pub quit: MenuId,
}

/// Owns the tray icon + menu items so they outlive the process.
pub struct TrayHandle {
    icon: TrayIcon,
    item_active: CheckMenuItem,
    item_autostart: CheckMenuItem,
    ids: MenuIds,
    state: IconState,
}

impl TrayHandle {
    /// Build the tray with the initial state + check states.
    pub fn new(state: IconState, active: bool, autostart: bool) -> Result<Self> {
        let item_show = MenuItem::new("Show / Hide window", true, None);
        let item_active = CheckMenuItem::new("Active", true, active, None);
        let item_autostart = CheckMenuItem::new("Start at login", true, autostart, None);
        let item_quit = MenuItem::new("Quit", true, None);

        let menu = Menu::new();
        menu.append(&item_show)?;
        menu.append(&item_active)?;
        menu.append(&item_autostart)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&item_quit)?;

        let ids = MenuIds {
            show: item_show.id().clone(),
            active: item_active.id().clone(),
            autostart: item_autostart.id().clone(),
            quit: item_quit.id().clone(),
        };

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(tooltip(state))
            .with_icon(make_icon(state)?)
            .build()?;

        Ok(Self {
            icon,
            item_active,
            item_autostart,
            ids,
            state,
        })
    }

    pub fn ids(&self) -> &MenuIds {
        &self.ids
    }

    /// Swap the icon + tooltip only when the effective state changes.
    pub fn set_state(&mut self, state: IconState) {
        if state == self.state {
            return;
        }
        self.state = state;
        if let Ok(icon) = make_icon(state) {
            let _ = self.icon.set_icon(Some(icon));
        }
        let _ = self.icon.set_tooltip(Some(tooltip(state)));
    }

    /// Keep the menu checkmarks in sync with the true state.
    pub fn set_checks(&self, active: bool, autostart: bool) {
        self.item_active.set_checked(active);
        self.item_autostart.set_checked(autostart);
    }
}

fn tooltip(state: IconState) -> &'static str {
    match state {
        IconState::Problem => "G13 — not connected",
        IconState::Active  => "G13 — Active",
        IconState::DryRun  => "G13 — Dry-run",
    }
}

#[cfg(test)]
mod tests {
    use super::{icon_rgba, icon_state, IconState};

    #[test]
    fn problem_takes_precedence_over_mode() {
        assert_eq!(icon_state(false, true), IconState::Problem); // disconnected beats Active
        assert_eq!(icon_state(false, false), IconState::Problem);
    }

    #[test]
    fn connected_reflects_mode() {
        assert_eq!(icon_state(true, true), IconState::Active);
        assert_eq!(icon_state(true, false), IconState::DryRun);
    }

    #[test]
    fn icon_rgba_buffer_matches_dimensions() {
        let (buf, w, h) = icon_rgba(IconState::Active);
        assert_eq!(buf.len() as u32, w * h * 4);
    }
}

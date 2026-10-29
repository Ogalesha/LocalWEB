//! System tray icon with localized Open and Quit actions.
//!
//! The tray is optional: when the desktop has no `StatusNotifier` watcher the
//! caller keeps the normal close behaviour instead of hiding the window with
//! no way to restore it.

use crate::{brand, lang::Lang};
use eframe::egui;
use std::sync::mpsc::Sender;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

/// Actions exposed by the tray menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrayCommand {
    Open,
    Quit,
}

/// Owns the platform tray icon and keeps its menu labels in sync.
pub(crate) struct Tray {
    _icon: TrayIcon,
    open: MenuItem,
    quit: MenuItem,
}

impl Tray {
    /// Creates the tray icon and forwards menu clicks to `sender`.
    ///
    /// Returns `None` when this desktop cannot show a tray icon, so the
    /// application can fall back to quitting when the window is closed.
    pub(crate) fn new(
        lang: Lang,
        context: egui::Context,
        sender: Sender<TrayCommand>,
    ) -> Option<Self> {
        if !host_has_tray() {
            return None;
        }
        let open = MenuItem::new(lang.action_open(), true, None);
        let quit = MenuItem::new(lang.tray_quit(), true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &quit]).ok()?;
        let data = brand::icon_data();
        let icon = Icon::from_rgba(data.rgba, data.width, data.height).ok()?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("LocalWeb")
            .with_icon(icon)
            .build()
            .ok()?;

        let open_id = open.id().clone();
        let quit_id = quit.id().clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let command = if event.id == open_id {
                Some(TrayCommand::Open)
            } else if event.id == quit_id {
                Some(TrayCommand::Quit)
            } else {
                None
            };
            if let Some(command) = command {
                let _ = sender.send(command);
                context.request_repaint();
            }
        }));

        Some(Self {
            _icon: tray,
            open,
            quit,
        })
    }

    /// Updates the menu after the interface language changes.
    pub(crate) fn set_lang(&self, lang: Lang) {
        self.open.set_text(lang.action_open());
        self.quit.set_text(lang.tray_quit());
    }
}

/// Returns whether the current desktop exposes a `StatusNotifier` watcher.
///
/// `tray-icon` registers with KSNI optimistically, so a successful build does
/// not prove the icon is visible. Checking the watcher name avoids hiding the
/// window on desktops without a tray implementation.
#[cfg(target_os = "linux")]
fn host_has_tray() -> bool {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };
    connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "NameHasOwner",
            &("org.kde.StatusNotifierWatcher",),
        )
        .ok()
        .and_then(|reply| reply.body().deserialize::<bool>().ok())
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
const fn host_has_tray() -> bool {
    true
}

//! System tray: minimize-to-tray with a small context menu.
//!
//! On macOS the tray lives in the menu bar. Left-click toggles window
//! visibility; the context menu adds Reload / Restart Backend / Quit. Closing
//! the main window goes to the tray instead of quitting (handled by
//! `on_window_event` in the app builder); use the tray menu's Quit (or
//! Cmd+Q) to actually exit.

use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

/// Menu id routed from both the native menu bar and the tray.
const TRAY_SHOW_HIDE: &str = "tray-show-hide";

pub fn setup(app: &tauri::AppHandle) -> tauri::Result<()> {
    let show_hide = MenuItemBuilder::with_id(TRAY_SHOW_HIDE, crate::t!("menu.show_hide"))
        .accelerator("CmdOrCtrl+T")
        .build(app)?;
    let reload = MenuItemBuilder::with_id("reload", crate::t!("menu.reload"))
        .accelerator("CmdOrCtrl+R")
        .build(app)?;
    let restart = MenuItemBuilder::with_id("restart", crate::t!("menu.restart"))
        .accelerator("CmdOrCtrl+Shift+R")
        .build(app)?;
    let quit = MenuItemBuilder::with_id("quit", crate::t!("menu.quit"))
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let main = SubmenuBuilder::new(app, "DSH")
        .item(&show_hide)
        .separator()
        .item(&reload)
        .item(&restart)
        .separator()
        .item(&quit)
        .build()?;

    let menu = MenuBuilder::new(app).items(&[&main]).build()?;

    let icon = app.default_window_icon().cloned();

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("DeepSeek Harness");

    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    builder
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_SHOW_HIDE => toggle_window(app),
            "reload" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.reload();
                }
            }
            "restart" => crate::spawn_dsh_web(app.clone()),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// Show+focus the window if hidden, otherwise hide it (toggle to/from tray).
pub fn toggle_window(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else { return };
    match win.is_visible() {
        Ok(true) => {
            let _ = win.hide();
        }
        _ => {
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

/// Bring the main window to the front (used when a notification fires).
pub fn focus(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else { return };
    let _ = win.show();
    let _ = win.set_focus();
}

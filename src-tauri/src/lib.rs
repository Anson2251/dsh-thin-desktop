use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, State};

pub mod i18n;
mod notify;
mod sse;
mod titles;
mod tray;

/// The transparent boot / reconnect page is bundled as `frontendDist`. This is
/// the URL we navigate back to when the backend disappears. Tauri serves it on
/// the wry asset protocol (http://tauri.localhost in dev, tauri://localhost in
/// release on non-Windows).
#[cfg(debug_assertions)]
const BOOT_PAGE_URL: &str = "http://tauri.localhost/index.html";
#[cfg(not(debug_assertions))]
const BOOT_PAGE_URL: &str = "tauri://localhost/index.html";

const DEFAULT_URL: &str = "http://127.0.0.1:3080";

/// State shared about the managed `dsh web` child process and its served URL.
struct DshState {
    child: Mutex<Option<Child>>,
    url: Mutex<Option<String>>,
    spawning: Mutex<bool>,
    loaded: AtomicBool,
}

#[derive(serde::Serialize, Clone)]
struct Status {
    running: bool,
    reconnecting: bool,
    url: Option<String>,
    detail: String,
}

fn parse_url_from_line(line: &str) -> Option<String> {
    // dsh prints e.g. `dsh web: http://127.0.0.1:3080`
    let idx = line.find("http://")?;
    let mut rest = line[idx..].trim().to_string();
    // Strip trailing whitespace / punctuation that may trail the URL.
    rest = rest
        .trim_end_matches(|c: char| c.is_whitespace() || c == ')' || c == ']' || c == '}' || c == '.')
        .to_string();
    if rest.contains("://") {
        Some(rest)
    } else {
        None
    }
}

/// Check whether the server is actually reachable at `url`.
fn check_reachable(url: &str) -> bool {
    let host_port = url
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/') // strip any path portion
        .next()
        .unwrap_or(DEFAULT_URL.trim_start_matches("http://"));

    let mut parts = host_port.rsplitn(2, ':');
    let port = parts.next().and_then(|p| p.parse::<u16>().ok());
    let host = parts.next();
    let addr: SocketAddr = match (host, port) {
        (Some(h), Some(p)) => match format!("{h}:{p}").parse() {
            Ok(a) => a,
            Err(_) => return false,
        },
        _ => match format!("{host_port}:3080").parse() {
            Ok(a) => a,
            Err(_) => return false,
        },
    };
    std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(400)).is_ok()
}

fn set_spawning(app: &AppHandle, value: bool) {
    if let Some(state) = app.try_state::<DshState>() {
        if let Ok(mut guard) = state.spawning.lock() {
            *guard = value;
        }
    }
}

fn navigate_win(app: &AppHandle, url: &str) {
    if let Some(win) = app.get_webview_window("main") {
        if let Ok(parsed) = url.parse() {
            let _ = win.navigate(parsed);
        }
    }
}

/// Start `dsh web` as a managed child and stream its stdout to detect the URL.
///
/// Spawning happens on a background thread so we never block the UI. Output
/// lines are echoed to the backend log and scanned for `http://host:port`.
pub(crate) fn spawn_dsh_web(app: AppHandle) {
    std::thread::spawn(move || {
        // First, if a server is already reachable at the default address, we do
        // not need to spawn a duplicate dsh process.
        if check_reachable(DEFAULT_URL) {
            let _ = app.emit(
                "dsh_status",
                Status {
                    running: true,
                    reconnecting: false,
                    url: Some(DEFAULT_URL.to_string()),
                    detail: format!("existing server found at {DEFAULT_URL}"),
                },
            );
            navigate_win(&app, DEFAULT_URL);
            titles::set_base(&app, DEFAULT_URL.to_string());
            titles::refresh(&app);
            sse::subscribe_events(app.clone(), DEFAULT_URL.to_string());
            monitor_backend(app.clone(), DEFAULT_URL.to_string());
            return;
        }

        set_spawning(&app, true);

        // Otherwise launch dsh web and capture stdout so we can parse the real
        // host:port it binds (it may pick a different port if 3080 is taken).
        let mut child = match Command::new("dsh")
            .arg("web")
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(c) => {
                let _ = app.emit(
                    "dsh_status",
                    Status {
                        running: true,
                        reconnecting: true,
                        url: None,
                        detail: "starting `dsh web`…".to_string(),
                    },
                );
                c
            }
            Err(e) => {
                set_spawning(&app, false);
                let _ = app.emit(
                    "dsh_status",
                    Status {
                        running: false,
                        reconnecting: false,
                        url: None,
                        detail: format!("failed to launch `dsh web`: {e}"),
                    },
                );
                return;
            }
        };

        // Attach stdout pipe and stash the child handle for later kill.
        if let Some(stdout) = child.stdout.take() {
            if let Some(state) = app.try_state::<DshState>() {
                if let Ok(mut guard) = state.child.lock() {
                    *guard = Some(child);
                }
            }

            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(text) => {
                        let _ = app.emit("dsh_log", text.clone());
                        if let Some(url) = parse_url_from_line(&text) {
                            set_spawning(&app, false);
                            let _ = app.emit(
                                "dsh_status",
                                Status {
                                    running: true,
                                    reconnecting: false,
                                    url: Some(url.clone()),
                                    detail: text.trim().to_string(),
                                },
                            );
                            navigate_win(&app, &url);
                            if let Some(state) = app.try_state::<DshState>() {
                                if let Ok(mut guard) = state.url.lock() {
                                    *guard = Some(url.clone());
                                }
                            }
                            // Subscribe to dsh's WebSocket event stream for
                            // native notifications, then keep watching
                            // connectivity.
                            titles::set_base(&app, url.clone());
                            titles::refresh(&app);
                            sse::subscribe_events(app.clone(), url.clone());
                            monitor_backend(app.clone(), url);
                            return;
                        }
                    }
                    Err(_) => break,
                }
            }
        } else {
            // No piped stdout (shouldn't happen): keep the child and just poll
            // the default address as a fallback probe.
            if let Some(state) = app.try_state::<DshState>() {
                if let Ok(mut guard) = state.child.lock() {
                    *guard = Some(child);
                }
            }
            monitor_backend(app.clone(), DEFAULT_URL.to_string());
        }
    });
}

/// Long-lived watcher: poll the backend URL and bounce the webview between the
/// live DSH UI and the local boot/reconnect page as availability changes.
fn monitor_backend(app: AppHandle, url: String) {
    std::thread::spawn(move || {
        let mut was_up = check_reachable(&url);
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let up = check_reachable(&url);

            if up && !was_up {
                // Recovered -> return to the DSH UI.
                let _ = app.emit(
                    "dsh_status",
                    Status {
                        running: true,
                        reconnecting: false,
                        url: Some(url.clone()),
                        detail: "connection restored".to_string(),
                    },
                );
                navigate_win(&app, &url);
            } else if !up && was_up && app.state::<DshState>().loaded.load(Ordering::SeqCst) {
                // Just went down -> clear the known URL and drop back to the
                // local boot/reconnect page.
                if let Some(state) = app.try_state::<DshState>() {
                    if let Ok(mut guard) = state.url.lock() {
                        *guard = None;
                    }
                }
                let _ = app.emit(
                    "dsh_status",
                    Status {
                        running: false,
                        reconnecting: false,
                        url: None,
                        detail: "connection lost — reconnect to the backend".to_string(),
                    },
                );
                navigate_win(&app, BOOT_PAGE_URL);
            }
            was_up = up;
        }
    });
}

/// Command invoked from the loader UI to (re)start the backend.
#[tauri::command]
fn start_dsh(app: AppHandle) {
    spawn_dsh_web(app);
}

/// Command that reports the active UI language ("zh" or "en") to the frontend.
#[tauri::command]
fn get_lang() -> &'static str {
    i18n::lang_code()
}

/// Command to retrieve current status (used on page load / reconnect).
#[tauri::command]
fn get_status(state: State<'_, DshState>) -> Status {
    let url = state.url.lock().map(|u| u.clone()).unwrap_or(None);
    let reconnecting = state.spawning.lock().map(|s| *s).unwrap_or(false);
    Status {
        running: url.is_some() || check_reachable(DEFAULT_URL),
        reconnecting,
        url,
        detail: "status".to_string(),
    }
}

/// Menu IDs used by the native menu event handler.
const MENU_RELOAD: &str = "reload";
const MENU_RESTART: &str = "restart";
const MENU_DEVTOOLS: &str = "devtools";
const MENU_QUIT: &str = "quit";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(DshState {
            child: Mutex::new(None),
            url: Mutex::new(None),
            spawning: Mutex::new(false),
            loaded: AtomicBool::new(false),
        })
        .manage(titles::Titles::default())
        .menu(|handle| {
            let reload = MenuItemBuilder::with_id(MENU_RELOAD, t!("menu.reload"))
                .accelerator("CmdOrCtrl+R")
                .build(handle)?;
            let devtools = MenuItemBuilder::with_id(MENU_DEVTOOLS, t!("menu.toggle_devtools"))
                .accelerator("CmdOrCtrl+Alt+I")
                .build(handle)?;
            let restart = MenuItemBuilder::with_id(MENU_RESTART, t!("menu.restart"))
                .accelerator("CmdOrCtrl+Shift+R")
                .build(handle)?;
            let quit = MenuItemBuilder::with_id(MENU_QUIT, t!("menu.quit"))
                .accelerator("CmdOrCtrl+Q")
                .build(handle)?;

            let view = SubmenuBuilder::new(handle, "View")
                .item(&reload)
                .item(&devtools)
                .build()?;
            // A native "Edit" menu restores the standard text-editing keyboard
            // shortcuts (Cmd/Ctrl+C, V, A, Z, Y …) that are otherwise lost when
            // a custom menu replaces the default one.
            let edit = SubmenuBuilder::new(handle, "Edit")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;
            let file = SubmenuBuilder::new(handle, "File")
                .item(&restart)
                .separator()
                .item(&quit)
                .build()?;

            MenuBuilder::new(handle)
                .items(&[&file, &edit, &view])
                .build()
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_RELOAD => {
                // Reload the current page: bounce back to the boot page then
                // back to the live URL, or simply re-navigate if we know it.
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.reload();
                }
            }
            MENU_RESTART => spawn_dsh_web(app.clone()),
            MENU_DEVTOOLS => {
                if let Some(win) = app.get_webview_window("main") {
                    if win.is_devtools_open() {
                        win.close_devtools();
                    } else {
                        win.open_devtools();
                    }
                }
            }
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .setup(|app| {
            // Mark the app as "loaded" so the monitor knows we've reached a
            // stable webview and may bounce it back to the boot page.
            if let Some(state) = app.try_state::<DshState>() {
                state.loaded.store(true, Ordering::SeqCst);
            }
            tray::setup(app.handle())?;
            let handle = app.handle().clone();
            spawn_dsh_web(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the main window hides it to the tray instead of quitting;
            // use the tray menu (or Cmd+Q) to fully exit.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let app = window.app_handle();
                    // Only hide if a tray was successfully set up.
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.hide();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![start_dsh, get_status, get_lang])
        .run(tauri::generate_context!())
        .expect("error while running dsh thin desktop");
}

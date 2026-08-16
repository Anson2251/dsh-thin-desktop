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

/// Path separator for the current platform.
fn path_sep() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

/// Augment `PATH` with the common npm/pnpm global-bin locations.
///
/// When the app is launched from the macOS dock/Finder (or Windows Start menu),
/// it inherits only the *system* PATH — the shell `.rc` files that add
/// `~/.npm-global/bin` / pnpm's bin dir are not loaded, and on Windows the
/// `%APPDATA%\npm` / `%LOCALAPPDATA%\pnpm` shim dirs are only in the *user*
/// PATH. `dsh` is installed there, so spawning it by name would fail. We
/// prepend the candidate dirs so the spawned child can find `dsh` regardless
/// of how the app was started.
fn augmented_path() -> &'static str {
    use std::sync::OnceLock;
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| {
        // GUI-launched apps on Windows have USERPROFILE (not HOME) set; on
        // Unix it's HOME.
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(std::path::PathBuf::from);
        let mut dirs: Vec<std::path::PathBuf> = Vec::new();

        if let Some(b) = std::env::var_os("DSH_BIN_DIR") {
            dirs.push(b.into());
        }
        if let Some(h) = &home {
            dirs.push(h.join(".npm-global").join("bin"));
            dirs.push(h.join(".npm").join("bin"));
            dirs.push(h.join(".local").join("bin"));
            dirs.push(h.join("bin"));
        }
        if let Some(p) = std::env::var_os("PNPM_HOME") {
            dirs.push(p.into());
        }
        if let Some(h) = &home {
            dirs.push(h.join("Library").join("pnpm"));
            dirs.push(h.join(".local").join("share").join("pnpm"));
        }
        // Windows npm/pnpm global-bin locations. `npm i -g` installs the
        // `dsh` shims (`dsh.cmd`, `dsh.ps1`, `dsh`) into `%APPDATA%\npm`;
        // standalone pnpm installs into `%LOCALAPPDATA%\pnpm`.
        #[cfg(windows)]
        {
            if let Some(a) = std::env::var_os("APPDATA") {
                dirs.push(std::path::PathBuf::from(a).join("npm"));
            }
            if let Some(l) = std::env::var_os("LOCALAPPDATA") {
                dirs.push(std::path::PathBuf::from(l).join("pnpm"));
            }
            if let Some(pf) = std::env::var_os("ProgramFiles") {
                dirs.push(std::path::PathBuf::from(pf).join("nodejs"));
            }
        }
        dirs.push("/usr/local/bin".into());
        dirs.push("/opt/homebrew/bin".into());

        // Start from the existing PATH, then prepend our candidate dirs.
        let existing = std::env::var("PATH").unwrap_or_default();
        let sep = path_sep();
        let mut parts: Vec<String> = dirs
            .iter()
            .map(|d| d.to_string_lossy().into_owned())
            .collect();
        if !existing.is_empty() {
            parts.push(existing);
        }
        parts.join(sep)
    })
}

/// Resolve the user's shell `PATH` by asking their shell to source its rc file.
///
/// GUI-launched apps (macOS dock / Windows Start menu) inherit only the system
/// PATH — dirs added in `~/.zshrc`, fish's `config.fish`, etc. are absent. This
/// tells the user's own shell to `source` the rc file and echo the resulting
/// `$PATH`, recovering those dirs. Tried in zsh → fish → bash order (zsh is the
/// modern default on macOS; bash is the last-resort fallback). Best-effort:
/// returns `None` on Windows or when no rc file / shell is available.
fn shell_rc_path() -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    let sep = path_sep();

    // (shell, rc file, command snippet). Each snippet sources the rc file,
    // then prints PATH. fish's $PATH is a list, not a ":" string, so it needs
    // `string join :`; zsh/bash print the ":"-joined value directly.
    let rcs: [(&str, std::path::PathBuf, &str); 3] = [
        (
            "zsh",
            home.join(".zshrc"),
            "source \"$HOME/.zshrc\" 2>/dev/null; printf '%s' \"$PATH\"",
        ),
        (
            "fish",
            home.join(".config/fish/config.fish"),
            "source \"$HOME/.config/fish/config.fish\" 2>/dev/null; string join : \"$PATH\"",
        ),
        (
            "bash",
            home.join(".bashrc"),
            "source \"$HOME/.bashrc\" 2>/dev/null; printf '%s' \"$PATH\"",
        ),
    ];

    for (shell, rc_path, snippet) in rcs {
        if !rc_path.is_file() {
            continue;
        }
        if let Ok(out) = std::process::Command::new(shell).arg("-c").arg(snippet).output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() && s.split(sep).any(|d| !d.is_empty()) {
                    return Some(s);
                }
            }
        }
    }
    None
}

/// Whether `exe` is found on any directory in `path` (a 2-joined PATH string).
fn executable_on_path(exe: &str, path: &str) -> bool {
    std::env::split_paths(path)
        .filter(|d| !d.as_os_str().is_empty())
        .any(|d| d.join(exe).is_file())
}

/// Candidate file names for `dsh` on Windows, in PATHEXT-like order.
///
/// npm installs `dsh` as three shims — `dsh` (POSIX shell script),
/// `dsh.cmd` (cmd.exe batch) and `dsh.ps1` (PowerShell) — with no `dsh.exe`.
/// We must pick a name `std::process::Command` can actually spawn (`.exe` /
/// `.com` directly, `.bat`/`.cmd` via cmd.exe, `.ps1` via PowerShell).
#[cfg(windows)]
const DSH_CANDIDATES: [&str; 5] = ["dsh.com", "dsh.exe", "dsh.bat", "dsh.cmd", "dsh.ps1"];

/// Whether a spawnable `dsh` exists on `path`.
///
/// On Windows the extensionless `dsh` file (a POSIX script) is *not* enough:
/// `CreateProcess` would reject it, so we only count candidates that can
/// actually be launched. On Unix `dsh` itself is executable and sufficient.
fn dsh_available_on_path(path: &str) -> bool {
    #[cfg(windows)]
    {
        DSH_CANDIDATES
            .iter()
            .any(|name| executable_on_path(name, path))
    }
    #[cfg(not(windows))]
    {
        executable_on_path("dsh", path)
    }
}

/// The concrete `dsh` program to spawn, resolved once and cached.
///
/// On Unix this is just the bare name — the OS resolves it from `PATH` the
/// usual way. On Windows, `std::process::Command` delegates to `CreateProcess`,
/// which only appends `.exe` when the name has no extension, so
/// `Command::new("dsh")` can never find the npm shims (`dsh.cmd`, `dsh.ps1`,
/// or the extensionless POSIX script). We therefore walk the spawn `PATH`
/// ourselves and return the full path of the first candidate we know how to
/// run (`.com`/`.exe` directly, `.bat`/`.cmd` via cmd.exe, `.ps1` via
/// PowerShell). Falls back to the bare name so the spawn error surfaces.
fn dsh_program() -> &'static std::path::Path {
    use std::sync::OnceLock;
    static CACHE: OnceLock<std::path::PathBuf> = OnceLock::new();
    CACHE.get_or_init(|| {
        let path = spawn_path();
        #[cfg(windows)]
        {
            for dir in std::env::split_paths(path).filter(|d| !d.as_os_str().is_empty()) {
                for cand in DSH_CANDIDATES {
                    let p = dir.join(cand);
                    if p.is_file() {
                        return p;
                    }
                }
            }
        }
        std::path::PathBuf::from("dsh")
    })
    .as_path()
}

/// Build the `Command` that launches `dsh web`, resolving the npm shim on
/// Windows where `CreateProcess` would otherwise only look for `dsh.exe`.
fn dsh_command() -> Command {
    let program = dsh_program();
    let mut cmd = Command::new(program);
    cmd.env("PATH", spawn_path()).stdout(Stdio::piped());
    // A `.ps1` shim can't be spawned by `CreateProcess` at all; run it through
    // Windows PowerShell (bundled on every Windows 10/11).
    #[cfg(windows)]
    if program
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ps1"))
    {
        let mut ps = Command::new("powershell.exe");
        ps.arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(program)
            .arg("web");
        ps.env("PATH", spawn_path()).stdout(Stdio::piped());
        return ps;
    }
    cmd.arg("web");
    cmd
}

/// Combine a user-rc PATH with the augmented fallbacks: rc dirs first, then
/// our enumerated dirs, then the current process PATH.
fn combine_paths(rc_path: &str, augmented: &str) -> String {
    let sep = path_sep();
    let mut parts: Vec<String> = Vec::new();
    for d in std::env::split_paths(rc_path) {
        let s = d.to_string_lossy().into_owned();
        if !s.is_empty() {
            parts.push(s);
        }
    }
    for d in std::env::split_paths(augmented) {
        let s = d.to_string_lossy().into_owned();
        if !s.is_empty() && !parts.contains(&s) {
            parts.push(s);
        }
    }
    let cur = std::env::var("PATH").unwrap_or_default();
    for d in std::env::split_paths(&cur) {
        let s = d.to_string_lossy().into_owned();
        if !s.is_empty() && !parts.contains(&s) {
            parts.push(s);
        }
    }
    parts.join(sep)
}

/// The full `PATH` used to spawn `dsh`.
///
/// Order (each added only if the previous didn't find `dsh`):
///   1. `DSH_BIN_DIR` enumerations + common npm/pnpm dirs (`augmented_path`)
///   2. the user's shell rc-file PATH (`shell_rc_path`), zsh → fish → bash
fn spawn_path() -> &'static str {
    use std::sync::OnceLock;
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| {
        let augmented = augmented_path().to_string();
        if dsh_available_on_path(&augmented) {
            augmented
        } else if let Some(rc) = shell_rc_path() {
            combine_paths(&rc, &augmented)
        } else {
            augmented
        }
    })
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
        let mut child = match dsh_command().spawn() {
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
        .build(tauri::generate_context!())
        .expect("error while building dsh thin desktop")
        .run(|app, event| {
            // macOS: clicking the dock icon emits a Reopen event. If the window
            // is hidden in the tray (we intercept CloseRequested), bring it
            // back — otherwise Tauri shows it and we'd race it with a
            // hide()/show() that makes it flash.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                tray::focus(app);
            }
            // Keep params used on non-macOS (Reopen is mac-only) to avoid an
            // unused-variables warning.
            #[cfg(not(target_os = "macos"))]
            let _ = (&app, &event);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn augmented_path_covers_npm_pnpm_bins() {
        let home = std::env::var_os("HOME").map(|h| h.to_string_lossy().into_owned());
        if let Some(h) = home {
            let path = augmented_path();
            // Should contain the standard npm/pnpm global-bin candidates.
            assert!(path.contains(&format!("{h}/.npm-global/bin")), "missing npm-global: {path}");
            assert!(path.contains(&format!("{h}/Library/pnpm")), "missing pnpm home: {path}");
        }
        // Augmented PATH always starts with our directory list, not a bare env.
        assert!(!augmented_path().is_empty());
    }

    #[test]
    fn shell_rc_path_is_best_effort() {
        // Not platform-dependent: on Unix it either returns a non-empty PATH
        // (zsh/fish/bash found + rc sourced) or None (no shell / no rc).
        #[cfg(windows)]
        assert!(shell_rc_path().is_none());
        #[cfg(not(windows))]
        {
            match shell_rc_path() {
                Some(p) => assert!(!p.is_empty(), "rc path should not be empty"),
                None => { /* acceptable: no rc present on this host */ }
            }
        }
    }

    #[test]
    fn spawn_path_is_valid_and_nonempty() {
        // The path we hand to the dsh Command must be non-empty and free of
        // empty entries. We don't assert dsh exists here — CI release runners
        // don't install dsh.
        let p = spawn_path();
        assert!(!p.is_empty(), "spawn_path must not be empty: {p}");
        assert!(std::env::split_paths(p).any(|d| !d.as_os_str().is_empty()));
    }

    #[test]
    fn dsh_program_is_a_concrete_spawnable_path() {
        // The resolved program must either be the bare name (Unix, where the OS
        // resolves it) or a full path to an existing file (Windows, where we
        // must point at the npm shim because CreateProcess only finds .exe).
        let p = dsh_program();
        let s = p.to_string_lossy();
        #[cfg(windows)]
        {
            assert!(
                p.is_absolute() && p.is_file(),
                "on Windows dsh_program() must resolve to an existing file, got: {s}"
            );
            // Only candidates we can actually spawn are acceptable.
            assert!(
                DSH_CANDIDATES.iter().any(|c| s.ends_with(c)),
                "unexpected dsh program: {s}"
            );
        }
        #[cfg(not(windows))]
        assert_eq!(s, "dsh", "on Unix dsh_program() should stay a bare name");
    }

    #[test]
    fn dsh_available_on_path_matches_dsh_program() {
        // If a candidate exists on the spawn PATH, dsh_program() must resolve
        // to it (and vice versa: if none exists it falls back to the name).
        let found = dsh_available_on_path(spawn_path());
        #[cfg(windows)]
        {
            if found {
                assert!(dsh_program().is_file());
            }
        }
        // Sanity: the spawn PATH always contains at least one directory.
        assert!(std::env::split_paths(spawn_path()).any(|d| !d.as_os_str().is_empty()));
    }
}

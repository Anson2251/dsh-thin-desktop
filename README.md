# DSH Thin Desktop

> **A native desktop window for DeepSeek Harness (`dsh web`).**
>
> 🇬🇧 English · [🇨🇳 中文](README-zh.md)

If you use the DeepSeek Harness web UI in a browser tab, this app gives you a
proper desktop experience instead: its own window, a menu bar, a system-tray
icon, and native notifications when dsh needs your attention.

---

## What is this?

**DSH Thin Desktop** is a thin wrapper around the DeepSeek Harness web UI. It
simply:

- **Starts `dsh web`** for you (or connects to one you're already running).
- Shows it in a **native window**, with a menu bar and a **tray icon**.
- Sends you **desktop notifications** when the agent needs a decision.

---

## Requirements

- **DeepSeek Harness (`dsh`)** already installed on your machine — the `dsh`
  command must be available on your `PATH`.
- A 64-bit desktop OS (macOS / Windows / Linux).
- **Only to build from source:** Rust toolchain. (See [Build from source](#build-from-source).)

---

## Install / Run

### Already have a built app?

Just launch it. On first launch the app starts `dsh web` automatically (or
reuses one that's already running) and loads it in a window.

### Build from source

```bash
# run in development mode
cd src-tauri && cargo run

# build a distributable release bundle (.app / .dmg on macOS)
cargo build --release
# or, if you have the Tauri CLI:
cargo tauri build
```

> No Node.js or frontend build step is required.

---

## Using it

### The window

- The window shows the live DeepSeek Harness web UI.
- The **window title** shows the name of the active session (falling back to a
  short session id when no title is set yet).
- Closing the window **hides** it to the tray instead of quitting — the app
  keeps running in the background.

### Menu bar

| Menu  | Item                         | Shortcut                        | What it does                              |
|-------|------------------------------|---------------------------------|-------------------------------------------|
| File  | Start / Restart Backend      | `Cmd/Ctrl+Shift+R`              | (Re)start the `dsh web` backend           |
| File  | Quit                         | `Cmd/Ctrl+Q`                    | Quit the app                              |
| Edit  | Undo / Redo / Cut / Copy / Paste / Select All | `Cmd/Ctrl+Z/Y/X/C/V/A` | Standard text-editing shortcuts in the web UI |
| View  | Reload                       | `Cmd/Ctrl+R`                    | Reload the current page                   |
| View  | Toggle DevTools              | `Cmd/Ctrl+Alt+I`                | Open/close the webview developer tools    |

### System tray

A tray / menu-bar icon keeps the app one click away:

- **Left-click** the tray icon to show or hide the window.
- **Right-click** for a small menu: Show/Hide, Reload, Restart Backend, Quit.
- Use **Quit** (or `Cmd/Ctrl+Q`) to fully exit (closing the window only hides it).

### Notifications

When the agent needs a human decision, the app shows a native desktop
notification *and* brings the window to the front:

- **`DSH 需要你批准` (approval)** — a tool call or sandbox upgrade needs your
  permission.
- **`DSH 提问` (question)** — the agent is asking you an interactive question.

> **Not seeing notifications?** macOS focuses/Do-Not-Disturb suppress banners
> (only a badge shows). Turn off Focus and allow notifications for the app the
> first time macOS asks. Also note that in **dev mode** (`cargo run`) notifications
> are shown under the *Terminal* app's identity; a **signed release build** shows
> them under your app's own name.

### Language

The app's own UI strings (menu, tray, notifications, boot page) are localized
between **English** and **中文**. The language is read from environment
variables — classic `LANG`/`LC_ALL`, with `DSH_LANG` as an explicit override:

```bash
# force Chinese (or just set LANG=zh_CN.UTF-8)
DSH_LANG=zh_CN.UTF-8 ./dsh-thin-desktop

# force English
DSH_LANG=en_US.UTF-8 ./dsh-thin-desktop
```

Internally this is a tiny `t!` macro (see `src/i18n.rs`) similar in spirit to
`vue-i18n`: compile-time keys, runtime lookup by language, `{}` placeholders.

---

## Troubleshooting

| Symptom | Likely cause & fix |
|---|---|
| Opens a "starting / reconnect" page and asks to retry | `dsh web` didn't start. Make sure `dsh` is on your `PATH`, then press **Start / Restart Backend**. |
| No notification banner | macOS Focus/Do-Not-Disturb is on, or the app hasn't been allowed to notify yet — allow it once when prompted. |
| Another app already uses port 3080 | The client follows whatever address `dsh web` reports and loads that instead. |

---

## For developers (brief)

- **Architecture:** a native Tauri webview loads the dsh web URL directly (no
  iframe). The Rust side starts `dsh web`, captures its output to detect the
  host/port, and monitors connectivity.
- **Notifications:** the app subscribes to dsh's real-time WS event stream
  (`/api/events.mux`) via WebSocket and natively notifies on approval/question
  events.
- **Layout:** `src/lib.rs` (lifecycle), `src/sse.rs` (event stream),
  `src/notify.rs` (notifications), `src/titles.rs` (session titles),
  `src/tray.rs` (tray), `src/i18n.rs` (i18n macro).
- Live smoke tests: `tests/ws_smoke.rs`, `tests/title_smoke.rs`.

---

## License

MIT

//! Session title resolution.
//!
//! The `session/subscribed` mux frame only carries a `sessionId`. To show a
//! human-readable name in the window title we call dsh's `session.list` RPC
//! and read `projections.values.title` for each session, caching the result.
//!
//! RPC wire format (same as the WS frames): POST `/api/session.list` with a
//! `{type:"client-request",rpcId,method:"session.list",payload:{}}` body; the
//! synchronous `server-response` returns `items[]`.

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;

/// Managed cache of session titles, keyed by session id.
pub struct Titles {
    base_url: Mutex<Option<String>>,
    by_session: Mutex<HashMap<String, Option<String>>>,
}

impl Default for Titles {
    fn default() -> Self {
        Self {
            base_url: Mutex::new(None),
            by_session: Mutex::new(HashMap::new()),
        }
    }
}

#[derive(serde::Deserialize)]
struct ListResponse {
    #[serde(default)]
    result: Option<ListResult>,
}

#[derive(serde::Deserialize)]
struct ListResult {
    #[serde(default)]
    value: Option<ListValue>,
}

#[derive(serde::Deserialize)]
struct ListValue {
    #[serde(default)]
    items: Vec<SessionRow>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionRow {
    session_id: String,
    #[serde(default)]
    projections: Option<Projections>,
}

#[derive(serde::Deserialize)]
struct Projections {
    #[serde(default)]
    values: HashMap<String, serde_json::Value>,
}

/// Remember the backend base URL (called once when the WS stream connects).
pub fn set_base(app: &tauri::AppHandle, base_url: String) {
    if let Some(state) = app.try_state::<Titles>() {
        if let Ok(mut g) = state.base_url.lock() {
            *g = Some(base_url);
        }
    }
}

/// Fetch `session.list` and repopulate the title cache (best effort).
pub fn refresh(app: &tauri::AppHandle) {
    let base = match app.try_state::<Titles>() {
        Some(st) => match st.base_url.lock() {
            Ok(g) => g.clone(),
            Err(_) => return,
        },
        None => return,
    };
    let Some(base) = base else { return };

    let url = format!("{base}/api/session.list");
    let rpc_id = format!(
        "title-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );

    let body = serde_json::json!({
        "type": "client-request",
        "rpcId": rpc_id,
        "method": "session.list",
        "payload": {},
    });

    let mut resp = match ureq::post(&url)
        .header("Content-Type", "application/json")
        .send_json(body)
    {
        Ok(r) => r,
        Err(_) => return,
    };
    let text = match resp.body_mut().read_to_string() {
        Ok(t) => t,
        Err(_) => return,
    };
    let parsed: ListResponse = match serde_json::from_str(&text) {
        Ok(p) => p,
        Err(_) => return,
    };

    if let Some(items) = parsed.result.and_then(|r| r.value).map(|v| v.items) {
        if let Some(st) = app.try_state::<Titles>() {
            if let Ok(mut by_session) = st.by_session.lock() {
                for row in items {
                    let title = match row.projections {
                        Some(p) => p
                            .values
                            .get("title")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        None => None,
                    };
                    by_session.insert(row.session_id, title);
                }
            }
        }
    }
}

/// Set the window title for a session id, using its cached human title when
/// known, otherwise the short-id fallback. If the session id is not yet in the
/// cache, kicks off a refresh once so a later frame can show the real title.
pub fn apply_to_window(app: &tauri::AppHandle, session_id: &str) {
    let state = match app.try_state::<Titles>() {
        Some(s) => s,
        None => return,
    };

    let unknown = !state
        .by_session
        .lock()
        .map(|g| g.contains_key(session_id))
        .unwrap_or(false);
    if unknown {
        refresh(app);
    }

    let cached = state
        .by_session
        .lock()
        .ok()
        .and_then(|g| g.get(session_id).cloned())
        .flatten()
        .filter(|t| !t.is_empty());

    let title = cached.unwrap_or_else(|| crate::notify::short(session_id));

    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_title(&crate::t!("win.title", title));
    }
}

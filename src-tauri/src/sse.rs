//! Subscribe to dsh's real-time event stream and relay interesting frames to
//! the Tauri notification system.
//!
//! The dsh `/api/events.mux` endpoint is physically a **WebSocket** (a plain
//! HTTP SSE GET to it returns `426 Upgrade Required`). The handshake uses
//! `Upgrade: websocket`, after which the server streams JSON `server-request`
//! envelopes whose `payload` is a mux frame (e.g. `approval/requested`,
//! `question/requested`).

use futures_util::StreamExt;
use tauri::Emitter;

/// Subscribe to the dsh event stream and stay connected forever, reconnecting
/// with capped exponential backoff so the client survives transient server
/// restarts. Runs on the async runtime provided by Tauri (tokio).
pub fn subscribe_events(app: tauri::AppHandle, base_url: String) {
    tauri::async_runtime::spawn(async move {
        // Convert http(s) -> ws(s) for the websocket handshake.
        let ws_url = if let Some(rest) = base_url.strip_prefix("http://") {
            format!("ws://{rest}/api/events.mux")
        } else if let Some(rest) = base_url.strip_prefix("https://") {
            format!("wss://{rest}/api/events.mux")
        } else {
            format!("{base_url}/api/events.mux")
        };

        let mut backoff = std::time::Duration::from_millis(500);
        loop {
            match run_once(&ws_url, &app).await {
                Ok(()) => { /* stream closed cleanly; reconnect after a short pause */ }
                Err(e) => {
                    let _ = app.emit("dsh_sse_error", e.to_string());
                }
            }
            tokio::time::sleep(backoff).await;
            backoff = std::cmp::min(backoff * 2, std::time::Duration::from_secs(15));
        }
    });
}

async fn run_once(ws_url: &str, app: &tauri::AppHandle) -> Result<(), String> {
    let (mut stream, _resp) = tokio_tungstenite::connect_async(ws_url)
        .await
        .map_err(|e| format!("ws connect failed: {e}"))?;

    while let Some(msg) = stream.next().await {
        let msg = msg.map_err(|e| format!("ws stream error: {e}"))?;
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            handle_server_message(app, &text);
        }
    }
    Err("ws stream closed".to_string())
}

/// Parse a dsh `server-request` envelope and forward its payload frame to the
/// notifier. Unknown/malformed frames are ignored.
fn handle_server_message(app: &tauri::AppHandle, raw: &str) {
    let Ok(envelope) = serde_json::from_str::<ServerRequest>(raw) else {
        return;
    };
    if envelope.kind != "server-request" {
        return;
    }
    if let Some(frame) = envelope.payload {
        if let Ok(json) = serde_json::to_string(&frame) {
            crate::notify::handle_frame(app, &json);
        }
    }
}

/// Minimal projection of a dsh server→client RPC envelope. The payload slot
/// holds the nested mux frame we care about (`approval/requested`,
/// `question/requested`, …).
#[derive(serde::Deserialize, Debug)]
struct ServerRequest {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    payload: Option<serde_json::Value>,
}

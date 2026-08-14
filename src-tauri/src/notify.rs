use serde::Deserialize;
use tauri_plugin_notification::NotificationExt;

/// A single frame on the dsh `/api/events.mux` SSE stream.
///
/// We only need a few fields of the `MuxFrame` union. Unknown fields (and
/// whole unknown frame kinds) are ignored so the client keeps working when
/// dsh adds new frame types.
#[derive(Debug, Deserialize)]
struct MuxFrame {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "toolName")]
    tool_name: Option<String>,
    reason: Option<String>,
    questions: Option<Vec<Question>>,
}

#[derive(Debug, Deserialize, Clone)]
struct Question {
    #[serde(default)]
    question: Option<String>,
}

/// Emit a native notification for one parsed mux frame. Only "worth it" frames
/// (permission approvals and model questions, which block on human input) are
/// surfaced; the high-volume agent `session/event` stream is intentionally not
/// relayed to avoid notification spam.
pub fn handle_frame(app: &tauri::AppHandle, raw: &str) {
    let frame: MuxFrame = match serde_json::from_str(raw) {
        Ok(f) => f,
        Err(_) => return, // not a frame we can / need to handle
    };

    match frame.kind.as_str() {
        "approval/requested" => {
            notify(
                app,
                crate::t!("notify.approval.title"),
                &approval_body(
                    frame.tool_name.as_deref(),
                    frame.reason.as_deref(),
                    frame.session_id.as_deref(),
                ),
            );
        }
        "question/requested" => {
            // Surface the first question (there is usually exactly one).
            let first = frame
                .questions
                .and_then(|qs| qs.first().map(|q| q.question.clone().unwrap_or_default()));
            let mut body = first
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| crate::t!("notify.question.fallback").to_string());
            if let Some(sid) = frame.session_id.as_deref() {
                body.push_str(&format!("\n({})", short(sid)));
            }
            notify(app, crate::t!("notify.question.title"), &body);
        }
        "approval/resolved" => {
            // Closing the loop is informative but not worth a banner by default.
        }
        "session/subscribed" => {
            // Reflect the active session in the window title (human-readable
            // title when available, falling back to the short id).
            if let Some(sid) = frame.session_id.as_deref() {
                crate::titles::apply_to_window(app, sid);
            }
        }
        _ => {} // session/event, session/queue, question/resolved … ignored
    }
}

/// Compact 8-char suffix of a session id for display.
pub(crate) fn short(id: &str) -> String {
    let len = id.len();
    let s: String = id.chars().skip(len.saturating_sub(8)).collect();
    let s = if s.is_empty() { id.to_string() } else { s };
    if s.len() > 8 {
        format!("…{}", &s[s.len() - 8..])
    } else {
        s
    }
}

fn approval_body(tool: Option<&str>, reason: Option<&str>, session: Option<&str>) -> String {
    let mut body = match tool {
        Some(t) => crate::t!("notify.approval.tool", t),
        None => crate::t!("notify.approval.generic").to_string(),
    };
    if let Some(r) = reason.filter(|r| !r.is_empty()) {
        body.push('\n');
        body.push_str(r);
    }
    if let Some(sid) = session {
        body.push_str(&format!("\n({})", short(sid)));
    }
    body
}

fn notify(app: &tauri::AppHandle, title: &str, body: &str) {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .ok();
}

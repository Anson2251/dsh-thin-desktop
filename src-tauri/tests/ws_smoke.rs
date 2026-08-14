use futures_util::StreamExt;

#[tokio::test(flavor = "multi_thread")]
async fn dsh_events_mux_is_websocket_and_parseable() {
    let url = "ws://127.0.0.1:3080/api/events.mux";
    eprintln!("connecting {url}");
    let (mut stream, resp) = tokio_tungstenite::connect_async(url)
        .await
        .expect("ws connect");
    eprintln!("connected, resp status {:?}", resp.status());
    let mut parsed = 0usize;
    let mut kinds = Vec::new();
    for _ in 0..25 {
        let item = tokio::time::timeout(std::time::Duration::from_secs(10), stream.next()).await;
        match item {
            Ok(Some(Ok(msg))) => {
                if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                    eprintln!("TEXT: {}", if text.len()>120 { &text[..120] } else { &text });
                    if let Ok(env) = serde_json::from_str::<Env>(&text) {
                        let _ = &env.kind;
                        parsed += 1;
                        if let Some(p) = env.payload {
                            if let Some(kind) = p.get("type").and_then(|v| v.as_str()) {
                                kinds.push(kind.to_string());
                            }
                        }
                    }
                }
            }
            Ok(Some(Err(e))) => { eprintln!("MSG ERR {e}"); break; }
            Ok(None) => { eprintln!("stream ended"); break; }
            Err(_) => { eprintln!("TIMEOUT"); break; }
        }
    }
    println!("parsed {parsed} envelopes; kinds: {kinds:?}");
    assert!(parsed > 0, "expected parseable envelopes");
}

#[derive(serde::Deserialize)]
struct Env {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    payload: Option<serde_json::Value>,
}

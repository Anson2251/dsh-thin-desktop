// Live smoke: hit dsh's session.list exactly the way titles.rs does and verify
// we can extract projections.values.title from a real session row.
#[test]
fn session_list_parses_title() {
    let url = "http://127.0.0.1:3080/api/session.list";
    let body = serde_json::json!({
        "type": "client-request",
        "rpcId": "smoke-title",
        "method": "session.list",
        "payload": {},
    });
    let mut resp = ureq::post(url)
        .header("Content-Type", "application/json")
        .send_json(body)
        .expect("session.list request");
    let text = resp.body_mut().read_to_string().expect("read body");
    #[derive(serde::Deserialize)]
    struct R { result: Option<RV> }
    #[derive(serde::Deserialize)]
    struct RV { value: Option<VV> }
    #[derive(serde::Deserialize)]
    struct VV { items: Vec<Row> }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Row { #[allow(dead_code)] session_id: String, projections: Option<Pj> }
    #[derive(serde::Deserialize)]
    struct Pj { values: std::collections::HashMap<String, serde_json::Value> }

    let r: R = serde_json::from_str(&text).expect("parse");
    let items = r.result.and_then(|x| x.value).map(|v| v.items).unwrap_or_default();
    assert!(!items.is_empty(), "expected at least one session row");
    let titles: Vec<String> = items.iter().filter_map(|row|
        row.projections.as_ref().and_then(|p| p.values.get("title"))
            .and_then(|v| v.as_str()).map(|s| s.to_string())
    ).collect();
    println!("extracted titles: {titles:?}");
    assert!(titles.iter().any(|t| !t.is_empty()), "no non-empty title extracted");
}

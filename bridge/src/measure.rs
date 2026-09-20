//! The dry run is measured by the app on this machine (2026-09-21).
//!
//! A deck carried as html is checked before it is written: the server
//! converts it, renders it through the canvas's own renderer, and needs a
//! browser to say whether the render fits (a slide past its box, a picture
//! box collapsed to a band, a label fused to its number). The server first
//! sent the render to a hosted browser; the founder refused that when the
//! Designless app on the customer's desk is a browser already. So the bridge
//! orchestrates: the dry run goes up once asking for the render
//! (`measure_at: "app"`), the app lays it out over IPC and answers numbers,
//! and the dry run goes up again carrying them (`layout_measure`); the
//! server judges and answers, and that answer is what the agent reads. The
//! render never enters the agent's context: the first answer is only ever
//! returned with its `_bridge` key removed, when the app is not open or the
//! measure fails, and then the answer itself says the layout was not
//! measured. Nothing here can fail a dry run.
use crate::anchored::ipc::{self, IpcResponse};
use serde_json::{json, Value};

/// A `less_canvas_compose` call with `dry_run` set: the one call this module takes.
pub fn dry_run_compose(frame: &Value) -> bool {
    if frame.get("method").and_then(Value::as_str) != Some("tools/call") {
        return false;
    }
    let Some(params) = frame.get("params") else { return false };
    if params.get("name").and_then(Value::as_str) != Some("less_canvas_compose") {
        return false;
    }
    let Some(args) = params.get("arguments") else { return false };
    match args.get("dry_run") {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// The call, asking the server for the render.
pub fn asking_for_the_render(frame: &Value) -> Value {
    let mut out = frame.clone();
    if let Some(args) = out.pointer_mut("/params/arguments").and_then(Value::as_object_mut) {
        args.insert("measure_at".into(), json!("app"));
        args.remove("layout_measure");
    }
    out
}

/// The call, carrying what the app measured.
pub fn carrying_the_measure(frame: &Value, page: &Value, slides: &Value, ms: u64) -> Value {
    let mut out = frame.clone();
    if let Some(args) = out.pointer_mut("/params/arguments").and_then(Value::as_object_mut) {
        args.remove("measure_at");
        args.insert("layout_measure".into(), json!({ "page": page, "slides": slides, "ms": ms }));
    }
    out
}

/// The render the server handed the bridge, when the answer carries one.
pub fn render_in(response: &Value) -> Option<(String, Value)> {
    let m = response.pointer("/result/structuredContent/_bridge/measure")?;
    let html = m.get("html")?.as_str()?.to_string();
    let page = m.get("page")?.clone();
    if html.is_empty() || !page.is_object() {
        return None;
    }
    Some((html, page))
}

/// The answer with the bridge's key removed: nothing under it is the agent's.
pub fn without_bridge(response: &Value) -> Value {
    let mut out = response.clone();
    if let Some(sc) = out.pointer_mut("/result/structuredContent").and_then(Value::as_object_mut) {
        sc.remove("_bridge");
    }
    out
}

/// The app's measure, when the app answered one.
pub async fn measured_by_the_app(html: &str, page: &Value) -> Option<(Value, Value, u64)> {
    let mut client = match ipc::connect().await {
        Ok(c) => c,
        Err(e) => {
            tracing::info!(error = %e, "layout not measured: the app is not open");
            return None;
        }
    };
    match client.measure(html, page).await {
        Ok(IpcResponse::Measure { ok: true, page: Some(p), slides: Some(s), ms, .. }) if s.is_array() => Some((p, s, ms.unwrap_or(0))),
        Ok(IpcResponse::Measure { reason, .. }) => {
            tracing::info!(reason = reason.as_deref().unwrap_or("no reason"), "layout not measured: the app declined");
            None
        }
        Ok(other) => {
            tracing::info!(answer = ?other, "layout not measured: the app answered something else");
            None
        }
        Err(e) => {
            tracing::info!(error = %e, "layout not measured: the app did not answer");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(args: Value) -> Value {
        json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"less_canvas_compose","arguments":args}})
    }

    #[test]
    fn only_a_dry_run_compose_is_taken() {
        assert!(dry_run_compose(&call(json!({"dry_run": true, "payload": {}}))));
        assert!(dry_run_compose(&call(json!({"dry_run": "true"}))));
        assert!(!dry_run_compose(&call(json!({"dry_run": false}))));
        assert!(!dry_run_compose(&call(json!({"payload": {}}))));
        let mut other = call(json!({"dry_run": true}));
        other["params"]["name"] = json!("less_canvas_status");
        assert!(!dry_run_compose(&other));
        assert!(!dry_run_compose(&json!({"method":"tools/list"})));
    }

    #[test]
    fn the_first_call_asks_for_the_render_and_the_second_carries_the_measure() {
        let frame = call(json!({"dry_run": true, "brand_slug": "b", "payload": {"html": "<html/>"}}));
        let first = asking_for_the_render(&frame);
        assert_eq!(first["params"]["arguments"]["measure_at"], json!("app"));
        assert_eq!(first["params"]["arguments"]["payload"]["html"], json!("<html/>"));
        let second = carrying_the_measure(&frame, &json!({"w":1080,"h":1350}), &json!([{"slide":1}]), 412);
        assert!(second["params"]["arguments"].get("measure_at").is_none());
        assert_eq!(second["params"]["arguments"]["layout_measure"]["ms"], json!(412));
        assert_eq!(second["params"]["arguments"]["layout_measure"]["slides"][0]["slide"], json!(1));
        assert_eq!(frame["params"]["arguments"].get("measure_at"), None, "the input is not touched");
    }

    #[test]
    fn the_render_is_read_from_the_bridge_key_and_the_key_never_reaches_the_agent() {
        let r = json!({"jsonrpc":"2.0","id":7,"result":{"content":[{"type":"text","text":"Dry run"}],"structuredContent":{"verdict":"green","_bridge":{"measure":{"html":"<html/>","page":{"w":1080,"h":1350}}}}}});
        let (html, page) = render_in(&r).expect("a render");
        assert_eq!(html, "<html/>");
        assert_eq!(page["h"], json!(1350));
        let stripped = without_bridge(&r);
        assert!(stripped["result"]["structuredContent"].get("_bridge").is_none());
        assert_eq!(stripped["result"]["structuredContent"]["verdict"], json!("green"));
        assert_eq!(stripped["result"]["content"][0]["text"], json!("Dry run"));
        assert!(render_in(&stripped).is_none());
        assert!(render_in(&json!({"result":{"structuredContent":{"_bridge":{"measure":{"html":"","page":{}}}}}})).is_none());
    }
}

//! A series' pages, shown to the agent straight from the app on this machine.
//!
//! When the series check wants members looked at, the Designless app renders
//! each member's pages off screen and lays them out as one sheet, held in its
//! memory. The check's answer names those sheets under `_bridge.look`; the
//! bridge asks the app for them over the local socket and hands them to the
//! agent as pictures inside that same answer. Nothing is written to disk and
//! the person's screen never moves. When the app is closed, too old, or the
//! sheets are gone, the answer says so in words, so the agent answers those
//! members as unseen rather than passing what it never saw. The key never
//! reaches the agent.
use crate::anchored::ipc::{self, IpcResponse, LookSheet};
use serde_json::{json, Value};

/// One sheet request the series check named: the app's request id and the
/// words that say whose pages they are.
#[derive(Debug, Clone, PartialEq)]
pub struct LookAsk {
    pub request_id: String,
    pub label: String,
}

/// The sheets an answer asks the bridge to show, in order. Empty when none.
pub fn asks_in(response: &Value) -> Vec<LookAsk> {
    let Some(list) = response.pointer("/result/structuredContent/_bridge/look").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|a| {
            let request_id = a.get("request_id")?.as_str()?.to_string();
            if request_id.is_empty() {
                return None;
            }
            let label = a.get("label").and_then(Value::as_str).unwrap_or("a member").to_string();
            Some(LookAsk { request_id, label })
        })
        .collect()
}

/// The answer with the pages added after what the server said: for each ask,
/// a line naming whose pages they are and each sheet as a picture; for an ask
/// the app could not answer, a line saying so. The bridge's key is removed.
pub fn with_the_pages(response: &Value, asks: &[LookAsk], got: &Result<Vec<LookSheet>, String>) -> Value {
    let mut out = crate::measure::without_bridge(response);
    let mut added: Vec<Value> = Vec::new();
    for ask in asks {
        let sheets: Vec<&LookSheet> = match got {
            Ok(all) => all.iter().filter(|s| s.request_id == ask.request_id).collect(),
            Err(_) => Vec::new(),
        };
        if sheets.is_empty() {
            let why = match got {
                Err(reason) => reason.clone(),
                Ok(_) => "the app no longer holds them; check again to make them anew".to_string(),
            };
            added.push(json!({ "type": "text", "text": format!("The pages of {} could not be shown here: {}. Answer that member as unseen.", ask.label, why) }));
            continue;
        }
        let n = sheets.len();
        for (i, s) in sheets.iter().enumerate() {
            let which = if n > 1 { format!(" (sheet {} of {})", i + 1, n) } else { String::new() };
            added.push(json!({ "type": "text", "text": format!("Pages of {}{}:", ask.label, which) }));
            added.push(json!({ "type": "image", "data": s.png_base64, "mimeType": "image/png" }));
        }
    }
    if let Some(content) = out.pointer_mut("/result/content").and_then(Value::as_array_mut) {
        content.extend(added);
    } else if let Some(result) = out.get_mut("result").and_then(Value::as_object_mut) {
        result.insert("content".into(), Value::Array(added));
    }
    out
}

/// The sheets of the named requests, from the app. Err carries words the
/// agent can pass on.
pub async fn sheets_from_the_app(request_ids: &[String]) -> Result<Vec<LookSheet>, String> {
    let mut client = match ipc::connect().await {
        Ok(c) => c,
        Err(e) => {
            tracing::info!(error = %e, "pages not shown: the app is not open");
            return Err("the Designless app is not open on this computer".into());
        }
    };
    match client.look_sheets(request_ids).await {
        Ok(IpcResponse::LookSheets { ok: true, sheets, missing, .. }) => {
            if let Some(m) = missing.filter(|m| !m.is_empty()) {
                tracing::info!(missing = ?m, "some pages were no longer held by the app");
            }
            Ok(sheets.unwrap_or_default())
        }
        Ok(IpcResponse::LookSheets { reason, .. }) => {
            Err(format!("the Designless app declined ({})", reason.as_deref().unwrap_or("no reason given")))
        }
        Ok(IpcResponse::Error { reason }) if reason.as_deref() == Some("unknown_op") => {
            Err("this Designless app is too old to show pages; update it".into())
        }
        Ok(other) => {
            tracing::info!(answer = ?other, "pages not shown: the app answered something else");
            Err("the Designless app did not hand the pages over".into())
        }
        Err(e) => {
            tracing::info!(error = %e, "pages not shown: the app did not answer");
            Err("the Designless app did not answer".into())
        }
    }
}

/// The answer as the agent should read it: unchanged when it asks for no
/// pages, otherwise with the pages from the app added and the key removed.
pub async fn shown(response: Value) -> Value {
    let asks = asks_in(&response);
    if asks.is_empty() {
        return response;
    }
    let ids: Vec<String> = asks.iter().map(|a| a.request_id.clone()).collect();
    let got = sheets_from_the_app(&ids).await;
    with_the_pages(&response, &asks, &got)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(look: Value) -> Value {
        json!({"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"2 of 2 not looked at yet."}],"structuredContent":{"job_id":"j","_bridge":{"look":look}}}})
    }
    fn sheet(id: &str, data: &str) -> LookSheet {
        LookSheet { request_id: id.into(), png_base64: data.into() }
    }

    #[test]
    fn the_asks_are_read_in_order_and_a_blank_id_is_skipped() {
        let r = answer(json!([{"request_id":"r1","label":"record-ana"},{"request_id":""},{"request_id":"r2"}]));
        assert_eq!(asks_in(&r), vec![
            LookAsk { request_id: "r1".into(), label: "record-ana".into() },
            LookAsk { request_id: "r2".into(), label: "a member".into() },
        ]);
        assert!(asks_in(&json!({"result":{"content":[]}})).is_empty());
    }

    #[test]
    fn each_sheet_arrives_as_a_picture_after_a_line_saying_whose_pages_they_are() {
        let r = answer(json!([{"request_id":"r1","label":"record-ana"},{"request_id":"r2","label":"the board"}]));
        let asks = asks_in(&r);
        let out = with_the_pages(&r, &asks, &Ok(vec![sheet("r1", "AAA"), sheet("r2", "BBB"), sheet("r2", "CCC")]));
        let c = out["result"]["content"].as_array().unwrap();
        assert_eq!(c[0]["text"], json!("2 of 2 not looked at yet."), "what the server said comes first");
        assert_eq!(c[1]["text"], json!("Pages of record-ana:"));
        assert_eq!(c[2], json!({"type":"image","data":"AAA","mimeType":"image/png"}));
        assert_eq!(c[3]["text"], json!("Pages of the board (sheet 1 of 2):"));
        assert_eq!(c[6]["data"], json!("CCC"));
        assert!(out["result"]["structuredContent"].get("_bridge").is_none(), "the key never reaches the agent");
        assert_eq!(out["result"]["structuredContent"]["job_id"], json!("j"));
    }

    #[test]
    fn pages_that_cannot_be_shown_are_said_so_and_answered_as_unseen() {
        let r = answer(json!([{"request_id":"r1","label":"record-ana"}]));
        let asks = asks_in(&r);
        let closed = with_the_pages(&r, &asks, &Err("the Designless app is not open on this computer".into()));
        let text = closed["result"]["content"][1]["text"].as_str().unwrap();
        assert!(text.contains("record-ana") && text.contains("not open") && text.contains("unseen"));
        let gone = with_the_pages(&r, &asks, &Ok(vec![]));
        assert!(gone["result"]["content"][1]["text"].as_str().unwrap().contains("no longer holds them"));
        assert!(gone["result"]["content"].as_array().unwrap().iter().all(|b| b["type"] != json!("image")));
    }

    #[test]
    fn the_apps_answer_is_read_and_an_older_app_is_named() {
        let frame = json!({"op":"look_sheets","ok":true,"sheets":[{"request_id":"r1","index":0,"width":1568,"height":1356,"png_base64":"AAA"}],"missing":["r2"]});
        match serde_json::from_value::<IpcResponse>(frame).unwrap() {
            IpcResponse::LookSheets { ok, sheets, missing, .. } => {
                assert!(ok);
                assert_eq!(sheets.unwrap()[0].png_base64, "AAA");
                assert_eq!(missing.unwrap(), vec!["r2".to_string()]);
            }
            other => panic!("read as {other:?}"),
        }
        // A bare refusal from an app of another age still reads.
        assert!(matches!(serde_json::from_value::<IpcResponse>(json!({"op":"look_sheets","ok":false})).unwrap(), IpcResponse::LookSheets { ok: false, .. }));
    }
}

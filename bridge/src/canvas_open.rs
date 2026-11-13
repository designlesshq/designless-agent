//! A canvas the answer asks to open is opened here, by the bridge.
//!
//! An agent's own shell may run in a sandbox that cannot ask macOS to open a
//! link, so its `open` of a `designless://` link fails and the person's
//! document never comes forward. The bridge is started by the host, outside
//! that sandbox. When an answer offers a canvas link under
//! `_bridge.canvas_open`, the bridge checks the link again, asks macOS to open
//! it in the Designless app, and only when that succeeds hands the agent the
//! answer's own words for an opened canvas, without the link, so the agent
//! does not open it a second time. When anything is wrong or the open fails,
//! the answer goes on exactly as it came, and the agent opens the link itself.
use serde_json::Value;
use std::future::Future;

#[cfg(target_os = "macos")]
const BUNDLE_ID: &str = "com.designless.canvas";

/// The answer as the agent should read it, after the bridge has opened the
/// canvas it offers, when it offers one.
pub async fn opened(response: Value) -> Value {
    opened_with(response, open_in_the_app).await
}

/// `opened`, with the opener given: tests pass one that opens nothing.
pub async fn opened_with<F, Fut>(response: Value, open: F) -> Value
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = bool>,
{
    let Some((url, success)) = offer_in(&response) else {
        return response;
    };
    if !open(url.clone()).await {
        tracing::info!("canvas not opened by the bridge; the agent is left the link");
        return response;
    }
    tracing::info!("canvas opened by the bridge");
    with_the_canvas_opened(&response, success)
}

/// The link to open and the words to show once it is open, when the answer
/// offers a canvas and every part of the offer checks out.
pub fn offer_in(response: &Value) -> Option<(String, Value)> {
    let result = response.get("result")?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let offer = result.pointer("/structuredContent/_bridge/canvas_open")?;
    if offer.get("version").and_then(Value::as_u64) != Some(1) {
        return None;
    }
    let url = offer.get("url")?.as_str()?;
    let named = result.pointer("/_meta/designless_open")?;
    let status = named.get("status")?.as_str()?;
    if !matches!(status, "staged" | "created" | "resumed") {
        return None;
    }
    let session = named.get("session_id")?.as_str()?;
    if named.get("url")?.as_str()? != url || !is_uuid(session) || !is_canvas_link(url, session) {
        return None;
    }
    let success = offer.get("success_content")?;
    let blocks = success.as_array()?;
    let all_text = blocks.iter().all(|b| {
        b.get("type").and_then(Value::as_str) == Some("text")
            && b.get("text").is_some_and(Value::is_string)
    });
    if blocks.is_empty() || !all_text {
        return None;
    }
    Some((url.to_string(), success.clone()))
}

/// `designless://canvas?…` and nothing else: no user, port, path or fragment,
/// only plain query characters, and exactly one `session` equal to `session`.
/// A name is letters, digits, `_` or `-` only: the app decodes names before it
/// reads them, so an encoded name such as `se%73sion` would be a second
/// `session` to the app while it looks like another name here.
fn is_canvas_link(url: &str, session: &str) -> bool {
    let Some(query) = url.strip_prefix("designless://canvas?") else {
        return false;
    };
    if url.len() > 512
        || query.is_empty()
        || !query
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-._~%=&+".contains(&c))
    {
        return false;
    }
    let mut sessions = 0;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            return false;
        };
        if key.is_empty()
            || !key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            || value.contains('=')
        {
            return false;
        }
        if key == "session" {
            sessions += 1;
            if value != session {
                return false;
            }
        }
    }
    sessions == 1
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// The answer with the opened canvas's words in place of the link, the link's
/// name gone from `_meta`, and the offer gone. Everything else stays.
fn with_the_canvas_opened(response: &Value, success: Value) -> Value {
    let mut out = response.clone();
    let Some(result) = out.get_mut("result").and_then(Value::as_object_mut) else {
        return response.clone();
    };
    result.insert("content".into(), success);
    let meta_left = match result.get_mut("_meta").and_then(Value::as_object_mut) {
        Some(meta) => {
            meta.remove("designless_open");
            !meta.is_empty()
        }
        None => true,
    };
    if !meta_left {
        result.remove("_meta");
    }
    if let Some(bridge) = out
        .pointer_mut("/result/structuredContent/_bridge")
        .and_then(Value::as_object_mut)
    {
        bridge.remove("canvas_open");
    }
    out
}

/// Ask macOS to open the link in the Designless app, starting the app when it
/// is closed. True only when macOS says it did.
#[cfg(target_os = "macos")]
async fn open_in_the_app(url: String) -> bool {
    let mut cmd = tokio::process::Command::new("/usr/bin/open");
    cmd.args(["-b", BUNDLE_ID, &url])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    matches!(tokio::time::timeout(std::time::Duration::from_secs(10), cmd.status()).await, Ok(Ok(s)) if s.success())
}

#[cfg(not(target_os = "macos"))]
async fn open_in_the_app(_url: String) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const SESSION: &str = "11111111-1111-4111-8111-111111111111";

    fn link() -> String {
        format!("designless://canvas?brand=apple-demo&session={SESSION}&template=rt%3Afixture")
    }

    fn answer(url: &str, status: &str) -> Value {
        json!({"jsonrpc":"2.0","id":7,"result":{
            "content":[{"type":"text","text":format!("# Session Staged\n\nopen \"{url}\"")}],
            "_meta":{"designless_open":{"url":url,"session_id":SESSION,"brand_slug":"apple-demo","status":status},"verified":{"slide_count":3}},
            "structuredContent":{"_bridge":{"canvas_open":{"version":1,"url":url,"success_content":[{"type":"text","text":"# Session Staged\n\nThe canvas link was sent to Designless. Do not open it again."}]}}}
        }})
    }

    /// An opener that opens nothing, says `ok`, and counts its calls and links.
    fn opener(
        ok: bool,
    ) -> (
        Arc<AtomicUsize>,
        impl FnOnce(String) -> std::future::Ready<bool>,
    ) {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        (calls, move |url: String| {
            assert_eq!(url, link(), "the link opened is the one offered");
            seen.fetch_add(1, Ordering::SeqCst);
            std::future::ready(ok)
        })
    }

    #[tokio::test]
    async fn an_opened_canvas_reads_as_opened_and_leaves_no_link() {
        let (calls, open) = opener(true);
        let out = opened_with(answer(&link(), "staged"), open).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let text = out["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Do not open it again"));
        assert!(!text.contains("designless://"), "{text}");
        assert!(out.pointer("/result/_meta/designless_open").is_none());
        assert_eq!(
            out["result"]["_meta"]["verified"],
            json!({"slide_count":3}),
            "the rest of _meta stays"
        );
        assert!(out
            .pointer("/result/structuredContent/_bridge/canvas_open")
            .is_none());
        assert_eq!(out["id"], json!(7));
    }

    #[tokio::test]
    async fn every_status_that_opens_a_canvas_is_opened() {
        for status in ["staged", "created", "resumed"] {
            let (calls, open) = opener(true);
            opened_with(answer(&link(), status), open).await;
            assert_eq!(calls.load(Ordering::SeqCst), 1, "{status}");
        }
    }

    #[tokio::test]
    async fn a_failed_open_leaves_the_answer_as_it_came() {
        let given = answer(&link(), "staged");
        let (calls, open) = opener(false);
        let out = opened_with(given.clone(), open).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            out, given,
            "the agent still reads the link and opens it itself"
        );
    }

    #[tokio::test]
    async fn an_answer_with_no_offer_is_untouched_and_opens_nothing() {
        let mut plain = answer(&link(), "staged");
        plain["result"]
            .as_object_mut()
            .unwrap()
            .remove("structuredContent");
        let (calls, open) = opener(true);
        assert_eq!(opened_with(plain.clone(), open).await, plain);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn an_offer_that_does_not_check_out_opens_nothing() {
        let other = "22222222-2222-4222-8222-222222222222";
        let bad_links = [
            format!("https://canvas?session={SESSION}"),
            format!("designless://elsewhere?session={SESSION}"),
            format!("designless://canvas/path?session={SESSION}"),
            format!("designless://user@canvas?session={SESSION}"),
            format!("designless://canvas:8080?session={SESSION}"),
            format!("designless://canvas?session={SESSION}#top"),
            format!("designless://canvas?session={SESSION}&session={SESSION}"),
            format!("designless://canvas?session={other}"),
            "designless://canvas?brand=apple-demo".to_string(),
            format!("designless://canvas?session={SESSION}&x=a b"),
            format!(" designless://canvas?session={SESSION}"),
            format!("designless://canvas?session={SESSION}&x=$(touch%20y)"),
            // An encoded name the app reads as a second session, before or after.
            format!("designless://canvas?se%73sion={other}&session={SESSION}"),
            format!("designless://canvas?session={SESSION}&se%73sion={other}"),
            format!("designless://canvas?ses+sion={other}&session={SESSION}"),
            format!("designless://canvas?s.ession={other}&session={SESSION}"),
        ];
        for url in bad_links {
            let (calls, open) = opener(true);
            let given = answer(&url, "staged");
            assert_eq!(opened_with(given.clone(), open).await, given, "{url}");
            assert_eq!(calls.load(Ordering::SeqCst), 0, "{url}");
        }
        let mut cases: Vec<Value> = Vec::new();
        let mut v = answer(&link(), "composed");
        cases.push(v.clone());
        v = answer(&link(), "staged");
        v["result"]["isError"] = json!(true);
        cases.push(v);
        v = answer(&link(), "staged");
        v["result"]["structuredContent"]["_bridge"]["canvas_open"]["version"] = json!(2);
        cases.push(v);
        v = answer(&link(), "staged");
        v["result"]["structuredContent"]["_bridge"]["canvas_open"]["success_content"] = json!([]);
        cases.push(v);
        v = answer(&link(), "staged");
        v["result"]["structuredContent"]["_bridge"]["canvas_open"]["success_content"] =
            json!([{"type":"image","data":"x"}]);
        cases.push(v);
        v = answer(&link(), "staged");
        v["result"]["_meta"]["designless_open"]["session_id"] = json!("not-a-uuid");
        cases.push(v);
        v = answer(&link(), "staged");
        v["result"]["_meta"]["designless_open"]["url"] =
            json!(format!("designless://canvas?session={SESSION}"));
        cases.push(v);
        v = answer(&link(), "staged");
        v["result"]["_meta"]
            .as_object_mut()
            .unwrap()
            .remove("designless_open");
        cases.push(v);
        for given in cases {
            let (calls, open) = opener(true);
            assert_eq!(opened_with(given.clone(), open).await, given);
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn after_the_open_the_bridge_key_is_dropped_as_before() {
        let (_, open) = opener(true);
        let out = crate::look::shown(opened_with(answer(&link(), "staged"), open).await).await;
        assert!(
            out.pointer("/result/structuredContent").is_none(),
            "the agent never sees the bridge's key"
        );
        let (_, open) = opener(false);
        let out = crate::look::shown(opened_with(answer(&link(), "staged"), open).await).await;
        assert!(out.pointer("/result/structuredContent").is_none());
        assert!(out["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("open \"designless://canvas?"));
    }
}

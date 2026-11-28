//! A file the person attached is carried to the server as bytes.
//!
//! A job can start from a spreadsheet or a Word document. The agent names the
//! file's path; the bridge runs on the same machine, as the same user, so it
//! carries the file's bytes to the server with the person's own sign-in, the
//! way a picked picture travels (picture.rs). The server reads the file. The
//! person's computer needs nothing installed for it, and the rows never pass
//! through the agent, so they cannot be retyped on the way.
//!
//! The answer is the server's, returned as it came. The bridge adds no words.
//!
//! Only a file that is what it says it is leaves the machine: named .xlsx or
//! .docx, and in the zip container both formats are. Any other path, a file
//! that cannot be read, and one past the ceiling go up as the call came, with
//! nothing read into it, and the server answers that.
use crate::auth::AuthProvider;
use crate::error::{BridgeError, BridgeResult};
use crate::identity::Identity;
use reqwest::Client;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// A file larger than this is not read at all. The server's own cap is lower
/// and is the one a person hears about; this one only bounds what is read.
pub const MAX_FILE_BYTES: u64 = 12 * 1024 * 1024;
/// How a zip container begins.
const ZIP_MAGIC: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];
const EXTENSIONS: [&str; 2] = ["xlsx", "docx"];

/// The call this module takes: the path the agent named, and the card with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub path: String,
    pub card: Value,
}

/// An argument that may arrive as an object or as JSON text.
fn object(v: &Value) -> Option<Value> {
    match v {
        Value::Object(_) => Some(v.clone()),
        Value::String(s) => serde_json::from_str::<Value>(s).ok().filter(Value::is_object),
        _ => None,
    }
}

/// The attached file's call, whether the tool was called by its own name or
/// through the tool that runs another by name. A notification is never one.
pub fn call(frame: &Value) -> Option<Call> {
    if frame.get("method")?.as_str()? != "tools/call" || frame.get("id").is_none_or(Value::is_null) {
        return None;
    }
    let params = frame.get("params")?;
    let mut name = params.get("name")?.as_str()?.to_string();
    let mut args = object(params.get("arguments")?)?;
    if name == "less_execute_tool" {
        name = args.get("tool_name")?.as_str()?.to_string();
        args = object(args.get("arguments")?)?;
    }
    if name != "less_job" || args.get("action")?.as_str()? != "card_file" {
        return None;
    }
    let path = args.get("local_file")?.as_str()?.trim().to_string();
    if path.is_empty() {
        return None;
    }
    let card = match args.get("card") {
        None | Some(Value::Null) => json!({}),
        Some(v) => object(v)?,
    };
    Some(Call { path, card })
}

/// The path as this machine reads it: a leading `~/` is the person's home.
fn local_path(path: &str, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

/// The file's name and bytes, when it is a file this call may carry.
pub async fn read(path: &Path) -> Option<(String, Vec<u8>)> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }
    let name = path.file_name()?.to_str()?.to_string();
    let meta = tokio::fs::metadata(path).await.ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    let bytes = tokio::fs::read(path).await.ok()?;
    if bytes.len() as u64 > MAX_FILE_BYTES || !bytes.starts_with(&ZIP_MAGIC) {
        return None;
    }
    Some((name, bytes))
}

/// The two parts as one body, and the boundary between them. The boundary is
/// named by the bytes and checked against them, so no file can contain it.
fn multipart(name: &str, bytes: &[u8], card: &Value) -> (String, Vec<u8>) {
    let card = card.to_string();
    let mut seed = 0u32;
    let boundary = loop {
        let digest = Sha256::new().chain_update(seed.to_le_bytes()).chain_update(bytes).finalize();
        let hex: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
        let candidate = format!("designless{hex}");
        let in_file = bytes.windows(candidate.len()).any(|w| w == candidate.as_bytes());
        if !in_file && !card.contains(&candidate) {
            break candidate;
        }
        seed += 1;
    };
    let name: String = name
        .chars()
        .map(|c| match c {
            '"' => "%22".to_string(),
            '\r' => "%0D".to_string(),
            '\n' => "%0A".to_string(),
            c => c.to_string(),
        })
        .collect();
    let mut body = Vec::with_capacity(bytes.len() + card.len() + 512);
    body.extend_from_slice(format!("--{boundary}\r\ncontent-disposition: form-data; name=\"card\"\r\n\r\n{card}\r\n").as_bytes());
    body.extend_from_slice(
        format!("--{boundary}\r\ncontent-disposition: form-data; name=\"file\"; filename=\"{name}\"\r\ncontent-type: application/octet-stream\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary, body)
}

/// Who is speaking, in the one header this route reads it from: the same
/// process and conversation every other call names (identity.rs).
fn actor(identity: &Identity) -> String {
    let session = identity.wire_session();
    let host = session.as_deref().and_then(|s| s.split(':').next()).unwrap_or("agent").to_string();
    let mut actor = json!({
        "id": format!("{}:{}", identity.harness, identity.actor),
        "harness": identity.harness,
        "label": format!("{host}:{}", identity.actor),
    });
    if let Some(session) = session {
        actor["session"] = json!(session);
    }
    actor.to_string()
}

async fn post_once(client: &Client, editor: &str, bearer: &str, identity: &Identity, boundary: &str, body: Vec<u8>) -> BridgeResult<reqwest::Response> {
    Ok(client
        .post(format!("{editor}/less/job/card_file"))
        .bearer_auth(bearer)
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .header("x-region", crate::proxy::edge_region())
        .header("x-designless-actor", actor(identity))
        .body(body)
        .send()
        .await?)
}

/// The server's answer to the attached file's call. None leaves the frame to
/// go up as it came: another call, or a file this one may not carry.
pub async fn answer(
    client: &Client,
    editor: &str,
    auth: &(dyn AuthProvider + Send + Sync),
    identity: &Identity,
    frame: &Value,
) -> Option<BridgeResult<Value>> {
    let call = call(frame)?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let Some((name, bytes)) = read(&local_path(&call.path, home.as_deref())).await else {
        tracing::info!("attached file not carried: not a file this call takes");
        return None;
    };
    let (boundary, body) = multipart(&name, &bytes, &call.card);
    Some(carried(client, editor, auth, identity, frame, &boundary, body).await)
}

async fn carried(
    client: &Client,
    editor: &str,
    auth: &(dyn AuthProvider + Send + Sync),
    identity: &Identity,
    frame: &Value,
    boundary: &str,
    body: Vec<u8>,
) -> BridgeResult<Value> {
    let bearer = auth.bearer_or_refresh().await?;
    let mut res = post_once(client, editor, &bearer, identity, boundary, body.clone()).await?;
    // As forward does: the app holds the fresh token, so a 401 is asked once more.
    if res.status().as_u16() == 401 {
        let bearer = auth.bearer_or_refresh().await?;
        res = post_once(client, editor, &bearer, identity, boundary, body).await?;
    }
    let status = res.status();
    let text = res.text().await?;
    // The server's own answer, a refusal included, is the agent's as it came.
    // Anything else (a gateway's page) is an upstream error like any other.
    let said = serde_json::from_str::<Value>(&text).is_ok_and(|v| v.is_object());
    if !said || status.as_u16() == 401 {
        return Err(BridgeError::UpstreamStatus { status: status.as_u16(), body: text });
    }
    tracing::info!(status = status.as_u16(), "attached file carried; the answer is the server's");
    Ok(json!({
        "jsonrpc": "2.0",
        "id": frame.get("id").cloned().unwrap_or(Value::Null),
        "result": {
            "content": [{ "type": "text", "text": text }],
            "isError": !status.is_success(),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn direct(args: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "less_job", "arguments": args } })
    }
    fn through(args: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "less_execute_tool", "arguments": { "tool_name": "less_job", "arguments": args } } })
    }
    fn identity() -> Identity {
        Identity { harness: "codex".into(), version: None, actor: "ab12cd34".into(), session: Some("codex:0a1b2c3d".into()), host: None }
    }
    fn zip(tail: &[u8]) -> Vec<u8> {
        let mut b = ZIP_MAGIC.to_vec();
        b.extend_from_slice(tail);
        b
    }
    async fn file(name: &str, bytes: &[u8]) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("bridge-attached-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let p = dir.join(name);
        tokio::fs::write(&p, bytes).await.unwrap();
        p
    }

    #[test]
    fn the_call_is_taken_by_its_own_name_and_through_the_tool_that_runs_it() {
        let args = json!({ "action": "card_file", "local_file": "/x/crew.xlsx", "card": { "title": "Crew" } });
        let want = Call { path: "/x/crew.xlsx".into(), card: json!({ "title": "Crew" }) };
        assert_eq!(call(&direct(args.clone())), Some(want.clone()));
        assert_eq!(call(&through(args.clone())), Some(want.clone()));
        assert_eq!(call(&through(json!(args.to_string()))), Some(want.clone()), "arguments sent as JSON text");
        assert_eq!(call(&direct(json!({ "action": "card_file", "local_file": "/x/crew.xlsx", "card": "{\"title\":\"Crew\"}" }))), Some(want), "a card sent as JSON text");
        assert_eq!(call(&direct(json!({ "action": "card_file", "local_file": "/x/crew.xlsx" }))).unwrap().card, json!({}));
    }

    #[test]
    fn no_other_call_is_taken() {
        for frame in [
            direct(json!({ "action": "card_start", "local_file": "/x/crew.xlsx", "card": {} })),
            direct(json!({ "action": "card_file", "card": {} })),
            direct(json!({ "action": "card_file", "local_file": "  ", "card": {} })),
            direct(json!({ "action": "card_file", "local_file": "/x/crew.xlsx", "card": "not json" })),
            json!({ "id": 1, "method": "tools/call", "params": { "name": "less_canvas_compose", "arguments": { "action": "card_file", "local_file": "/x/crew.xlsx" } } }),
            json!({ "id": 1, "method": "tools/call", "params": { "name": "less_execute_tool", "arguments": { "tool_name": "less_canvas_compose", "arguments": { "action": "card_file", "local_file": "/x/crew.xlsx" } } } }),
            json!({ "id": 1, "method": "tools/list" }),
            json!({ "id": null, "method": "tools/call", "params": { "name": "less_job", "arguments": { "action": "card_file", "local_file": "/x/crew.xlsx" } } }),
            json!({ "method": "tools/call", "params": { "name": "less_job", "arguments": { "action": "card_file", "local_file": "/x/crew.xlsx" } } }),
        ] {
            assert_eq!(call(&frame), None, "{frame}");
        }
    }

    #[test]
    fn a_home_path_is_the_persons_home() {
        assert_eq!(local_path("~/Desktop/crew.xlsx", Some(Path::new("/Users/p"))), PathBuf::from("/Users/p/Desktop/crew.xlsx"));
        assert_eq!(local_path("/a/~/crew.xlsx", Some(Path::new("/Users/p"))), PathBuf::from("/a/~/crew.xlsx"));
        assert_eq!(local_path("~other/crew.xlsx", Some(Path::new("/Users/p"))), PathBuf::from("~other/crew.xlsx"));
    }

    #[tokio::test]
    async fn only_a_sheet_or_a_word_document_leaves_the_machine() {
        let good = zip(b"rest");
        let p = file("Crew List.XLSX", &good).await;
        assert_eq!(read(&p).await, Some(("Crew List.XLSX".into(), good.clone())), "the name is the file's own, never its folder");
        assert!(read(&file("posts.docx", &good).await).await.is_some());
        assert_eq!(read(&file("id_rsa", &good).await).await, None, "no extension");
        assert_eq!(read(&file("list.numbers", &good).await).await, None, "another kind");
        assert_eq!(read(&file("old.xls", &good).await).await, None, "the old kind");
        assert_eq!(read(&file("secrets.xlsx", b"PRIVATE KEY").await).await, None, "named a sheet, and is not one");
        assert_eq!(read(&file("empty.xlsx", b"").await).await, None);
        assert_eq!(read(Path::new("/no/such/crew.xlsx")).await, None);
        assert_eq!(read(p.parent().unwrap()).await, None, "a folder");
        let dir = p.parent().unwrap().join("folder.xlsx");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        assert_eq!(read(&dir).await, None, "a folder named as a sheet");
    }

    #[tokio::test]
    async fn a_file_past_the_ceiling_is_not_read() {
        let p = file("big.xlsx", b"").await;
        let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
        f.set_len(MAX_FILE_BYTES + 1).unwrap();
        assert_eq!(read(&p).await, None);
    }

    #[test]
    fn the_body_holds_the_bytes_and_the_card_and_a_name_cannot_break_out_of_it() {
        let bytes = zip(b"a\r\n--designless\r\n\0\xff end");
        let (boundary, body) = multipart("cr\"ew\r\nx.xlsx", &bytes, &json!({ "title": "Crew 李小龍" }));
        assert!(body.windows(bytes.len()).any(|w| w == bytes.as_slice()), "byte for byte");
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("filename=\"cr%22ew%0D%0Ax.xlsx\""));
        assert!(text.contains("{\"title\":\"Crew 李小龍\"}"));
        assert_eq!(text.matches(&boundary).count(), 3, "two parts and the end");
    }

    /// One request read whole: headers, then as many bytes as it declared.
    async fn one_request(s: &mut tokio::net::TcpStream) -> (String, Vec<u8>) {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = s.read(&mut chunk).await.unwrap();
            buf.extend_from_slice(&chunk[..n]);
            if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&buf[..at]).to_string();
                let len: usize = head.to_ascii_lowercase().lines().find_map(|l| l.strip_prefix("content-length: ").map(|v| v.trim().parse().unwrap())).unwrap_or(0);
                if buf.len() >= at + 4 + len {
                    return (head, buf[at + 4..at + 4 + len].to_vec());
                }
            }
            assert!(n > 0, "the request ended early");
        }
    }

    /// A server that answers each request in turn and keeps what it was sent.
    async fn server(answers: Vec<(u16, &'static str)>) -> (String, tokio::task::JoinHandle<Vec<(String, Vec<u8>)>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/functions/v1/less-editor", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut seen = Vec::new();
            for (status, body) in answers {
                let (mut s, _) = listener.accept().await.unwrap();
                seen.push(one_request(&mut s).await);
                let _ = s.write_all(format!("HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await;
            }
            seen
        });
        (url, task)
    }

    struct Tokens(Arc<AtomicUsize>);
    #[async_trait::async_trait]
    impl AuthProvider for Tokens {
        async fn bearer_or_refresh(&self) -> BridgeResult<String> {
            Ok(format!("t{}", self.0.fetch_add(1, Ordering::SeqCst)))
        }
    }
    fn tokens() -> Tokens {
        Tokens(Arc::new(AtomicUsize::new(0)))
    }

    #[tokio::test]
    async fn the_file_arrives_byte_for_byte_and_the_answer_is_the_servers_own() {
        let bytes = zip(&(0..=255u8).cycle().take(40_000).collect::<Vec<u8>>());
        let p = file("crew.xlsx", &bytes).await;
        let said = r#"{"columns":["Name"],"hint":"Say which column fills which field."}"#;
        let (url, seen) = server(vec![(200, said)]).await;
        let frame = through(json!({ "action": "card_file", "local_file": p.to_str().unwrap(), "card": { "title": "Crew" } }));
        let got = answer(&Client::new(), &url, &tokens(), &identity(), &frame).await.unwrap().unwrap();
        assert_eq!(got, json!({ "jsonrpc": "2.0", "id": 3, "result": { "content": [{ "type": "text", "text": said }], "isError": false } }));
        let seen = seen.await.unwrap();
        let (head, body) = &seen[0];
        assert!(head.starts_with("POST /functions/v1/less-editor/less/job/card_file "));
        let head = head.to_ascii_lowercase();
        assert!(head.contains("authorization: bearer t0"));
        assert!(head.contains("content-type: multipart/form-data; boundary=designless"));
        assert!(head.contains(r#""session":"codex:0a1b2c3d""#) && head.contains(r#""id":"codex:ab12cd34""#) && head.contains(r#""label":"codex:ab12cd34""#), "{head}");
        assert!(body.windows(bytes.len()).any(|w| w == bytes.as_slice()));
        let text = String::from_utf8_lossy(body);
        assert!(text.contains("filename=\"crew.xlsx\"") && !text.contains(p.parent().unwrap().to_str().unwrap()), "the name travels, the folder does not");
    }

    #[tokio::test]
    async fn a_refusal_is_the_servers_words_and_marked_an_error() {
        let p = file("crew.xlsx", &zip(b"x")).await;
        let said = r#"{"error":"not_open","message":"Job cards are not open here."}"#;
        let (url, _seen) = server(vec![(403, said)]).await;
        let got = answer(&Client::new(), &url, &tokens(), &identity(), &direct(json!({ "action": "card_file", "local_file": p.to_str().unwrap() }))).await.unwrap().unwrap();
        assert_eq!(got["result"]["isError"], json!(true));
        assert_eq!(got["result"]["content"][0]["text"], json!(said));
    }

    #[tokio::test]
    async fn an_expired_sign_in_is_asked_once_more_with_a_fresh_token() {
        let p = file("crew.xlsx", &zip(b"x")).await;
        let (url, seen) = server(vec![(401, r#"{"error":"expired"}"#), (200, r#"{"ok":true}"#)]).await;
        let got = answer(&Client::new(), &url, &tokens(), &identity(), &direct(json!({ "action": "card_file", "local_file": p.to_str().unwrap() }))).await.unwrap().unwrap();
        assert_eq!(got["result"]["isError"], json!(false));
        let seen = seen.await.unwrap();
        assert!(seen[0].0.to_ascii_lowercase().contains("bearer t0") && seen[1].0.to_ascii_lowercase().contains("bearer t1"));
        assert_eq!(seen[0].1, seen[1].1, "the same file both times");
    }

    #[tokio::test]
    async fn a_gateway_page_is_an_upstream_error_and_is_not_sent_twice() {
        let p = file("crew.xlsx", &zip(b"x")).await;
        let (url, seen) = server(vec![(502, "<html>bad gateway</html>")]).await;
        let got = answer(&Client::new(), &url, &tokens(), &identity(), &direct(json!({ "action": "card_file", "local_file": p.to_str().unwrap() }))).await.unwrap();
        assert!(matches!(got, Err(BridgeError::UpstreamStatus { status: 502, .. })));
        assert_eq!(seen.await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_file_it_may_not_carry_leaves_the_call_to_go_up_and_nothing_is_sent() {
        let p = file("secrets.xlsx", b"PRIVATE KEY").await;
        let frame = direct(json!({ "action": "card_file", "local_file": p.to_str().unwrap() }));
        // No server is listening: a request would be an error, not None.
        assert!(answer(&Client::new(), "http://127.0.0.1:9", &tokens(), &identity(), &frame).await.is_none());
    }
}

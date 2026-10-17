//! After a clear, the name on the wire follows the conversation, end to end.
//!
//! The built binary runs twice under this test process, which stands in for
//! the host: once as the session-start hook, the way a host runs it after a
//! clear, and once as the MCP server, against a stand-in desktop socket and a
//! local stand-in for the server that records what each call says. Both find
//! their host by the process tree, so this is the real lookup on both sides,
//! not a value handed from one to the other.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

const BIN: &str = env!("CARGO_BIN_EXE_designless-mcp-bridge");
const OLD: &str = "3f2c9a10-6b7d-4e21-9c55-0a1b2c3d4e5f";
const NEW: &str = "0f0e0d0c-0b0a-4908-8706-050403020100";

fn name(id: &str) -> String {
    let d = Sha256::digest(id.as_bytes());
    format!("claude:{}", d.iter().take(4).map(|b| format!("{b:02x}")).collect::<String>())
}

struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Scratch {
        // Under /tmp, not $TMPDIR: a socket path must stay under 104 bytes.
        let d = PathBuf::from(format!("/tmp/dlh-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
        Scratch(d)
    }
    fn path(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().into_owned()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A stand-in desktop: grants access, hands out a dummy token, and has an
/// empty inbox. Every line of every connection gets its answer.
fn desktop(sock: &str) {
    let listener = std::os::unix::net::UnixListener::bind(sock).unwrap();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut r = BufReader::new(conn.try_clone().unwrap());
                let mut w = conn;
                let mut line = String::new();
                while r.read_line(&mut line).unwrap_or(0) > 0 {
                    let op = serde_json::from_str::<Value>(&line).ok().and_then(|v| v["op"].as_str().map(str::to_string));
                    let reply = match op.as_deref() {
                        Some("ping") => r#"{"op":"pong"}"#,
                        Some("request_access") => r#"{"op":"access_granted","userId":"u"}"#,
                        Some("get_token") => r#"{"op":"token","value":"dummy"}"#,
                        Some("list_inbox") => r#"{"op":"inbox","sessions":[],"attn_dark":null}"#,
                        _ => r#"{"op":"error","error":"unknown_op"}"#,
                    };
                    let _ = w.write_all(format!("{reply}\n").as_bytes());
                    line.clear();
                }
            });
        }
    });
}

/// A stand-in server: answers every call and records the session it named
/// ("-" for none).
fn server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let mut r = BufReader::new(conn.try_clone().unwrap());
            let (mut session, mut length) = ("-".to_string(), 0usize);
            let mut line = String::new();
            while r.read_line(&mut line).unwrap_or(0) > 0 && line != "\r\n" {
                let l = line.to_ascii_lowercase();
                if let Some(v) = l.strip_prefix("x-designless-session:") {
                    session = v.trim().to_string();
                }
                if let Some(v) = l.strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
                line.clear();
            }
            let mut body = vec![0u8; length];
            let _ = r.read_exact(&mut body);
            let id = serde_json::from_slice::<Value>(&body).ok().map(|v| v["id"].clone()).unwrap_or(Value::Null);
            log.lock().unwrap().push(session);
            let reply = json!({ "jsonrpc": "2.0", "id": id, "result": {} }).to_string();
            let mut w = conn;
            let _ = w.write_all(
                format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}", reply.len()).as_bytes(),
            );
        }
    });
    (url, seen)
}

fn me() -> String {
    std::process::id().to_string()
}

fn outer() -> String {
    std::os::unix::process::parent_id().to_string()
}

/// The session-start hook, run as the host runs it.
fn hook(s: &Scratch, source: &str, id: &str, env: &[(&str, &str)]) -> String {
    let input = json!({ "session_id": id, "source": source, "cwd": s.path(""), "hook_event_name": "SessionStart" }).to_string();
    let mut child = Command::new(BIN)
        .args(["hook", "session-start-inbox"])
        .env_clear()
        .env("HOME", s.path("home"))
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("DESIGNLESS_IPC_SOCKET", s.path("ipc.sock"))
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    String::from_utf8(out.stdout).unwrap()
}

/// The MCP server, started with `started_in` in its environment and an
/// inherited `CLAUDE_PID` that is not its host. Two calls; returns the
/// sessions the server heard.
fn bridge(s: &Scratch, url: &str, seen: &Arc<Mutex<Vec<String>>>, started_in: &str, client: &str) -> Vec<String> {
    seen.lock().unwrap().clear();
    let mut child = Command::new(BIN)
        .env_clear()
        .env("HOME", s.path("home"))
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("DESIGNLESS_IPC_SOCKET", s.path("ipc.sock"))
        .env("DESIGNLESS_MCP_URL", url)
        .env("DESIGNLESS_BRIDGE_MODE", "anchored")
        .env("DESIGNLESS_BRIDGE_NO_AUTOLAUNCH", "1")
        .env("CLAUDE_CODE_SESSION_ID", started_in)
        .env("CLAUDE_PID", outer())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let frames = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": client, "version": "1" } } }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    ];
    for f in frames {
        stdin.write_all(format!("{f}\n").as_bytes()).unwrap();
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        assert!(line.contains("\"result\""), "{line}");
    }
    drop(stdin);
    let _ = child.wait();
    seen.lock().unwrap().clone()
}

fn pointer(s: &Scratch) -> PathBuf {
    s.0.join(format!("home/.designless/host-session/{}.json", me()))
}

#[test]
fn after_a_clear_the_bridge_names_the_conversation_the_hook_heard() {
    let s = Scratch::new("clear");
    desktop(&s.path("ipc.sock"));
    let (url, seen) = server();
    let (old, new) = (name(OLD), name(NEW));

    // Before any clear: the name the server started with.
    assert_eq!(bridge(&s, &url, &seen, OLD, "claude-code"), vec![old.clone(), old.clone()]);

    // Starts that do not move the conversation on, or hooks that are not this
    // host's Claude Code conversation, leave nothing.
    let claude = |id: &'static str| [("CLAUDE_PID", me()), ("CLAUDE_CODE_SESSION_ID", id.to_string())];
    let as_env = |v: &[(&'static str, String)]| v.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>();
    let run = |source: &str, id: &str, env: Vec<(&str, String)>| {
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        hook(&s, source, id, &env)
    };
    let out = run("startup", NEW, as_env(&claude(NEW)));
    assert!(out.starts_with(r#"{"hookSpecificOutput":{"hookEventName":"SessionStart""#), "the hook still says what it says: {out}");
    assert!(!pointer(&s).exists(), "a first start leaves nothing");
    run("compact", NEW, as_env(&claude(NEW)));
    assert!(!pointer(&s).exists(), "a compaction leaves nothing");
    run("clear", NEW, vec![("CLAUDE_PID", outer()), ("CLAUDE_CODE_SESSION_ID", NEW.into())]);
    assert!(!pointer(&s).exists(), "a CLAUDE_PID that is not the process above names no host");
    run("clear", NEW, vec![("CLAUDE_CODE_SESSION_ID", NEW.into())]);
    assert!(!pointer(&s).exists(), "no CLAUDE_PID, no host");
    run("clear", NEW, vec![("CLAUDE_PID", me()), ("CLAUDE_CODE_SESSION_ID", OLD.into()), ("CODEX_THREAD_ID", NEW.into())]);
    assert!(!pointer(&s).exists(), "a Codex conversation started from Claude Code is not Claude Code's");
    assert_eq!(bridge(&s, &url, &seen, OLD, "claude-code"), vec![old.clone(), old.clone()]);

    // The clear: the hook leaves the new name and the same bridge setup sends it.
    let out = run("clear", NEW, as_env(&claude(NEW)));
    assert!(out.starts_with(r#"{"hookSpecificOutput":{"hookEventName":"SessionStart""#), "{out}");
    let body = std::fs::read_to_string(pointer(&s)).unwrap();
    assert!(body.contains(&new) && !body.contains(NEW) && !body.contains(OLD), "the name travels, the id does not: {body}");
    assert_eq!(bridge(&s, &url, &seen, OLD, "claude-code"), vec![new.clone(), new.clone()]);

    // A resume back to the first conversation follows it too.
    run("resume", OLD, as_env(&claude(OLD)));
    assert_eq!(bridge(&s, &url, &seen, NEW, "claude-code"), vec![old.clone(), old.clone()]);
    run("clear", NEW, as_env(&claude(NEW)));

    // A Codex server that inherited the Claude Code id names nothing, file or not.
    assert_eq!(bridge(&s, &url, &seen, OLD, "codex-mcp-client"), vec!["-".to_string(), "-".to_string()]);

    // A file that is not this host's (its start time is another process's) is ignored.
    let tampered = body.replace("\"started\":\"", "\"started\":\"9");
    std::fs::write(pointer(&s), tampered).unwrap();
    assert_eq!(bridge(&s, &url, &seen, OLD, "claude-code"), vec![old.clone(), old]);
}

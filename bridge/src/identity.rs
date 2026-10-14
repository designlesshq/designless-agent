//! The bridge names who is speaking (2026-09-23).
//!
//! Every upstream frame carries two headers the server turns into one actor:
//! `x-designless-harness`, the harness the MCP handshake announced (Claude
//! Code, Codex, Cursor, …) normalised to a slug with its version, and
//! `x-designless-actor`, an eight-hex id for this bridge process. Identity
//! comes from the transport, not from what an agent writes in a prompt: the
//! harness is read off the `initialize` frame's `clientInfo`, the id is
//! derived once per process, and neither can be set by a tool call.
//! `DESIGNLESS_HARNESS` overrides the handshake for a harness that announces
//! itself vaguely (the same variable the launcher may set per host).
//!
//! A third header, `x-designless-session`, names the conversation (2026-10-14):
//! the host session the agent is working in, which outlives a bridge restart
//! and ends when the conversation does. Each host says it its own way: Claude
//! Code puts it in its MCP servers' environment, Codex on every tool call.
//! Only a short hash of it travels (`session_label`), so the id itself never
//! leaves this machine. The canvas watcher and the hooks name their own
//! session the same way (`own_session_name`), so they can tell a request the
//! server holds for this session from one it holds for another.

use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// A slug the server paints as a colour: claude-code, codex, cursor, gemini, windsurf, unknown.
    pub harness: String,
    /// The harness's own version string, when the handshake carried one.
    pub version: Option<String>,
    /// Eight hex characters, stable for this process.
    pub actor: String,
    /// The host session this bridge serves, as `session_label` names it, once the host has said.
    pub session: Option<String>,
}

/// The name a host session goes by on the wire: the host, a colon, and the
/// first eight hex of a SHA-256 of the host's own session id. Hashed rather
/// than cut, because some hosts begin their ids with a timestamp, so two
/// sessions started within a minute would share a prefix. The watcher derives
/// the same name from the id it is given. Anything that does not look like an
/// id names nothing.
pub fn session_label(host: &str, session_id: &str) -> Option<String> {
    let id = session_id.trim();
    if id.is_empty() || id.len() > 128 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    let digest = Sha256::digest(id.as_bytes());
    let hash: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    Some(format!("{host}:{hash}"))
}

/// Claude Code sets its session id in the environment of every MCP server it
/// starts, and a cleared conversation starts a new server with the new id.
pub fn session_from_env() -> Option<String> {
    std::env::var("CLAUDE_CODE_SESSION_ID").ok().and_then(|id| session_label("claude", &id))
}

/// Codex gives its MCP servers no session in the environment, but names it on
/// every tool call, in the turn metadata.
pub fn session_from_frame(frame: &Value) -> Option<String> {
    let id = frame.pointer("/params/_meta/x-codex-turn-metadata/session_id")?.as_str()?;
    session_label("codex", id)
}

/// The host a process runs under, read from the variables it was started
/// with: Claude Code sets its session id for everything it starts, and Codex
/// marks what it starts with `CODEX_` variables. Claude Code is asked first,
/// so a Codex setting kept in a shell profile does not rename a Claude Code
/// session. Any other host names nothing.
pub fn host_named_by<I, K, V>(vars: I) -> Option<&'static str>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut codex = false;
    for (name, value) in vars {
        let name = name.as_ref();
        if name == "CLAUDE_CODE_SESSION_ID" && !value.as_ref().trim().is_empty() {
            return Some("claude");
        }
        codex |= name.starts_with("CODEX_");
    }
    codex.then_some("codex")
}

/// The host this process runs under, from its own environment.
pub fn host_from_env() -> Option<&'static str> {
    host_named_by(std::env::vars_os().map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned())))
}

/// The name the server knows this process's session by, given the host's own
/// id for it: the same name the bridge sends for that session, so a request
/// the server holds for it can be told apart from one held for another. None
/// when the host does not say who it is.
pub fn own_session_name(session_id: &str) -> Option<String> {
    session_label(host_from_env()?, session_id)
}

/// The slug a harness name maps to. Case and punctuation are the harness's
/// business; the server keys colours on the slug.
pub fn harness_slug(name: &str) -> String {
    let s: String = name
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-');
    for known in ["claude-code", "codex", "cursor", "gemini", "windsurf"] {
        if s == known || s.starts_with(known) {
            return known.to_string();
        }
    }
    if s.contains("claude") {
        return "claude-code".to_string();
    }
    if s.contains("codex") {
        return "codex".to_string();
    }
    if s.contains("cursor") {
        return "cursor".to_string();
    }
    "unknown".to_string()
}

/// The harness the `initialize` frame announces, or None for any other frame.
pub fn harness_from_initialize(frame: &Value) -> Option<(String, Option<String>)> {
    if frame.get("method").and_then(Value::as_str) != Some("initialize") {
        return None;
    }
    let info = frame.get("params")?.get("clientInfo")?;
    let name = info.get("name").and_then(Value::as_str)?;
    let version = info.get("version").and_then(Value::as_str).map(|v| v.chars().take(32).collect());
    Some((harness_slug(name), version))
}

/// Eight hex characters for this process: a hash of the machine's name, the
/// process id and the moment it started. Never a secret, never a person.
pub fn process_actor_id(host: &str, pid: u32, started_unix_ms: u128) -> String {
    let mut h = Sha256::new();
    h.update(host.as_bytes());
    h.update(pid.to_le_bytes());
    h.update(started_unix_ms.to_le_bytes());
    let digest = h.finalize();
    digest.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

impl Identity {
    /// Detected once at startup: the id now, the harness from the environment
    /// (or unknown until the handshake names it).
    pub fn detect() -> Self {
        let host = std::env::var("HOSTNAME")
            .ok()
            .or_else(hostname_fallback)
            .unwrap_or_else(|| "host".to_string());
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let actor = process_actor_id(&host, std::process::id(), started);
        let harness = std::env::var("DESIGNLESS_HARNESS")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|v| harness_slug(&v))
            .unwrap_or_else(|| "unknown".to_string());
        Self { harness, version: None, actor, session: session_from_env() }
    }

    /// The handshake names the harness, unless the environment already did. A
    /// tool call may name the session. A Claude Code session id this bridge
    /// inherited is not the session of another host that started it.
    pub fn learn_from(&mut self, frame: &Value) {
        if let Some(session) = session_from_frame(frame) {
            self.session = Some(session);
        }
        let pinned = std::env::var("DESIGNLESS_HARNESS").ok().filter(|v| !v.trim().is_empty()).is_some();
        if !pinned {
            if let Some((harness, version)) = harness_from_initialize(frame) {
                self.harness = harness;
                self.version = version;
            }
        }
        let inherited = self.session.as_deref().is_some_and(|s| s.starts_with("claude:"));
        if inherited && self.harness != "claude-code" && self.harness != "unknown" {
            self.session = None;
        }
    }

    /// `claude-code/2.3.1`, or `claude-code` when no version was announced.
    pub fn harness_header(&self) -> String {
        match &self.version {
            Some(v) if !v.is_empty() => format!("{}/{}", self.harness, v),
            _ => self.harness.clone(),
        }
    }
}

fn hostname_fallback() -> Option<String> {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_harness_name_becomes_the_slug_the_server_paints() {
        assert_eq!(harness_slug("claude-code"), "claude-code");
        assert_eq!(harness_slug("Claude Code"), "claude-code");
        assert_eq!(harness_slug("claude-desktop"), "claude-code");
        assert_eq!(harness_slug("codex-cli"), "codex");
        assert_eq!(harness_slug("Cursor"), "cursor");
        assert_eq!(harness_slug("gemini-cli"), "gemini");
        assert_eq!(harness_slug("emacs-mcp"), "unknown");
        assert_eq!(harness_slug(""), "unknown");
    }

    #[test]
    fn the_initialize_frame_names_the_harness_and_no_other_frame_does() {
        let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "claude-code", "version": "2.3.1" } } });
        assert_eq!(harness_from_initialize(&init), Some(("claude-code".to_string(), Some("2.3.1".to_string()))));
        let call = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "less_canvas_compose", "arguments": {} } });
        assert_eq!(harness_from_initialize(&call), None);
        let bare = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} });
        assert_eq!(harness_from_initialize(&bare), None);
    }

    #[test]
    fn the_process_id_is_eight_hex_stable_for_the_same_inputs_and_different_for_another_process() {
        let a = process_actor_id("m5.local", 4242, 1_700_000_000_000);
        assert_eq!(a.len(), 8);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(a, process_actor_id("m5.local", 4242, 1_700_000_000_000));
        assert_ne!(a, process_actor_id("m5.local", 4243, 1_700_000_000_000));
    }

    #[test]
    fn the_identity_learns_the_harness_from_the_handshake_and_says_it_with_its_version() {
        let mut id = Identity { harness: "unknown".into(), version: None, actor: "ab12cd34".into(), session: None };
        id.learn_from(&json!({ "method": "initialize", "params": { "clientInfo": { "name": "Codex", "version": "0.9.0" } } }));
        assert_eq!(id.harness, "codex");
        assert_eq!(id.harness_header(), "codex/0.9.0");
        let bare = Identity { harness: "cursor".into(), version: None, actor: "ab12cd34".into(), session: None };
        assert_eq!(bare.harness_header(), "cursor");
    }

    #[test]
    fn a_session_is_named_by_its_host_and_a_short_hash_of_its_id() {
        let a = session_label("claude", "3f2c9a10-6b7d-4e21-9c55-0a1b2c3d4e5f").unwrap();
        assert!(a.starts_with("claude:"), "{a}");
        let hash = &a["claude:".len()..];
        assert_eq!(hash.len(), 8);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(Some(a.clone()), session_label("claude", " 3f2c9a10-6b7d-4e21-9c55-0a1b2c3d4e5f "), "the same id always has the same name");
        assert!(!a.contains("3f2c9a10"), "the id itself does not travel");
        // Ids that begin with a timestamp still get different names.
        let one = session_label("codex", "0190abcd-0000-7000-8000-000000000001").unwrap();
        let two = session_label("codex", "0190abcd-1111-7000-8000-000000000002").unwrap();
        assert_ne!(one, two);
        for bad in ["", "   ", "has space", "semi;colon", &"x".repeat(129)] {
            assert_eq!(session_label("claude", bad), None, "{bad:?}");
        }
    }

    #[test]
    fn codex_names_the_session_on_every_tool_call_and_nowhere_else() {
        let call = json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {
            "name": "less_canvas_ops", "arguments": {},
            "_meta": { "callId": "exec-1", "x-codex-turn-metadata": { "session_id": "0190abcd-0000-7000-8000-000000000001", "turn_id": "t" } } } });
        assert_eq!(session_from_frame(&call), session_label("codex", "0190abcd-0000-7000-8000-000000000001"));
        let claude_call = json!({ "method": "tools/call", "params": { "name": "x", "_meta": { "claudecode/toolUseId": "toolu_1", "progressToken": 2 } } });
        assert_eq!(session_from_frame(&claude_call), None);
        assert_eq!(session_from_frame(&json!({ "method": "tools/list" })), None);
    }

    #[test]
    fn the_identity_keeps_the_latest_session_a_tool_call_names() {
        let mut id = Identity { harness: "unknown".into(), version: None, actor: "ab12cd34".into(), session: None };
        id.learn_from(&json!({ "method": "initialize", "params": { "clientInfo": { "name": "codex-mcp-client", "version": "0.162.0" } } }));
        assert_eq!(id.session, None, "Codex has said nothing about its session yet");
        let turn = |sid: &str| json!({ "method": "tools/call", "params": { "_meta": { "x-codex-turn-metadata": { "session_id": sid } } } });
        id.learn_from(&turn("0190abcd-0000-7000-8000-000000000001"));
        assert_eq!(id.session, session_label("codex", "0190abcd-0000-7000-8000-000000000001"));
        id.learn_from(&json!({ "method": "tools/list" }));
        assert_eq!(id.session, session_label("codex", "0190abcd-0000-7000-8000-000000000001"), "a later frame without one keeps it");
        id.learn_from(&turn("0190abce-2222-7000-8000-000000000003"));
        assert_eq!(id.session, session_label("codex", "0190abce-2222-7000-8000-000000000003"));
    }

    #[test]
    fn the_host_is_named_by_what_it_leaves_in_the_environment() {
        let vars = |v: &[(&'static str, &'static str)]| v.to_vec();
        assert_eq!(host_named_by(vars(&[("HOME", "/h"), ("CLAUDE_CODE_SESSION_ID", "3f2c9a10-6b7d-4e21-9c55-0a1b2c3d4e5f")])), Some("claude"));
        assert_eq!(host_named_by(vars(&[("HOME", "/h"), ("CODEX_HOME", "/h/.codex")])), Some("codex"));
        assert_eq!(host_named_by(vars(&[("CODEX_SANDBOX", "seatbelt")])), Some("codex"));
        // A Codex setting in a shell profile does not rename a Claude Code session, in either order.
        assert_eq!(host_named_by(vars(&[("CODEX_HOME", "/h/.codex"), ("CLAUDE_CODE_SESSION_ID", "abc")])), Some("claude"));
        assert_eq!(host_named_by(vars(&[("CLAUDE_CODE_SESSION_ID", "abc"), ("CODEX_HOME", "/h/.codex")])), Some("claude"));
        // Nothing to go on: no name.
        for none in [vec![], vars(&[("HOME", "/h")]), vars(&[("CLAUDE_CODE_SESSION_ID", "  ")]), vars(&[("CLAUDE_PID", "4242")]), vars(&[("MY_CODEX_HOME", "/x")]), vars(&[("codex_home", "/x")])] {
            assert_eq!(host_named_by(none.clone()), None, "{none:?}");
        }
        // The name is the one the bridge sends for the same session.
        let id = "3f2c9a10-6b7d-4e21-9c55-0a1b2c3d4e5f";
        let host = host_named_by(vars(&[("CLAUDE_CODE_SESSION_ID", id)])).unwrap();
        assert_eq!(session_label(host, id), session_label("claude", id));
    }

    #[test]
    fn a_claude_session_inherited_by_another_host_is_dropped_and_kept_by_claude_code() {
        let inherited = session_label("claude", "3f2c9a10-6b7d-4e21-9c55-0a1b2c3d4e5f");
        let mut codex = Identity { harness: "unknown".into(), version: None, actor: "ab12cd34".into(), session: inherited.clone() };
        codex.learn_from(&json!({ "method": "initialize", "params": { "clientInfo": { "name": "codex-mcp-client" } } }));
        assert_eq!(codex.session, None, "a Codex run started from a Claude Code shell is not that Claude session");
        let mut claude = Identity { harness: "unknown".into(), version: None, actor: "ab12cd34".into(), session: inherited.clone() };
        claude.learn_from(&json!({ "method": "initialize", "params": { "clientInfo": { "name": "claude-code", "version": "2.1.295" } } }));
        assert_eq!(claude.session, inherited);
    }
}

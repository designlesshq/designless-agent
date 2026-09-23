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
        Self { harness, version: None, actor }
    }

    /// The handshake names the harness, unless the environment already did.
    pub fn learn_from(&mut self, frame: &Value) {
        if std::env::var("DESIGNLESS_HARNESS").ok().filter(|v| !v.trim().is_empty()).is_some() {
            return;
        }
        if let Some((harness, version)) = harness_from_initialize(frame) {
            self.harness = harness;
            self.version = version;
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
        let mut id = Identity { harness: "unknown".into(), version: None, actor: "ab12cd34".into() };
        id.learn_from(&json!({ "method": "initialize", "params": { "clientInfo": { "name": "Codex", "version": "0.9.0" } } }));
        assert_eq!(id.harness, "codex");
        assert_eq!(id.harness_header(), "codex/0.9.0");
        let bare = Identity { harness: "cursor".into(), version: None, actor: "ab12cd34".into() };
        assert_eq!(bare.harness_header(), "cursor");
    }
}

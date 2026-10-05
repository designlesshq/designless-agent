//! What this Mac can do, carried to the server as a fact.
//!
//! The Designless app writes `~/.designless/machine.json` on the machine it
//! runs on. The bridge reads it for every server-bound frame and sends
//! `x-designless-machine: git=0|1`. The server decides what an agent is told;
//! the bridge only carries what the file says. A missing, unreadable or
//! unrecognised file sends no header, which the server reads as "unknown".

use serde_json::Value;
use std::path::{Path, PathBuf};

pub const HEADER: &str = "x-designless-machine";

pub fn facts_path(home: &Path) -> PathBuf {
    home.join(".designless").join("machine.json")
}

/// The header value for the facts in `body`, or None when it says nothing usable.
pub fn header_value_from(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    if v.get("version").and_then(Value::as_u64) != Some(1) {
        return None;
    }
    let git = v.get("git_runnable")?.as_bool()?;
    Some(format!("git={}", if git { 1 } else { 0 }))
}

/// Read the facts for the user running the bridge. Never fails.
pub fn header_value() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let body = std::fs::read_to_string(facts_path(Path::new(&home))).ok()?;
    header_value_from(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mac_where_git_cannot_run_says_git_0() {
        let body = r#"{"version":1,"git_runnable":false,"checked_at":"2026-10-06T00:00:00Z"}"#;
        assert_eq!(header_value_from(body).as_deref(), Some("git=0"));
    }

    #[test]
    fn a_mac_where_git_runs_says_git_1() {
        assert_eq!(
            header_value_from(r#"{"version":1,"git_runnable":true}"#).as_deref(),
            Some("git=1")
        );
    }

    #[test]
    fn anything_else_sends_nothing() {
        for body in [
            "",
            "{not json",
            "{}",
            r#"{"version":2,"git_runnable":false}"#,
            r#"{"version":1,"git_runnable":"no"}"#,
            r#"{"version":1}"#,
        ] {
            assert_eq!(header_value_from(body), None, "{body}");
        }
    }
}

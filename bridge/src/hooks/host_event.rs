//! A conversation's end, told to the desktop app (2026-10-17).
//!
//! Only the host sees a conversation end. When it does, a sheet that
//! conversation staged and never filled can say so, instead of waiting out
//! its quiet clock. The hook tells the desktop app over its local socket, and
//! the app passes it on: the hook holds no credentials, as no hook does.
//!
//! The frame says one thing, that a conversation ended, and names it the way
//! the bridge names it on every call (`own_session_name`: the host and a short
//! hash, never the host's own id). Nothing else travels: not why it ended, not
//! where, not what was running. A host that does not say which session this
//! is gets no frame at all.
//!
//! Fail open, always. No socket, an older desktop that answers `unknown_op`,
//! a slow desktop, any answer or none: the hook prints nothing and exits 0.

use super::js::{self, J};
use super::probe;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The whole exchange with the desktop. A conversation end gets one second
/// from the stricter host, and the launcher and this binary start inside it,
/// so the hook keeps well clear of it, and of the other hooks' 700 ms.
pub const BUDGET_MS: u64 = 300;

/// The most of an answer the hook reads. The answer only says whether this
/// desktop knows the op, and either way there is nothing to do.
const ANSWER_CAP: usize = 4096;

/// The one event this hook reports.
pub const CONVERSATION_END: &str = "conversation_end";

/// The line the desktop is sent for a conversation `session` names.
pub fn frame(event: &str, session: &str) -> String {
    let quote = |s: &str| js::stringify(&J::Str(s.to_string()));
    format!("{{\"op\":\"host_event\",\"event\":{},\"session\":{}}}\n", quote(event), quote(session))
}

/// The frame for a host's conversation-end input, given how a session id is
/// named. None when the input names no session or the name cannot be had.
pub fn conversation_end_frame(raw: &str, name: impl Fn(&str) -> Option<String>) -> Option<String> {
    let input = js::parse(raw)?;
    let Some(J::Str(sid)) = js::get(Some(&input), "session_id") else { return None };
    Some(frame(CONVERSATION_END, &name(sid)?))
}

/// The SessionEnd hook. Prints nothing, whatever happens.
pub async fn conversation_end(raw: &str) -> Option<String> {
    if let Some(line) = conversation_end_frame(raw, crate::identity::own_session_name) {
        tell(&line, BUDGET_MS).await;
    }
    None
}

/// Write `line` to the desktop and read at most its answer, within `budget_ms`.
/// Waiting for the answer lets the desktop finish with the connection before
/// it closes; the answer itself is never acted on.
pub async fn tell(line: &str, budget_ms: u64) {
    let Some(sp) = probe::socket_path() else { return };
    if !probe::dir_is_safe(&sp.dir) || !std::path::Path::new(&sp.sock).exists() {
        return;
    }
    let exchange = async {
        let mut s = tokio::net::UnixStream::connect(&sp.sock).await.ok()?;
        s.write_all(line.as_bytes()).await.ok()?;
        let mut seen = 0usize;
        let mut chunk = [0u8; 512];
        while seen < ANSWER_CAP {
            let n = s.read(&mut chunk).await.ok()?;
            if n == 0 || chunk[..n].contains(&b'\n') {
                break;
            }
            seen += n;
        }
        Some(())
    };
    let _ = tokio::time::timeout(Duration::from_millis(budget_ms), exchange).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{host_named_by, session_label};

    const OLD: &str = "0f0e0d0c-0b0a-4908-8706-050403020100";
    const NEW: &str = "3f2c9a10-6b7d-4e21-9c55-0a1b2c3d4e5f";

    /// `own_session_name` against a fixed environment instead of this process's.
    fn named_in(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |sid| session_label(host_named_by(vars.iter().copied(), sid)?, sid)
    }

    fn fields(line: &str) -> Vec<(String, J)> {
        assert!(line.ends_with('\n') && !line[..line.len() - 1].contains('\n'), "one line: {line:?}");
        match js::parse(line.trim_end()) {
            Some(J::Obj(pairs)) => pairs,
            other => panic!("not an object: {other:?}"),
        }
    }

    #[test]
    fn the_frame_carries_the_op_the_event_and_the_session_and_nothing_else() {
        let line = frame(CONVERSATION_END, "claude:1a2b3c4d");
        assert_eq!(line, "{\"op\":\"host_event\",\"event\":\"conversation_end\",\"session\":\"claude:1a2b3c4d\"}\n");
        let keys: Vec<String> = fields(&line).into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["op", "event", "session"]);
    }

    #[test]
    fn a_full_host_input_sends_only_the_name() {
        let input = format!(
            r#"{{"session_id":"{OLD}","transcript_path":"/Users/someone/.claude/projects/x/{OLD}.jsonl","cwd":"/Users/someone/work","hook_event_name":"SessionEnd","reason":"prompt_input_exit","turn_id":"t-9","model":"m"}}"#
        );
        let line = conversation_end_frame(&input, named_in(&[("CLAUDE_CODE_SESSION_ID", OLD)])).unwrap();
        assert_eq!(line, frame(CONVERSATION_END, &session_label("claude", OLD).unwrap()));
        for kept in [OLD, "/Users", "someone", "work", "prompt_input_exit", "SessionEnd", "t-9", "transcript"] {
            assert!(!line.contains(kept), "{kept} stays on this machine: {line}");
        }
    }

    #[test]
    fn a_cleared_conversation_is_named_by_the_id_it_ended_like_any_other_reason() {
        // A clear ends the old conversation under its old id, then starts a new one.
        let vars: &'static [(&str, &str)] = &[("CLAUDE_CODE_SESSION_ID", OLD)];
        let end = |reason: &str| conversation_end_frame(&format!(r#"{{"session_id":"{OLD}","reason":"{reason}"}}"#), named_in(vars));
        let cleared = end("clear").expect("a clear names the conversation it ended");
        assert_eq!(cleared, frame(CONVERSATION_END, &session_label("claude", OLD).unwrap()));
        assert_ne!(Some(cleared.clone()), session_label("claude", NEW).map(|n| frame(CONVERSATION_END, &n)));
        for reason in ["prompt_input_exit", "logout", "resume", "other", "bypass_permissions_disabled", ""] {
            assert_eq!(end(reason).as_ref(), Some(&cleared), "{reason}");
        }
    }

    #[test]
    fn codex_names_its_own_conversation() {
        let line = conversation_end_frame(&format!(r#"{{"session_id":"{OLD}","reason":"other"}}"#), named_in(&[("CODEX_THREAD_ID", OLD)]));
        assert_eq!(line, Some(frame(CONVERSATION_END, &session_label("codex", OLD).unwrap())));
    }

    #[test]
    fn no_name_no_frame() {
        let input = format!(r#"{{"session_id":"{OLD}","reason":"other"}}"#);
        // A host that does not say who it is, and a Claude Code id that is another session's.
        assert_eq!(conversation_end_frame(&input, named_in(&[])), None);
        assert_eq!(conversation_end_frame(&input, named_in(&[("CLAUDE_CODE_SESSION_ID", NEW)])), None);
        // No session in the input, or nothing that can be one.
        let anyone = named_in(&[("CODEX_SANDBOX", "seatbelt")]);
        for raw in ["", "not json", "null", "[]", "{}", r#"{"session_id":null}"#, r#"{"session_id":42}"#, r#"{"session_id":""}"#, r#"{"session_id":"has space"}"#, r#"{"sessionId":"abc"}"#] {
            assert_eq!(conversation_end_frame(raw, &anyone), None, "{raw}");
        }
    }
}

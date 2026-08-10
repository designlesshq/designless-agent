//! Anchored mode — Electron desktop is the trust anchor.
//!
//! The bridge talks to the Designless desktop app over local IPC. On startup
//! it requests consent; thereafter every MCP frame triggers an IPC `get_token`
//! that reads the Electron-managed keychain just-in-time and returns the
//! current Supabase JWT.
//!
//! Outcomes from the initial consent handshake. The bridge stays alive in every
//! one of them and never opens a browser; what differs is the error each MCP
//! frame then returns, which is the user's diagnosis for the whole session.
//!
//! - `Ready(AnchoredAuth)` — user clicked Allow, or had already granted.
//! - `UserDenied(DeniedAuth)` — user clicked Deny. The hint points at the tray
//!   menu, because clearing the grant is the only way forward.
//! - `Unavailable(DeniedAuth)` — unavailable for a known reason: the app is
//!   closed, nobody is signed in, or the session is unusable. Each reports
//!   itself, because each has a different fix.
//! - `None` — the reason is not known (IPC failed, malformed reply, a denial
//!   carrying no reason). `main.rs` supplies the conservative hint.

pub mod ipc;

use crate::auth::AuthProvider;
use crate::error::{BridgeError, BridgeResult};

/// Result of attempting to bring up anchored mode. Discriminates between
/// "use this provider" and "surface a recovery hint".
pub enum AnchoredInit {
    /// User granted access (or already granted previously). Use this provider.
    Ready(AnchoredAuth),
    /// User explicitly denied. Surface a clear error so they know to click
    /// Disconnect Claude Code if they change their mind.
    UserDenied(DeniedAuth),
    /// Anchored mode is unavailable for a known reason: the app is closed,
    /// nobody is signed in, or the session is unusable. Each reports itself.
    Unavailable(DeniedAuth),
}

/// Try to initialise anchored mode. Returns `Ok(None)` for soft-failure cases
/// (no signed-in user, invalid session, IPC error) — the caller surfaces a
/// recovery hint and stays alive (never a browser). Returns `Ok(Some(...))`
/// when anchored mode is the right outcome (whether granted or denied).
pub async fn try_init() -> BridgeResult<Option<AnchoredInit>> {
    let mut client = match ipc::connect().await {
        Ok(c) => c,
        // A closed app is not a denial, and `AppNotOpen` carries the code and
        // hint that say so.
        Err(BridgeError::AppNotOpen) => {
            tracing::info!("Designless desktop app is not open");
            return Ok(Some(AnchoredInit::Unavailable(DeniedAuth::app_not_open())));
        }
        Err(e) => {
            tracing::warn!(error = %e, "anchored IPC connect failed; will surface a recovery hint");
            return Ok(None);
        }
    };

    let pid = std::process::id();
    let reply = match client.request_access("claude-code", pid).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "request_access failed; will surface a recovery hint");
            return Ok(None);
        }
    };

    match reply {
        ipc::IpcResponse::AccessGranted { user_id } => {
            tracing::info!(user_id = %user_id, "anchored mode ready");
            Ok(Some(AnchoredInit::Ready(AnchoredAuth { user_id })))
        }
        ipc::IpcResponse::AccessDenied { reason } => match reason.as_deref() {
            Some("user_denied") => {
                tracing::info!("user denied Claude Code access via Designless app");
                Ok(Some(AnchoredInit::UserDenied(DeniedAuth {
                    fault: Fault::Denied,
                    hint: "Designless denied Claude Code access. Click 'Disconnect Claude Code' in the menu bar to clear the grant and try again.".into(),
                })))
            }
            // The desktop's contract names exactly three: user_denied,
            // no_session, invalid_session — and its own header says "Read
            // `reason`, not the op name". Both of the remaining two mean the app
            // ANSWERED, so telling the user it is unreachable is false.
            Some(reason) => {
                tracing::info!(%reason, "anchored unavailable; surfacing the reason");
                Ok(Some(AnchoredInit::Unavailable(DeniedAuth::not_signed_in(reason))))
            }
            None => {
                tracing::info!("access_denied carried no reason; falling back to the generic hint");
                Ok(None)
            }
        },
        other => {
            tracing::warn!(?other, "unexpected IPC reply to request_access; will surface a recovery hint");
            Ok(None)
        }
    }
}

/// Auth provider that reads JWTs via a fresh IPC `get_token` on every call.
/// No caching: the desktop app is the rotation authority and refreshes the
/// token on demand, so each call returns a current JWT. In the rare event a
/// token expires in flight, the proxy asks again once (a fresh `get_token`)
/// and retries the frame.
pub struct AnchoredAuth {
    #[allow(dead_code)] // surfaced via logs + reserved for future per-user audit lines
    user_id: String,
}

#[async_trait::async_trait]
impl AuthProvider for AnchoredAuth {
    async fn bearer_or_refresh(&self) -> BridgeResult<String> {
        let mut client = ipc::connect().await?;
        match client.get_token().await? {
            ipc::IpcResponse::Token { value } => Ok(value),
            // One sentence cannot describe both outcomes. A genuine sign-out
            // should send someone to the login screen; a token the app cannot
            // currently serve should not, because the session may be perfectly
            // fine and signing in again would change nothing.
            //
            // The wire set is closed at two literals. Anything else is treated as
            // "cannot tell" and falls back to the conservative message: an
            // unrecognised value means this bridge is older than the app, not
            // that the session has ended.
            ipc::IpcResponse::NoSession { reason } => Err(BridgeError::NoBearer(
                match reason.as_deref() {
                    Some("signed_out") => {
                        "Designless app has no signed-in user. Open the app, sign in, and retry."
                            .into()
                    }
                    Some("unavailable") => {
                        "Designless app could not provide a token. It may still be signed in, so \
                         check the app rather than signing in again — if it is asking you to \
                         update, that is why."
                            .to_string()
                    }
                    // Absent (old desktop) or unrecognised: keep the original
                    // sentence. Guessing here is the one move that makes things
                    // worse for users who have not updated yet.
                    _ => "Designless app has no signed-in user. Open the app, sign in, and retry."
                        .into(),
                },
            )),
            ipc::IpcResponse::Error { reason } => Err(BridgeError::NoBearer(format!(
                "Designless IPC reported error: {}",
                reason.unwrap_or_else(|| "unknown".into())
            ))),
            other => Err(BridgeError::Protocol(format!(
                "unexpected IPC reply to get_token: {other:?}"
            ))),
        }
    }
}

/// Which failure a hard-fail provider reports.
///
/// The bridge stays alive and answers every MCP frame with the same error, so
/// this choice is the user's diagnosis for the whole session. Each variant maps
/// to its own error code and hint in `proxy.rs`.
enum Fault {
    /// The desktop app is not running.
    AppNotOpen,
    /// The app answered and cannot supply a session. The message says which.
    NotSignedIn(String),
    /// The user refused the grant. Also the current fallback for the
    /// can't-tell path in `main.rs`, where the "approve the request" hint does
    /// not fit — worth splitting if that path grows a hint of its own.
    Denied,
}

/// Hard-failure provider. Every call returns the same error so Claude Code's
/// /mcp panel shows a recovery hint the user can act on.
pub struct DeniedAuth {
    fault: Fault,
    hint: String,
}

impl DeniedAuth {
    /// Build a hard-fail provider with a custom recovery hint. Used when the
    /// desktop app is unreachable or has no signed-in user: the bridge stays
    /// alive and every MCP frame returns this hint via the /mcp panel instead
    /// of silently minting a separate identity.
    pub fn with_hint(hint: impl Into<String>) -> Self {
        Self { fault: Fault::Denied, hint: hint.into() }
    }

    /// The app is not running. Distinct from a refusal and from a signed-out
    /// session, and the fastest of the three to act on.
    fn app_not_open() -> Self {
        Self { fault: Fault::AppNotOpen, hint: String::new() }
    }

    /// The app answered and cannot supply a session. `reason` is the desktop's
    /// own wire literal, carried through rather than replaced with a guess.
    fn not_signed_in(reason: &str) -> Self {
        let msg = match reason {
            "no_session" => "Designless app has no signed-in user. Open the app, sign in, then \
                             reconnect this MCP server."
                .to_string(),
            "invalid_session" => "Designless app has a session it cannot use. Open the app and \
                                  sign in again, then reconnect this MCP server."
                .to_string(),
            // An unrecognised literal means this bridge is older than the app,
            // not that the session ended. Report the literal rather than guess.
            other => format!(
                "Designless app declined the connection ({other}). Open the app and check it is \
                 signed in; if it is asking you to update, that is why."
            ),
        };
        Self { fault: Fault::NotSignedIn(msg), hint: String::new() }
    }
}

#[async_trait::async_trait]
impl AuthProvider for DeniedAuth {
    async fn bearer_or_refresh(&self) -> BridgeResult<String> {
        // Three problems with three different fixes, so three different errors.
        Err(match &self.fault {
            Fault::AppNotOpen => BridgeError::AppNotOpen,
            Fault::NotSignedIn(msg) => BridgeError::NoBearer(msg.clone()),
            Fault::Denied => BridgeError::AccessDenied(self.hint.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthProvider;

    /// The error a provider hands back for one MCP frame.
    ///
    /// tokio is already a dependency, so this needs no new crate: a test helper
    /// that adds one to a shipped binary is a cost every user pays.
    async fn err_of(p: &DeniedAuth) -> BridgeError {
        p.bearer_or_refresh().await.unwrap_err()
    }

    #[tokio::test]
    async fn three_problems_do_not_collapse_into_one() {
        // A closed app, a signed-out app and an unusable session need three
        // different actions, so they must not share one error.
        assert!(matches!(err_of(&DeniedAuth::app_not_open()).await, BridgeError::AppNotOpen));
        assert!(matches!(err_of(&DeniedAuth::not_signed_in("no_session")).await, BridgeError::NoBearer(_)));
        assert!(matches!(err_of(&DeniedAuth::with_hint("x")).await, BridgeError::AccessDenied(_)));
    }

    #[tokio::test]
    async fn every_wire_reason_produces_a_distinct_sentence() {
        // The desktop's access_denied contract names exactly these two besides
        // user_denied. If two ever read the same, the user cannot tell which
        // problem they have.
        let sentences_src: Vec<DeniedAuth> = ["no_session", "invalid_session", "some_future_literal"]
            .iter()
            .map(|r| DeniedAuth::not_signed_in(r))
            .collect::<Vec<_>>();
        let mut sentences: Vec<String> = Vec::new();
        for p in sentences_src { sentences.push(err_of(&p).await.to_string()); }
        let unique: std::collections::HashSet<&String> = sentences.iter().collect();
        assert_eq!(unique.len(), sentences.len(), "two reasons read identically: {sentences:?}");
    }

    #[tokio::test]
    async fn every_not_signed_in_message_names_something_to_do() {
        // Scoped to NotSignedIn on purpose. For those the message IS the
        // actionable sentence, and proxy.rs repeats it as the hint. AppNotOpen
        // is the other shape: its message states the condition and the action
        // lives in the hint beside it, so asserting the message alone would be
        // asserting the wrong half.
        //
        // A diagnosis with no action is the failure this surface exists to
        // avoid — the user learns which of three problems they have and still
        // cannot move.
        let acts = ["open the app", "open designless", "reconnect", "sign in"];
        for reason in ["no_session", "invalid_session", "some_future_literal"] {
            let s = err_of(&DeniedAuth::not_signed_in(reason)).await.to_string();
            let lower = s.to_lowercase();
            assert!(
                acts.iter().any(|a| lower.contains(a)),
                "{reason} states a problem with no action: {s}",
            );
        }
    }

    #[tokio::test]
    async fn an_unknown_reason_admits_it_rather_than_guessing() {
        // An unrecognised literal means this bridge is older than the app, not
        // that the session ended. Asserting "signed out" there is the one move
        // that makes things worse for users who have not updated.
        let s = err_of(&DeniedAuth::not_signed_in("brand_new_literal")).await.to_string();
        assert!(s.contains("brand_new_literal"), "the unknown reason must be surfaced: {s}");
        assert!(!s.contains("has no signed-in user"), "must not assert a diagnosis it does not have: {s}");
    }

    #[tokio::test]
    async fn nothing_a_user_reads_names_internal_infrastructure() {
        // These strings ship to users, who cannot act on infrastructure names.
        //
        // The word list holds only terms this crate already states publicly in
        // its own description and module docs, so the guard adds no vocabulary
        // of its own. The length rule is what covers everything unlisted: a
        // long unbroken token in a user-facing sentence is a credential, an id
        // or a path, and none of those belong in one.
        const NAMED: [&str; 3] = ["supabase", "keychain", "jwt"];
        for p in [
            DeniedAuth::app_not_open(),
            DeniedAuth::not_signed_in("no_session"),
            DeniedAuth::not_signed_in("invalid_session"),
            DeniedAuth::not_signed_in("some_future_literal"),
        ] {
            let s = err_of(&p).await.to_string();
            let lower = s.to_lowercase();
            for n in NAMED {
                assert!(!lower.contains(n), "user-facing string names infrastructure ({n}): {s}");
            }
            for word in s.split_whitespace() {
                let bare = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                assert!(
                    bare.len() < 24,
                    "user-facing string carries a {}-character token, which reads as a credential or an id: {s}",
                    bare.len(),
                );
            }
        }
    }
}

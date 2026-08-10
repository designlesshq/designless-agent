//! Anchored mode — Electron desktop is the trust anchor.
//!
//! The bridge talks to the Designless desktop app over local IPC. On startup
//! it requests consent; thereafter every MCP frame triggers an IPC `get_token`
//! that reads the Electron-managed keychain just-in-time and returns the
//! current Supabase JWT.
//!
//! Three outcomes from the initial consent handshake:
//!
//! - `Ready(AnchoredAuth)`         — user clicked Allow (or already granted)
//! - `UserDenied(DeniedAuth)`      — user clicked Deny; bridge stays alive
//!                                   but every MCP frame returns an error
//!                                   with a hint pointing at the tray menu
//! - `None`                        — desktop app reports no signed-in user /
//!                                   invalid session, or the IPC layer itself
//!                                   failed (connect, malformed reply). main.rs
//!                                   maps this to a recovery hint and stays
//!                                   alive; it never opens a browser.

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
    /// Anchored mode is not available and we know WHY. Previously every one of
    /// these returned `Ok(None)`, and the caller replaced the reason with one
    /// fixed sentence about the app being unreachable — which is wrong whenever
    /// the app answered.
    Unavailable(DeniedAuth),
}

/// Try to initialise anchored mode. Returns `Ok(None)` for soft-failure cases
/// (no signed-in user, invalid session, IPC error) — the caller surfaces a
/// recovery hint and stays alive (never a browser). Returns `Ok(Some(...))`
/// when anchored mode is the right outcome (whether granted or denied).
pub async fn try_init() -> BridgeResult<Option<AnchoredInit>> {
    let mut client = match ipc::connect().await {
        Ok(c) => c,
        // A closed app is not a denial. `AppNotOpen` already exists, already
        // carries the right code and the right hint, and was unreachable from
        // here because this arm threw the error away.
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

/// Hard-failure provider for the "user clicked Deny" path. Every call returns
/// the same error so Claude Code's /mcp panel shows the recovery hint.
/// Which failure this provider reports.
///
/// The bridge stays alive and answers EVERY MCP frame with the same error, so
/// this choice is the user-facing diagnosis for the whole session. It used to be
/// a single value: every soft failure — app closed, nobody signed in, session
/// unusable — became `AccessDenied`, whose hint reads "Open the Designless app
/// and approve the access request". There is nothing to approve when the app is
/// not running, and signing in is not the fix when the app cannot be reached.
///
/// The accurate error variants and hints already existed in proxy.rs and were
/// simply unreachable from this path.
enum Fault {
    /// The desktop app is not running. -32006, "open the app".
    AppNotOpen,
    /// The app answered; nobody is signed in, or the session is not usable.
    /// -32003, and the message says which.
    NotSignedIn(String),
    /// The user actively refused the grant. -32002, "approve the request" —
    /// the only case where that sentence is true.
    Denied,
}

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

    /// The app is not running. Distinct from "denied" and from "signed out",
    /// and the one the user can act on fastest.
    fn app_not_open() -> Self {
        Self { fault: Fault::AppNotOpen, hint: String::new() }
    }

    /// The app answered and cannot give us a session. `reason` is the desktop's
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
            // NOT that we know what happened. Say the true thing.
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
        // Three different problems with three different fixes. Collapsing them
        // made all three unactionable.
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
    /// tokio is already a dependency with the `macros` feature, so this needs no
    /// new crate — a test helper that adds a dependency to a shipped binary is a
    /// cost paid by every user for something only CI runs.
    async fn err_of(p: &DeniedAuth) -> BridgeError {
        p.bearer_or_refresh().await.unwrap_err()
    }

    #[tokio::test]
    async fn three_problems_do_not_collapse_into_one() {
        // The regression: every soft failure became AccessDenied, whose hint
        // reads "Open the Designless app and approve the access request".
        // There is nothing to approve when the app is not running, and signing
        // in is not the fix when the app cannot be reached.
        assert!(matches!(err_of(&DeniedAuth::app_not_open()).await, BridgeError::AppNotOpen));
        assert!(matches!(err_of(&DeniedAuth::not_signed_in("no_session")).await, BridgeError::NoBearer(_)));
        assert!(matches!(err_of(&DeniedAuth::with_hint("x")).await, BridgeError::AccessDenied(_)));
    }

    #[tokio::test]
    async fn every_wire_reason_produces_a_distinct_sentence() {
        // The desktop's access_denied contract names exactly these two besides
        // user_denied. If two of them ever read the same, the user cannot tell
        // which problem they have.
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
    async fn an_unknown_reason_admits_it_rather_than_guessing() {
        // An unrecognised literal means this bridge is older than the app, NOT
        // that we know the session ended. Asserting "signed out" there is the
        // one move that makes things worse for users who have not updated.
        let s = err_of(&DeniedAuth::not_signed_in("brand_new_literal")).await.to_string();
        assert!(s.contains("brand_new_literal"), "the unknown reason must be surfaced: {s}");
        assert!(!s.contains("has no signed-in user"), "must not assert a diagnosis it does not have: {s}");
    }

    #[tokio::test]
    async fn nothing_a_user_reads_names_internal_infrastructure() {
        // This repo is public and these strings ship to users.
        let banned = ["supabase", "service_role", "jwt", "keychain", "sb_secret", "postgres"];
        for p in [
            DeniedAuth::app_not_open(),
            DeniedAuth::not_signed_in("no_session"),
            DeniedAuth::not_signed_in("invalid_session"),
        ] {
            let s = err_of(&p).await.to_string().to_lowercase();
            for b in banned {
                assert!(!s.contains(b), "user-facing string names internal infrastructure ({b}): {s}");
            }
        }
    }
}

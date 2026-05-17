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
//! - `None`                        — Electron reports no signed-in user or
//!                                   invalid session. Under the default
//!                                   (auto) policy main.rs falls back to
//!                                   standalone (browser OAuth); under the
//!                                   anchored policy it stays alive with a
//!                                   recovery hint and never opens a browser.
//!
//! Errors from the IPC layer itself (connect failed, malformed reply, etc.)
//! resolve the same way — best-effort under the auto policy, strict (hint,
//! no browser) under the anchored policy.

pub mod ipc;

use crate::auth::AuthProvider;
use crate::error::{BridgeError, BridgeResult};

/// Reachability probe used by main.rs to decide initial mode.
pub async fn probe() -> std::io::Result<bool> {
    ipc::probe_reachable().await
}

/// Result of attempting to bring up anchored mode. Discriminates between
/// "use this provider" and "fall back to standalone".
pub enum AnchoredInit {
    /// User granted access (or already granted previously). Use this provider.
    Ready(AnchoredAuth),
    /// User explicitly denied. Don't fall back to standalone — surface a clear
    /// error so they know to click Disconnect Claude Code if they change their
    /// mind.
    UserDenied(DeniedAuth),
}

/// Try to initialise anchored mode. Returns `Ok(None)` for soft-failure cases
/// where the caller should fall back to standalone mode (no signed-in user,
/// invalid session, IPC error). Returns `Ok(Some(...))` when anchored mode is
/// the right outcome (whether granted or denied).
pub async fn try_init() -> BridgeResult<Option<AnchoredInit>> {
    let mut client = match ipc::connect().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "anchored IPC connect failed; falling back to standalone");
            return Ok(None);
        }
    };

    let pid = std::process::id();
    let reply = match client.request_access("claude-code", pid).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "request_access failed; falling back to standalone");
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
                    hint: "Designless denied Claude Code access. Click 'Disconnect Claude Code' in the menu bar to clear the grant and try again.".into(),
                })))
            }
            other => {
                tracing::info!(reason = ?other, "anchored unavailable; falling back to standalone");
                Ok(None)
            }
        },
        other => {
            tracing::warn!(?other, "unexpected IPC reply to request_access; falling back");
            Ok(None)
        }
    }
}

/// Auth provider that reads JWTs via IPC each call. No caching: Electron is
/// the rotation authority. The bridge stays out of refresh logic — if the
/// returned token is near expiry, the proxy passes it as-is; mcp.designless.app
/// returns 401 (rare; Electron refreshes proactively at 5min remaining), and
/// the next get_token call fetches the rotated token.
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
            ipc::IpcResponse::NoSession => Err(BridgeError::NoBearer(
                "Designless app has no signed-in user. Open the app, sign in, and retry.".into(),
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
pub struct DeniedAuth {
    hint: String,
}

impl DeniedAuth {
    /// Build a hard-fail provider with a custom recovery hint. Used by the
    /// anchored-required policy when the desktop app is unreachable or has no
    /// signed-in user: the bridge stays alive and every MCP frame returns this
    /// hint via the /mcp panel instead of silently switching to browser OAuth.
    pub fn with_hint(hint: impl Into<String>) -> Self {
        Self { hint: hint.into() }
    }
}

#[async_trait::async_trait]
impl AuthProvider for DeniedAuth {
    async fn bearer_or_refresh(&self) -> BridgeResult<String> {
        Err(BridgeError::AccessDenied(self.hint.clone()))
    }
}

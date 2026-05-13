//! Auth provider abstraction.
//!
//! Both anchored and standalone modes implement `AuthProvider`. The proxy loop
//! calls `bearer_or_refresh()` for each forwarded frame, gets back a current
//! Bearer string, attaches it as `Authorization: Bearer <token>`, and forwards
//! to the remote MCP endpoint.
//!
//! Refresh policy is owned by each provider:
//!
//! - Anchored: bridge re-reads the keychain entry on every call (Electron is
//!   the rotation authority; bridge is a read-only consumer). When `exp` is
//!   within 60s, bridge IPCs back to Electron to request a fresh token rather
//!   than refreshing itself.
//!
//! - Standalone: bridge owns the access+refresh pair in `~/.designless/auth.json`.
//!   When access expires within 60s, the bridge POSTs to `/less/oauth/token`
//!   with `grant_type=refresh_token`, rotates the pair atomically, and returns
//!   the new access token.

use crate::error::BridgeResult;

/// A source of Bearer tokens, possibly refreshed inline.
#[async_trait::async_trait]
pub trait AuthProvider {
    /// Returns a Bearer token suitable for `Authorization: Bearer <token>`.
    ///
    /// Implementations refresh on-demand if the current access token is within
    /// 60 seconds of expiry. Errors are propagated so the proxy can return a
    /// structured MCP error to Claude Code (with a recovery hint in the
    /// human-readable message).
    async fn bearer_or_refresh(&self) -> BridgeResult<String>;
}

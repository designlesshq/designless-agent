//! Auth provider abstraction.
//!
//! The proxy loop calls `bearer_or_refresh()` for each forwarded frame, gets
//! back a current Bearer string, attaches it as `Authorization: Bearer <token>`,
//! and forwards to the remote MCP endpoint.
//!
//! The desktop app is the single trust anchor and the token-rotation authority;
//! the bridge is a read-only consumer. `AnchoredAuth` fetches the current JWT
//! via a fresh IPC `get_token` on every call (the desktop app reads its
//! keychain just-in-time and refreshes on demand), so no token is cached or
//! refreshed bridge-side. `DeniedAuth` is the no-session fallback: it returns a
//! recovery-hint error on every call when the desktop app is unreachable or has
//! denied access.

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

//! Designless MCP bridge entry point.
//!
//! The bridge is a stdio MCP server that forwards JSON-RPC frames from Claude
//! Code to the remote HTTP MCP endpoint at `mcp.designless.app/mcp`.
//!
//! The Designless desktop app is the single trust anchor. On startup the
//! bridge performs a consent handshake over local IPC, then reads the Supabase
//! JWT the desktop app maintains in the OS keychain
//! (`designless-canvas:access-token`). The desktop app owns and rotates every
//! token; the bridge is a read-only consumer that re-reads the keychain — via
//! a fresh IPC `get_token` — on every forwarded frame. If the desktop app
//! isn't reachable or has no signed-in user, the bridge stays alive and
//! surfaces a recovery hint via Claude Code's `/mcp` panel; it never opens a
//! browser or mints a separate identity.
//!
//! Downstream: HTTPS POST to mcp.designless.app/mcp with
//! `Authorization: Bearer <token>`, frame relayed back to stdout. On an
//! upstream 401 the bridge asks the desktop app for a fresh token once and
//! retries the frame (the desktop app is the rotation authority).
//!
//! Stdio discipline: this process writes **only MCP frames to stdout**. Logs,
//! diagnostics, and errors all go to stderr (captured by Claude Code's /mcp
//! panel).

use anyhow::Result;
use tracing_subscriber::EnvFilter;

mod anchored;
mod auth;
mod error;
mod mcp;
mod paths;
mod proxy;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "designless-mcp-bridge starting (anchored)"
    );

    // Desktop app required. On any miss, stay alive with a recovery hint
    // instead of opening a browser — a silent fallback would mint a second,
    // divergent identity (separate consent grant + token store) parallel to
    // the desktop app's.
    let auth: Box<dyn auth::AuthProvider + Send + Sync> = match anchored::try_init().await? {
        Some(anchored::AnchoredInit::Ready(a)) => {
            tracing::info!("anchored auth ready");
            Box::new(a)
        }
        Some(anchored::AnchoredInit::UserDenied(d)) => {
            tracing::warn!("anchored access denied; returning hint until the grant is cleared");
            Box::new(d)
        }
        None => {
            tracing::warn!(
                "desktop app unreachable or no signed-in user; surfacing recovery hint"
            );
            Box::new(anchored::DeniedAuth::with_hint(
                "The Designless desktop app isn't reachable. Open Designless, sign in, \
                 then reconnect this MCP server from the /mcp panel.",
            ))
        }
    };

    proxy::serve_stdio(auth).await
}

/// Configure stderr-only logging. `DESIGNLESS_BRIDGE_LOG` filters; default is `info`.
/// Stdout is reserved for MCP protocol — never written to from any tracing layer.
fn init_tracing() {
    let filter = EnvFilter::try_from_env("DESIGNLESS_BRIDGE_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .compact()
        .init();
}

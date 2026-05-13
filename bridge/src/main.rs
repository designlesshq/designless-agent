//! Designless MCP bridge entry point.
//!
//! The bridge is a stdio MCP server that forwards JSON-RPC frames from Claude Code
//! to the remote HTTP MCP endpoint at `mcp.designless.app/mcp`. It operates in two
//! modes determined at startup:
//!
//! - **Anchored** — the Designless desktop app is reachable over local IPC. The
//!   bridge issues a consent handshake, then reads the Supabase JWT that Electron
//!   has populated in the OS keychain (`designless-canvas:access-token`). All
//!   tokens are owned and rotated by Electron; the bridge is a read-only consumer.
//!
//! - **Standalone** — no Electron desktop reachable (Cowork sandboxes, desktop
//!   installs without the Designless app, CI). The bridge runs its own PKCE
//!   OAuth flow against `/less/oauth/*`, listens on `127.0.0.1:0` for the
//!   callback, opens the user's browser, and stores the resulting access/refresh
//!   pair at `~/.designless/auth.json` (mode 0600).
//!
//! The downstream is the same in both modes: HTTPS POST to mcp.designless.app/mcp
//! with `Authorization: Bearer <token>`, frame relayed back to stdout.
//!
//! Stdio discipline: this process writes **only MCP frames to stdout**. Logs,
//! diagnostics, and errors all go to stderr (captured by Claude Code's /mcp panel).

use anyhow::Result;
use tracing_subscriber::EnvFilter;

mod anchored;
mod auth;
mod error;
mod mcp;
mod paths;
mod proxy;
mod standalone;

/// Mode resolved at startup. Anchored is preferred when available.
#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Anchored,
    Standalone,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "designless-mcp-bridge starting"
    );

    let reachable = anchored::probe().await.unwrap_or(false);
    tracing::info!(anchored_reachable = reachable, "IPC probe complete");

    let auth: Box<dyn auth::AuthProvider + Send + Sync> = if reachable {
        match anchored::try_init().await? {
            Some(anchored::AnchoredInit::Ready(a)) => {
                tracing::info!(mode = ?Mode::Anchored, "auth mode resolved");
                Box::new(a)
            }
            Some(anchored::AnchoredInit::UserDenied(d)) => {
                // Stay alive so Claude Code can surface the recovery hint via /mcp panel.
                tracing::warn!("user denied anchored access; bridge will return errors until tray menu clears the grant");
                Box::new(d)
            }
            None => {
                tracing::info!(mode = ?Mode::Standalone, "anchored unavailable; falling back");
                Box::new(standalone::StandaloneAuth::new().await?)
            }
        }
    } else {
        tracing::info!(mode = ?Mode::Standalone, "auth mode resolved");
        Box::new(standalone::StandaloneAuth::new().await?)
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

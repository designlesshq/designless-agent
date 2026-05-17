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

/// Auth-mode policy, resolved from `DESIGNLESS_BRIDGE_MODE`. The plugin
/// launcher sets this deterministically based on whether the Designless
/// desktop app is installed and running. Unset or unrecognised preserves the
/// historical auto-detect behavior, so CI, sandboxes, and existing installs
/// that don't set the variable are unaffected.
#[derive(Debug, Clone, Copy)]
enum ModePolicy {
    /// Desktop app required. Never open a browser; if the app is unreachable,
    /// stay alive and surface a recovery hint via the /mcp panel.
    Anchored,
    /// Skip the IPC probe entirely and run the bridge's own OAuth flow.
    /// Correct where there is no desktop app (CI, sandboxes, web-only).
    Standalone,
    /// Historical behavior: prefer anchored, fall back to standalone.
    Auto,
}

fn mode_policy() -> ModePolicy {
    match std::env::var("DESIGNLESS_BRIDGE_MODE").ok().as_deref() {
        Some("anchored") => ModePolicy::Anchored,
        Some("standalone") => ModePolicy::Standalone,
        _ => ModePolicy::Auto,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "designless-mcp-bridge starting"
    );

    let policy = mode_policy();
    tracing::info!(?policy, "auth-mode policy resolved");

    let auth: Box<dyn auth::AuthProvider + Send + Sync> = match policy {
        ModePolicy::Standalone => {
            tracing::info!(mode = ?Mode::Standalone, "auth mode resolved (forced standalone)");
            Box::new(standalone::StandaloneAuth::new().await?)
        }

        ModePolicy::Anchored => {
            // Desktop app required. On any miss, stay alive with a recovery
            // hint instead of falling back to browser OAuth — a silent
            // fallback would mint a second, divergent identity (separate
            // consent grant and token store) parallel to the desktop app's.
            match anchored::try_init().await? {
                Some(anchored::AnchoredInit::Ready(a)) => {
                    tracing::info!(mode = ?Mode::Anchored, "auth mode resolved");
                    Box::new(a)
                }
                Some(anchored::AnchoredInit::UserDenied(d)) => {
                    tracing::warn!("anchored access denied; returning hint until the grant is cleared");
                    Box::new(d)
                }
                None => {
                    tracing::warn!(
                        "desktop app required but unreachable; surfacing recovery hint (no browser fallback)"
                    );
                    Box::new(anchored::DeniedAuth::with_hint(
                        "The Designless desktop app isn't reachable. Open Designless, sign in, \
                         then reconnect this MCP server from the /mcp panel.",
                    ))
                }
            }
        }

        ModePolicy::Auto => {
            // Historical behavior, unchanged: prefer anchored, fall back to
            // standalone. Preserved verbatim for CI / sandboxes / installs
            // that do not set DESIGNLESS_BRIDGE_MODE.
            let reachable = anchored::probe().await.unwrap_or(false);
            tracing::info!(anchored_reachable = reachable, "IPC probe complete");

            if reachable {
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
            }
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

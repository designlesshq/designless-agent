//! Standalone mode — bridge owns its own OAuth flow against `/less/oauth/*`.
//!
//! Activated when the Designless desktop app is not reachable over local IPC
//! (Cowork sandboxes, desktop installs without the Designless app, CI).
//! Functionally identical to today's Claude-Code-driven OAuth dance — same
//! provider endpoints, same browser hop, same loopback callback — just
//! implemented inside the bridge instead of in Claude Code's MCP client.

mod browser;
mod listener;
mod oauth;
mod pkce;
mod storage;

use crate::auth::AuthProvider;
use crate::error::{BridgeError, BridgeResult};
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Refresh tokens that are within this many seconds of expiry. Matches plan
/// v4 §5.6 design (60-second leeway) and gives the proxy enough margin to
/// avoid a 401 race on long-running calls.
const REFRESH_LEEWAY_SECS: i64 = 60;

pub struct StandaloneAuth {
    client: Client,
    /// Cached token state. Mutex serializes refresh / OAuth across concurrent
    /// MCP frames so we never run two PKCE flows in parallel.
    cache: Arc<Mutex<Option<storage::AuthFile>>>,
}

impl StandaloneAuth {
    pub async fn new() -> BridgeResult<Self> {
        let client = Client::builder()
            .user_agent(format!("designless-mcp-bridge/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let cached = storage::load().await?;
        Ok(Self {
            client,
            cache: Arc::new(Mutex::new(cached)),
        })
    }

    async fn perform_pkce_flow(&self) -> BridgeResult<storage::AuthFile> {
        let pkce = pkce::PkcePair::new();
        let state = pkce::random_state();
        let listener_handle = listener::start()?;
        let port = listener_handle.port;
        let auth_url = oauth::build_authorize_url(port, &state, &pkce.challenge);

        // Print URL to stderr so /mcp panel + manual fallback both have it.
        // Tracing also captures this, but explicit eprintln! ensures the user
        // sees an unmistakable banner even with DESIGNLESS_BRIDGE_LOG=off.
        eprintln!();
        eprintln!("=========================================================");
        eprintln!(" Designless authorization required");
        eprintln!(" Opening your browser. If it doesn't open, visit:");
        eprintln!("   {auth_url}");
        eprintln!("=========================================================");
        eprintln!();
        tracing::info!(%auth_url, "opening browser for OAuth consent");

        // Best-effort open. If it fails, the user has the URL above.
        if let Err(e) = browser::open(&auth_url).await {
            tracing::warn!(error = %e, "browser open failed; user must visit URL manually");
        }

        let cb = listener_handle.wait().await?;
        if cb.state != state {
            return Err(BridgeError::Protocol(
                "OAuth callback state did not match — possible CSRF, aborting".into(),
            ));
        }

        let tokens = oauth::exchange_code(&self.client, &cb.code, &pkce.verifier, port).await?;
        storage::save_atomic(&tokens).await?;
        tracing::info!("PKCE flow complete — tokens written to ~/.designless/auth.json");
        Ok(tokens)
    }
}

#[async_trait::async_trait]
impl AuthProvider for StandaloneAuth {
    async fn bearer_or_refresh(&self) -> BridgeResult<String> {
        // Single-threaded auth state: serialize all bearer reads through the
        // same mutex so concurrent frames never trigger two PKCE flows.
        let mut cache = self.cache.lock().await;

        // Path 1: cached token, not expired → fast path
        if let Some(ref file) = *cache {
            if !file.is_access_expired(REFRESH_LEEWAY_SECS) {
                return Ok(file.access_token.clone());
            }
            // Path 2: cached token, expiring → refresh
            tracing::info!("access token within refresh leeway, attempting refresh");
            match oauth::refresh(&self.client, &file.refresh_token).await {
                Ok(new_tokens) => {
                    storage::save_atomic(&new_tokens).await?;
                    let bearer = new_tokens.access_token.clone();
                    *cache = Some(new_tokens);
                    return Ok(bearer);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "refresh failed; clearing tokens and falling back to PKCE");
                    let _ = storage::delete().await;
                    *cache = None;
                    // fall through to PKCE
                }
            }
        }

        // Path 3: no cache (first run or refresh failure) → full PKCE flow
        let tokens = self.perform_pkce_flow().await?;
        let bearer = tokens.access_token.clone();
        *cache = Some(tokens);
        Ok(bearer)
    }
}

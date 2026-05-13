//! MCP frame forwarding from stdio to the remote HTTP MCP endpoint.
//!
//! For each frame received from Claude Code over stdin:
//!   1. Ask the AuthProvider for a current Bearer.
//!   2. POST the frame to `https://mcp.designless.app/mcp` with the Bearer.
//!   3. Stream the response back to stdout.
//!
//! Auth errors are translated into JSON-RPC error responses so Claude Code's
//! `/mcp` panel surfaces a useful recovery hint rather than a generic stdio
//! crash.

use crate::auth::AuthProvider;
use crate::error::{BridgeError, BridgeResult};
use crate::mcp::{FrameReader, FrameWriter};
use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};

/// Upstream MCP endpoint. Compile-time constant for now; environment override
/// supported for testing via `DESIGNLESS_MCP_URL`.
const DEFAULT_UPSTREAM: &str = "https://mcp.designless.app/mcp";

pub async fn serve_stdio(auth: Box<dyn AuthProvider + Send + Sync>) -> Result<()> {
    let upstream = std::env::var("DESIGNLESS_MCP_URL").unwrap_or_else(|_| DEFAULT_UPSTREAM.into());
    let client = Client::builder()
        .user_agent(format!(
            "designless-mcp-bridge/{}",
            env!("CARGO_PKG_VERSION")
        ))
        // Reasonable per-request timeout for MCP tool calls (some are slow).
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let mut reader = FrameReader::new();
    let mut writer = FrameWriter::new();

    tracing::info!(upstream = %upstream, "proxy ready");

    while let Some(frame) = reader.read_frame().await? {
        let id = frame.get("id").cloned();
        let response = match forward(&client, &upstream, &*auth, &frame).await {
            Ok(v) => v,
            Err(e) => error_response(id, &e),
        };
        writer.write_frame(&response).await?;
    }

    tracing::info!("stdin EOF — exiting cleanly");
    Ok(())
}

async fn forward(
    client: &Client,
    upstream: &str,
    auth: &(dyn AuthProvider + Send + Sync),
    frame: &Value,
) -> BridgeResult<Value> {
    let bearer = auth.bearer_or_refresh().await?;
    let res = client
        .post(upstream)
        .bearer_auth(&bearer)
        .header("content-type", "application/json")
        .json(frame)
        .send()
        .await?;

    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(BridgeError::UpstreamStatus {
            status: status.as_u16(),
            body,
        });
    }

    let json: Value = res.json().await?;
    Ok(json)
}

/// Build a JSON-RPC error response. Includes a human-readable hint in
/// `error.data.hint` so the orchestrator skill (or any client reading the
/// frame) can surface actionable text.
fn error_response(id: Option<Value>, err: &BridgeError) -> Value {
    let (code, hint) = match err {
        BridgeError::IpcUnreachable => (
            -32001,
            "Open the Designless desktop app and sign in, or wait for standalone-mode OAuth.",
        ),
        BridgeError::AccessDenied(_) => (
            -32002,
            "Open the Designless app and approve the access request.",
        ),
        BridgeError::NoBearer(_) => (
            -32003,
            "Sign in to the Designless app (anchored mode) or complete the OAuth prompt (standalone mode).",
        ),
        BridgeError::UpstreamStatus { status: 401, .. } => (
            -32004,
            "Session expired. Re-authenticate via the Designless app or by completing the OAuth prompt.",
        ),
        BridgeError::UpstreamStatus { .. } => (-32005, "Upstream MCP error. Visit https://designless.app/help if persistent."),
        BridgeError::Protocol(_) => (-32700, "MCP protocol error."),
        BridgeError::Io(_) | BridgeError::Http(_) | BridgeError::Json(_) => (
            -32000,
            "Bridge IO error. Check network and retry.",
        ),
    };

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": err.to_string(),
            "data": { "hint": hint },
        },
    })
}

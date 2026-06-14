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
        // JSON-RPC notifications (no `id`) must never receive a response. The MCP
        // Streamable HTTP server answers them with 202 Accepted + empty body;
        // forward fire-and-forget and emit nothing. Emitting a frame here — in
        // particular the spurious {"error", id:null} from failing to decode that
        // empty 202 body — is an unsolicited response a strict client rejects,
        // failing the connect handshake right after `notifications/initialized`.
        let is_notification = matches!(id, None | Some(Value::Null));
        let result = forward(&client, &upstream, &*auth, &frame).await;
        if is_notification {
            if let Err(e) = result {
                tracing::debug!(error = %e, "notification upstream result ignored");
            }
            continue;
        }
        let response = match result {
            Ok(v) => v,
            Err(e) => error_response(id, &e),
        };
        writer.write_frame(&response).await?;
    }

    tracing::info!("stdin EOF — exiting cleanly");
    Ok(())
}

/// Single attempt: fetch a current Bearer and POST the frame upstream.
async fn post_once(
    client: &Client,
    upstream: &str,
    auth: &(dyn AuthProvider + Send + Sync),
    frame: &Value,
) -> BridgeResult<reqwest::Response> {
    let bearer = auth.bearer_or_refresh().await?;
    let res = client
        .post(upstream)
        .bearer_auth(&bearer)
        .header("content-type", "application/json")
        // MCP Streamable HTTP transport spec requires the client to accept
        // both JSON and SSE — the server is allowed to upgrade to streaming
        // for any response and rejects requests that don't advertise both
        // with a 406 Not Acceptable.
        .header("accept", "application/json, text/event-stream")
        .json(frame)
        .send()
        .await?;
    Ok(res)
}

async fn forward(
    client: &Client,
    upstream: &str,
    auth: &(dyn AuthProvider + Send + Sync),
    frame: &Value,
) -> BridgeResult<Value> {
    let mut res = post_once(client, upstream, auth, frame).await?;

    // The desktop app is the token-rotation authority. On a 401, asking it
    // again (`bearer_or_refresh()` → a fresh IPC `get_token`) yields a
    // freshly-refreshed token; retry the frame exactly once. Covers the rare
    // race where the token expired in flight or just after it was read.
    if res.status().as_u16() == 401 {
        tracing::warn!("upstream 401 — requesting a fresh token from the desktop app and retrying once");
        res = post_once(client, upstream, auth, frame).await?;
    }

    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(BridgeError::UpstreamStatus {
            status: status.as_u16(),
            body,
        });
    }

    // The MCP Streamable HTTP transport allows servers to respond either with
    // straight JSON or with an SSE stream. We accept both. Branch on the
    // response content-type.
    let content_type = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();

    if content_type.starts_with("text/event-stream") {
        // SSE: extract the data line(s) and parse the last JSON-RPC frame.
        // For non-streaming responses (e.g. tools/list, single tool call result),
        // the server emits one `event: message` followed by one `data:` line.
        // For streaming responses (progress events), multiple frames may appear;
        // we currently surface only the terminal frame to Claude Code. Wiring
        // intermediate progress events through stdio is a future enhancement.
        let body = res.text().await?;
        let mut last_frame: Option<Value> = None;
        for event_block in body.split("\n\n") {
            // SSE field syntax: each line is `field: value`. Concatenate `data:`
            // lines from a single event with newlines per the spec.
            let mut data_parts: Vec<&str> = Vec::new();
            for line in event_block.lines() {
                if let Some(rest) = line.strip_prefix("data:") {
                    data_parts.push(rest.trim_start());
                }
            }
            if data_parts.is_empty() {
                continue;
            }
            let payload = data_parts.join("\n");
            match serde_json::from_str::<Value>(&payload) {
                Ok(frame) => last_frame = Some(frame),
                Err(err) => tracing::warn!(error = %err, "skipping unparseable SSE data payload"),
            }
        }
        last_frame.ok_or_else(|| {
            BridgeError::Protocol("SSE response had no parseable data frames".into())
        })
    } else {
        let json: Value = res.json().await?;
        Ok(json)
    }
}

/// Build a JSON-RPC error response. Includes a human-readable hint in
/// `error.data.hint` so the orchestrator skill (or any client reading the
/// frame) can surface actionable text.
fn error_response(id: Option<Value>, err: &BridgeError) -> Value {
    let (code, hint) = match err {
        BridgeError::IpcUnreachable => (
            -32001,
            "Open the Designless desktop app and sign in, then reconnect this MCP server from the /mcp panel.",
        ),
        BridgeError::AccessDenied(_) => (
            -32002,
            "Open the Designless app and approve the access request.",
        ),
        BridgeError::NoBearer(_) => (
            -32003,
            "Sign in to the Designless desktop app, then reconnect this MCP server from the /mcp panel.",
        ),
        BridgeError::UpstreamStatus { status: 401, .. } => (
            -32004,
            "Session expired. Open the Designless desktop app and sign in, then retry.",
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

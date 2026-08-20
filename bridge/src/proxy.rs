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
use crate::integrity::Integrity;
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

    // Attestation identity for the integrity fence. Recomputed per frame in
    // post_once; None only when the tree layout is unrecognizable (dev builds
    // run from cargo target dirs), in which case no header is sent and the
    // server treats the client as legacy.
    let integrity = Integrity::detect();

    tracing::info!(upstream = %upstream, attesting = integrity.is_some(), "proxy ready");

    while let Some(frame) = reader.read_frame().await? {
        let id = frame.get("id").cloned();
        let integrity = integrity.as_ref();
        // JSON-RPC notifications (no `id`) must never receive a response. The MCP
        // Streamable HTTP server answers them with 202 Accepted + empty body;
        // forward fire-and-forget and emit nothing. Emitting a frame here — in
        // particular the spurious {"error", id:null} from failing to decode that
        // empty 202 body — is an unsolicited response a strict client rejects,
        // failing the connect handshake right after `notifications/initialized`.
        let is_notification = matches!(id, None | Some(Value::Null));
        let result = forward(&client, &upstream, &*auth, integrity, &frame).await;
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
    integrity: Option<&Integrity>,
    frame: &Value,
) -> BridgeResult<reqwest::Response> {
    let bearer = auth.bearer_or_refresh().await?;
    // Per-frame attestation: the tree is re-hashed for every call, so an edit
    // mid-session is caught on the agent's next server-bound action — the
    // tightest fence that exists for local files.
    let mut req = client
        .post(upstream)
        .bearer_auth(&bearer)
        .header("content-type", "application/json");
    if let Some(integrity) = integrity {
        req = req.header("x-designless-plugin-integrity", integrity.header_value());
    }
    let res = req
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
    integrity: Option<&Integrity>,
    frame: &Value,
) -> BridgeResult<Value> {
    let mut res = post_once(client, upstream, auth, integrity, frame).await?;

    // The desktop app is the token-rotation authority. On a 401, asking it
    // again (`bearer_or_refresh()` → a fresh IPC `get_token`) yields a
    // freshly-refreshed token; retry the frame exactly once. Covers the rare
    // race where the token expired in flight or just after it was read.
    if res.status().as_u16() == 401 {
        tracing::warn!("upstream 401 — requesting a fresh token from the desktop app and retrying once");
        res = post_once(client, upstream, auth, integrity, frame).await?;
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
            "Open the Designless desktop app and sign in, then reconnect the Designless server from your editor's MCP settings.",
        ),
        // NAMES THE ACTION THAT ALWAYS WORKS. This error has two producers: a
        // per-frame connect, where retrying after reopening the app succeeds, and
        // a provider built once at startup, where it cannot — that one holds the
        // same answer for the whole session, so only reconnecting clears it.
        // Reconnecting works for both, so it is what the hint names.
        BridgeError::AppNotOpen => (
            -32006,
            "The Designless desktop app is not open. Open Designless and sign in, then retry. If tools still fail, reconnect the Designless server from your editor's MCP settings.",
        ),
        BridgeError::AccessDenied(_) => (
            -32002,
            "Open the Designless app and approve the access request.",
        ),
        // THE HINT USED TO CONTRADICT THE MESSAGE IT SHIPPED WITH. NoBearer's
        // inner string is already reason-specific — for the desktop's
        // `unavailable` literal it says "It may still be signed in, so check the
        // app rather than signing in again" — while this hint said "Sign in to
        // the Designless desktop app". A client reading `data.hint` got the
        // opposite advice from a client reading `message`, on the same frame.
        //
        // The inner string IS the actionable sentence, so it is the hint.
        BridgeError::NoBearer(msg) => (-32003, msg.as_str()),
        BridgeError::UpstreamStatus { status: 401, .. } => (
            -32004,
            "Session expired. Open the Designless desktop app and sign in, then retry.",
        ),
        // 428 Precondition Required = the integrity fence: the server compared
        // this install's attested tree hash against the released one and
        // refused. The body's hint names the recovery (reinstall/update).
        BridgeError::UpstreamStatus { status: 428, body } => (
            -32007,
            upstream_hint(body).unwrap_or(
                "This plugin's files differ from the released version, so the server declined the session. Update or reinstall the Designless plugin to restore the verified state.",
            ),
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

/// Pull the server's `hint` field out of a refusal body, if it parses.
fn upstream_hint(body: &str) -> Option<&'static str> {
    // The hint text is server-owned and dynamic; leaking it through a static
    // return type is not possible, so the mapping above uses its own copy and
    // this helper only confirms the body carried one (logged for diagnosis).
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    if let Some(hint) = parsed.as_ref().and_then(|v| v.get("hint")).and_then(|h| h.as_str()) {
        tracing::warn!(server_hint = %hint, "integrity refusal from server");
    }
    None
}

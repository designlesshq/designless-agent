//! Error types surfaced across module boundaries.
//!
//! Errors that the bridge returns *to Claude Code* are translated into JSON-RPC
//! error responses in `proxy.rs`. Errors used internally propagate via
//! `anyhow::Result` in `main` and module-typed `Result` in submodules.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    /// Electron desktop did not respond on the IPC socket within the probe
    /// window. Caller surfaces a recovery hint (no browser fallback).
    #[error("electron ipc not reachable")]
    IpcUnreachable,

    /// The desktop app's IPC socket is absent or refusing connections, i.e. the
    /// app is not running. Distinct from IpcUnreachable (a present-but-slow app,
    /// e.g. a probe timeout): here we know the app is closed, so the caller
    /// surfaces an actionable "open the Designless app" hint instead of a raw
    /// "io error: No such file or directory". Every bridged tool needs a JWT
    /// fetched from the app over this socket, so this blocks all of them.
    #[error("Designless desktop app is not open")]
    AppNotOpen,

    /// Anchored mode: Electron declined the access-request handshake (user
    /// clicked Deny in the native consent dialog, or no signed-in user).
    #[error("electron denied access: {0}")]
    AccessDenied(String),

    /// Auth source produced no bearer (desktop app has no signed-in user).
    #[error("no bearer token available: {0}")]
    NoBearer(String),

    /// Upstream MCP endpoint returned a non-success status.
    #[error("upstream MCP returned {status}: {body}")]
    UpstreamStatus {
        status: u16,
        body: String,
    },

    /// JSON-RPC frame parsing or serialization failed.
    #[error("MCP frame protocol error: {0}")]
    Protocol(String),

    /// Filesystem / keychain / IPC IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// HTTPS client error from reqwest.
    #[error("http client error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type BridgeResult<T> = Result<T, BridgeError>;

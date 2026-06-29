//! IPC client for anchored mode.
//!
//! Connects to the Designless desktop app's bridge-ipc server (electron/bridge-ipc.js)
//! over a Unix domain socket (macOS/Linux) or named pipe (Windows). Wire protocol
//! is newline-delimited JSON frames, matching the server side exactly.

use crate::error::{BridgeError, BridgeResult};
use crate::paths::{ipc_endpoint, IpcEndpoint};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::timeout;

/// Probe window for first connection attempt during mode resolution.
const PROBE_TIMEOUT: Duration = Duration::from_millis(250);

/// Per-IPC-call timeout (request_access can sit on a user dialog, so this is
/// generous; get_token should return in milliseconds).
const CALL_TIMEOUT: Duration = Duration::from_secs(300);

/// Full connection for sustained IPC use. Caller wraps in IpcClient.
pub async fn connect() -> BridgeResult<IpcClient> {
    let stream = connect_inner(PROBE_TIMEOUT).await?;
    Ok(IpcClient::new(stream))
}

/// Concrete stream type (varies by platform).
enum Stream {
    #[cfg(unix)]
    Unix(tokio::net::UnixStream),
    #[cfg(windows)]
    Pipe(tokio::net::windows::named_pipe::NamedPipeClient),
}

/// Classify a connect() IO error. A missing socket file (`NotFound`) or a
/// refused connection (`ConnectionRefused`, a stale socket left by a crashed
/// app) means the Designless desktop app is not running: surface the actionable
/// `AppNotOpen` rather than a raw `io error: No such file or directory`. Any
/// other IO error (permissions, etc.) stays a generic `Io`.
#[cfg_attr(not(any(unix, windows)), allow(dead_code))]
fn classify_connect_error(e: std::io::Error) -> BridgeError {
    match e.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
            BridgeError::AppNotOpen
        }
        _ => BridgeError::Io(e),
    }
}

async fn connect_inner(t: Duration) -> BridgeResult<Stream> {
    let endpoint = ipc_endpoint();
    let fut = async {
        match endpoint {
            #[cfg(unix)]
            IpcEndpoint::UnixSocket(path) => {
                // On macOS the socket lives in a shared-/tmp per-user dir; only
                // connect if that dir is our own private 0700, owner-only dir
                // (defends against a planted socket). Linux uses
                // XDG_RUNTIME_DIR, which is already user-private.
                #[cfg(target_os = "macos")]
                let dir_ok = path
                    .parent()
                    .map(crate::paths::ipc_dir_is_safe)
                    .unwrap_or(false);
                #[cfg(not(target_os = "macos"))]
                let dir_ok = true;

                if dir_ok {
                    tokio::net::UnixStream::connect(&path)
                        .await
                        .map(Stream::Unix)
                        .map_err(classify_connect_error)
                } else {
                    Err(BridgeError::IpcUnreachable)
                }
            }
            #[cfg(windows)]
            IpcEndpoint::NamedPipe(path) => {
                use tokio::net::windows::named_pipe::ClientOptions;
                ClientOptions::new()
                    .open(&path)
                    .map(Stream::Pipe)
                    .map_err(classify_connect_error)
            }
            #[allow(unreachable_patterns)]
            _ => Err(BridgeError::IpcUnreachable),
        }
    };
    timeout(t, fut)
        .await
        .map_err(|_| BridgeError::IpcUnreachable)?
}

/// A connected IPC client. Each frame is a request/reply pair; the client
/// owns both halves of the duplex stream.
pub struct IpcClient {
    inner: ClientInner,
}

enum ClientInner {
    #[cfg(unix)]
    Unix {
        reader: BufReader<tokio::net::unix::OwnedReadHalf>,
        writer: tokio::net::unix::OwnedWriteHalf,
    },
    #[cfg(windows)]
    Pipe {
        // Named pipes don't expose owned halves; we serialize access via &mut self.
        pipe: tokio::net::windows::named_pipe::NamedPipeClient,
    },
}

impl IpcClient {
    fn new(stream: Stream) -> Self {
        let inner = match stream {
            #[cfg(unix)]
            Stream::Unix(s) => {
                let (r, w) = s.into_split();
                ClientInner::Unix {
                    reader: BufReader::new(r),
                    writer: w,
                }
            }
            #[cfg(windows)]
            Stream::Pipe(p) => ClientInner::Pipe { pipe: p },
        };
        Self { inner }
    }

    /// Send a JSON frame and read one JSON reply, blocking up to CALL_TIMEOUT.
    async fn send_frame(&mut self, frame: &Value) -> BridgeResult<IpcResponse> {
        let mut payload = serde_json::to_string(frame)?;
        payload.push('\n');
        let reply_text = timeout(CALL_TIMEOUT, async {
            match &mut self.inner {
                #[cfg(unix)]
                ClientInner::Unix { reader, writer } => {
                    writer.write_all(payload.as_bytes()).await?;
                    writer.flush().await?;
                    let mut buf = String::new();
                    let bytes = reader.read_line(&mut buf).await?;
                    if bytes == 0 {
                        return Err(BridgeError::Io(std::io::Error::new(
                            std::io::ErrorKind::UnexpectedEof,
                            "IPC closed before reply",
                        )));
                    }
                    Ok::<String, BridgeError>(buf)
                }
                #[cfg(windows)]
                ClientInner::Pipe { pipe } => {
                    use tokio::io::AsyncReadExt;
                    pipe.write_all(payload.as_bytes()).await?;
                    pipe.flush().await?;
                    // Read until newline. Named pipes don't have BufReader split,
                    // so we read into a small buffer per call.
                    let mut out = Vec::with_capacity(256);
                    let mut chunk = [0u8; 256];
                    loop {
                        let n = pipe.read(&mut chunk).await?;
                        if n == 0 {
                            return Err(BridgeError::Io(std::io::Error::new(
                                std::io::ErrorKind::UnexpectedEof,
                                "IPC closed before reply",
                            )));
                        }
                        out.extend_from_slice(&chunk[..n]);
                        if out.contains(&b'\n') {
                            break;
                        }
                    }
                    Ok::<String, BridgeError>(String::from_utf8_lossy(&out).into_owned())
                }
            }
        })
        .await
        .map_err(|_| BridgeError::Protocol("IPC call timed out".into()))??;

        let trimmed = reply_text.trim_end();
        let parsed: IpcResponse = serde_json::from_str(trimmed)?;
        Ok(parsed)
    }

    /// Send `request_access` and parse the reply. The bridge waits up to
    /// CALL_TIMEOUT (5 minutes) so the user has time to click the consent
    /// dialog.
    pub async fn request_access(&mut self, client: &str, pid: u32) -> BridgeResult<IpcResponse> {
        self.send_frame(&json!({"op": "request_access", "client": client, "pid": pid}))
            .await
    }

    /// Fetch the current Supabase JWT. The Electron side reads keychain
    /// just-in-time; this never blocks on user interaction.
    pub async fn get_token(&mut self) -> BridgeResult<IpcResponse> {
        self.send_frame(&json!({"op": "get_token"})).await
    }
}

/// All possible reply frames the server emits. Matches electron/bridge-ipc.js.
#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
pub enum IpcResponse {
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "access_granted")]
    AccessGranted {
        #[serde(rename = "userId")]
        user_id: String,
    },
    #[serde(rename = "access_denied")]
    AccessDenied { reason: Option<String> },
    #[serde(rename = "token")]
    Token { value: String },
    #[serde(rename = "no_session")]
    NoSession,
    #[serde(rename = "error")]
    Error { reason: Option<String> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[test]
    fn missing_socket_maps_to_app_not_open() {
        // A closed app leaves no socket file: connect() returns NotFound.
        assert!(matches!(
            classify_connect_error(Error::from(ErrorKind::NotFound)),
            BridgeError::AppNotOpen
        ));
    }

    #[test]
    fn refused_socket_maps_to_app_not_open() {
        // A crashed app can leave a stale socket that refuses connections.
        assert!(matches!(
            classify_connect_error(Error::from(ErrorKind::ConnectionRefused)),
            BridgeError::AppNotOpen
        ));
    }

    #[test]
    fn other_io_errors_stay_generic() {
        assert!(matches!(
            classify_connect_error(Error::from(ErrorKind::PermissionDenied)),
            BridgeError::Io(_)
        ));
    }
}

//! MCP frame I/O over stdio.
//!
//! Per MCP protocol, JSON-RPC frames are exchanged over the transport. For stdio
//! transport, each frame is newline-delimited JSON (no Content-Length headers
//! like HTTP transport uses). Claude Code's MCP SDK conforms to this.
//!
//! We use a `BufReader` over stdin and a buffered stdout writer. Each line is
//! parsed as a JSON value at the proxy boundary; the bridge does not need to
//! understand the JSON-RPC method semantics — it forwards opaquely.

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Stdin, Stdout};

/// Reader bound to stdin, line-delimited JSON.
pub struct FrameReader {
    inner: BufReader<Stdin>,
    buf: String,
}

impl FrameReader {
    pub fn new() -> Self {
        Self {
            inner: BufReader::new(tokio::io::stdin()),
            buf: String::with_capacity(8192),
        }
    }

    /// Reads one JSON-RPC frame. Returns `None` on EOF (clean shutdown).
    pub async fn read_frame(&mut self) -> std::io::Result<Option<Value>> {
        self.buf.clear();
        let bytes = self.inner.read_line(&mut self.buf).await?;
        if bytes == 0 {
            return Ok(None); // EOF
        }
        let line = self.buf.trim_end();
        if line.is_empty() {
            // Defensive: ignore blank lines, recurse via box-future-free loop.
            return Box::pin(self.read_frame()).await;
        }
        let value: Value = serde_json::from_str(line).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("invalid JSON frame: {e}"))
        })?;
        Ok(Some(value))
    }
}

/// Writer bound to stdout, line-delimited JSON.
pub struct FrameWriter {
    inner: Stdout,
}

impl FrameWriter {
    pub fn new() -> Self {
        Self {
            inner: tokio::io::stdout(),
        }
    }

    /// Writes one JSON frame and flushes immediately so Claude Code sees the
    /// response without buffering delay.
    pub async fn write_frame(&mut self, frame: &Value) -> std::io::Result<()> {
        let mut s = serde_json::to_string(frame)?;
        s.push('\n');
        self.inner.write_all(s.as_bytes()).await?;
        self.inner.flush().await?;
        Ok(())
    }
}

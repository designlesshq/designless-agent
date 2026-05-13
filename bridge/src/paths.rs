//! Cross-platform path resolution for the bridge.
//!
//! - **IPC socket path** for anchored mode:
//!   - macOS: `${TMPDIR}/Designless.sock` — short enough for the 104-byte
//!     `sun_path` limit.
//!   - Linux: `${XDG_RUNTIME_DIR}/Designless/ipc.sock` with `/tmp` fallback.
//!   - Windows: `\\.\pipe\com.designless.canvas`.
//!
//! - **Auth file path** for standalone mode: `~/.designless/auth.json`.
//!
//! - **Keychain service names** (anchored mode reads only; Electron writes):
//!   `designless-canvas:access-token`, `designless-canvas:refresh-token`,
//!   `designless-mcp:granted` (consent grant flag).

use std::path::PathBuf;

/// IPC endpoint where the Designless desktop app listens (anchored mode).
pub fn ipc_endpoint() -> IpcEndpoint {
    #[cfg(target_os = "macos")]
    {
        let tmp = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into());
        IpcEndpoint::UnixSocket(PathBuf::from(tmp).join("Designless.sock"))
    }
    #[cfg(target_os = "linux")]
    {
        let runtime = std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        IpcEndpoint::UnixSocket(runtime.join("Designless").join("ipc.sock"))
    }
    #[cfg(target_os = "windows")]
    {
        IpcEndpoint::NamedPipe(r"\\.\pipe\com.designless.canvas".into())
    }
}

#[derive(Debug, Clone)]
pub enum IpcEndpoint {
    #[allow(dead_code)] // dead on Windows builds
    UnixSocket(PathBuf),
    #[allow(dead_code)] // dead on Unix builds
    NamedPipe(String),
}

/// `~/.designless/auth.json` — standalone-mode token storage. Used by phase 3.
#[allow(dead_code)]
pub fn standalone_auth_file() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".designless").join("auth.json"))
}

/// Keychain service names used by the Electron desktop app. Anchored mode reads;
/// never writes these (rotation is Electron's responsibility). Consumed by
/// phase 2.1 (keychain reader) and phase 4 (granted flag check).
#[allow(dead_code)]
pub mod keychain {
    pub const ACCESS_TOKEN: &str = "designless-canvas:access-token";
    pub const REFRESH_TOKEN: &str = "designless-canvas:refresh-token";
    pub const GRANT_FLAG: &str = "designless-mcp:granted";
}

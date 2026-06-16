//! Cross-platform path resolution for the bridge.
//!
//! - **IPC socket path** for anchored mode:
//!   - macOS: `/tmp/designless-<uid>/ipc.sock`. A stable per-user path that
//!     does NOT depend on `$TMPDIR`. The GUI app (LaunchServices Darwin temp)
//!     and a CLI-spawned bridge (the CLI's `$TMPDIR`) get different `$TMPDIR`
//!     values, so a `$TMPDIR`-derived path made the two sides miss each other.
//!     uid is identical in every launch context and stays well under the
//!     104-byte `sun_path` limit.
//!   - Linux: `${XDG_RUNTIME_DIR}/Designless/ipc.sock` with `/tmp` fallback.
//!   - Windows: `\\.\pipe\com.designless.canvas`.

use std::path::PathBuf;

/// IPC endpoint where the Designless desktop app listens (anchored mode).
pub fn ipc_endpoint() -> IpcEndpoint {
    #[cfg(target_os = "macos")]
    {
        IpcEndpoint::UnixSocket(macos_ipc_dir().join("ipc.sock"))
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

/// Per-user IPC directory on macOS: `/tmp/designless-<uid>`. Stable across
/// launch contexts (unlike `$TMPDIR`). The desktop app creates it `0700`,
/// owner-only; the connect side refuses any socket inside a dir that does not
/// pass `ipc_dir_is_safe`, so a socket another local user planted in the
/// shared `/tmp` is never trusted.
#[cfg(target_os = "macos")]
pub fn macos_ipc_dir() -> PathBuf {
    // getuid() is infallible and thread-safe per POSIX.
    PathBuf::from(format!("/tmp/designless-{}", unsafe { libc::getuid() }))
}

/// True iff `dir` is a real directory (not a symlink), owned by the current
/// uid, with no group/other permission bits (mode `0o700`). Checked before
/// connecting through the shared-`/tmp` IPC dir.
#[cfg(target_os = "macos")]
pub fn ipc_dir_is_safe(dir: &std::path::Path) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    match std::fs::symlink_metadata(dir) {
        Ok(md) => {
            md.file_type().is_dir()
                && md.uid() == unsafe { libc::getuid() }
                && (md.permissions().mode() & 0o077) == 0
        }
        Err(_) => false,
    }
}

#[derive(Debug, Clone)]
pub enum IpcEndpoint {
    #[allow(dead_code)] // dead on Windows builds
    UnixSocket(PathBuf),
    #[allow(dead_code)] // dead on Unix builds
    NamedPipe(String),
}

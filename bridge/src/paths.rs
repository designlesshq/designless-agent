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
//!
//! - **`DESIGNLESS_IPC_SOCKET`** names the endpoint explicitly (an absolute
//!   socket path on Unix, a pipe name on Windows) for a setup that runs more
//!   than one Designless app on a machine. Unset, the default above applies
//!   and nothing changes. On macOS the parent directory of an explicit path is
//!   still held to `ipc_dir_is_safe`.

use std::path::PathBuf;

/// The environment variable that names the endpoint explicitly.
pub const IPC_SOCKET_ENV: &str = "DESIGNLESS_IPC_SOCKET";

/// IPC endpoint where the Designless desktop app listens (anchored mode).
pub fn ipc_endpoint() -> IpcEndpoint {
    ipc_endpoint_from(std::env::var(IPC_SOCKET_ENV).ok().as_deref())
}

/// The endpoint for an explicit override, or the platform default when the
/// override is absent or blank. Pure, so the override rule is testable
/// without touching the process environment.
pub fn ipc_endpoint_from(explicit: Option<&str>) -> IpcEndpoint {
    if let Some(v) = explicit.map(str::trim).filter(|v| !v.is_empty()) {
        #[cfg(windows)]
        {
            return IpcEndpoint::NamedPipe(v.to_string());
        }
        #[cfg(not(windows))]
        {
            return IpcEndpoint::UnixSocket(PathBuf::from(v));
        }
    }
    default_ipc_endpoint()
}

fn default_ipc_endpoint() -> IpcEndpoint {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn an_explicit_path_is_used_verbatim() {
        match ipc_endpoint_from(Some("/tmp/designless-501/other.sock")) {
            IpcEndpoint::UnixSocket(p) => assert_eq!(p, PathBuf::from("/tmp/designless-501/other.sock")),
            _ => panic!("expected a unix socket"),
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn absent_or_blank_falls_back_to_the_default() {
        let default = match default_ipc_endpoint() {
            IpcEndpoint::UnixSocket(p) => p,
            _ => panic!("expected a unix socket"),
        };
        for v in [None, Some(""), Some("   ")] {
            match ipc_endpoint_from(v) {
                IpcEndpoint::UnixSocket(p) => assert_eq!(p, default),
                _ => panic!("expected a unix socket"),
            }
        }
        // The default is the address every shipped desktop app listens on.
        assert!(default.ends_with("ipc.sock"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_default_lives_in_the_per_user_dir() {
        match default_ipc_endpoint() {
            IpcEndpoint::UnixSocket(p) => assert_eq!(p.parent(), Some(macos_ipc_dir().as_path())),
            _ => panic!("expected a unix socket"),
        }
    }
}

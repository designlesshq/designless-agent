//! The desktop-app pre-flight, run before the bridge serves its first frame.
//!
//! The bridge authenticates through the Designless desktop app. On a machine
//! where the app is installed, that is the only correct path, so before
//! serving: detect the app, bring it up if it is installed but not running,
//! and wait briefly for its socket. The host spawns the bridge with no
//! interactive channel, so nothing here can ask; it decides and says why on
//! stderr, which the host's MCP panel shows.
//!
//! Escape hatches: `DESIGNLESS_BRIDGE_MODE` set explicitly skips detection;
//! `DESIGNLESS_BRIDGE_NO_AUTOLAUNCH` skips opening the app.
//!
//! Stdout is never written from here: it carries MCP frames only.

#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
const BUNDLE_ID: &str = "com.designless.canvas";

/// How the bridge should authenticate: `anchored` (through the desktop app)
/// or `standalone` (no desktop app on this machine).
#[cfg(target_os = "macos")]
pub async fn resolve_mode() -> String {
    // Respect an explicit choice (CI, power users, host overrides).
    if let Ok(explicit) = std::env::var("DESIGNLESS_BRIDGE_MODE") {
        if !explicit.is_empty() {
            return explicit;
        }
    }

    // An explicit endpoint names the app to reach. Detection and auto-launch
    // are about the standard app at its default address, so neither applies.
    let explicit_socket = std::env::var(crate::paths::IPC_SOCKET_ENV).unwrap_or_default();
    let explicit_socket = explicit_socket.trim();
    if !explicit_socket.is_empty() {
        if !probe_socket(Duration::from_millis(400)).await {
            eprintln!(
                "Designless: no app is answering at {explicit_socket} (DESIGNLESS_IPC_SOCKET). \
                 Open that app and sign in, then reconnect this MCP server from the /mcp panel."
            );
        }
        return "anchored".into();
    }

    // No desktop app: a genuine standalone environment.
    if !app_installed().await {
        return "standalone".into();
    }

    if probe_socket(Duration::from_millis(400)).await {
        return "anchored".into();
    }

    if std::env::var_os("DESIGNLESS_BRIDGE_NO_AUTOLAUNCH").is_some_and(|v| !v.is_empty()) {
        return "anchored".into();
    }

    eprintln!("Designless: desktop app is installed but not running \u{2014} opening it to authenticate\u{2026}");
    launch_app().await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while tokio::time::Instant::now() < deadline {
        if probe_socket(Duration::from_millis(500)).await {
            return "anchored".into();
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Still anchored, so the bridge shows an "open Designless" hint instead
    // of silently minting a second identity.
    eprintln!(
        "Designless: desktop app did not become ready in time. Open Designless and \
         sign in, then reconnect this MCP server from the /mcp panel."
    );
    "anchored".into()
}

#[cfg(not(target_os = "macos"))]
pub async fn resolve_mode() -> String {
    std::env::var("DESIGNLESS_BRIDGE_MODE").ok().filter(|m| !m.is_empty()).unwrap_or_else(|| "anchored".into())
}

#[cfg(target_os = "macos")]
async fn app_installed() -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        "/Applications/Designless.app".to_string(),
        format!("{home}/Applications/Designless.app"),
    ];
    if candidates.iter().any(|p| std::path::Path::new(p).exists()) {
        return true;
    }
    let query = format!("kMDItemCFBundleIdentifier == '{BUNDLE_ID}'");
    let mut cmd = tokio::process::Command::new("/usr/bin/mdfind");
    cmd.arg(query)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true);
    match tokio::time::timeout(Duration::from_secs(3), cmd.output()).await {
        Ok(Ok(out)) if out.status.success() => !String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        _ => false,
    }
}

/// Is the desktop app answering at the socket the bridge will use? Only
/// trusted inside a private, owner-only, non-symlink directory.
#[cfg(target_os = "macos")]
async fn probe_socket(timeout: Duration) -> bool {
    let crate::paths::IpcEndpoint::UnixSocket(path) = crate::paths::ipc_endpoint() else { return false };
    let dir = match path.parent() {
        Some(d) if d.as_os_str().is_empty() => std::path::Path::new("."),
        Some(d) => d,
        None => return false,
    };
    if !crate::paths::ipc_dir_is_safe(dir) {
        return false;
    }
    matches!(tokio::time::timeout(timeout, tokio::net::UnixStream::connect(&path)).await, Ok(Ok(_)))
}

#[cfg(target_os = "macos")]
async fn launch_app() -> bool {
    let open = |args: &[&str]| {
        let mut c = tokio::process::Command::new("/usr/bin/open");
        c.args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        c
    };
    if matches!(open(&["-b", BUNDLE_ID]).status().await, Ok(s) if s.success()) {
        return true;
    }
    matches!(open(&["-a", "Designless"]).status().await, Ok(s) if s.success())
}

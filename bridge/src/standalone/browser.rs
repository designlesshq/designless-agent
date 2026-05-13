//! Cross-platform browser launcher.
//!
//! - macOS: `open <url>`
//! - Windows: `cmd /c start "" "<url>"` (empty title prevents `start` from
//!   eating the URL as a title)
//! - Linux / Cowork: `xdg-open <url>` — in Cowork sandboxes, this is routed by
//!   the sandbox UI to the user's actual browser (same mechanism Claude Code's
//!   built-in OAuth flow uses today)
//!
//! Best-effort: failures are non-fatal because the caller has already printed
//! the URL to stderr. The user can copy-paste manually.

use crate::error::{BridgeError, BridgeResult};
use tokio::process::Command;
use url::Url;

pub async fn open(url: &Url) -> BridgeResult<()> {
    let url_str = url.as_str();
    let (cmd, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url_str])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/c", "start", "", url_str])
    } else {
        ("xdg-open", vec![url_str])
    };

    let status = Command::new(cmd)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| {
            BridgeError::NoBearer(format!(
                "could not spawn `{cmd}` to open browser: {e} — open this URL manually: {url_str}"
            ))
        })?;

    if !status.success() {
        return Err(BridgeError::NoBearer(format!(
            "`{cmd}` exited non-zero — open this URL manually: {url_str}"
        )));
    }
    Ok(())
}

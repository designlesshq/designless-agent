//! Atomic file-based token storage at `~/.designless/auth.json`.
//!
//! - Mode 0600 enforced on Unix via set_permissions after write, before rename.
//! - Atomic write: temp file → fsync → rename. Crash mid-write leaves the
//!   prior token intact.
//! - Schema versioned for future migration.
//! - Corrupt parse → treat as no file (warn to stderr, return None); caller
//!   triggers full PKCE flow.

use crate::error::{BridgeError, BridgeResult};
use crate::paths::standalone_auth_file;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthFile {
    pub schema_version: u32,
    pub access_token: String,
    pub refresh_token: String,
    /// RFC 3339 / ISO 8601 — when the access_token expires (UTC)
    pub expires_at: String,
    pub scope: String,
    /// RFC 3339 — when this token pair was issued (audit + debug aid)
    pub issued_at: String,
}

impl AuthFile {
    /// True if the access token is within `leeway_secs` of expiry, or already
    /// expired, or has an unparseable expires_at (defensive: treat unknown
    /// state as expired).
    pub fn is_access_expired(&self, leeway_secs: i64) -> bool {
        match chrono::DateTime::parse_from_rfc3339(&self.expires_at) {
            Ok(exp) => exp - chrono::Duration::seconds(leeway_secs) <= chrono::Utc::now(),
            Err(_) => true,
        }
    }
}

pub async fn load() -> BridgeResult<Option<AuthFile>> {
    let Some(path) = standalone_auth_file() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path).await?;
    match serde_json::from_str::<AuthFile>(&contents) {
        Ok(f) => Ok(Some(f)),
        Err(e) => {
            // Don't propagate corrupt-file error; clear path forward is re-auth.
            tracing::warn!(error = %e, path = %path.display(), "corrupt auth.json, will re-auth");
            Ok(None)
        }
    }
}

pub async fn save_atomic(file: &AuthFile) -> BridgeResult<()> {
    let path = standalone_auth_file()
        .ok_or_else(|| BridgeError::NoBearer("cannot resolve home directory for ~/.designless".into()))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let tmp = path.with_extension("json.new");
    let bytes = serde_json::to_vec_pretty(file)?;

    // Open + write + fsync + close (drop)
    {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)
            .await?;
        f.write_all(&bytes).await?;
        f.sync_all().await?;
    }

    // Tighten permissions BEFORE rename so the final file never appears with
    // permissive mode, even momentarily.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&tmp, perms)?;
    }

    fs::rename(&tmp, &path).await?;
    Ok(())
}

pub async fn delete() -> BridgeResult<()> {
    let Some(path) = standalone_auth_file() else {
        return Ok(());
    };
    if path.exists() {
        fs::remove_file(&path).await?;
    }
    Ok(())
}

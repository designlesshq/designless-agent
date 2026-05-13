//! OAuth 2.0 (RFC 6749) authorization-code grant with PKCE (RFC 7636),
//! against the Designless OAuth provider at `https://designless.app/less/oauth/*`.
//!
//! - `build_authorize_url` → constructs the URL the user opens in the browser
//! - `exchange_code`        → POST /less/oauth/token (grant_type=authorization_code)
//! - `refresh`              → POST /less/oauth/token (grant_type=refresh_token)
//!
//! Client identifier is the pre-seeded `designless-mcp-bridge` entry in
//! `KNOWN_CLIENTS` (server-side provider.ts). Loopback redirect validation at
//! the provider matches by host+protocol, so any 127.0.0.1:RANDOM_PORT works.

use crate::error::{BridgeError, BridgeResult};
use crate::standalone::storage::AuthFile;
use reqwest::Client;
use serde::Deserialize;
use url::Url;

const AUTHORIZE_URL: &str = "https://designless.app/less/oauth/authorize";
const TOKEN_URL: &str = "https://designless.app/less/oauth/token";
const CLIENT_ID: &str = "designless-mcp-bridge";
const SCOPE: &str = "less:mcp:full";

pub fn build_authorize_url(port: u16, state: &str, code_challenge: &str) -> Url {
    let mut url = Url::parse(AUTHORIZE_URL).expect("AUTHORIZE_URL is a valid constant URL");
    let redirect = format!("http://127.0.0.1:{port}/cb");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", &redirect)
        .append_pair("scope", SCOPE)
        .append_pair("state", state)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256");
    url
}

#[derive(Debug, Deserialize)]
struct TokenResponseRaw {
    access_token: String,
    refresh_token: String,
    expires_in: i64, // seconds until expiry
    scope: Option<String>,
    #[serde(rename = "token_type", default)]
    _token_type: String,
}

impl TokenResponseRaw {
    fn into_auth_file(self) -> AuthFile {
        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::seconds(self.expires_in);
        AuthFile {
            schema_version: 1,
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at: expires_at.to_rfc3339(),
            scope: self.scope.unwrap_or_else(|| SCOPE.into()),
            issued_at: now.to_rfc3339(),
        }
    }
}

pub async fn exchange_code(
    client: &Client,
    code: &str,
    code_verifier: &str,
    port: u16,
) -> BridgeResult<AuthFile> {
    let redirect_uri = format!("http://127.0.0.1:{port}/cb");
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &redirect_uri),
            ("client_id", CLIENT_ID),
            ("code_verifier", code_verifier),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(BridgeError::UpstreamStatus { status, body });
    }

    let raw: TokenResponseRaw = resp.json().await?;
    Ok(raw.into_auth_file())
}

pub async fn refresh(client: &Client, refresh_token: &str) -> BridgeResult<AuthFile> {
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", CLIENT_ID),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(BridgeError::UpstreamStatus { status, body });
    }

    let raw: TokenResponseRaw = resp.json().await?;
    Ok(raw.into_auth_file())
}

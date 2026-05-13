//! Loopback HTTP listener for the OAuth callback.
//!
//! Binds `127.0.0.1:0` (kernel-picked port), serves a single GET, returns
//! `(code, state)` extracted from query string. The user sees a small success
//! page so they know they can close the tab.
//!
//! Timeout is 5 minutes — long enough for a slow consent flow (Google SSO,
//! email-verify, etc.) without leaving the bridge hung forever.

use crate::error::{BridgeError, BridgeResult};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task;
use url::Url;

const LISTENER_TIMEOUT: Duration = Duration::from_secs(300);

const SUCCESS_BODY: &str = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>Designless — authorized</title>
<style>body{font-family:-apple-system,BlinkMacSystemFont,Inter,Segoe UI,system-ui,sans-serif;background:#0a0a0f;color:#fff;text-align:center;padding:6rem 2rem;margin:0}h2{font-weight:600;letter-spacing:-0.01em;margin:0 0 0.5rem}p{color:rgba(255,255,255,0.6);margin:0}</style>
</head><body><h2>You're connected.</h2><p>You can close this window and return to Claude Code.</p></body></html>"#;

const ERROR_BODY: &str = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>Designless — authorization failed</title>
<style>body{font-family:-apple-system,BlinkMacSystemFont,Inter,Segoe UI,system-ui,sans-serif;background:#0a0a0f;color:#fff;text-align:center;padding:6rem 2rem;margin:0}h2{font-weight:600;letter-spacing:-0.01em;margin:0 0 0.5rem}p{color:rgba(255,255,255,0.6);margin:0}</style>
</head><body><h2>Authorization didn't complete.</h2><p>You can close this window and try again.</p></body></html>"#;

pub struct CallbackResult {
    pub code: String,
    pub state: String,
}

pub struct ListenerHandle {
    pub port: u16,
    rx: oneshot::Receiver<BridgeResult<CallbackResult>>,
}

impl ListenerHandle {
    pub async fn wait(self) -> BridgeResult<CallbackResult> {
        let outer = tokio::time::timeout(LISTENER_TIMEOUT, self.rx).await;
        match outer {
            Err(_) => Err(BridgeError::NoBearer(
                "OAuth callback timed out after 5 minutes — re-run and complete consent in the browser".into(),
            )),
            Ok(Err(_)) => Err(BridgeError::NoBearer("listener task died unexpectedly".into())),
            Ok(Ok(result)) => result,
        }
    }
}

pub fn start() -> BridgeResult<ListenerHandle> {
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| BridgeError::NoBearer(format!("could not bind localhost listener: {e}")))?;
    let port = server.server_addr().to_ip().map(|a| a.port()).ok_or_else(|| {
        BridgeError::NoBearer("listener address has no port".into())
    })?;
    let (tx, rx) = oneshot::channel();

    task::spawn_blocking(move || {
        let result = listen_for_callback(server);
        let _ = tx.send(result);
    });

    Ok(ListenerHandle { port, rx })
}

fn listen_for_callback(server: tiny_http::Server) -> BridgeResult<CallbackResult> {
    // Block waiting for the single OAuth callback.
    let request = server
        .recv()
        .map_err(|e| BridgeError::NoBearer(format!("listener accept failed: {e}")))?;

    // Parse URL — request.url() returns the path + query only, prefix with scheme+host
    let full = format!("http://127.0.0.1{}", request.url());
    let url = Url::parse(&full)
        .map_err(|e| BridgeError::Protocol(format!("malformed callback URL: {e}")))?;

    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            _ => {}
        }
    }

    let body = if error.is_some() || code.is_none() { ERROR_BODY } else { SUCCESS_BODY };
    let response = tiny_http::Response::from_string(body).with_header(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
            .expect("static header bytes are valid"),
    );
    let _ = request.respond(response);

    if let Some(err) = error {
        return Err(BridgeError::NoBearer(format!("OAuth provider returned error: {err}")));
    }
    match (code, state) {
        (Some(c), Some(s)) => Ok(CallbackResult { code: c, state: s }),
        _ => Err(BridgeError::Protocol("callback missing code or state parameter".into())),
    }
}

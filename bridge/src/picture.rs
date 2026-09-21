//! A picture the agent picks from disk is stored before the call goes up.
//!
//! `less_canvas_set_image` with `source.local_file` used to carry only the
//! path: the server wrote a pointer to it, the desktop read the file off the
//! user's disk (macOS asked for permission the first time, mid-compose),
//! downscaled it, stored it, committed the store id back as a ledger op and
//! checkpointed; four writes and a hash move for one picture, and the slot
//! showed a spinner until the dialog was answered (2026-09-20). The bridge
//! runs on the same machine as the agent, as the user, with the file in
//! reach and the user's bearer in hand: it reads the picture here, scales it
//! to the size the canvas would have (max edge 2160, encoded under the
//! desktop's own 300 KB data-URL ceiling), names it by its bytes exactly as
//! the desktop does (`a` + FNV-1a 64 over `mime + ' ' + base64`), stores it
//! in the session's body store, and rewrites the call to carry the store
//! pointer. The server then writes the slot as resolved; the desktop paints
//! it from the store on its next frame. No bytes ever ride the agent's
//! context: the picture goes bridge to server and nowhere else.
//!
//! A picture the bridge cannot prepare (a format it does not decode, a call
//! with no session id, a store that refuses) goes up unchanged, and the
//! desktop path takes it as before. Nothing here can fail a set_image.
use base64::Engine;
use image::ImageReader;
use reqwest::Client;
use serde_json::{json, Value};
use std::io::Cursor;
use std::path::Path;

/// The canvas's own raster edge at 2x (raster-cap.js: box max edge × 2 on a
/// 1080-wide sheet), and its data-URL ceiling (RASTER_MAX_BYTES).
pub const MAX_EDGE: u32 = 2160;
pub const MAX_DATA_URL_LEN: usize = 300_000;
/// A file larger than this is not read at all.
pub const MAX_FILE_BYTES: u64 = 40 * 1024 * 1024;
const JPEG_QUALITIES: [u8; 4] = [85, 78, 70, 60];

/// The picture, prepared: what goes into the store and the name it goes under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub data_url: String,
    pub hash: String,
    pub mime: &'static str,
}

/// The pointer scheme the canvas resolves (asset-hash.js ASSET_POINTER_SCHEME).
pub const POINTER_SCHEME: &str = "designless-asset:";

/// FNV-1a 64 over the string's UTF-16 code units, sixteen lowercase hex:
/// asset-hash.js `fnv1a64`, byte for byte. The inputs here are a mime and a
/// base64 payload, both ASCII, so code units and bytes agree.
pub fn fnv1a64(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for cu in s.encode_utf16() {
        h ^= cu as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// `a` + fnv1a64(mime + ' ' + base64): asset-hash.js `assetHash`.
pub fn asset_hash(mime: &str, b64: &str) -> String {
    format!("a{}", fnv1a64(&format!("{mime} {b64}")))
}

/// Decode, scale to the canvas's edge, and encode under its ceiling. JPEG
/// for a photograph, PNG kept for a picture with transparency. None when the
/// bytes are not a picture this bridge decodes, or cannot be brought under
/// the ceiling.
pub fn prepare_bytes(bytes: &[u8]) -> Option<Prepared> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?;
    let format = reader.format()?;
    let mut img = reader.decode().ok()?;
    let has_alpha = img.color().has_alpha() && format == image::ImageFormat::Png;
    let (w, h) = (img.width(), img.height());
    let longest = w.max(h);
    if longest > MAX_EDGE {
        let scale = MAX_EDGE as f64 / longest as f64;
        let nw = ((w as f64 * scale).round() as u32).max(1);
        let nh = ((h as f64 * scale).round() as u32).max(1);
        img = img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3);
    }
    let b64 = base64::engine::general_purpose::STANDARD;
    if has_alpha {
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
        let encoded = b64.encode(&out);
        let data_url = format!("data:image/png;base64,{encoded}");
        if data_url.len() <= MAX_DATA_URL_LEN {
            return Some(Prepared { hash: asset_hash("image/png", &encoded), data_url, mime: "image/png" });
        }
        // A transparent picture over the ceiling is left to the desktop.
        return None;
    }
    // A photograph: JPEG, quality stepping down, then the edge, until it fits.
    let mut current = img.to_rgb8();
    for _ in 0..4 {
        for q in JPEG_QUALITIES {
            let mut out = Vec::new();
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q);
            if enc.encode_image(&current).is_err() {
                return None;
            }
            let encoded = b64.encode(&out);
            let data_url = format!("data:image/jpeg;base64,{encoded}");
            if data_url.len() <= MAX_DATA_URL_LEN {
                return Some(Prepared { hash: asset_hash("image/jpeg", &encoded), data_url, mime: "image/jpeg" });
            }
        }
        let (cw, ch) = (current.width(), current.height());
        if cw.max(ch) <= 320 {
            break;
        }
        current = image::imageops::resize(&current, (cw * 3 / 4).max(1), (ch * 3 / 4).max(1), image::imageops::FilterType::Triangle);
    }
    None
}

/// Read and prepare a picture from disk. None for a path that cannot be
/// read, is too large, or is not a picture this bridge decodes.
pub async fn prepare_file(path: &Path) -> Option<Prepared> {
    let meta = tokio::fs::metadata(path).await.ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    let bytes = tokio::fs::read(path).await.ok()?;
    tokio::task::spawn_blocking(move || prepare_bytes(&bytes)).await.ok()?
}

/// The set_image call's local file and session, when the call carries both.
pub fn local_picture_call(frame: &Value) -> Option<(String, String)> {
    if frame.get("method")?.as_str()? != "tools/call" {
        return None;
    }
    let params = frame.get("params")?;
    if params.get("name")?.as_str()? != "less_canvas_set_image" {
        return None;
    }
    let args = params.get("arguments")?;
    let session = args.get("session_id")?.as_str()?.to_string();
    let path = args.get("source")?.get("local_file")?.as_str()?.to_string();
    if session.is_empty() || path.is_empty() {
        return None;
    }
    Some((path, session))
}

/// The same call, carrying the store pointer in place of the path.
pub fn with_pointer(frame: &Value, hash: &str) -> Value {
    let mut out = frame.clone();
    if let Some(source) = out
        .get_mut("params")
        .and_then(|p| p.get_mut("arguments"))
        .and_then(|a| a.get_mut("source"))
    {
        let alt = source.get("alt").cloned();
        *source = json!({ "asset": format!("{POINTER_SCHEME}{hash}") });
        if let Some(alt) = alt {
            source["alt"] = alt;
        }
    }
    out
}

/// Store the prepared picture in the session's body store. Ok(()) when the
/// store took it (or already held it); Err with the reason otherwise.
pub async fn store_picture(client: &Client, editor: &str, bearer: &str, session_id: &str, prepared: &Prepared) -> Result<(), String> {
    let res = client
        .post(format!("{editor}/less/canvas-frame-body"))
        .bearer_auth(bearer)
        .header("content-type", "application/json")
        .header("x-region", crate::proxy::edge_region())
        .json(&json!({
            "op": "store",
            "session_id": session_id,
            "bodies": [{ "content_hash": prepared.hash, "body": prepared.data_url }],
        }))
        .send()
        .await
        .map_err(|e| format!("the store did not answer: {e}"))?;
    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("the store answered {}: {}", status.as_u16(), body.chars().take(200).collect::<String>()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_the_desktops() {
        // asset-hash.js vectors: fnv1a64 of "" and of a short string.
        assert_eq!(fnv1a64(""), "cbf29ce484222325");
        assert_eq!(fnv1a64("a"), "af63dc4c8601ec8c");
        assert_eq!(asset_hash("image/png", "AAAA"), format!("a{}", fnv1a64("image/png AAAA")));
    }

    #[test]
    fn a_photograph_is_scaled_to_the_edge_and_under_the_ceiling() {
        let img = image::RgbImage::from_fn(3000, 2000, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(img).write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        let p = prepare_bytes(&bytes).expect("prepared");
        assert_eq!(p.mime, "image/jpeg");
        assert!(p.data_url.starts_with("data:image/jpeg;base64,"));
        assert!(p.data_url.len() <= MAX_DATA_URL_LEN);
        assert!(p.hash.starts_with('a') && p.hash.len() == 17);
        let back = ImageReader::new(Cursor::new(base64::engine::general_purpose::STANDARD.decode(&p.data_url["data:image/jpeg;base64,".len()..]).unwrap())).with_guessed_format().unwrap().decode().unwrap();
        assert!(back.width().max(back.height()) <= MAX_EDGE);
    }

    #[test]
    fn a_transparent_picture_stays_png() {
        let img = image::RgbaImage::from_fn(64, 64, |x, _| image::Rgba([0, 0, 0, if x % 2 == 0 { 0 } else { 255 }]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(img).write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        let p = prepare_bytes(&bytes).expect("prepared");
        assert_eq!(p.mime, "image/png");
    }

    #[test]
    fn not_a_picture_is_left_alone() {
        assert!(prepare_bytes(b"hello, not a picture").is_none());
    }

    #[test]
    fn only_a_set_image_call_with_a_session_and_a_local_file_is_taken() {
        let f = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "less_canvas_set_image", "arguments": { "session_id": "s", "slot_id": "image_01", "source": { "local_file": "/x.jpg" }, "alt": "Cover" } } });
        assert_eq!(local_picture_call(&f), Some(("/x.jpg".into(), "s".into())));
        let no_session = json!({ "method": "tools/call", "params": { "name": "less_canvas_set_image", "arguments": { "artefact_id": "d", "source": { "local_file": "/x.jpg" } } } });
        assert_eq!(local_picture_call(&no_session), None);
        let url = json!({ "method": "tools/call", "params": { "name": "less_canvas_set_image", "arguments": { "session_id": "s", "source": { "url": "https://x/y.jpg" } } } });
        assert_eq!(local_picture_call(&url), None);
        let other = json!({ "method": "tools/call", "params": { "name": "less_canvas_compose", "arguments": {} } });
        assert_eq!(local_picture_call(&other), None);
        let rewritten = with_pointer(&f, "a0123456789abcdef");
        assert_eq!(rewritten["params"]["arguments"]["source"], json!({ "asset": "designless-asset:a0123456789abcdef" }));
        assert_eq!(rewritten["params"]["arguments"]["alt"], json!("Cover"));
        assert_eq!(rewritten["params"]["arguments"]["slot_id"], json!("image_01"));
    }
}

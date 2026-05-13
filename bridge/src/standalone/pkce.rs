//! PKCE (RFC 7636) verifier + challenge generation, plus state parameter.
//!
//! - Verifier: 64 random bytes → 86-char URL-safe base64 (within the 43–128
//!   range the spec allows).
//! - Challenge: BASE64URL(SHA-256(verifier)).
//! - State: 16 random bytes → 22-char URL-safe base64. Used to mitigate CSRF
//!   on the authorization-callback hop.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

const VERIFIER_BYTES: usize = 64;
const STATE_BYTES: usize = 16;

pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
}

impl PkcePair {
    pub fn new() -> Self {
        let mut bytes = [0u8; VERIFIER_BYTES];
        rand::thread_rng().fill_bytes(&mut bytes);
        let verifier = URL_SAFE_NO_PAD.encode(bytes);
        let challenge = challenge_for(&verifier);
        Self { verifier, challenge }
    }
}

fn challenge_for(verifier: &str) -> String {
    let mut h = Sha256::new();
    h.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(h.finalize())
}

pub fn random_state() -> String {
    let mut bytes = [0u8; STATE_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

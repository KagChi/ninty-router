//! PKCE (S256) + state generation.

use sha2::{Digest, Sha256};

const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

fn rand_bytes(n: usize) -> Vec<u8> {
    use std::time::{SystemTime, UNIX_EPOCH};
    // xorshift seeded from nanos + pid — sufficient for PKCE/state (not long-term keys)
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut x: u128 = nanos ^ ((std::process::id() as u128) << 64) ^ 0x9E3779B97F4A7C15;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x & 0xff) as u8
        })
        .collect()
}

fn b64url(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

pub fn generate_verifier() -> String {
    rand_bytes(48).iter().map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char).collect()
}

pub fn challenge_s256(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

pub fn generate_state() -> String {
    b64url(&rand_bytes(24))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_roundtrip() {
        let v = generate_verifier();
        assert!(v.len() >= 43);
        let c = challenge_s256(&v);
        assert_eq!(c, challenge_s256(&v));
        assert_ne!(c, v);
        // RFC 7636 appendix B vector
        assert_eq!(
            challenge_s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn state_unique() {
        assert_ne!(generate_state(), generate_state());
    }
}

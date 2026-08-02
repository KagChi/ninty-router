//! Dashboard session auth: bcrypt password + signed JWT cookie.

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use ninty_core::error::{Error, Result};
use serde::{Deserialize, Serialize};

pub const SESSION_COOKIE: &str = "ninty_session";
const SESSION_TTL_SECS: i64 = 7 * 24 * 3600;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: i64,
}

fn jwt_secret() -> Vec<u8> {
    format!("{}:session", ninty_core::config::api_key_secret()).into_bytes()
}

pub fn create_session() -> Result<String> {
    let claims = Claims {
        sub: "admin".into(),
        exp: chrono::Utc::now().timestamp() + SESSION_TTL_SECS,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(&jwt_secret()),
    )
    .map_err(|e| Error::Internal(format!("jwt encode: {e}")))
}

pub fn verify_session(token: &str) -> bool {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(&jwt_secret()),
        &Validation::default(),
    )
    .is_ok()
}

pub fn hash_password(password: &str) -> Result<String> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| Error::Internal(format!("bcrypt: {e}")))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

/// Extract session token from a Cookie header value.
pub fn token_from_cookie_header(cookie_header: &str) -> Option<String> {
    cookie_header.split(';').find_map(|part| {
        let mut kv = part.trim().splitn(2, '=');
        match (kv.next(), kv.next()) {
            (Some(k), Some(v)) if k == SESSION_COOKIE => Some(v.to_string()),
            _ => None,
        }
    })
}

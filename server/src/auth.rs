use actix_web::http::header::HeaderMap;
use anyhow::Result;
use argon2::{password_hash::SaltString, Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use jsonwebtoken::{encode, decode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!(e))?
        .to_string();
    Ok(password_hash)
}

pub fn verify_password(hash: &str, password: &str) -> Result<bool> {
    let parsed = PasswordHash::new(hash).map_err(|e| anyhow::anyhow!(e))?;
    Ok(Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

fn jwt_secret() -> Vec<u8> {
    env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".to_string()).into_bytes()
}

pub fn create_token(username: &str, expires_seconds: i64) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    let exp = (now + expires_seconds) as usize;
    let claims = Claims { sub: username.to_string(), exp };
    let header = Header::new(Algorithm::HS256);
    let token = encode(&header, &claims, &EncodingKey::from_secret(&jwt_secret()))?;
    Ok(token)
}

pub fn decode_token(token: &str) -> Result<String> {
    let mut v = Validation::new(Algorithm::HS256);
    v.validate_exp = true;
    let token_data = decode::<Claims>(token, &DecodingKey::from_secret(&jwt_secret()), &v)?;
    Ok(token_data.claims.sub)
}

pub fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers.get("Authorization").and_then(|hv| hv.to_str().ok()).and_then(|s| {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() == 2 && parts[0].to_lowercase() == "bearer" {
            Some(parts[1].to_string())
        } else {
            None
        }
    })
}

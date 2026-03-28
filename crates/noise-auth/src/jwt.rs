use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("Token creation failed: {0}")]
    Creation(String),
    #[error("Token validation failed: {0}")]
    Validation(String),
    #[error("Token expired")]
    Expired,
}

/// JWT claims payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject: user UUID.
    pub sub: String,
    /// Username.
    pub username: String,
    /// Role.
    pub role: String,
    /// Issued at (Unix timestamp).
    pub iat: u64,
    /// Expiry (Unix timestamp).
    pub exp: u64,
}

pub struct TokenService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    /// Token validity in seconds (default: 86400 = 24h).
    pub ttl_seconds: u64,
}

impl TokenService {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
            ttl_seconds: 86400,
        }
    }

    pub fn issue(&self, user_id: Uuid, username: &str, role: &str) -> Result<String, JwtError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let claims = Claims {
            sub: user_id.to_string(),
            username: username.to_owned(),
            role: role.to_owned(),
            iat: now,
            exp: now + self.ttl_seconds,
        };
        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| JwtError::Creation(e.to_string()))
    }

    pub fn verify(&self, token: &str) -> Result<Claims, JwtError> {
        decode::<Claims>(token, &self.decoding_key, &Validation::default())
            .map(|td| td.claims)
            .map_err(|e| {
                if e.to_string().contains("expired") {
                    JwtError::Expired
                } else {
                    JwtError::Validation(e.to_string())
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_and_verify_roundtrip() {
        let svc = TokenService::new(b"test-secret-key-32-bytes-minimum!!");
        let uid = Uuid::new_v4();
        let token = svc.issue(uid, "alice", "analyst").unwrap();
        let claims = svc.verify(&token).unwrap();
        assert_eq!(claims.username, "alice");
        assert_eq!(claims.role, "analyst");
        assert_eq!(claims.sub, uid.to_string());
    }

    #[test]
    fn tampered_token_fails_verification() {
        let svc = TokenService::new(b"test-secret-key-32-bytes-minimum!!");
        let mut token = svc.issue(Uuid::new_v4(), "bob", "viewer").unwrap();
        token.push_str("tampered");
        assert!(svc.verify(&token).is_err());
    }
}

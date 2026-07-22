use jiff::{Timestamp, ToSpan};

use crate::error::AuthError;

/// Apple's OIDC quirk (duck-voyage.md §2): the client secret is not a static
/// string but a short-lived ES256-signed JWT derived from an Apple-issued
/// private key. Config holds the key and IDs; the secret is minted at runtime
/// (fresh per token exchange, so expiry never bites).
pub struct AppleSecret {
    pub team_id: String,
    pub key_id: String,
    pub private_key_pem: String,
}

#[derive(serde::Serialize)]
struct AppleClaims<'a> {
    iss: &'a str,
    iat: i64,
    exp: i64,
    aud: &'a str,
    sub: &'a str,
}

impl AppleSecret {
    /// Mint a client-secret JWT valid for 30 minutes — plenty for one token
    /// exchange, far below Apple's 6-month maximum.
    pub fn mint(&self, client_id: &str) -> Result<String, AuthError> {
        let now = Timestamp::now();
        let claims = AppleClaims {
            iss: &self.team_id,
            iat: now.as_second(),
            exp: (now + 30.minutes()).as_second(),
            aud: "https://appleid.apple.com",
            sub: client_id,
        };
        let header = {
            let mut h = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
            h.kid = Some(self.key_id.clone());
            h
        };
        let key = jsonwebtoken::EncodingKey::from_ec_pem(self.private_key_pem.as_bytes())
            .map_err(|e| AuthError::AppleSecret(format!("bad private key: {e}")))?;
        jsonwebtoken::encode(&header, &claims, &key)
            .map_err(|e| AuthError::AppleSecret(e.to_string()))
    }
}

use jiff::{Timestamp, ToSpan};

use crate::error::AuthError;

/// The `user` form field Apple includes on the **first** authorization
/// callback only — the sole place Apple ever discloses the user's name (it
/// is never in the id_token). Miss it and the name is gone forever.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct AppleCallbackUser {
    name: Option<AppleUserName>,
    email: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppleUserName {
    first_name: Option<String>,
    last_name: Option<String>,
}

impl AppleCallbackUser {
    /// Parse the raw JSON form value. `None` on malformed input — this is
    /// best-effort enrichment, never a login failure.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        serde_json::from_str(raw).ok()
    }

    pub(crate) fn display_name(&self) -> Option<String> {
        let name = self.name.as_ref()?;
        let full = [name.first_name.as_deref(), name.last_name.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        (!full.trim().is_empty()).then(|| full.trim().to_owned())
    }

    pub(crate) fn email(&self) -> Option<&str> {
        self.email.as_deref().filter(|e| !e.is_empty())
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_first_callback_user_payload() {
        let raw = r#"{"name":{"firstName":"Jane","lastName":"Mallard"},"email":"jane@example.com"}"#;
        let user = AppleCallbackUser::parse(raw).expect("valid payload");
        assert_eq!(user.display_name().as_deref(), Some("Jane Mallard"));
        assert_eq!(user.email(), Some("jane@example.com"));
    }

    #[test]
    fn tolerates_partial_and_garbage_payloads() {
        let first_only = AppleCallbackUser::parse(r#"{"name":{"firstName":"Jane"}}"#).unwrap();
        assert_eq!(first_only.display_name().as_deref(), Some("Jane"));
        assert_eq!(first_only.email(), None);
        assert!(AppleCallbackUser::parse(r#"{}"#).unwrap().display_name().is_none());
        assert!(AppleCallbackUser::parse("not json").is_none());
    }
}

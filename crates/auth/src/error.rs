#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("unknown OIDC provider {0:?}")]
    UnknownProvider(String),
    #[error("OIDC discovery failed for {provider}: {detail}")]
    Discovery { provider: String, detail: String },
    #[error("login flow not found (expired, replayed, or forged callback)")]
    UnknownFlow,
    #[error("state mismatch on OIDC callback")]
    StateMismatch,
    #[error("token exchange failed: {0}")]
    TokenExchange(String),
    #[error("id_token missing or invalid: {0}")]
    IdToken(String),
    #[error("Apple client secret could not be minted: {0}")]
    AppleSecret(String),
    #[error("invalid configuration: {0}")]
    Config(String),
}

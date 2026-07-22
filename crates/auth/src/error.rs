#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("unknown OIDC provider {0:?}")]
    UnknownProvider(String),
    #[error("OIDC discovery failed for {provider}: {detail}")]
    Discovery { provider: String, detail: String },
    #[error("no login flow in session (expired or replayed callback)")]
    NoFlowInSession,
    #[error("state mismatch on OIDC callback")]
    StateMismatch,
    #[error("token exchange failed: {0}")]
    TokenExchange(String),
    #[error("id_token missing or invalid: {0}")]
    IdToken(String),
    #[error("session store error: {0}")]
    Session(#[from] tower_sessions::session::Error),
    #[error("Apple client secret could not be minted: {0}")]
    AppleSecret(String),
    #[error("invalid configuration: {0}")]
    Config(String),
}

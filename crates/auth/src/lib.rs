//! OIDC + session glue: provider discovery, the PKCE authorization-code
//! flow, and the typed session extractors that make unauthenticated writes
//! unrepresentable in handler signatures.

mod apple;
mod error;
mod providers;
mod session;

pub use apple::AppleSecret;
pub use error::AuthError;
pub use providers::{
    LoginFlow, OidcIdentity, OidcProviderConfig, OidcProviders, ProviderSummary, SecretSource,
};
pub use session::{AdminUser, AuthenticatedUser, SessionUser};

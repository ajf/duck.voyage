use domain::OidcSubject;
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope,
};

use crate::apple::AppleSecret;
use crate::error::AuthError;

/// How a provider's client secret is obtained.
pub enum SecretSource {
    Static(String),
    /// Apple: minted at runtime from the configured key (see [`AppleSecret`]).
    Apple(AppleSecret),
}

/// One configured provider, pre-discovery.
pub struct OidcProviderConfig {
    /// URL path slug and config key: "google", "entra", "apple", "keycloak".
    pub slug: String,
    /// Human label for the login page button.
    pub display_name: String,
    pub issuer_url: String,
    pub client_id: String,
    pub secret: SecretSource,
}

struct ProviderHandle {
    slug: String,
    display_name: String,
    metadata: CoreProviderMetadata,
    client_id: String,
    secret: SecretSource,
}

impl ProviderHandle {
    /// Apple deviates from the common path in several ways (scopes, response
    /// mode, token-endpoint auth, where the name arrives); everything checks
    /// this rather than string-matching slugs.
    fn is_apple(&self) -> bool {
        matches!(self.secret, SecretSource::Apple(_))
    }

    /// Apple only accepts the literal scopes `name` and `email`; everyone
    /// else speaks standard `profile` + `email`.
    fn scopes(&self) -> Vec<Scope> {
        let names: &[&str] = if self.is_apple() { &["name", "email"] } else { &["profile", "email"] };
        names.iter().map(|s| Scope::new((*s).into())).collect()
    }
}

/// What the login page needs to render a provider button.
#[derive(Clone)]
pub struct ProviderSummary {
    pub slug: String,
    pub display_name: String,
}

/// The verified outcome of a completed login.
pub struct OidcIdentity {
    pub subject: OidcSubject,
    pub display_name: Option<String>,
    pub email: Option<String>,
}

/// The interim state of one login: minted at `begin`, consumed at
/// `complete`. The caller persists it keyed by `state` — **not** in the
/// session, because Apple's form_post callback is a cross-site POST that
/// arrives without cookies. Possession of the unguessable `state` token is
/// the retrieval credential.
pub struct LoginFlow {
    /// The OIDC `state` parameter (a CSRF token) — the storage key.
    pub state: String,
    pub provider: String,
    pub pkce_verifier: String,
    pub nonce: String,
    pub return_to: Option<String>,
}

/// All configured providers, discovered once at boot. Login begins with a
/// redirect to the provider and completes on the callback; the interim
/// [`LoginFlow`] is persisted by the caller between the two.
pub struct OidcProviders {
    providers: Vec<ProviderHandle>,
    redirect_base: String,
    http: openidconnect::reqwest::Client,
}

impl OidcProviders {
    /// Discover metadata for every configured provider. Fails fast: a
    /// misconfigured provider is a boot error, not a runtime surprise.
    pub async fn discover(
        configs: Vec<OidcProviderConfig>,
        base_url: &str,
    ) -> Result<Self, AuthError> {
        let http = openidconnect::reqwest::ClientBuilder::new()
            // OIDC requires resisting SSRF-style redirects on provider endpoints.
            .redirect(openidconnect::reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| AuthError::Config(e.to_string()))?;
        let mut providers = Vec::new();
        for config in configs {
            let issuer = IssuerUrl::new(config.issuer_url.clone())
                .map_err(|e| AuthError::Config(format!("{}: {e}", config.slug)))?;
            let metadata = CoreProviderMetadata::discover_async(issuer, &http)
                .await
                .map_err(|e| AuthError::Discovery {
                    provider: config.slug.clone(),
                    detail: e.to_string(),
                })?;
            providers.push(ProviderHandle {
                slug: config.slug,
                display_name: config.display_name,
                metadata,
                client_id: config.client_id,
                secret: config.secret,
            });
        }
        Ok(Self {
            providers,
            redirect_base: base_url.trim_end_matches('/').to_owned(),
            http,
        })
    }

    pub fn summaries(&self) -> Vec<ProviderSummary> {
        self.providers
            .iter()
            .map(|p| ProviderSummary {
                slug: p.slug.clone(),
                display_name: p.display_name.clone(),
            })
            .collect()
    }

    fn handle(&self, slug: &str) -> Result<&ProviderHandle, AuthError> {
        self.providers
            .iter()
            .find(|p| p.slug == slug)
            .ok_or_else(|| AuthError::UnknownProvider(slug.to_owned()))
    }

    fn client(
        &self,
        handle: &ProviderHandle,
    ) -> Result<
        CoreClient<
            openidconnect::EndpointSet,
            openidconnect::EndpointNotSet,
            openidconnect::EndpointNotSet,
            openidconnect::EndpointNotSet,
            openidconnect::EndpointMaybeSet,
            openidconnect::EndpointMaybeSet,
        >,
        AuthError,
    > {
        let secret = match &handle.secret {
            SecretSource::Static(s) => s.clone(),
            SecretSource::Apple(apple) => apple.mint(&handle.client_id)?,
        };
        let redirect = RedirectUrl::new(format!(
            "{}/auth/callback/{}",
            self.redirect_base, handle.slug
        ))
        .map_err(|e| AuthError::Config(e.to_string()))?;
        let client = CoreClient::from_provider_metadata(
            handle.metadata.clone(),
            ClientId::new(handle.client_id.clone()),
            Some(ClientSecret::new(secret)),
        )
        .set_redirect_uri(redirect);
        // Apple's token endpoint rejects HTTP Basic client auth; the secret
        // must travel in the request body.
        Ok(match handle.is_apple() {
            true => client.set_auth_type(openidconnect::AuthType::RequestBody),
            false => client,
        })
    }

    /// Build the provider redirect URL and the flow state the caller must
    /// persist until the callback.
    pub fn begin(
        &self,
        slug: &str,
        return_to: Option<String>,
    ) -> Result<(openidconnect::url::Url, LoginFlow), AuthError> {
        let handle = self.handle(slug)?;
        let client = self.client(handle)?;
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .set_pkce_challenge(pkce_challenge);
        request = handle
            .scopes()
            .into_iter()
            .fold(request, |req, scope| req.add_scope(scope));
        if handle.is_apple() {
            // Apple requires form_post whenever scopes are requested — the
            // callback arrives as a POST (handled by the POST callback route).
            request = request.add_extra_param("response_mode", "form_post");
        }
        let (auth_url, csrf, nonce) = request.url();
        Ok((
            auth_url,
            LoginFlow {
                state: csrf.secret().clone(),
                provider: slug.to_owned(),
                pkce_verifier: pkce_verifier.secret().clone(),
                nonce: nonce.secret().clone(),
                return_to,
            },
        ))
    }

    /// Handle the provider callback: exchange the code (PKCE), verify the
    /// id_token (signature, iss, aud, nonce, expiry), and return the
    /// identity. The caller retrieved `flow` by the callback's `state`
    /// parameter, which already proves state integrity; the provider match
    /// is re-checked here.
    ///
    /// `user_payload` is the raw `user` form field Apple includes on the
    /// *first* authorization only — the sole source of the user's name for
    /// Apple logins. Ignored (never trusted) for other providers.
    pub async fn complete(
        &self,
        slug: &str,
        flow: LoginFlow,
        code: &str,
        user_payload: Option<&str>,
    ) -> Result<OidcIdentity, AuthError> {
        (flow.provider == slug)
            .then_some(())
            .ok_or(AuthError::StateMismatch)?;

        let handle = self.handle(slug)?;
        let client = self.client(handle)?;
        let token = client
            .exchange_code(AuthorizationCode::new(code.to_owned()))
            .map_err(|e| AuthError::TokenExchange(e.to_string()))?
            .set_pkce_verifier(PkceCodeVerifier::new(flow.pkce_verifier))
            .request_async(&self.http)
            .await
            .map_err(|e| AuthError::TokenExchange(e.to_string()))?;

        use openidconnect::TokenResponse;
        let id_token = token
            .id_token()
            .ok_or_else(|| AuthError::IdToken("provider returned no id_token".into()))?;
        let claims = id_token
            .claims(&client.id_token_verifier(), &Nonce::new(flow.nonce))
            .map_err(|e| AuthError::IdToken(e.to_string()))?;

        // Apple's name never appears in the id_token; it arrives once, in
        // the first callback's `user` field.
        let apple_user = handle
            .is_apple()
            .then(|| user_payload.and_then(crate::apple::AppleCallbackUser::parse))
            .flatten();
        let display_name = claims
            .name()
            .and_then(|n| n.get(None))
            .map(|n| n.as_str().to_owned())
            .or_else(|| apple_user.as_ref().and_then(|u| u.display_name()))
            .or_else(|| {
                claims
                    .preferred_username()
                    .map(|u| u.as_str().to_owned())
            });
        let email = claims
            .email()
            .map(|e| e.as_str().to_owned())
            .or_else(|| apple_user.as_ref().and_then(|u| u.email().map(str::to_owned)));
        Ok(OidcIdentity {
            subject: OidcSubject::new(
                claims.issuer().as_str().to_owned(),
                claims.subject().as_str().to_owned(),
            ),
            display_name,
            email,
        })
    }
}

use auth::{AppleSecret, OidcProviderConfig, SecretSource};
use domain::KeyGeneration;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required env var {0}")]
    Missing(&'static str),
    #[error("invalid value for {var}: {detail}")]
    Invalid { var: String, detail: String },
    #[error("no FF1 keys configured (need at least DUCK_KEY_GEN_0)")]
    NoKeys,
}

/// Product knobs (duck-voyage.md §8/§13): env-overridable, sane defaults.
#[derive(Clone, Copy)]
pub struct Caps {
    pub flocks_per_user: i64,
    pub mint_batch_max: u16,
    pub unoriginated_max: i64,
    pub missing_after_days: i64,
    pub front_page_limit: i64,
}

/// Typed config loaded from env at boot. The app refuses to start when a
/// required value is missing — fail fast, no unwraps later.
///
/// This is generic self-hostable software: nothing here assumes a particular
/// platform. Reasonable defaults mean a minimal install needs only
/// `DATABASE_URL`, `BASE_URL`, one FF1 key, and one OIDC provider.
pub struct AppConfig {
    pub database_url: String,
    pub base_url: String,
    pub listen_addr: std::net::SocketAddr,
    /// Key rate limits on `X-Forwarded-For`-style headers instead of the TCP
    /// peer address. Opt-in: only correct when running behind a trusted
    /// reverse proxy / load balancer; trusting these headers while directly
    /// exposed lets clients spoof their IP.
    pub trust_proxy_headers: bool,
    pub ff1_keys: Vec<(KeyGeneration, [u8; 32])>,
    pub current_generation: KeyGeneration,
    pub storage: StorageConfig,
    pub oidc: Vec<OidcProviderConfig>,
    /// (issuer, subject) pairs granted is_admin on login.
    pub admin_identities: Vec<(String, String)>,
    pub caps: Caps,
}

/// Where photos live. S3-compatible when `STORAGE_ENDPOINT` is set
/// (MinIO, AWS, …); otherwise a plain directory on disk — the
/// zero-dependency default for small self-hosted installs.
pub enum StorageConfig {
    S3 {
        endpoint: String,
        bucket: String,
        access_key: String,
        secret_key: String,
    },
    Local {
        path: std::path::PathBuf,
    },
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let required = |var: &'static str| std::env::var(var).map_err(|_| ConfigError::Missing(var));
        let optional = |var: &str| std::env::var(var).ok().filter(|v| !v.is_empty());

        let ff1_keys: Vec<(KeyGeneration, [u8; 32])> = (0u16..)
            .map(|generation| {
                optional(&format!("DUCK_KEY_GEN_{generation}"))
                    .map(|hex_key| Self::parse_key(generation, &hex_key))
            })
            .take_while(Option::is_some)
            .flatten()
            .collect::<Result<_, _>>()?;
        (!ff1_keys.is_empty()).then_some(()).ok_or(ConfigError::NoKeys)?;

        let current_generation = KeyGeneration::new(
            required("DUCK_KEY_CURRENT")?
                .parse::<u16>()
                .map_err(|e| ConfigError::Invalid {
                    var: "DUCK_KEY_CURRENT".into(),
                    detail: e.to_string(),
                })?,
        );

        let base_url = required("BASE_URL")?.trim_end_matches('/').to_owned();
        // Default listener is dual-stack: `[::]` accepts both IPv6 and
        // IPv4-mapped connections on Linux. `LISTEN_ADDR` overrides entirely
        // (e.g. `0.0.0.0:3000` on a v6-less host); `PORT` tweaks just the port.
        let listen_addr = match optional("LISTEN_ADDR") {
            Some(addr) => addr
                .parse::<std::net::SocketAddr>()
                .map_err(|e| ConfigError::Invalid {
                    var: "LISTEN_ADDR".into(),
                    detail: e.to_string(),
                })?,
            None => {
                let port = optional("PORT")
                    .map(|p| {
                        p.parse::<u16>().map_err(|e| ConfigError::Invalid {
                            var: "PORT".into(),
                            detail: e.to_string(),
                        })
                    })
                    .transpose()?
                    .unwrap_or(3000);
                std::net::SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, port))
            }
        };

        let admin_identities = optional("ADMIN_IDENTITIES")
            .map(|raw| {
                raw.split(',')
                    .filter(|pair| !pair.trim().is_empty())
                    .map(|pair| {
                        pair.split_once('|')
                            .map(|(iss, sub)| (iss.trim().to_owned(), sub.trim().to_owned()))
                            .ok_or_else(|| ConfigError::Invalid {
                                var: "ADMIN_IDENTITIES".into(),
                                detail: format!("expected issuer|subject, got {pair:?}"),
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();

        let cap = |var: &str, default: i64| -> i64 {
            optional(var).and_then(|v| v.parse().ok()).unwrap_or(default)
        };

        let storage = match optional("STORAGE_ENDPOINT") {
            Some(endpoint) => StorageConfig::S3 {
                endpoint,
                bucket: required("STORAGE_BUCKET")?,
                access_key: required("STORAGE_ACCESS_KEY")?,
                secret_key: required("STORAGE_SECRET_KEY")?,
            },
            None => StorageConfig::Local {
                path: optional("STORAGE_LOCAL_PATH")
                    .unwrap_or_else(|| "./photos".into())
                    .into(),
            },
        };

        let trust_proxy_headers = optional("TRUST_PROXY_HEADERS")
            .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        Ok(Self {
            database_url: required("DATABASE_URL")?,
            listen_addr,
            trust_proxy_headers,
            ff1_keys,
            current_generation,
            storage,
            oidc: Self::oidc_from_env(&optional)?,
            admin_identities,
            caps: Caps {
                flocks_per_user: cap("CAP_FLOCKS_PER_USER", 10),
                mint_batch_max: cap("CAP_MINT_BATCH_MAX", 100) as u16,
                unoriginated_max: cap("CAP_UNORIGINATED_MAX", 200),
                missing_after_days: cap("MISSING_AFTER_DAYS", 365),
                front_page_limit: cap("FRONT_PAGE_LIMIT", 20),
            },
            base_url,
        })
    }

    fn parse_key(generation: u16, hex_key: &str) -> Result<(KeyGeneration, [u8; 32]), ConfigError> {
        let var = format!("DUCK_KEY_GEN_{generation}");
        let bytes = hex::decode(hex_key.trim()).map_err(|e| ConfigError::Invalid {
            var: var.clone(),
            detail: e.to_string(),
        })?;
        let key: [u8; 32] = bytes.try_into().map_err(|_| ConfigError::Invalid {
            var,
            detail: "FF1 keys must be exactly 32 bytes of hex".into(),
        })?;
        Ok((KeyGeneration::new(generation), key))
    }

    /// Each provider activates when its env vars are present. The big three
    /// (Google, Microsoft, Apple) have well-known issuers and quirks, so
    /// they get dedicated variables. *Any other* spec-compliant OIDC
    /// provider — Keycloak, Authentik, Authelia, Zitadel, Okta, … — is
    /// configured generically: `OIDC_<SLUG>_ISSUER`, `OIDC_<SLUG>_CLIENT_ID`,
    /// `OIDC_<SLUG>_SECRET`, and optionally `OIDC_<SLUG>_DISPLAY_NAME`.
    /// The slug becomes the login-route path segment, lowercased.
    fn oidc_from_env(
        optional: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Vec<OidcProviderConfig>, ConfigError> {
        let google = optional("OIDC_GOOGLE_CLIENT_ID").zip(optional("OIDC_GOOGLE_SECRET")).map(
            |(client_id, secret)| OidcProviderConfig {
                slug: "google".into(),
                display_name: "Google".into(),
                issuer_url: "https://accounts.google.com".into(),
                client_id,
                secret: SecretSource::Static(secret),
            },
        );
        let entra = optional("OIDC_ENTRA_CLIENT_ID")
            .zip(optional("OIDC_ENTRA_SECRET"))
            .zip(optional("OIDC_ENTRA_TENANT"))
            .map(|((client_id, secret), tenant)| OidcProviderConfig {
                slug: "entra".into(),
                display_name: "Microsoft".into(),
                issuer_url: format!("https://login.microsoftonline.com/{tenant}/v2.0"),
                client_id,
                secret: SecretSource::Static(secret),
            });
        let apple = optional("OIDC_APPLE_CLIENT_ID")
            .zip(optional("OIDC_APPLE_TEAM_ID"))
            .zip(optional("OIDC_APPLE_KEY_ID").zip(optional("OIDC_APPLE_PRIVATE_KEY")))
            .map(|((client_id, team_id), (key_id, private_key_pem))| OidcProviderConfig {
                slug: "apple".into(),
                display_name: "Apple".into(),
                issuer_url: "https://appleid.apple.com".into(),
                client_id,
                secret: SecretSource::Apple(AppleSecret { team_id, key_id, private_key_pem }),
            });
        // Generic providers: every OIDC_<SLUG>_ISSUER in the environment
        // declares one. Slugs owned by the dedicated branches above are
        // reserved (their issuer is not configurable).
        const RESERVED: [&str; 3] = ["GOOGLE", "ENTRA", "APPLE"];
        let generic = std::env::vars()
            .filter_map(|(key, _)| {
                key.strip_prefix("OIDC_")
                    .and_then(|rest| rest.strip_suffix("_ISSUER"))
                    .filter(|slug| !RESERVED.contains(slug))
                    .map(str::to_owned)
            })
            .map(|slug| {
                let var = |suffix: &str| optional(&format!("OIDC_{slug}_{suffix}"));
                let require = |suffix: &str| {
                    var(suffix).ok_or_else(|| ConfigError::Invalid {
                        var: format!("OIDC_{slug}_{suffix}"),
                        detail: format!("required because OIDC_{slug}_ISSUER is set"),
                    })
                };
                let display_name = var("DISPLAY_NAME").unwrap_or_else(|| {
                    let lower = slug.to_lowercase();
                    let mut chars = lower.chars();
                    chars
                        .next()
                        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                        .unwrap_or(lower)
                });
                Ok(OidcProviderConfig {
                    slug: slug.to_lowercase(),
                    display_name,
                    issuer_url: require("ISSUER")?,
                    client_id: require("CLIENT_ID")?,
                    secret: SecretSource::Static(require("SECRET")?),
                })
            })
            .collect::<Result<Vec<_>, ConfigError>>()?;
        Ok([google, entra, apple]
            .into_iter()
            .flatten()
            .chain(generic)
            .collect())
    }
}

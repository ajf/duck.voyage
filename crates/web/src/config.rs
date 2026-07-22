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
pub struct AppConfig {
    pub database_url: String,
    pub base_url: String,
    pub listen_port: u16,
    pub ff1_keys: Vec<(KeyGeneration, [u8; 32])>,
    pub current_generation: KeyGeneration,
    pub storage: StorageConfig,
    pub oidc: Vec<OidcProviderConfig>,
    /// (issuer, subject) pairs granted is_admin on login.
    pub admin_identities: Vec<(String, String)>,
    pub caps: Caps,
}

pub struct StorageConfig {
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
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
        let listen_port = optional("PORT")
            .map(|p| {
                p.parse::<u16>().map_err(|e| ConfigError::Invalid {
                    var: "PORT".into(),
                    detail: e.to_string(),
                })
            })
            .transpose()?
            .unwrap_or(3000);

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

        Ok(Self {
            database_url: required("DATABASE_URL")?,
            listen_port,
            ff1_keys,
            current_generation,
            storage: StorageConfig {
                endpoint: required("STORAGE_ENDPOINT")?,
                bucket: required("STORAGE_BUCKET")?,
                access_key: required("STORAGE_ACCESS_KEY")?,
                secret_key: required("STORAGE_SECRET_KEY")?,
            },
            oidc: Self::oidc_from_env(&optional),
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

    /// Each provider activates when its env vars are present; issuer URLs for
    /// the big three are fixed knowledge, Keycloak's comes from config.
    fn oidc_from_env(optional: &dyn Fn(&str) -> Option<String>) -> Vec<OidcProviderConfig> {
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
        let keycloak = optional("OIDC_KEYCLOAK_ISSUER")
            .zip(optional("OIDC_KEYCLOAK_CLIENT_ID"))
            .zip(optional("OIDC_KEYCLOAK_SECRET"))
            .map(|((issuer_url, client_id), secret)| OidcProviderConfig {
                slug: "keycloak".into(),
                display_name: "Keycloak".into(),
                issuer_url,
                client_id,
                secret: SecretSource::Static(secret),
            });
        [google, entra, apple, keycloak].into_iter().flatten().collect()
    }
}

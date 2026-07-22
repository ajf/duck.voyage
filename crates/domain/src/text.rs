/// Bounded, trimmed, non-empty text newtypes. Each carries its own maximum
/// length; construction is the only place raw strings cross the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TextError {
    #[error("must not be empty")]
    Empty,
    #[error("too long: {got} characters (maximum {max})")]
    TooLong { got: usize, max: usize },
}

macro_rules! bounded_text {
    ($(#[$doc:meta])* $name:ident, max = $max:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            pub const MAX_CHARS: usize = $max;

            pub fn parse(raw: &str) -> Result<Self, TextError> {
                let trimmed = raw.trim();
                let count = trimmed.chars().count();
                match count {
                    0 => Err(TextError::Empty),
                    n if n > Self::MAX_CHARS => Err(TextError::TooLong { got: n, max: Self::MAX_CHARS }),
                    _ => Ok(Self(trimmed.to_owned())),
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

bounded_text! {
    /// Optional name given to a duck at origination.
    DuckName, max = 80
}
bounded_text! {
    /// Required description given to a duck at origination.
    DuckDescription, max = 2_000
}
bounded_text! {
    /// Free-text note on a sighting.
    Note, max = 2_000
}
bounded_text! {
    /// Body of a comment on a duck page.
    CommentBody, max = 2_000
}
bounded_text! {
    /// A user's label for one of their flocks ("Alaska trip 2026").
    FlockLabel, max = 120
}

/// Object-store key for an uploaded photo. Generated server-side, never a
/// public URL; serving goes back through the app.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PhotoKey(String);

impl PhotoKey {
    pub fn new(key: String) -> Self {
        Self(key)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A stored photo reference: object key plus the content type it was
/// re-encoded to. Both always travel together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoRef {
    pub key: PhotoKey,
    pub content_type: String,
}

/// A user's OIDC identity: the `(iss, sub)` pair, which is the only stable
/// cross-login identifier a provider guarantees.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OidcSubject {
    issuer: String,
    subject: String,
}

impl OidcSubject {
    pub fn new(issuer: String, subject: String) -> Self {
        Self { issuer, subject }
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }
}

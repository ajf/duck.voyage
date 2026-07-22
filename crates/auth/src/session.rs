use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Redirect, Response};
use domain::UserId;
use tower_sessions::Session;

/// What we keep in the server-side session after login. Small on purpose:
/// anything else is a DB read away.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionUser {
    pub user_id: i64,
    pub display_name: Option<String>,
    pub is_admin: bool,
}

const USER_KEY: &str = "user";

impl SessionUser {
    pub async fn store(self, session: &Session) -> Result<(), tower_sessions::session::Error> {
        session.insert(USER_KEY, self).await
    }

    pub async fn clear(session: &Session) -> Result<(), tower_sessions::session::Error> {
        session.flush().await
    }
}

/// Typed proof of login. Mutation handlers take this extractor, so an
/// unauthenticated write doesn't typecheck. Rejection redirects to the login
/// page with the original destination preserved.
pub struct AuthenticatedUser {
    pub id: UserId,
    pub display_name: Option<String>,
    pub is_admin: bool,
}

/// Typed proof of admin. Rejection is 403 for logged-in non-admins,
/// login redirect otherwise.
pub struct AdminUser(pub AuthenticatedUser);

pub enum AuthRejection {
    LoginRedirect(String),
    Forbidden,
    SessionUnavailable,
}

impl IntoResponse for AuthRejection {
    fn into_response(self) -> Response {
        match self {
            Self::LoginRedirect(return_to) => {
                Redirect::to(&format!("/login?return_to={}", urlencoded(&return_to))).into_response()
            }
            Self::Forbidden => (axum::http::StatusCode::FORBIDDEN, "admin only").into_response(),
            Self::SessionUnavailable => {
                (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "session unavailable").into_response()
            }
        }
    }
}

/// Percent-encode enough for a query-string value.
fn urlencoded(raw: &str) -> String {
    raw.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'/' => {
                char::from(b).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

impl<S: Send + Sync> FromRequestParts<S> for AuthenticatedUser {
    type Rejection = AuthRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRejection::SessionUnavailable)?;
        let user: Option<SessionUser> = session
            .get(USER_KEY)
            .await
            .map_err(|_| AuthRejection::SessionUnavailable)?;
        user.map(|u| AuthenticatedUser {
            id: UserId::new(u.user_id),
            display_name: u.display_name,
            is_admin: u.is_admin,
        })
        .ok_or_else(|| AuthRejection::LoginRedirect(parts.uri.path().to_owned()))
    }
}

/// `Option<AuthenticatedUser>` for pages that are public but render extra
/// affordances for a logged-in viewer. Never redirects.
impl<S: Send + Sync> OptionalFromRequestParts<S> for AuthenticatedUser {
    type Rejection = AuthRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRejection::SessionUnavailable)?;
        let user: Option<SessionUser> = session
            .get(USER_KEY)
            .await
            .map_err(|_| AuthRejection::SessionUnavailable)?;
        Ok(user.map(|u| AuthenticatedUser {
            id: UserId::new(u.user_id),
            display_name: u.display_name,
            is_admin: u.is_admin,
        }))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for AdminUser {
    type Rejection = AuthRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user =
            <AuthenticatedUser as FromRequestParts<S>>::from_request_parts(parts, state).await?;
        user.is_admin
            .then_some(Self(user))
            .ok_or(AuthRejection::Forbidden)
    }
}

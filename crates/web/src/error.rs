use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Handler-level error. Anything that isn't a deliberate 404/400 becomes a
/// logged 500 — details never reach the client.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    /// Deliberately identical for "code invalid", "never minted", and
    /// "allocated but not yours" — the count-hiding 404 (§3.5).
    #[error("not found")]
    NotFound,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error(transparent)]
    Repo(#[from] storage::RepoError),
    #[error(transparent)]
    Auth(#[from] auth::AuthError),
    #[error(transparent)]
    PhotoStore(#[from] storage::PhotoStoreError),
    #[error("session error: {0}")]
    Session(#[from] tower_sessions::session::Error),
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                crate::views::error_page("404", "No such duck. Check the code and try again?"),
            )
                .into_response(),
            Self::BadRequest(detail) => (StatusCode::BAD_REQUEST, detail).into_response(),
            // Expired/replayed/forged login callbacks are a client condition:
            // send them around again rather than alarming anyone with a 500.
            Self::Auth(auth::AuthError::UnknownFlow | auth::AuthError::StateMismatch) => (
                StatusCode::BAD_REQUEST,
                crate::views::error_page(
                    "login expired",
                    "That login attempt expired or was already used. Please log in again.",
                ),
            )
                .into_response(),
            other => {
                tracing::error!(error = %other, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    crate::views::error_page("500", "Something went wrong on our end."),
                )
                    .into_response()
            }
        }
    }
}

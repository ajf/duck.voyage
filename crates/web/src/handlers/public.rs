use auth::AuthenticatedUser;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use jiff::{Timestamp, ToSpan};

use crate::error::WebError;
use crate::handlers::nav;
use crate::state::AppState;
use crate::views::Page;

pub async fn front(
    State(state): State<AppState>,
    user: Option<AuthenticatedUser>,
) -> Result<Response, WebError> {
    let nav = nav(&state, user.as_ref()).await?;
    let limit = state.caps().front_page_limit;
    let recent = state.ducks().recently_found(limit).await?;
    let most = state.ducks().most_sighted(limit).await?;
    Ok(Page::front(&nav, &recent, &most).into_response())
}

pub async fn missing(
    State(state): State<AppState>,
    user: Option<AuthenticatedUser>,
) -> Result<Response, WebError> {
    let nav = nav(&state, user.as_ref()).await?;
    // Hours, not days: jiff (correctly) refuses calendar units on Timestamp
    // arithmetic without a timezone, and the threshold is coarse anyway.
    let hours = i64::from(i32::try_from(state.caps().missing_after_days).unwrap_or(365)) * 24;
    let cutoff = Timestamp::now() - hours.hours();
    let ducks = state.ducks().missing_since(cutoff).await?;
    Ok(Page::missing(&nav, &ducks).into_response())
}

pub async fn htmx_js() -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/javascript"),
            (header::CACHE_CONTROL, "public, max-age=604800"),
        ],
        include_bytes!("../../static/htmx.min.js").as_slice(),
    )
        .into_response()
}

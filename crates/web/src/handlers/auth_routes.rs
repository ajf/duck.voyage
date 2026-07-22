use auth::SessionUser;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use tower_sessions::Session;

use crate::error::WebError;
use crate::handlers::nav;
use crate::state::AppState;
use crate::views::Page;

#[derive(serde::Deserialize)]
pub struct ReturnTo {
    pub return_to: Option<String>,
}

/// Only ever redirect back into our own site.
fn safe_return_to(raw: Option<String>) -> Option<String> {
    raw.filter(|r| r.starts_with('/') && !r.starts_with("//"))
}

pub async fn login_page(
    State(state): State<AppState>,
    user: Option<auth::AuthenticatedUser>,
    Query(params): Query<ReturnTo>,
) -> Result<Response, WebError> {
    let nav = nav(&state, user.as_ref()).await?;
    Ok(Page::login(
        &nav,
        &state.oidc().summaries(),
        safe_return_to(params.return_to).as_deref(),
    )
    .into_response())
}

pub async fn begin(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(params): Query<ReturnTo>,
    session: Session,
) -> Result<Redirect, WebError> {
    let url = state
        .oidc()
        .begin(&provider, &session, safe_return_to(params.return_to))
        .await?;
    Ok(Redirect::to(url.as_str()))
}

#[derive(serde::Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}

/// GET for every provider; Apple uses `response_mode=form_post`, hence the
/// POST twin below sharing this body.
pub async fn callback_get(
    state: State<AppState>,
    provider: Path<String>,
    session: Session,
    Query(params): Query<CallbackParams>,
) -> Result<Redirect, WebError> {
    callback(state, provider, session, params).await
}

pub async fn callback_post(
    state: State<AppState>,
    provider: Path<String>,
    session: Session,
    Form(params): Form<CallbackParams>,
) -> Result<Redirect, WebError> {
    callback(state, provider, session, params).await
}

async fn callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    session: Session,
    params: CallbackParams,
) -> Result<Redirect, WebError> {
    let (identity, return_to) = state
        .oidc()
        .complete(&provider, &session, &params.code, &params.state)
        .await?;
    let grant_admin =
        state.is_admin_identity(identity.subject.issuer(), identity.subject.subject());
    let user = state
        .users()
        .upsert(
            &identity.subject,
            identity.display_name.as_deref(),
            identity.email.as_deref(),
            grant_admin,
        )
        .await?;
    // Rotate the session id on privilege change (fixation hygiene).
    session.cycle_id().await?;
    SessionUser {
        user_id: user.id.get(),
        display_name: user.display_name.clone(),
        is_admin: user.is_admin,
    }
    .store(&session)
    .await?;
    Ok(Redirect::to(
        safe_return_to(return_to).as_deref().unwrap_or("/"),
    ))
}

pub async fn logout(session: Session) -> Result<Redirect, WebError> {
    SessionUser::clear(&session).await?;
    Ok(Redirect::to("/"))
}

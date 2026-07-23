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
) -> Result<Redirect, WebError> {
    let (url, flow) = state
        .oidc()
        .begin(&provider, safe_return_to(params.return_to))?;
    // Parked in the database, keyed by the state token: Apple's form_post
    // callback arrives without cookies, so the session can't carry this.
    state
        .oidc_flows()
        .put(&storage::StoredLoginFlow {
            state: flow.state,
            provider: flow.provider,
            pkce_verifier: flow.pkce_verifier,
            nonce: flow.nonce,
            return_to: flow.return_to,
        })
        .await?;
    Ok(Redirect::to(url.as_str()))
}

#[derive(serde::Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
    /// Apple only: JSON blob with the user's name, present on the very
    /// first authorization and never again.
    pub user: Option<String>,
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
    // Successful requests are otherwise silent; this receipt line makes
    // "did the provider's callback arrive at all?" answerable from logs.
    tracing::info!(
        provider,
        has_user_payload = params.user.is_some(),
        "oidc callback received"
    );
    let stored = state
        .oidc_flows()
        .take(&params.state)
        .await?
        .ok_or(auth::AuthError::UnknownFlow)?;
    let return_to = stored.return_to.clone();
    let identity = state
        .oidc()
        .complete(
            &provider,
            auth::LoginFlow {
                state: stored.state,
                provider: stored.provider,
                pkce_verifier: stored.pkce_verifier,
                nonce: stored.nonce,
                return_to: stored.return_to,
            },
            &params.code,
            params.user.as_deref(),
        )
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

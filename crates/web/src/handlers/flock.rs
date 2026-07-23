use auth::AuthenticatedUser;
use axum::extract::{Path, State};
use axum::response::Redirect;
use axum::Form;
use domain::{FlockId, FlockLabel, FlockSeq, Profanity};

use crate::error::WebError;
use crate::state::AppState;

#[derive(serde::Deserialize)]
pub struct CreateFlockForm {
    pub label: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Form(form): Form<CreateFlockForm>,
) -> Result<Redirect, WebError> {
    let caps = state.caps();
    let owned = state.flocks().count_for_owner(user.id).await?;
    (owned < caps.flocks_per_user).then_some(()).ok_or_else(|| {
        WebError::BadRequest(format!("flock cap reached ({} per user)", caps.flocks_per_user))
    })?;
    let label = form
        .label
        .as_deref()
        .filter(|l| !l.trim().is_empty())
        .map(|l| FlockLabel::parse(l).map_err(|e| WebError::BadRequest(format!("label: {e}"))))
        .transpose()?;
    state
        .flocks()
        .create(user.id, state.codec().current_generation(), label.as_ref())
        .await?;
    Ok(Redirect::to("/me/flocks?m=Flock+created"))
}

#[derive(serde::Deserialize)]
pub struct MintForm {
    pub count: u16,
}

/// Mint the next N codes into a flock. Seqs are dense (max+1 onward); the
/// UNIQUE constraint catches a concurrent mint and the user just retries.
pub async fn mint(
    State(state): State<AppState>,
    Path(flock_id): Path<i64>,
    user: AuthenticatedUser,
    Form(form): Form<MintForm>,
) -> Result<Redirect, WebError> {
    let caps = state.caps();
    let flock = state
        .flocks()
        .by_id(FlockId::new(flock_id))
        .await?
        .filter(|f| f.owner == user.id)
        .ok_or(WebError::NotFound)?;
    let count = form.count.clamp(1, caps.mint_batch_max);
    let unoriginated = state.ducks().unoriginated_count_for_owner(user.id).await?;
    (unoriginated + i64::from(count) <= caps.unoriginated_max)
        .then_some(())
        .ok_or_else(|| {
            WebError::BadRequest(format!(
                "you have {unoriginated} unoriginated ducks — originate some before minting more \
                 (cap {})",
                caps.unoriginated_max
            ))
        })?;
    // Fail fast on a key-configuration problem (the only way encoding can
    // fail); inside the mint closure encoding is then infallible.
    state
        .codec()
        .encode(flock.generation, &flock.code, FlockSeq::new(1).expect("1 is valid"))
        .map_err(|e| WebError::BadRequest(e.to_string()))?;
    // The mint itself is atomic in the repo (per-flock advisory lock), so
    // concurrent mints — including from other instances — serialize. Codes
    // that would spell something rude are skipped (a hole in the seq space
    // is harmless; scans of it 404).
    state
        .ducks()
        .mint_batch(flock.id, count, |seq| {
            state
                .codec()
                .encode(flock.generation, &flock.code, seq)
                .ok()
                .filter(|code| !Profanity::matches(code.as_str()))
        })
        .await
        .map_err(|e| match e {
            storage::RepoError::FlockFull => WebError::BadRequest(e.to_string()),
            other => WebError::Repo(other),
        })?;
    Ok(Redirect::to("/me/flocks?m=Codes+minted"))
}

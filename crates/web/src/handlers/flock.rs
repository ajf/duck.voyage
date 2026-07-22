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
    // Walk seqs from the high-water mark, skipping any whose encrypted code
    // happens to spell something rude — the skipped seq is simply never
    // minted (a hole in the sequence is harmless; scans of it 404).
    let start = state.ducks().max_seq(flock.id).await? + 1;
    let entries: Vec<(FlockSeq, domain::DuckCode)> = (u32::from(start)..)
        .map(|raw| {
            let seq = FlockSeq::new(raw).map_err(|_| {
                WebError::BadRequest(format!("flock is full ({} codes max)", FlockSeq::MAX))
            })?;
            let code = state
                .codec()
                .encode(flock.generation, &flock.code, seq)
                .map_err(|e| WebError::BadRequest(e.to_string()))?;
            Ok((seq, code))
        })
        .filter(|entry| {
            entry
                .as_ref()
                .map(|(_, code)| !Profanity::matches(code.as_str()))
                .unwrap_or(true)
        })
        .take(usize::from(count))
        .collect::<Result<_, WebError>>()?;
    state.ducks().mint(flock.id, &entries).await?;
    Ok(Redirect::to("/me/flocks?m=Codes+minted"))
}

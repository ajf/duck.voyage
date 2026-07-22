//! The scan funnel and everything on a duck page. The load-bearing rule
//! (§3.5): an allocated duck is a 404 to everyone but its flock owner, and
//! every failure mode returns the *same* 404.

use auth::AuthenticatedUser;
use axum::extract::{Multipart, Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use domain::{
    CommentBody, Coordinates, Duck, DuckCode, DuckDescription, DuckLifecycle, DuckName, Flock,
    Note, PhotoRef, SightingId, VesselId,
};
use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use storage::NewSighting;

use crate::error::WebError;
use crate::handlers::{nav, FormData};
use crate::photo_pipeline::PhotoPipeline;
use crate::qr::QrLabel;
use crate::state::AppState;
use crate::views::Page;

/// Steps 1–4 of the validation funnel: parse, flock lookup, decode, existence
/// check — every failure collapses into the same `NotFound`.
async fn resolve(state: &AppState, raw_code: &str) -> Result<(Duck, Flock), WebError> {
    let code = DuckCode::parse(raw_code).map_err(|_| WebError::NotFound)?;
    let flock = state
        .flocks()
        .by_code(&code.flock_code())
        .await?
        .ok_or(WebError::NotFound)?;
    let seq = state
        .codec()
        .decode(flock.generation, &code)
        .map_err(|_| WebError::NotFound)?;
    let duck = state
        .ducks()
        .by_flock_and_seq(flock.id, seq)
        .await?
        .ok_or(WebError::NotFound)?;
    // Belt-and-braces: the stored code must match the scanned string; a
    // mismatch means a key/tweak configuration bug, not a bad scan.
    if duck.code != code {
        tracing::error!(scanned = %code, stored = %duck.code, "code mismatch — check FF1 key config");
        return Err(WebError::NotFound);
    }
    Ok((duck, flock))
}

fn is_owner(user: Option<&AuthenticatedUser>, flock: &Flock) -> bool {
    user.is_some_and(|u| u.id == flock.owner)
}

pub async fn page(
    State(state): State<AppState>,
    Path(raw_code): Path<String>,
    user: Option<AuthenticatedUser>,
    axum::extract::Query(flash): axum::extract::Query<FlashParam>,
) -> Result<Response, WebError> {
    let (duck, flock) = resolve(&state, &raw_code).await?;
    let nav = nav(&state, user.as_ref()).await?;
    match &duck.lifecycle {
        // Allocated and Staged read as 404 to everyone but the owner, who
        // gets the define form or the set-sail page respectively.
        DuckLifecycle::Allocated => is_owner(user.as_ref(), &flock)
            .then(|| Page::origination_form(&nav, &duck.code).into_response())
            .ok_or(WebError::NotFound),
        DuckLifecycle::Staged(details) => is_owner(user.as_ref(), &flock)
            .then(|| Page::staged(&nav, &duck.code, details).into_response())
            .ok_or(WebError::NotFound),
        DuckLifecycle::Sailing { details, since } => {
            let (description, name) = (&details.description, &details.name);
            let at = since;
            let sightings = state.sightings().for_duck(duck.id).await?;
            let comments = state.comments().for_duck(duck.id).await?;
            let vessels = state.vessels().options().await?;
            let is_following = match &user {
                Some(u) => state.follows().is_following(u.id, duck.id).await?,
                None => false,
            };
            Ok(Page::duck(
                &nav,
                flash.m.as_deref(),
                &duck.code,
                name.as_ref().map(DuckName::as_str),
                description.as_str(),
                at,
                is_owner(user.as_ref(), &flock),
                is_following,
                &sightings,
                &comments,
                &vessels,
            )
            .into_response())
        }
    }
}

#[derive(serde::Deserialize)]
pub struct FlashParam {
    pub m: Option<String>,
}

pub async fn originate(
    State(state): State<AppState>,
    Path(raw_code): Path<String>,
    user: AuthenticatedUser,
    multipart: Multipart,
) -> Result<Redirect, WebError> {
    let (duck, flock) = resolve(&state, &raw_code).await?;
    // A non-owner posting here gets the same 404 as a bad code.
    is_owner(Some(&user), &flock).then_some(()).ok_or(WebError::NotFound)?;
    if !matches!(duck.lifecycle, DuckLifecycle::Allocated) {
        return Ok(Redirect::to(&format!("/d/{}", duck.code.as_str())));
    }
    let form = FormData::read(multipart).await?;
    let description = DuckDescription::parse(form.require_text("description")?)
        .map_err(|e| WebError::BadRequest(format!("description: {e}")))?;
    let name = form
        .text("name")
        .map(|n| DuckName::parse(n).map_err(|e| WebError::BadRequest(format!("name: {e}"))))
        .transpose()?;
    let photo_bytes = form.file("photo").ok_or_else(|| {
        WebError::BadRequest("a photo of the duck is required to originate it".into())
    })?;
    let processed = PhotoPipeline::process(photo_bytes)
        .map_err(|e| WebError::BadRequest(e.to_string()))?;
    let key = state.photos().new_key("origin");
    state.photos().put(&key, processed.bytes).await?;
    let photo = PhotoRef { key, content_type: processed.content_type.to_owned() };
    state
        .ducks()
        .stage(duck.id, &description, name.as_ref(), &photo)
        .await?;
    // "It's already in place" shortcut: define and set sail in one step.
    if form.text("set_sail").is_some() {
        state.ducks().set_sail(duck.id).await?;
        return Ok(Redirect::to(&format!("/d/{}?m=Duck+defined+and+set+sail.+Bon+voyage!", duck.code.as_str())));
    }
    Ok(Redirect::to(&format!(
        "/d/{}?m=Duck+defined.+Scan+its+sticker+when+it%27s+in+place+to+set+sail.",
        duck.code.as_str()
    )))
}

/// The owner scanned (or clicked through to) a staged duck and confirmed:
/// set it sailing.
pub async fn set_sail(
    State(state): State<AppState>,
    Path(raw_code): Path<String>,
    user: AuthenticatedUser,
) -> Result<Redirect, WebError> {
    let (duck, flock) = resolve(&state, &raw_code).await?;
    is_owner(Some(&user), &flock).then_some(()).ok_or(WebError::NotFound)?;
    state.ducks().set_sail(duck.id).await?;
    Ok(Redirect::to(&format!("/d/{}?m=Bon+voyage!", duck.code.as_str())))
}

/// Only sailing ducks accept interaction; the check is shared by every
/// mutation below.
async fn resolve_sailing(state: &AppState, raw_code: &str) -> Result<(Duck, Flock), WebError> {
    let (duck, flock) = resolve(state, raw_code).await?;
    duck.lifecycle
        .is_sailing()
        .then_some(())
        .ok_or(WebError::NotFound)?;
    Ok((duck, flock))
}

pub async fn log_sighting(
    State(state): State<AppState>,
    Path(raw_code): Path<String>,
    user: AuthenticatedUser,
    multipart: Multipart,
) -> Result<Redirect, WebError> {
    let (duck, _) = resolve_sailing(&state, &raw_code).await?;
    let form = FormData::read(multipart).await?;
    let vessel_id = VesselId::new(
        form.require_text("vessel_id")?
            .parse::<i64>()
            .map_err(|e| WebError::BadRequest(format!("vessel_id: {e}")))?,
    );
    state
        .vessels()
        .by_id(vessel_id)
        .await?
        .ok_or_else(|| WebError::BadRequest("unknown vessel".into()))?;
    // DECISION: the datetime-local input is taken as UTC for v1; sightings
    // are day-granularity in practice and phones don't send their zone here.
    let seen_at = DateTime::strptime("%Y-%m-%dT%H:%M", form.require_text("seen_at")?)
        .map_err(|e| WebError::BadRequest(format!("seen_at: {e}")))?
        .to_zoned(TimeZone::UTC)
        .map_err(|e| WebError::BadRequest(format!("seen_at: {e}")))?
        .timestamp();
    let note = form
        .text("note")
        .map(|n| Note::parse(n).map_err(|e| WebError::BadRequest(format!("note: {e}"))))
        .transpose()?;
    let photo = match form.file("photo") {
        None => None,
        Some(bytes) => {
            let processed = PhotoPipeline::process(bytes)
                .map_err(|e| WebError::BadRequest(e.to_string()))?;
            let key = state.photos().new_key("sighting");
            state.photos().put(&key, processed.bytes).await?;
            Some(PhotoRef { key, content_type: processed.content_type.to_owned() })
        }
    };
    // Browser-provided GPS: optional, but always lat+lon together.
    let coordinates = match (form.text("latitude"), form.text("longitude")) {
        (None, None) => None,
        (Some(lat), Some(lon)) => {
            let parse = |v: &str, field| {
                v.parse::<f64>()
                    .map_err(|e| WebError::BadRequest(format!("{field}: {e}")))
            };
            Some(
                Coordinates::new(parse(lat, "latitude")?, parse(lon, "longitude")?)
                    .map_err(|e| WebError::BadRequest(e.to_string()))?,
            )
        }
        _ => return Err(WebError::BadRequest("latitude/longitude must come together".into())),
    };
    state
        .sightings()
        .log(NewSighting {
            duck_id: duck.id,
            vessel_id,
            user_id: user.id,
            seen_at,
            note,
            photo,
            coordinates,
        })
        .await?;
    Ok(Redirect::to(&format!("/d/{}?m=Find+recorded!", duck.code.as_str())))
}

pub async fn comment(
    State(state): State<AppState>,
    Path(raw_code): Path<String>,
    user: AuthenticatedUser,
    Form(form): Form<CommentForm>,
) -> Result<Redirect, WebError> {
    let (duck, _) = resolve_sailing(&state, &raw_code).await?;
    let body = CommentBody::parse(&form.body)
        .map_err(|e| WebError::BadRequest(format!("comment: {e}")))?;
    state.comments().add(duck.id, user.id, &body).await?;
    Ok(Redirect::to(&format!("/d/{}", duck.code.as_str())))
}

#[derive(serde::Deserialize)]
pub struct CommentForm {
    pub body: String,
}

pub async fn follow(
    State(state): State<AppState>,
    Path(raw_code): Path<String>,
    user: AuthenticatedUser,
) -> Result<Redirect, WebError> {
    let (duck, _) = resolve_sailing(&state, &raw_code).await?;
    state.follows().follow(user.id, duck.id).await?;
    Ok(Redirect::to(&format!("/d/{}", duck.code.as_str())))
}

pub async fn unfollow(
    State(state): State<AppState>,
    Path(raw_code): Path<String>,
    user: AuthenticatedUser,
) -> Result<Redirect, WebError> {
    let (duck, _) = resolve_sailing(&state, &raw_code).await?;
    state.follows().unfollow(user.id, duck.id).await?;
    Ok(Redirect::to(&format!("/d/{}", duck.code.as_str())))
}

/// QR label PNG. Owner-only until the duck sails; public afterward.
pub async fn qr_png(
    State(state): State<AppState>,
    Path(raw_code): Path<String>,
    user: Option<AuthenticatedUser>,
) -> Result<Response, WebError> {
    let (duck, flock) = resolve(&state, &raw_code).await?;
    (duck.lifecycle.is_sailing() || is_owner(user.as_ref(), &flock))
        .then_some(())
        .ok_or(WebError::NotFound)?;
    let png = QrLabel::png(state.base_url(), &duck.code)
        .map_err(|e| WebError::BadRequest(e.to_string()))?;
    Ok(([(header::CONTENT_TYPE, "image/png")], png).into_response())
}

/// The duck's defining photo: public once sailing, owner-only while staged.
pub async fn origin_photo(
    State(state): State<AppState>,
    Path(raw_code): Path<String>,
    user: Option<AuthenticatedUser>,
) -> Result<Response, WebError> {
    let (duck, flock) = resolve(&state, &raw_code).await?;
    let visible = duck.lifecycle.is_sailing() || is_owner(user.as_ref(), &flock);
    match (visible, duck.lifecycle.details()) {
        (true, Some(details)) => serve_photo(&state, &details.photo).await,
        _ => Err(WebError::NotFound),
    }
}

/// A sighting's photo, addressed through its duck so raw object keys never
/// appear in URLs.
pub async fn sighting_photo(
    State(state): State<AppState>,
    Path((raw_code, sighting_id)): Path<(String, i64)>,
) -> Result<Response, WebError> {
    let (duck, _) = resolve_sailing(&state, &raw_code).await?;
    let sighting = state
        .sightings()
        .by_id(SightingId::new(sighting_id))
        .await?
        .filter(|s| s.duck_id == duck.id)
        .ok_or(WebError::NotFound)?;
    let photo = sighting.photo.as_ref().ok_or(WebError::NotFound)?;
    serve_photo(&state, photo).await
}

async fn serve_photo(state: &AppState, photo: &PhotoRef) -> Result<Response, WebError> {
    let bytes = state.photos().get(&photo.key).await?;
    Ok((
        [
            (header::CONTENT_TYPE, photo.content_type.clone()),
            (header::CACHE_CONTROL, "public, max-age=86400".to_owned()),
        ],
        bytes,
    )
        .into_response())
}

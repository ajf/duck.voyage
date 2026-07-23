//! The admin section: global overview plus the destructive levers. Every
//! handler takes `AdminUser`, so a non-admin can't reach any of it.

use auth::AdminUser;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use domain::{FlockId, PhotoKey, UserId};

use crate::error::WebError;
use crate::handlers::nav;
use crate::state::AppState;
use crate::views::Page;

pub async fn overview(
    State(state): State<AppState>,
    admin: AdminUser,
    axum::extract::Query(flash): axum::extract::Query<crate::handlers::duck::FlashParam>,
) -> Result<Response, WebError> {
    let nav = nav(&state, Some(&admin.0)).await?;
    let providers = state.oidc().summaries();
    let users = state.admin().users().await?;
    let flocks = state.admin().flocks().await?;
    Ok(Page::admin(&nav, flash.m.as_deref(), &providers, &users, &flocks).into_response())
}

pub async fn delete_flock(
    State(state): State<AppState>,
    Path(flock_id): Path<i64>,
    _admin: AdminUser,
) -> Result<Redirect, WebError> {
    let photo_keys = state
        .admin()
        .delete_flock(FlockId::new(flock_id))
        .await?
        .ok_or(WebError::NotFound)?;
    cleanup_photos(&state, photo_keys).await;
    Ok(Redirect::to("/admin?m=Flock+deleted"))
}

pub async fn delete_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    admin: AdminUser,
) -> Result<Redirect, WebError> {
    let target = UserId::new(user_id);
    (target != admin.0.id)
        .then_some(())
        .ok_or_else(|| WebError::BadRequest("you can't delete your own account".into()))?;
    let photo_keys = state.admin().delete_user(target).await?.ok_or_else(|| {
        WebError::BadRequest("no such user, or the user is an admin (demote first)".into())
    })?;
    cleanup_photos(&state, photo_keys).await;
    Ok(Redirect::to("/admin?m=User+deleted"))
}

/// Best-effort object-store cleanup: the rows are already gone, so the
/// photos are unreachable either way; failures are logged, not fatal.
async fn cleanup_photos(state: &AppState, keys: Vec<String>) {
    for key in keys {
        if let Err(e) = state.photos().delete(&PhotoKey::new(key)).await {
            tracing::warn!(error = %e, "orphaned photo not removed from store");
        }
    }
}

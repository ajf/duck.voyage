use auth::AuthenticatedUser;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use storage::FlockDuckStatus;

use crate::error::WebError;
use crate::handlers::duck::FlashParam;
use crate::handlers::nav;
use crate::state::AppState;
use crate::views::Page;

pub async fn me(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Response, WebError> {
    let nav = nav(&state, Some(&user)).await?;
    let notifications = state.notifications().feed(user.id, 50).await?;
    let sightings = state.sightings().for_user(user.id).await?;
    let follows = state.follows().followed_ducks(user.id).await?;
    Ok(Page::me(&nav, &notifications, &sightings, &follows).into_response())
}

pub async fn mark_notifications_read(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Redirect, WebError> {
    state.notifications().mark_all_read(user.id).await?;
    Ok(Redirect::to("/me"))
}

pub async fn my_flocks(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(flash): Query<FlashParam>,
) -> Result<Response, WebError> {
    let nav = nav(&state, Some(&user)).await?;
    let flocks = state.flocks().for_owner(user.id).await?;
    let mut with_ducks: Vec<(domain::Flock, Vec<FlockDuckStatus>)> = Vec::new();
    for flock in flocks {
        let ducks = state.ducks().for_flock(flock.id).await?;
        with_ducks.push((flock, ducks));
    }
    Ok(Page::flocks(&nav, flash.m.as_deref(), &with_ducks, state.caps().mint_batch_max)
        .into_response())
}

//! Storage edge: sqlx repositories over Postgres, plus the object-store
//! wrapper for photos. Row→domain conversion happens at this boundary; no
//! raw rows escape.

mod db;
mod error;
mod photos;
mod repos;
mod rows;
mod summaries;

pub use db::Db;
/// Re-exported so downstream crates (session store) can share our pool
/// without pinning sqlx themselves.
pub use sqlx::PgPool;
pub use error::RepoError;
pub use photos::{PhotoStore, PhotoStoreError};
pub use repos::{
    CommentRepo, DuckRepo, FlockRepo, FollowRepo, NewSighting, NotificationRepo, OidcFlowRepo,
    SightingRepo, StoredLoginFlow, UserRepo, VesselRepo,
};
pub use summaries::{
    CommentView, DuckSummary, FlockDuckStatus, FollowedDuck, MySighting, NotificationView,
    SightingView, VesselOption,
};

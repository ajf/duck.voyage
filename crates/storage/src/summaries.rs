//! Read models: shapes queried for display, distinct from the entities.

use domain::{
    CommentId, Coordinates, Duck, DuckCode, FlockCode, FlockId, SightingId, UserId, VesselId,
};
use jiff::Timestamp;

/// A duck as it appears on the front page or the missing-ducks page: its
/// public identity plus last-sighting context.
#[derive(Debug, Clone)]
pub struct DuckSummary {
    pub code: DuckCode,
    pub name: Option<String>,
    pub sighting_count: i64,
    /// How many distinct vessels it has been found aboard.
    pub unique_vessels: i64,
    pub last_seen_at: Timestamp,
    pub last_vessel_name: String,
}

/// One row of the front page's "latest finds" feed.
#[derive(Debug, Clone)]
pub struct RecentFind {
    pub duck_code: DuckCode,
    pub duck_name: Option<String>,
    pub vessel_name: String,
    pub by_display_name: Option<String>,
    pub seen_at: Timestamp,
}

/// One row of a duck page's sighting history.
#[derive(Debug, Clone)]
pub struct SightingView {
    pub id: SightingId,
    pub seen_at: Timestamp,
    pub vessel_name: String,
    pub by_display_name: Option<String>,
    pub note: Option<String>,
    pub has_photo: bool,
    pub coordinates: Option<Coordinates>,
    pub created_at: Timestamp,
}

/// One row of a duck page's comment thread.
#[derive(Debug, Clone)]
pub struct CommentView {
    pub id: CommentId,
    pub by_display_name: Option<String>,
    pub body: String,
    pub created_at: Timestamp,
}

/// A duck in the flock dashboard: full entity plus how often it's been found.
#[derive(Debug, Clone)]
pub struct FlockDuckStatus {
    pub duck: Duck,
    pub sighting_count: i64,
}

/// A sighting in the `/me` history: what you found, where, when.
#[derive(Debug, Clone)]
pub struct MySighting {
    pub duck_code: DuckCode,
    pub duck_name: Option<String>,
    pub vessel_name: String,
    pub seen_at: Timestamp,
}

/// A followed duck on `/me`.
#[derive(Debug, Clone)]
pub struct FollowedDuck {
    pub duck_code: DuckCode,
    pub duck_name: Option<String>,
    pub followed_at: Timestamp,
}

/// One activity-feed row: a duck you follow was sighted.
#[derive(Debug, Clone)]
pub struct NotificationView {
    pub duck_code: DuckCode,
    pub duck_name: Option<String>,
    pub vessel_name: String,
    pub seen_at: Timestamp,
    pub created_at: Timestamp,
    pub unread: bool,
}

/// A user as the admin overview sees them: identity plus activity counts.
#[derive(Debug, Clone)]
pub struct AdminUserOverview {
    pub id: UserId,
    pub issuer: String,
    pub subject: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub is_admin: bool,
    pub created_at: Timestamp,
    pub flocks: i64,
    pub sightings: i64,
    pub comments: i64,
}

/// A flock as the admin overview sees it.
#[derive(Debug, Clone)]
pub struct AdminFlockOverview {
    pub id: FlockId,
    pub code: FlockCode,
    pub label: Option<String>,
    pub owner_id: UserId,
    pub owner_name: Option<String>,
    pub ducks: i64,
    pub sailing: i64,
    pub sightings: i64,
    pub created_at: Timestamp,
}

/// An entry in the sighting form's vessel picker, carrying the current
/// operator so the form can group by cruise line.
#[derive(Debug, Clone)]
pub struct VesselOption {
    pub id: VesselId,
    pub name: String,
    pub line: Option<String>,
}

//! Read models: shapes queried for display, distinct from the entities.

use domain::{Coordinates, Duck, DuckCode, SightingId, VesselId};
use jiff::Timestamp;

/// A duck as it appears on the front page or the missing-ducks page: its
/// public identity plus last-sighting context.
#[derive(Debug, Clone)]
pub struct DuckSummary {
    pub code: DuckCode,
    pub name: Option<String>,
    pub sighting_count: i64,
    pub last_seen_at: Timestamp,
    pub last_vessel_name: String,
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

/// An entry in the sighting form's vessel picker, carrying the current
/// operator so the form can group by cruise line.
#[derive(Debug, Clone)]
pub struct VesselOption {
    pub id: VesselId,
    pub name: String,
    pub line: Option<String>,
}

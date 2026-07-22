use jiff::Timestamp;

use crate::codes::{DuckCode, FlockCode, FlockSeq, KeyGeneration};
use crate::coords::Coordinates;
use crate::ids::{
    CommentId, CruiseLineId, DuckId, FlockId, NotificationId, SightingId, UserId, VesselId,
};
use crate::imo::ImoNumber;
use crate::text::{CommentBody, DuckDescription, DuckName, FlockLabel, Note, OidcSubject, PhotoRef};

/// A duck's definition: attached by the flock owner, required before the
/// duck can set sail. Only exists from the `Staged` state onward, so
/// "allocated duck with a photo" is unrepresentable.
#[derive(Debug, Clone, PartialEq)]
pub struct DuckDetails {
    pub defined_at: Timestamp,
    pub description: DuckDescription,
    pub name: Option<DuckName>,
    pub photo: PhotoRef,
}

/// A duck's lifecycle. `Allocated` and `Staged` ducks are publicly
/// indistinguishable from nonexistent codes; only `Sailing` ducks are live.
/// Staging lets the owner define the duck at home (photo, description) and
/// set it loose later — typically by scanning the printed sticker once it's
/// placed aboard.
#[derive(Debug, Clone, PartialEq)]
pub enum DuckLifecycle {
    Allocated,
    Staged(DuckDetails),
    Sailing { details: DuckDetails, since: Timestamp },
}

impl DuckLifecycle {
    pub fn is_sailing(&self) -> bool {
        matches!(self, Self::Sailing { .. })
    }

    pub fn details(&self) -> Option<&DuckDetails> {
        match self {
            Self::Allocated => None,
            Self::Staged(details) | Self::Sailing { details, .. } => Some(details),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Duck {
    pub id: DuckId,
    pub code: DuckCode,
    pub flock_id: FlockId,
    pub seq: FlockSeq,
    pub lifecycle: DuckLifecycle,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Flock {
    pub id: FlockId,
    pub code: FlockCode,
    pub generation: KeyGeneration,
    pub owner: UserId,
    pub label: Option<FlockLabel>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppUser {
    pub id: UserId,
    pub oidc: OidcSubject,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub is_admin: bool,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Vessel {
    pub id: VesselId,
    pub imo: Option<ImoNumber>,
    pub name: String,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CruiseLine {
    pub id: CruiseLineId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sighting {
    pub id: SightingId,
    pub duck_id: DuckId,
    pub vessel_id: VesselId,
    pub user_id: UserId,
    pub seen_at: Timestamp,
    pub note: Option<Note>,
    pub photo: Option<PhotoRef>,
    pub coordinates: Option<Coordinates>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub id: CommentId,
    pub duck_id: DuckId,
    pub user_id: UserId,
    pub body: CommentBody,
    pub created_at: Timestamp,
}

/// One row of a user's in-app activity feed: a followed duck was sighted.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: NotificationId,
    pub user_id: UserId,
    pub duck_id: DuckId,
    pub sighting_id: SightingId,
    pub created_at: Timestamp,
    pub read_at: Option<Timestamp>,
}

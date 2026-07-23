//! Raw row structs and their conversions into domain entities. All row→domain
//! mapping goes through `TryFrom`, so a value that violates domain invariants
//! surfaces as [`RepoError::Corrupt`] instead of leaking outward.

use domain::{
    AppUser, Comment, CommentBody, CommentId, Coordinates, Duck, DuckCode, DuckDescription,
    DuckDetails, DuckId, DuckLifecycle, DuckName, Flock, FlockCode, FlockId, FlockLabel, FlockSeq,
    ImoNumber, KeyGeneration, OidcSubject, PhotoKey, PhotoRef, Sighting, SightingId, UserId,
    Vessel, VesselId,
};
use jiff_sqlx::Timestamp as SqlTimestamp;

use crate::error::RepoError;

pub(crate) struct DuckRow {
    pub id: i64,
    pub code: String,
    pub flock_id: i64,
    pub flock_seq: i16,
    pub name: Option<String>,
    pub description: Option<String>,
    pub originated_at: Option<SqlTimestamp>,
    pub set_sail_at: Option<SqlTimestamp>,
    pub deleted_at: Option<SqlTimestamp>,
    pub comments_locked_at: Option<SqlTimestamp>,
    pub origin_photo_key: Option<String>,
    pub origin_photo_content_type: Option<String>,
    pub created_at: SqlTimestamp,
}

impl TryFrom<DuckRow> for Duck {
    type Error = RepoError;

    fn try_from(r: DuckRow) -> Result<Self, RepoError> {
        let corrupt = |detail: String| RepoError::Corrupt { table: "duck", detail };
        let lifecycle = match (
            r.originated_at,
            r.set_sail_at,
            r.description,
            r.origin_photo_key,
            r.origin_photo_content_type,
        ) {
            (None, None, None, None, None) => DuckLifecycle::Allocated,
            (Some(defined_at), sailed, Some(description), Some(key), Some(content_type)) => {
                let details = DuckDetails {
                    defined_at: defined_at.to_jiff(),
                    description: DuckDescription::parse(&description)
                        .map_err(|e| corrupt(format!("description: {e}")))?,
                    name: r
                        .name
                        .map(|n| DuckName::parse(&n).map_err(|e| corrupt(format!("name: {e}"))))
                        .transpose()?,
                    photo: PhotoRef { key: PhotoKey::new(key), content_type },
                };
                match sailed {
                    None => DuckLifecycle::Staged(details),
                    Some(since) => DuckLifecycle::Sailing { details, since: since.to_jiff() },
                }
            }
            _ => return Err(corrupt("inconsistent origination columns".into())),
        };
        Ok(Duck {
            id: DuckId::new(r.id),
            code: DuckCode::parse(&r.code).map_err(|e| corrupt(format!("code: {e}")))?,
            flock_id: FlockId::new(r.flock_id),
            seq: FlockSeq::new(u32::try_from(r.flock_seq).unwrap_or(0))
                .map_err(|e| corrupt(format!("flock_seq: {e}")))?,
            lifecycle,
            deleted_at: r.deleted_at.map(|t| t.to_jiff()),
            comments_locked_at: r.comments_locked_at.map(|t| t.to_jiff()),
            created_at: r.created_at.to_jiff(),
        })
    }
}

pub(crate) struct FlockRow {
    pub id: i64,
    pub flock_code: String,
    pub key_generation: i16,
    pub owner_user_id: i64,
    pub label: Option<String>,
    pub created_at: SqlTimestamp,
}

impl TryFrom<FlockRow> for Flock {
    type Error = RepoError;

    fn try_from(r: FlockRow) -> Result<Self, RepoError> {
        let corrupt = |detail: String| RepoError::Corrupt { table: "flock", detail };
        Ok(Flock {
            id: FlockId::new(r.id),
            code: FlockCode::parse(&r.flock_code).map_err(|e| corrupt(format!("flock_code: {e}")))?,
            generation: KeyGeneration::new(
                u16::try_from(r.key_generation)
                    .map_err(|_| corrupt(format!("key_generation {}", r.key_generation)))?,
            ),
            owner: UserId::new(r.owner_user_id),
            label: r
                .label
                .map(|l| FlockLabel::parse(&l).map_err(|e| corrupt(format!("label: {e}"))))
                .transpose()?,
            created_at: r.created_at.to_jiff(),
        })
    }
}

pub(crate) struct UserRow {
    pub id: i64,
    pub issuer: String,
    pub subject: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub is_admin: bool,
    pub created_at: SqlTimestamp,
}

impl From<UserRow> for AppUser {
    fn from(r: UserRow) -> Self {
        AppUser {
            id: UserId::new(r.id),
            oidc: OidcSubject::new(r.issuer, r.subject),
            display_name: r.display_name,
            email: r.email,
            is_admin: r.is_admin,
            created_at: r.created_at.to_jiff(),
        }
    }
}

pub(crate) struct VesselRow {
    pub id: i64,
    pub imo_number: Option<String>,
    pub name: String,
    pub created_at: SqlTimestamp,
}

impl TryFrom<VesselRow> for Vessel {
    type Error = RepoError;

    fn try_from(r: VesselRow) -> Result<Self, RepoError> {
        Ok(Vessel {
            id: VesselId::new(r.id),
            imo: r
                .imo_number
                .map(|raw| {
                    ImoNumber::parse(&raw)
                        .map_err(|e| RepoError::corrupt("vessel", format!("imo_number: {e}")))
                })
                .transpose()?,
            name: r.name,
            created_at: r.created_at.to_jiff(),
        })
    }
}

pub(crate) struct SightingRow {
    pub id: i64,
    pub duck_id: i64,
    pub vessel_id: i64,
    pub user_id: i64,
    pub seen_at: SqlTimestamp,
    pub note: Option<String>,
    pub photo_key: Option<String>,
    pub photo_content_type: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub created_at: SqlTimestamp,
}

impl TryFrom<SightingRow> for Sighting {
    type Error = RepoError;

    fn try_from(r: SightingRow) -> Result<Self, RepoError> {
        let corrupt = |detail: String| RepoError::Corrupt { table: "sighting", detail };
        let photo = match (r.photo_key, r.photo_content_type) {
            (None, None) => None,
            (Some(key), Some(content_type)) => Some(PhotoRef { key: PhotoKey::new(key), content_type }),
            _ => return Err(corrupt("inconsistent photo columns".into())),
        };
        let coordinates = match (r.latitude, r.longitude) {
            (None, None) => None,
            (Some(lat), Some(lon)) => Some(
                Coordinates::new(lat, lon).map_err(|e| corrupt(format!("coordinates: {e}")))?,
            ),
            _ => return Err(corrupt("inconsistent coordinate columns".into())),
        };
        Ok(Sighting {
            id: SightingId::new(r.id),
            duck_id: DuckId::new(r.duck_id),
            vessel_id: VesselId::new(r.vessel_id),
            user_id: UserId::new(r.user_id),
            seen_at: r.seen_at.to_jiff(),
            note: r
                .note
                .map(|n| Note::parse(&n).map_err(|e| corrupt(format!("note: {e}"))))
                .transpose()?,
            photo,
            coordinates,
            created_at: r.created_at.to_jiff(),
        })
    }
}

use domain::Note;

pub(crate) struct CommentRow {
    pub id: i64,
    pub duck_id: i64,
    pub user_id: i64,
    pub body: String,
    pub created_at: SqlTimestamp,
}

impl TryFrom<CommentRow> for Comment {
    type Error = RepoError;

    fn try_from(r: CommentRow) -> Result<Self, RepoError> {
        Ok(Comment {
            id: CommentId::new(r.id),
            duck_id: DuckId::new(r.duck_id),
            user_id: UserId::new(r.user_id),
            body: CommentBody::parse(&r.body)
                .map_err(|e| RepoError::corrupt("duck_comment", format!("body: {e}")))?,
            created_at: r.created_at.to_jiff(),
        })
    }
}

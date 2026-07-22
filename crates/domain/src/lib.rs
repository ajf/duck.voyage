//! Pure domain logic for the duck tracker: identifiers, the FF1 code codec,
//! entities, and checksum machinery. No I/O, no async — everything here is
//! testable as plain functions-on-types.

mod base36;
mod codec;
mod codes;
mod coords;
mod damm;
mod entities;
mod ids;
mod imo;
mod profanity;
mod text;

pub use base36::Base36;
pub use codec::{CodecError, DuckCodec};
pub use codes::{
    DuckCode, DuckCodeError, FlockCode, FlockCodeError, FlockSeq, FlockSeqError, KeyGeneration,
};
pub use coords::{Coordinates, CoordinatesError};
pub use damm::Damm36;
pub use entities::{
    AppUser, Comment, CruiseLine, Duck, DuckDetails, DuckLifecycle, Flock, Notification, Sighting,
    Vessel,
};
pub use profanity::Profanity;
pub use ids::{
    CommentId, CruiseLineId, DuckId, FlockId, NotificationId, SightingId, UserId, VesselId,
};
pub use imo::{ImoError, ImoNumber};
pub use text::{
    CommentBody, DuckDescription, DuckName, FlockLabel, Note, OidcSubject, PhotoKey, PhotoRef,
    TextError,
};

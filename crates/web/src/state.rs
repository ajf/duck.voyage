use std::sync::Arc;

use auth::OidcProviders;
use domain::DuckCodec;
use storage::{
    CommentRepo, Db, DuckRepo, FlockRepo, FollowRepo, NotificationRepo, OidcFlowRepo, PhotoStore,
    SightingRepo, UserRepo, VesselRepo,
};

use crate::config::Caps;

/// Shared application state. Repos are constructed on demand from the pool
/// (cheap `Arc` clone), so handlers just call `state.ducks()`.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    db: Db,
    codec: DuckCodec,
    photos: PhotoStore,
    oidc: OidcProviders,
    base_url: String,
    caps: Caps,
    admin_identities: Vec<(String, String)>,
}

impl AppState {
    pub fn new(
        db: Db,
        codec: DuckCodec,
        photos: PhotoStore,
        oidc: OidcProviders,
        base_url: String,
        caps: Caps,
        admin_identities: Vec<(String, String)>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner { db, codec, photos, oidc, base_url, caps, admin_identities }),
        }
    }

    pub fn codec(&self) -> &DuckCodec {
        &self.inner.codec
    }

    pub fn photos(&self) -> &PhotoStore {
        &self.inner.photos
    }

    pub fn oidc(&self) -> &OidcProviders {
        &self.inner.oidc
    }

    pub fn base_url(&self) -> &str {
        &self.inner.base_url
    }

    pub fn caps(&self) -> Caps {
        self.inner.caps
    }

    pub fn is_admin_identity(&self, issuer: &str, subject: &str) -> bool {
        self.inner
            .admin_identities
            .iter()
            .any(|(iss, sub)| iss == issuer && sub == subject)
    }

    pub fn users(&self) -> UserRepo {
        UserRepo(self.inner.db.pool().clone())
    }

    pub fn flocks(&self) -> FlockRepo {
        FlockRepo(self.inner.db.pool().clone())
    }

    pub fn ducks(&self) -> DuckRepo {
        DuckRepo(self.inner.db.pool().clone())
    }

    pub fn sightings(&self) -> SightingRepo {
        SightingRepo(self.inner.db.pool().clone())
    }

    pub fn comments(&self) -> CommentRepo {
        CommentRepo(self.inner.db.pool().clone())
    }

    pub fn follows(&self) -> FollowRepo {
        FollowRepo(self.inner.db.pool().clone())
    }

    pub fn notifications(&self) -> NotificationRepo {
        NotificationRepo(self.inner.db.pool().clone())
    }

    pub fn vessels(&self) -> VesselRepo {
        VesselRepo(self.inner.db.pool().clone())
    }

    pub fn oidc_flows(&self) -> OidcFlowRepo {
        OidcFlowRepo(self.inner.db.pool().clone())
    }
}

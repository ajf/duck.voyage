use std::sync::Arc;

use bytes::Bytes;
use domain::PhotoKey;
use object_store::aws::AmazonS3Builder;
use object_store::local::LocalFileSystem;
use object_store::path::Path as ObjectPath;
use object_store::ObjectStore;
use rand::Rng;

#[derive(Debug, thiserror::Error)]
pub enum PhotoStoreError {
    #[error("object store error: {0}")]
    Store(#[from] object_store::Error),
}

/// Photo blob storage behind `object_store`: MinIO locally, Tigris/S3 in
/// prod, plain filesystem for tests — same interface. Keys are opaque and
/// private; serving goes back through the app.
#[derive(Clone)]
pub struct PhotoStore {
    store: Arc<dyn ObjectStore>,
}

impl PhotoStore {
    /// S3-compatible backend (MinIO, Tigris). `allow_http` accommodates the
    /// local MinIO endpoint.
    pub fn s3_compatible(
        endpoint: &str,
        bucket: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self, PhotoStoreError> {
        let store = AmazonS3Builder::new()
            .with_endpoint(endpoint)
            .with_bucket_name(bucket)
            .with_access_key_id(access_key)
            .with_secret_access_key(secret_key)
            .with_region("auto")
            .with_allow_http(true)
            .with_virtual_hosted_style_request(false)
            .build()?;
        Ok(Self { store: Arc::new(store) })
    }

    pub fn local(root: &std::path::Path) -> Result<Self, PhotoStoreError> {
        Ok(Self {
            store: Arc::new(LocalFileSystem::new_with_prefix(root)?),
        })
    }

    /// Mint a fresh opaque key under a kind prefix (`origin/…`, `sighting/…`).
    pub fn new_key(&self, kind: &str) -> PhotoKey {
        let mut rng = rand::rng();
        let suffix: String = (0..32)
            .map(|_| char::from_digit(rng.random_range(0..16), 16).expect("hex digit"))
            .collect();
        PhotoKey::new(format!("{kind}/{suffix}"))
    }

    pub async fn put(&self, key: &PhotoKey, bytes: Bytes) -> Result<(), PhotoStoreError> {
        self.store
            .put(&ObjectPath::from(key.as_str()), bytes.into())
            .await?;
        Ok(())
    }

    pub async fn get(&self, key: &PhotoKey) -> Result<Bytes, PhotoStoreError> {
        Ok(self
            .store
            .get(&ObjectPath::from(key.as_str()))
            .await?
            .bytes()
            .await?)
    }
}

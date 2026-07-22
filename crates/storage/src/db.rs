use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::error::RepoError;

/// Connection handle. Repos are constructed from this; the pool is an `Arc`
/// internally, so cloning is cheap.
#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn connect(database_url: &str) -> Result<Self, RepoError> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    /// Apply pending migrations (called once at startup).
    pub async fn migrate(&self) -> Result<(), RepoError> {
        sqlx::migrate!("../../migrations")
            .run(&self.pool)
            .await
            .map_err(|e| RepoError::Sql(e.into()))
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

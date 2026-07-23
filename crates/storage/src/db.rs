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
    /// Cluster-wide advisory key for serializing startup work. Arbitrary but
    /// fixed: "duck" in ASCII.
    const STARTUP_LOCK_KEY: i64 = 0x6475636b;

    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, RepoError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    /// Apply pending migrations (called once at startup). Safe under
    /// concurrent instance boots: sqlx's migrator holds its own Postgres
    /// advisory lock while running.
    pub async fn migrate(&self) -> Result<(), RepoError> {
        sqlx::migrate!("../../migrations")
            .run(&self.pool)
            .await
            .map_err(|e| RepoError::Sql(e.into()))
    }

    /// Run `work` while holding a cluster-wide advisory lock, serializing
    /// non-idempotent startup steps (e.g. third-party table creation that
    /// lacks its own locking) across concurrently booting instances.
    pub async fn with_startup_lock<T, E>(
        &self,
        work: impl Future<Output = Result<T, E>>,
    ) -> Result<Result<T, E>, RepoError> {
        let mut conn = self.pool.acquire().await?;
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(Self::STARTUP_LOCK_KEY)
            .execute(&mut *conn)
            .await?;
        let result = work.await;
        sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(Self::STARTUP_LOCK_KEY)
            .execute(&mut *conn)
            .await?;
        Ok(result)
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

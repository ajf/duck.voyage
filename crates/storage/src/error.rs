#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("database error: {0}")]
    Sql(#[from] sqlx::Error),
    /// A stored row failed domain validation on the way out — this means the
    /// database contains data the application could never have written.
    #[error("corrupt row in {table}: {detail}")]
    Corrupt { table: &'static str, detail: String },
}

impl RepoError {
    pub fn corrupt(table: &'static str, detail: impl std::fmt::Display) -> Self {
        Self::Corrupt {
            table,
            detail: detail.to_string(),
        }
    }
}

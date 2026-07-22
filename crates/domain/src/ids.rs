/// Surrogate-key newtypes. All are `i64` to match Postgres `BIGINT` identity
/// columns, but each is its own type — a `VesselId` can never be passed where
/// a `DuckId` is wanted.
macro_rules! id_newtype {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(i64);

        impl $name {
            pub fn new(id: i64) -> Self {
                Self(id)
            }

            pub fn get(self) -> i64 {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id_newtype! {
    /// Internal, sequential duck primary key. Never leaves the server.
    DuckId
}
id_newtype! {
    /// Surrogate primary key of a flock row.
    FlockId
}
id_newtype! {
    /// Internal user primary key.
    UserId
}
id_newtype! {
    /// Surrogate primary key of a vessel row.
    VesselId
}
id_newtype! {
    /// Surrogate primary key of a cruise line (operator) row.
    CruiseLineId
}
id_newtype! {
    /// Surrogate primary key of a sighting row.
    SightingId
}
id_newtype! {
    /// Surrogate primary key of a comment row.
    CommentId
}
id_newtype! {
    /// Surrogate primary key of a notification row.
    NotificationId
}

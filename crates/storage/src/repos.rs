//! Repositories: all SQL lives here, compile-time-checked by sqlx. Behaviour
//! hangs off repo types holding a pool clone; rows convert to domain entities
//! via the `TryFrom` impls in `rows`.

use domain::{
    AppUser, Base36, CommentBody, CommentId, Coordinates, Duck, DuckCode, DuckDescription, DuckId,
    DuckName, Flock, FlockCode, FlockId, FlockLabel, FlockSeq, KeyGeneration, Note, OidcSubject,
    PhotoRef, Sighting, SightingId, UserId, VesselId,
};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx;
use rand::Rng;
use sqlx::PgPool;

use crate::error::RepoError;
use crate::rows::{DuckRow, FlockRow, SightingRow, UserRow, VesselRow};
use crate::summaries::{
    CommentView, DuckSummary, FlockDuckStatus, FollowedDuck, MySighting, NotificationView,
    SightingView, VesselOption,
};

pub struct UserRepo(pub PgPool);

impl UserRepo {
    /// Upsert on `(iss, sub)`. `display_name`/`email` only overwrite when
    /// present (Apple sends the name exactly once); `grant_admin` can set but
    /// never clear the flag.
    pub async fn upsert(
        &self,
        oidc: &OidcSubject,
        display_name: Option<&str>,
        email: Option<&str>,
        grant_admin: bool,
    ) -> Result<AppUser, RepoError> {
        let row = sqlx::query_as!(
            UserRow,
            r#"
            INSERT INTO app_user (issuer, subject, display_name, email, is_admin)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (issuer, subject) DO UPDATE SET
                display_name = COALESCE(EXCLUDED.display_name, app_user.display_name),
                email        = COALESCE(EXCLUDED.email, app_user.email),
                is_admin     = app_user.is_admin OR EXCLUDED.is_admin
            RETURNING id, issuer, subject, display_name, email, is_admin,
                      created_at AS "created_at: jiff_sqlx::Timestamp"
            "#,
            oidc.issuer(),
            oidc.subject(),
            display_name,
            email,
            grant_admin,
        )
        .fetch_one(&self.0)
        .await?;
        Ok(row.into())
    }

    pub async fn by_id(&self, id: UserId) -> Result<Option<AppUser>, RepoError> {
        let row = sqlx::query_as!(
            UserRow,
            r#"
            SELECT id, issuer, subject, display_name, email, is_admin,
                   created_at AS "created_at: jiff_sqlx::Timestamp"
            FROM app_user WHERE id = $1
            "#,
            id.get(),
        )
        .fetch_optional(&self.0)
        .await?;
        Ok(row.map(Into::into))
    }
}

pub struct FlockRepo(pub PgPool);

impl FlockRepo {
    /// Create a flock under a randomly drawn unused prefix. The prefix space
    /// is 36³ = 46,656; a UNIQUE-constrained insert with retry is all the
    /// collision handling needed.
    pub async fn create(
        &self,
        owner: UserId,
        generation: KeyGeneration,
        label: Option<&FlockLabel>,
    ) -> Result<Flock, RepoError> {
        // Bounded retry: each attempt draws uniformly, so repeated collisions
        // only happen when the space is nearly full. Profane draws (`ASS`
        // happened in the field) are skipped and simply cost one attempt.
        for _ in 0..64 {
            let candidate: String = {
                let mut rng = rand::rng();
                (0..FlockCode::LEN)
                    .map(|_| Base36::char_of(rng.random_range(0..36)))
                    .collect()
            };
            if domain::Profanity::matches(&candidate) {
                continue;
            }
            let row = sqlx::query_as!(
                FlockRow,
                r#"
                INSERT INTO flock (flock_code, key_generation, owner_user_id, label)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (flock_code) DO NOTHING
                RETURNING id, flock_code, key_generation, owner_user_id, label,
                          created_at AS "created_at: jiff_sqlx::Timestamp"
                "#,
                candidate,
                i16::try_from(generation.get()).expect("generation fits smallint"),
                owner.get(),
                label.map(FlockLabel::as_str),
            )
            .fetch_optional(&self.0)
            .await?;
            if let Some(row) = row {
                return row.try_into();
            }
        }
        Err(RepoError::corrupt("flock", "could not draw an unused prefix in 64 attempts"))
    }

    pub async fn by_id(&self, id: FlockId) -> Result<Option<Flock>, RepoError> {
        let row = sqlx::query_as!(
            FlockRow,
            r#"
            SELECT id, flock_code, key_generation, owner_user_id, label,
                   created_at AS "created_at: jiff_sqlx::Timestamp"
            FROM flock WHERE id = $1
            "#,
            id.get(),
        )
        .fetch_optional(&self.0)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    pub async fn by_code(&self, code: &FlockCode) -> Result<Option<Flock>, RepoError> {
        let row = sqlx::query_as!(
            FlockRow,
            r#"
            SELECT id, flock_code, key_generation, owner_user_id, label,
                   created_at AS "created_at: jiff_sqlx::Timestamp"
            FROM flock WHERE flock_code = $1
            "#,
            code.as_str(),
        )
        .fetch_optional(&self.0)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    pub async fn for_owner(&self, owner: UserId) -> Result<Vec<Flock>, RepoError> {
        let rows = sqlx::query_as!(
            FlockRow,
            r#"
            SELECT id, flock_code, key_generation, owner_user_id, label,
                   created_at AS "created_at: jiff_sqlx::Timestamp"
            FROM flock WHERE owner_user_id = $1 ORDER BY created_at
            "#,
            owner.get(),
        )
        .fetch_all(&self.0)
        .await?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub async fn count_for_owner(&self, owner: UserId) -> Result<i64, RepoError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM flock WHERE owner_user_id = $1"#,
            owner.get(),
        )
        .fetch_one(&self.0)
        .await?)
    }
}

pub struct DuckRepo(pub PgPool);

impl DuckRepo {
    /// Mint the next `count` codes into a flock, atomically. The whole
    /// operation — reading the high-water mark, computing codes via
    /// `encode`, inserting — runs in one transaction holding a per-flock
    /// advisory lock, so concurrent mints (multiple app instances included)
    /// serialize instead of racing the UNIQUE constraints.
    ///
    /// `encode` maps a candidate seq to its code; returning `None` skips
    /// that seq permanently (profanity filtering — holes are harmless).
    pub async fn mint_batch(
        &self,
        flock: FlockId,
        count: u16,
        encode: impl Fn(FlockSeq) -> Option<DuckCode>,
    ) -> Result<Vec<Duck>, RepoError> {
        let mut tx = self.0.begin().await?;
        // Two-int advisory key: (class 1 = mint, flock id). Released at
        // commit/rollback.
        sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
            .bind(1i32)
            .bind(i32::try_from(flock.get()).expect("flock ids fit i32"))
            .execute(&mut *tx)
            .await?;
        let max = sqlx::query_scalar!(
            r#"SELECT COALESCE(MAX(flock_seq), 0) AS "max!" FROM duck WHERE flock_id = $1"#,
            flock.get(),
        )
        .fetch_one(&mut *tx)
        .await?;

        let mut entries: Vec<(FlockSeq, DuckCode)> = Vec::with_capacity(usize::from(count));
        let mut candidate = u32::try_from(max).expect("flock_seq is CHECKed to 1..=10000") + 1;
        while entries.len() < usize::from(count) {
            let seq = FlockSeq::new(candidate).map_err(|_| RepoError::FlockFull)?;
            if let Some(code) = encode(seq) {
                entries.push((seq, code));
            }
            candidate += 1;
        }

        let seqs: Vec<i16> = entries
            .iter()
            .map(|(s, _)| i16::try_from(s.get()).expect("seq <= 10000"))
            .collect();
        let codes: Vec<String> = entries.iter().map(|(_, c)| c.as_str().to_owned()).collect();
        let rows = sqlx::query_as!(
            DuckRow,
            r#"
            INSERT INTO duck (flock_id, flock_seq, code)
            SELECT $1, u.seq, u.code
            FROM UNNEST($2::smallint[], $3::text[]) AS u(seq, code)
            RETURNING id, code, flock_id, flock_seq, name, description,
                      originated_at AS "originated_at: jiff_sqlx::Timestamp",
                      set_sail_at AS "set_sail_at: jiff_sqlx::Timestamp",
                      origin_photo_key, origin_photo_content_type,
                      created_at AS "created_at: jiff_sqlx::Timestamp"
            "#,
            flock.get(),
            &seqs,
            &codes,
        )
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub async fn by_id(&self, id: DuckId) -> Result<Option<Duck>, RepoError> {
        let row = sqlx::query_as!(
            DuckRow,
            r#"
            SELECT id, code, flock_id, flock_seq, name, description,
                   originated_at AS "originated_at: jiff_sqlx::Timestamp",
                   set_sail_at AS "set_sail_at: jiff_sqlx::Timestamp",
                   origin_photo_key, origin_photo_content_type,
                   created_at AS "created_at: jiff_sqlx::Timestamp"
            FROM duck WHERE id = $1
            "#,
            id.get(),
        )
        .fetch_optional(&self.0)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    /// The scan-funnel lookup: a decoded candidate seq either has a row or the
    /// code was never minted.
    pub async fn by_flock_and_seq(
        &self,
        flock: FlockId,
        seq: FlockSeq,
    ) -> Result<Option<Duck>, RepoError> {
        let row = sqlx::query_as!(
            DuckRow,
            r#"
            SELECT id, code, flock_id, flock_seq, name, description,
                   originated_at AS "originated_at: jiff_sqlx::Timestamp",
                   set_sail_at AS "set_sail_at: jiff_sqlx::Timestamp",
                   origin_photo_key, origin_photo_content_type,
                   created_at AS "created_at: jiff_sqlx::Timestamp"
            FROM duck WHERE flock_id = $1 AND flock_seq = $2
            "#,
            flock.get(),
            i16::try_from(seq.get()).expect("seq <= 10000"),
        )
        .fetch_optional(&self.0)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    /// Attach the duck's definition (stage it). Does not set it sailing.
    /// Returns `None` when the duck already has details (lost race / double
    /// submit) — the caller re-reads.
    pub async fn stage(
        &self,
        id: DuckId,
        description: &DuckDescription,
        name: Option<&DuckName>,
        photo: &PhotoRef,
    ) -> Result<Option<Duck>, RepoError> {
        let row = sqlx::query_as!(
            DuckRow,
            r#"
            UPDATE duck
            SET originated_at = now(), description = $2, name = $3,
                origin_photo_key = $4, origin_photo_content_type = $5
            WHERE id = $1 AND originated_at IS NULL
            RETURNING id, code, flock_id, flock_seq, name, description,
                      originated_at AS "originated_at: jiff_sqlx::Timestamp",
                      set_sail_at AS "set_sail_at: jiff_sqlx::Timestamp",
                      origin_photo_key, origin_photo_content_type,
                      created_at AS "created_at: jiff_sqlx::Timestamp"
            "#,
            id.get(),
            description.as_str(),
            name.map(DuckName::as_str),
            photo.key.as_str(),
            photo.content_type,
        )
        .fetch_optional(&self.0)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    /// Set a staged duck sailing (live). Returns `None` when the duck has no
    /// details yet or already sailed.
    pub async fn set_sail(&self, id: DuckId) -> Result<Option<Duck>, RepoError> {
        let row = sqlx::query_as!(
            DuckRow,
            r#"
            UPDATE duck
            SET set_sail_at = now()
            WHERE id = $1 AND originated_at IS NOT NULL AND set_sail_at IS NULL
            RETURNING id, code, flock_id, flock_seq, name, description,
                      originated_at AS "originated_at: jiff_sqlx::Timestamp",
                      set_sail_at AS "set_sail_at: jiff_sqlx::Timestamp",
                      origin_photo_key, origin_photo_content_type,
                      created_at AS "created_at: jiff_sqlx::Timestamp"
            "#,
            id.get(),
        )
        .fetch_optional(&self.0)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    /// How many allocated-but-unoriginated ducks a user holds (mint cap).
    pub async fn unoriginated_count_for_owner(&self, owner: UserId) -> Result<i64, RepoError> {
        Ok(sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM duck d JOIN flock f ON f.id = d.flock_id
            WHERE f.owner_user_id = $1 AND d.originated_at IS NULL
            "#,
            owner.get(),
        )
        .fetch_one(&self.0)
        .await?)
    }

    /// Every duck in a flock with its sighting count — the flock dashboard.
    pub async fn for_flock(&self, flock: FlockId) -> Result<Vec<FlockDuckStatus>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT d.id, d.code, d.flock_id, d.flock_seq, d.name, d.description,
                   d.originated_at AS "originated_at: jiff_sqlx::Timestamp",
                   d.set_sail_at AS "set_sail_at: jiff_sqlx::Timestamp",
                   d.origin_photo_key, d.origin_photo_content_type,
                   d.created_at AS "created_at: jiff_sqlx::Timestamp",
                   (SELECT COUNT(*) FROM sighting s WHERE s.duck_id = d.id) AS "sighting_count!"
            FROM duck d WHERE d.flock_id = $1 ORDER BY d.flock_seq
            "#,
            flock.get(),
        )
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|r| {
                let duck: Duck = DuckRow {
                    id: r.id,
                    code: r.code,
                    flock_id: r.flock_id,
                    flock_seq: r.flock_seq,
                    name: r.name,
                    description: r.description,
                    originated_at: r.originated_at,
                    set_sail_at: r.set_sail_at,
                    origin_photo_key: r.origin_photo_key,
                    origin_photo_content_type: r.origin_photo_content_type,
                    created_at: r.created_at,
                }
                .try_into()?;
                Ok(FlockDuckStatus { duck, sighting_count: r.sighting_count })
            })
            .collect()
    }

    /// Front page: most recently found originated ducks.
    pub async fn recently_found(&self, limit: i64) -> Result<Vec<DuckSummary>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT d.code, d.name,
                   (SELECT COUNT(*) FROM sighting sc WHERE sc.duck_id = d.id) AS "sighting_count!",
                   last.seen_at AS "last_seen_at!: jiff_sqlx::Timestamp",
                   v.name AS "last_vessel_name!"
            FROM duck d
            JOIN LATERAL (
                SELECT s.seen_at, s.vessel_id FROM sighting s
                WHERE s.duck_id = d.id ORDER BY s.seen_at DESC LIMIT 1
            ) last ON true
            JOIN vessel v ON v.id = last.vessel_id
            ORDER BY last.seen_at DESC
            LIMIT $1
            "#,
            limit,
        )
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(DuckSummary {
                    code: DuckCode::parse(&r.code)
                        .map_err(|e| RepoError::corrupt("duck", format!("code: {e}")))?,
                    name: r.name,
                    sighting_count: r.sighting_count,
                    last_seen_at: r.last_seen_at.to_jiff(),
                    last_vessel_name: r.last_vessel_name,
                })
            })
            .collect()
    }

    /// Front page: ducks with the most sightings.
    pub async fn most_sighted(&self, limit: i64) -> Result<Vec<DuckSummary>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT d.code, d.name,
                   (SELECT COUNT(*) FROM sighting sc WHERE sc.duck_id = d.id) AS "sighting_count!",
                   last.seen_at AS "last_seen_at!: jiff_sqlx::Timestamp",
                   v.name AS "last_vessel_name!"
            FROM duck d
            JOIN LATERAL (
                SELECT s.seen_at, s.vessel_id FROM sighting s
                WHERE s.duck_id = d.id ORDER BY s.seen_at DESC LIMIT 1
            ) last ON true
            JOIN vessel v ON v.id = last.vessel_id
            ORDER BY 3 DESC, last.seen_at DESC
            LIMIT $1
            "#,
            limit,
        )
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(DuckSummary {
                    code: DuckCode::parse(&r.code)
                        .map_err(|e| RepoError::corrupt("duck", format!("code: {e}")))?,
                    name: r.name,
                    sighting_count: r.sighting_count,
                    last_seen_at: r.last_seen_at.to_jiff(),
                    last_vessel_name: r.last_vessel_name,
                })
            })
            .collect()
    }

    /// Missing ducks: sighted at least once, then silent since `cutoff`.
    /// The INNER JOIN encodes "was known to be traveling"; never-sighted
    /// originated ducks are merely unreleased, not lost.
    pub async fn missing_since(&self, cutoff: Timestamp) -> Result<Vec<DuckSummary>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT d.code, d.name,
                   (SELECT COUNT(*) FROM sighting sc WHERE sc.duck_id = d.id) AS "sighting_count!",
                   last.seen_at AS "last_seen_at!: jiff_sqlx::Timestamp",
                   v.name AS "last_vessel_name!"
            FROM duck d
            JOIN LATERAL (
                SELECT s.seen_at, s.vessel_id FROM sighting s
                WHERE s.duck_id = d.id ORDER BY s.seen_at DESC LIMIT 1
            ) last ON true
            JOIN vessel v ON v.id = last.vessel_id
            WHERE last.seen_at < $1
            ORDER BY last.seen_at ASC
            "#,
            cutoff.to_sqlx() as _,
        )
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(DuckSummary {
                    code: DuckCode::parse(&r.code)
                        .map_err(|e| RepoError::corrupt("duck", format!("code: {e}")))?,
                    name: r.name,
                    sighting_count: r.sighting_count,
                    last_seen_at: r.last_seen_at.to_jiff(),
                    last_vessel_name: r.last_vessel_name,
                })
            })
            .collect()
    }
}

/// Insert model for a new sighting.
pub struct NewSighting {
    pub duck_id: DuckId,
    pub vessel_id: VesselId,
    pub user_id: UserId,
    pub seen_at: Timestamp,
    pub note: Option<Note>,
    pub photo: Option<PhotoRef>,
    pub coordinates: Option<Coordinates>,
}

pub struct SightingRepo(pub PgPool);

impl SightingRepo {
    /// Log a find. One transaction: insert the sighting, notify existing
    /// followers (not the finder), then auto-follow the finder.
    pub async fn log(&self, new: NewSighting) -> Result<SightingId, RepoError> {
        let mut tx = self.0.begin().await?;
        let sighting_id = sqlx::query_scalar!(
            r#"
            INSERT INTO sighting (duck_id, vessel_id, user_id, seen_at, note, photo_key,
                                  photo_content_type, latitude, longitude)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id AS "id!"
            "#,
            new.duck_id.get(),
            new.vessel_id.get(),
            new.user_id.get(),
            new.seen_at.to_sqlx() as _,
            new.note.as_ref().map(Note::as_str),
            new.photo.as_ref().map(|p| p.key.as_str()),
            new.photo.as_ref().map(|p| p.content_type.as_str()),
            new.coordinates.map(Coordinates::latitude),
            new.coordinates.map(Coordinates::longitude),
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO notification (user_id, duck_id, sighting_id)
            SELECT f.user_id, $1, $2 FROM duck_follow f
            WHERE f.duck_id = $1 AND f.user_id <> $3
            "#,
            new.duck_id.get(),
            sighting_id,
            new.user_id.get(),
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO duck_follow (user_id, duck_id) VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
            new.user_id.get(),
            new.duck_id.get(),
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(SightingId::new(sighting_id))
    }

    pub async fn by_id(&self, id: SightingId) -> Result<Option<Sighting>, RepoError> {
        let row = sqlx::query_as!(
            SightingRow,
            r#"
            SELECT id, duck_id, vessel_id, user_id,
                   seen_at AS "seen_at: jiff_sqlx::Timestamp",
                   note, photo_key, photo_content_type, latitude, longitude,
                   created_at AS "created_at: jiff_sqlx::Timestamp"
            FROM sighting WHERE id = $1
            "#,
            id.get(),
        )
        .fetch_optional(&self.0)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    /// A duck page's sighting history, newest first.
    pub async fn for_duck(&self, duck: DuckId) -> Result<Vec<SightingView>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT s.id, s.seen_at AS "seen_at: jiff_sqlx::Timestamp", s.note,
                   s.photo_key IS NOT NULL AS "has_photo!",
                   s.latitude, s.longitude,
                   s.created_at AS "created_at: jiff_sqlx::Timestamp",
                   v.name AS vessel_name, u.display_name AS by_display_name
            FROM sighting s
            JOIN vessel v ON v.id = s.vessel_id
            JOIN app_user u ON u.id = s.user_id
            WHERE s.duck_id = $1
            ORDER BY s.seen_at DESC
            "#,
            duck.get(),
        )
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|r| {
                let coordinates = r
                    .latitude
                    .zip(r.longitude)
                    .map(|(lat, lon)| {
                        Coordinates::new(lat, lon)
                            .map_err(|e| RepoError::corrupt("sighting", format!("coordinates: {e}")))
                    })
                    .transpose()?;
                Ok(SightingView {
                    id: SightingId::new(r.id),
                    seen_at: r.seen_at.to_jiff(),
                    vessel_name: r.vessel_name,
                    by_display_name: r.by_display_name,
                    note: r.note,
                    has_photo: r.has_photo,
                    coordinates,
                    created_at: r.created_at.to_jiff(),
                })
            })
            .collect()
    }

    /// `/me`: everything this user has found.
    pub async fn for_user(&self, user: UserId) -> Result<Vec<MySighting>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT d.code AS duck_code, d.name AS duck_name, v.name AS vessel_name,
                   s.seen_at AS "seen_at: jiff_sqlx::Timestamp"
            FROM sighting s
            JOIN duck d ON d.id = s.duck_id
            JOIN vessel v ON v.id = s.vessel_id
            WHERE s.user_id = $1
            ORDER BY s.seen_at DESC
            "#,
            user.get(),
        )
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(MySighting {
                    duck_code: DuckCode::parse(&r.duck_code)
                        .map_err(|e| RepoError::corrupt("duck", format!("code: {e}")))?,
                    duck_name: r.duck_name,
                    vessel_name: r.vessel_name,
                    seen_at: r.seen_at.to_jiff(),
                })
            })
            .collect()
    }
}

pub struct CommentRepo(pub PgPool);

impl CommentRepo {
    pub async fn add(
        &self,
        duck: DuckId,
        user: UserId,
        body: &CommentBody,
    ) -> Result<CommentId, RepoError> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO duck_comment (duck_id, user_id, body) VALUES ($1, $2, $3) RETURNING id"#,
            duck.get(),
            user.get(),
            body.as_str(),
        )
        .fetch_one(&self.0)
        .await?;
        Ok(CommentId::new(id))
    }

    pub async fn for_duck(&self, duck: DuckId) -> Result<Vec<CommentView>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT c.body, c.created_at AS "created_at: jiff_sqlx::Timestamp",
                   u.display_name AS by_display_name
            FROM duck_comment c JOIN app_user u ON u.id = c.user_id
            WHERE c.duck_id = $1 ORDER BY c.created_at
            "#,
            duck.get(),
        )
        .fetch_all(&self.0)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| CommentView {
                by_display_name: r.by_display_name,
                body: r.body,
                created_at: r.created_at.to_jiff(),
            })
            .collect())
    }
}

pub struct FollowRepo(pub PgPool);

impl FollowRepo {
    pub async fn follow(&self, user: UserId, duck: DuckId) -> Result<(), RepoError> {
        sqlx::query!(
            r#"INSERT INTO duck_follow (user_id, duck_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
            user.get(),
            duck.get(),
        )
        .execute(&self.0)
        .await?;
        Ok(())
    }

    pub async fn unfollow(&self, user: UserId, duck: DuckId) -> Result<(), RepoError> {
        sqlx::query!(
            r#"DELETE FROM duck_follow WHERE user_id = $1 AND duck_id = $2"#,
            user.get(),
            duck.get(),
        )
        .execute(&self.0)
        .await?;
        Ok(())
    }

    pub async fn is_following(&self, user: UserId, duck: DuckId) -> Result<bool, RepoError> {
        Ok(sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM duck_follow WHERE user_id = $1 AND duck_id = $2
            ) AS "exists!"
            "#,
            user.get(),
            duck.get(),
        )
        .fetch_one(&self.0)
        .await?)
    }

    /// `/me`: ducks this user follows.
    pub async fn followed_ducks(&self, user: UserId) -> Result<Vec<FollowedDuck>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT d.code AS duck_code, d.name AS duck_name,
                   f.created_at AS "followed_at: jiff_sqlx::Timestamp"
            FROM duck_follow f JOIN duck d ON d.id = f.duck_id
            WHERE f.user_id = $1 ORDER BY f.created_at DESC
            "#,
            user.get(),
        )
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(FollowedDuck {
                    duck_code: DuckCode::parse(&r.duck_code)
                        .map_err(|e| RepoError::corrupt("duck", format!("code: {e}")))?,
                    duck_name: r.duck_name,
                    followed_at: r.followed_at.to_jiff(),
                })
            })
            .collect()
    }
}

pub struct NotificationRepo(pub PgPool);

impl NotificationRepo {
    /// The activity feed: sightings of ducks this user follows, newest first.
    pub async fn feed(&self, user: UserId, limit: i64) -> Result<Vec<NotificationView>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT d.code AS duck_code, d.name AS duck_name, v.name AS vessel_name,
                   s.seen_at AS "seen_at: jiff_sqlx::Timestamp",
                   n.created_at AS "created_at: jiff_sqlx::Timestamp",
                   n.read_at IS NULL AS "unread!"
            FROM notification n
            JOIN duck d ON d.id = n.duck_id
            JOIN sighting s ON s.id = n.sighting_id
            JOIN vessel v ON v.id = s.vessel_id
            WHERE n.user_id = $1
            ORDER BY n.created_at DESC
            LIMIT $2
            "#,
            user.get(),
            limit,
        )
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(NotificationView {
                    duck_code: DuckCode::parse(&r.duck_code)
                        .map_err(|e| RepoError::corrupt("duck", format!("code: {e}")))?,
                    duck_name: r.duck_name,
                    vessel_name: r.vessel_name,
                    seen_at: r.seen_at.to_jiff(),
                    created_at: r.created_at.to_jiff(),
                    unread: r.unread,
                })
            })
            .collect()
    }

    pub async fn unread_count(&self, user: UserId) -> Result<i64, RepoError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM notification WHERE user_id = $1 AND read_at IS NULL"#,
            user.get(),
        )
        .fetch_one(&self.0)
        .await?)
    }

    pub async fn mark_all_read(&self, user: UserId) -> Result<(), RepoError> {
        sqlx::query!(
            r#"UPDATE notification SET read_at = now() WHERE user_id = $1 AND read_at IS NULL"#,
            user.get(),
        )
        .execute(&self.0)
        .await?;
        Ok(())
    }
}

/// An in-flight OIDC login flow parked between the redirect to the provider
/// and its callback. Keyed by the unguessable `state` token so retrieval
/// works without cookies (Apple's form_post callback carries none).
pub struct StoredLoginFlow {
    pub state: String,
    pub provider: String,
    pub pkce_verifier: String,
    pub nonce: String,
    pub return_to: Option<String>,
}

pub struct OidcFlowRepo(pub PgPool);

impl OidcFlowRepo {
    /// Park a flow, opportunistically sweeping expired ones.
    pub async fn put(&self, flow: &StoredLoginFlow) -> Result<(), RepoError> {
        sqlx::query!(r#"DELETE FROM oidc_flow WHERE created_at < now() - interval '15 minutes'"#)
            .execute(&self.0)
            .await?;
        sqlx::query!(
            r#"
            INSERT INTO oidc_flow (state, provider, pkce_verifier, nonce, return_to)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            flow.state,
            flow.provider,
            flow.pkce_verifier,
            flow.nonce,
            flow.return_to.as_deref(),
        )
        .execute(&self.0)
        .await?;
        Ok(())
    }

    /// Retrieve-and-delete: each flow completes at most once, and stale
    /// flows are as good as absent.
    pub async fn take(&self, state: &str) -> Result<Option<StoredLoginFlow>, RepoError> {
        let row = sqlx::query!(
            r#"
            DELETE FROM oidc_flow
            WHERE state = $1 AND created_at > now() - interval '15 minutes'
            RETURNING state, provider, pkce_verifier, nonce, return_to
            "#,
            state,
        )
        .fetch_optional(&self.0)
        .await?;
        Ok(row.map(|r| StoredLoginFlow {
            state: r.state,
            provider: r.provider,
            pkce_verifier: r.pkce_verifier,
            nonce: r.nonce,
            return_to: r.return_to,
        }))
    }
}

pub struct VesselRepo(pub PgPool);

impl VesselRepo {
    /// The sighting form's vessel picker, with each vessel's *current*
    /// operator so the form can group ships by cruise line.
    pub async fn options(&self) -> Result<Vec<VesselOption>, RepoError> {
        let rows = sqlx::query!(
            r#"
            SELECT v.id, v.name, l.name AS "line_name?"
            FROM vessel v
            LEFT JOIN vessel_operator vo ON vo.vessel_id = v.id AND vo.valid_to IS NULL
            LEFT JOIN cruise_line l ON l.id = vo.cruise_line_id
            ORDER BY l.name NULLS LAST, v.name
            "#
        )
        .fetch_all(&self.0)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| VesselOption { id: VesselId::new(r.id), name: r.name, line: r.line_name })
            .collect())
    }

    pub async fn by_id(&self, id: VesselId) -> Result<Option<domain::Vessel>, RepoError> {
        let row = sqlx::query_as!(
            VesselRow,
            r#"
            SELECT id, imo_number, name, created_at AS "created_at: jiff_sqlx::Timestamp"
            FROM vessel WHERE id = $1
            "#,
            id.get(),
        )
        .fetch_optional(&self.0)
        .await?;
        row.map(TryInto::try_into).transpose()
    }
}

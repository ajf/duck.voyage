-- Initial schema, per duck-voyage.md §5. Ordering differs from the doc:
-- app_user comes first because flock references it.

-- People who log in via OIDC. Identity = (iss, sub).
-- Admin bootstrap: config lists admin (issuer, subject) pairs; the OIDC
-- callback sets is_admin on upsert when the identity matches.
CREATE TABLE app_user (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    issuer       TEXT        NOT NULL,
    subject      TEXT        NOT NULL,
    display_name TEXT,
    email        TEXT,
    is_admin     BOOLEAN     NOT NULL DEFAULT false,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (issuer, subject)
);

-- Cruise lines (operators), normalized so each has one canonical spelling.
CREATE TABLE cruise_line (
    id   BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name TEXT   NOT NULL UNIQUE
);

-- Vessels are a curated set, seeded/refreshed from a vessel database API.
-- The IMO number is the rename-stable identity anchor: optional, UNIQUE when
-- present so one hull = one row.
CREATE TABLE vessel (
    id         BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    imo_number CHAR(7),                        -- nullable; validated in-app
    name       TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX vessel_imo_idx ON vessel (imo_number) WHERE imo_number IS NOT NULL;

-- Operator history: who ran the vessel, when. valid_to NULL = current.
-- A sighting's operator-at-the-time is resolved by joining on seen_at.
CREATE TABLE vessel_operator (
    id             BIGINT NOT NULL GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    vessel_id      BIGINT NOT NULL REFERENCES vessel (id),
    cruise_line_id BIGINT NOT NULL REFERENCES cruise_line (id),
    valid_from     DATE   NOT NULL,
    valid_to       DATE,
    CHECK (valid_to IS NULL OR valid_to > valid_from)
);
CREATE UNIQUE INDEX vessel_operator_current_idx
    ON vessel_operator (vessel_id) WHERE valid_to IS NULL;
CREATE INDEX vessel_operator_vessel_idx ON vessel_operator (vessel_id);

-- A flock of codes grabbed by one user, who originates and tracks those ducks.
-- flock_code is the visible 3-char prefix on every duck code in the flock,
-- drawn randomly from the unused pool (UNIQUE enforces one owner per prefix).
CREATE TABLE flock (
    id             BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    flock_code     CHAR(3)     NOT NULL UNIQUE, -- base36; random draw, never reused
    key_generation SMALLINT    NOT NULL,        -- FF1 key for every code in this flock
    owner_user_id  BIGINT      NOT NULL REFERENCES app_user (id),
    label          TEXT,                        -- user's name for it ("Alaska trip 2026")
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX flock_owner_idx ON flock (owner_user_id);

-- The duck registry: validity = the row exists. Sequential id is private;
-- code is what's printed. Origination = photo + description attached by the
-- flock owner; until then the duck 404s for everyone else.
CREATE TABLE duck (
    id                        BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code                      TEXT        NOT NULL UNIQUE, -- prefix + FF1(seq) + check char
    flock_id                  BIGINT      NOT NULL REFERENCES flock (id),
    flock_seq                 SMALLINT    NOT NULL,        -- 1..10000 within the flock
    name                      TEXT,                        -- optional, given at origination
    description               TEXT,                        -- required at origination
    originated_at             TIMESTAMPTZ,                 -- NULL = allocated, not yet live
    origin_photo_key          TEXT,
    origin_photo_content_type TEXT,
    created_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (flock_id, flock_seq),                          -- lookup path for scans
    CHECK (flock_seq BETWEEN 1 AND 10000),
    CHECK ((originated_at IS NULL) = (origin_photo_key IS NULL)),
    CHECK ((originated_at IS NULL) = (description IS NULL)),
    CHECK ((origin_photo_key IS NULL) = (origin_photo_content_type IS NULL))
);

-- A single logged find. Always attributed: writes require login.
-- At most one photo, folded into the row (both photo columns set or neither).
CREATE TABLE sighting (
    id                 BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    duck_id            BIGINT      NOT NULL REFERENCES duck (id),
    vessel_id          BIGINT      NOT NULL REFERENCES vessel (id),
    user_id            BIGINT      NOT NULL REFERENCES app_user (id),
    seen_at            TIMESTAMPTZ NOT NULL,
    note               TEXT,
    photo_key          TEXT,
    photo_content_type TEXT,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((photo_key IS NULL) = (photo_content_type IS NULL))
);
CREATE INDEX sighting_duck_idx ON sighting (duck_id);
CREATE INDEX sighting_user_idx ON sighting (user_id);

-- Comments on a duck's page.
CREATE TABLE duck_comment (
    id         BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    duck_id    BIGINT      NOT NULL REFERENCES duck (id),
    user_id    BIGINT      NOT NULL REFERENCES app_user (id),
    body       TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX duck_comment_duck_idx ON duck_comment (duck_id);

-- A user following a duck: notified when it's found again. Auto-created when
-- you log a sighting; also toggleable from the duck page.
CREATE TABLE duck_follow (
    user_id    BIGINT      NOT NULL REFERENCES app_user (id),
    duck_id    BIGINT      NOT NULL REFERENCES duck (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, duck_id)
);
CREATE INDEX duck_follow_duck_idx ON duck_follow (duck_id);

-- In-app activity feed rows: "a duck you follow was sighted". Backing store
-- for follower notifications; delivery channels (email etc.) come later.
CREATE TABLE notification (
    id          BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id     BIGINT      NOT NULL REFERENCES app_user (id),
    duck_id     BIGINT      NOT NULL REFERENCES duck (id),
    sighting_id BIGINT      NOT NULL REFERENCES sighting (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    read_at     TIMESTAMPTZ
);
CREATE INDEX notification_user_idx ON notification (user_id, created_at DESC);

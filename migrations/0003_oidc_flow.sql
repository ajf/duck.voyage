-- In-flight OIDC login flows, keyed by the unguessable `state` token.
-- Lives in the database (not the session) because Apple's form_post
-- callback is a cross-site POST that browsers strip session cookies from.
-- Rows are single-use (deleted on retrieval) and expire after 15 minutes.
CREATE TABLE oidc_flow (
    state         TEXT        PRIMARY KEY,
    provider      TEXT        NOT NULL,
    pkce_verifier TEXT        NOT NULL,
    nonce         TEXT        NOT NULL,
    return_to     TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

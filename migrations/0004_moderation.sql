-- Moderation controls (flock owner or admin).
-- Duck deletion is soft: the row must survive so the flock's sequence
-- high-water mark stands and a printed code can never be re-minted as a
-- different duck. Comment locking is a timestamp for audit, not a boolean.
ALTER TABLE duck
    ADD COLUMN deleted_at         TIMESTAMPTZ,
    ADD COLUMN comments_locked_at TIMESTAMPTZ;

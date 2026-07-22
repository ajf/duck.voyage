-- Duck staging: details can be attached at home (originated_at + photo +
-- description) without the duck going live; set_sail_at is the live moment,
-- typically triggered by the owner scanning the printed sticker in place.
ALTER TABLE duck ADD COLUMN set_sail_at TIMESTAMPTZ;

-- Ducks originated before this migration were immediately live.
UPDATE duck SET set_sail_at = originated_at WHERE originated_at IS NOT NULL;

-- Sailing requires details; staged-or-later requires details (existing
-- CHECKs already tie originated_at to description/photo).
ALTER TABLE duck ADD CONSTRAINT duck_sail_requires_details
    CHECK (set_sail_at IS NULL OR originated_at IS NOT NULL);

-- Browser-captured GPS on sightings (map phase groundwork; captured now so
-- the data accrues). Both columns set or neither.
ALTER TABLE sighting
    ADD COLUMN latitude  DOUBLE PRECISION,
    ADD COLUMN longitude DOUBLE PRECISION;
ALTER TABLE sighting ADD CONSTRAINT sighting_coords_pair
    CHECK ((latitude IS NULL) = (longitude IS NULL));
ALTER TABLE sighting ADD CONSTRAINT sighting_coords_range
    CHECK (latitude IS NULL
           OR (latitude BETWEEN -90 AND 90 AND longitude BETWEEN -180 AND 180));

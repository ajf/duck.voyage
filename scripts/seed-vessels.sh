#!/usr/bin/env bash
# Phase 1 vessel seed (duck-voyage.md §12: "initial seed can be a manual
# import in Phase 1"). Idempotent. IMO check digits are valid; Phase 2's
# provider sync will correct any that drifted from reality.
set -euo pipefail

podman exec -i duck-pg psql -U duck -d duck <<'SQL'
INSERT INTO cruise_line (name) VALUES
    ('Royal Caribbean International'),
    ('Carnival Cruise Line'),
    ('Norwegian Cruise Line'),
    ('Disney Cruise Line'),
    ('Cunard Line')
ON CONFLICT (name) DO NOTHING;

INSERT INTO vessel (imo_number, name) VALUES
    ('9383936', 'Oasis of the Seas'),
    ('9383948', 'Allure of the Seas'),
    ('9682875', 'Harmony of the Seas'),
    ('9744001', 'Symphony of the Seas'),
    ('9456032', 'Icon of the Seas'),
    ('9712084', 'Carnival Vista'),
    ('9918468', 'Disney Wish'),
    ('9819117', 'Norwegian Encore'),
    ('9241061', 'Queen Mary 2')
ON CONFLICT DO NOTHING;

INSERT INTO vessel_operator (vessel_id, cruise_line_id, valid_from)
SELECT v.id, l.id, d.since
FROM (VALUES
    ('9383936', 'Royal Caribbean International', DATE '2009-12-01'),
    ('9383948', 'Royal Caribbean International', DATE '2010-12-01'),
    ('9682875', 'Royal Caribbean International', DATE '2016-05-01'),
    ('9744001', 'Royal Caribbean International', DATE '2018-03-01'),
    ('9456032', 'Royal Caribbean International', DATE '2024-01-01'),
    ('9712084', 'Carnival Cruise Line',          DATE '2016-05-01'),
    ('9918468', 'Disney Cruise Line',            DATE '2022-07-01'),
    ('9819117', 'Norwegian Cruise Line',         DATE '2019-11-01'),
    ('9241061', 'Cunard Line',                   DATE '2004-01-01')
) AS d(imo, line, since)
JOIN vessel v ON v.imo_number = d.imo
JOIN cruise_line l ON l.name = d.line
WHERE NOT EXISTS (
    SELECT 1 FROM vessel_operator vo
    WHERE vo.vessel_id = v.id AND vo.valid_to IS NULL
);

SELECT count(*) AS vessels FROM vessel;
SQL

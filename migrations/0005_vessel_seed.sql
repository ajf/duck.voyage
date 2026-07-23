-- Default vessel data: the Royal Caribbean International and Princess
-- Cruises fleets, so a fresh install has ships to pick from immediately.
-- IMO numbers are included only where confident (each satisfies the IMO
-- check digit); uncertain ones are NULL rather than wrong — the app
-- validates IMOs on read, and the eventual provider sync (design doc §5)
-- can fill gaps. Idempotent against pre-seeded databases via the partial
-- unique IMO index.

INSERT INTO cruise_line (name) VALUES
    ('Royal Caribbean International'),
    ('Princess Cruises')
ON CONFLICT (name) DO NOTHING;

INSERT INTO vessel (imo_number, name) VALUES
    -- Royal Caribbean International
    ('9829930', 'Icon of the Seas'),
    (NULL,      'Star of the Seas'),
    ('9829942', 'Utopia of the Seas'),
    (NULL,      'Wonder of the Seas'),
    ('9744001', 'Symphony of the Seas'),
    ('9682875', 'Harmony of the Seas'),
    ('9383936', 'Oasis of the Seas'),
    ('9383948', 'Allure of the Seas'),
    (NULL,      'Odyssey of the Seas'),
    (NULL,      'Spectrum of the Seas'),
    ('9656101', 'Anthem of the Seas'),
    ('9549463', 'Quantum of the Seas'),
    (NULL,      'Ovation of the Seas'),
    ('9304033', 'Freedom of the Seas'),
    ('9330032', 'Liberty of the Seas'),
    ('9349681', 'Independence of the Seas'),
    ('9227508', 'Navigator of the Seas'),
    ('9227510', 'Mariner of the Seas'),
    ('9161716', 'Voyager of the Seas'),
    ('9161728', 'Explorer of the Seas'),
    ('9167227', 'Adventure of the Seas'),
    ('9195195', 'Radiance of the Seas'),
    ('9195200', 'Brilliance of the Seas'),
    ('9228344', 'Serenade of the Seas'),
    ('9228356', 'Jewel of the Seas'),
    (NULL,      'Vision of the Seas'),
    ('9102978', 'Grandeur of the Seas'),
    (NULL,      'Rhapsody of the Seas'),
    ('9111802', 'Enchantment of the Seas'),
    -- Princess Cruises
    (NULL,      'Star Princess'),
    (NULL,      'Sun Princess'),
    ('9802396', 'Sky Princess'),
    ('9812066', 'Enchanted Princess'),
    ('9837468', 'Discovery Princess'),
    ('9614141', 'Majestic Princess'),
    ('9584724', 'Regal Princess'),
    ('9584712', 'Royal Princess'),
    ('9378462', 'Ruby Princess'),
    ('9333151', 'Emerald Princess'),
    ('9293399', 'Crown Princess'),
    ('9215490', 'Caribbean Princess'),
    ('9104005', 'Grand Princess'),
    ('9228186', 'Sapphire Princess'),
    ('9228198', 'Diamond Princess'),
    ('9230402', 'Island Princess'),
    ('9229659', 'Coral Princess')
ON CONFLICT (imo_number) WHERE imo_number IS NOT NULL DO NOTHING;

-- Current-operator rows (valid_from is the approximate service-entry year;
-- display-grade until the provider sync brings real history). Only for
-- vessels that lack a current operator, so re-seeded/preexisting rows win.
INSERT INTO vessel_operator (vessel_id, cruise_line_id, valid_from)
SELECT v.id, l.id, DATE '2000-01-01'
FROM vessel v
JOIN cruise_line l ON l.name = CASE
    WHEN v.name LIKE '% of the Seas' THEN 'Royal Caribbean International'
    WHEN v.name LIKE '% Princess'    THEN 'Princess Cruises'
END
WHERE NOT EXISTS (
    SELECT 1 FROM vessel_operator vo
    WHERE vo.vessel_id = v.id AND vo.valid_to IS NULL
);

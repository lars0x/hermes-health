-- Migration 010: move reference & optimal ranges into their own tables.
--
-- Previously ranges were four columns on `biomarkers` (a single unisex set).
-- They now live in two parallel tables so that we can:
--   * store multiple candidate ranges per biomarker (different sources) and
--     mark one as the active default (is_default),
--   * vary ranges by sex and age band,
--   * record the provenance (source) of each range independently for the
--     reference range vs the optimal range (they often come from different
--     authorities).
--
-- The two tables are structurally identical; reference_ranges holds the clinical
-- "normal" interval, optimal_ranges holds the tighter health-optimization target.
--
-- Age uses integer sentinels (0..200), never NULL: SQLite treats NULLs as
-- distinct in UNIQUE/partial indexes, which would silently permit duplicate
-- defaults. Every (biomarker, sex) has one open-age (0..200) default row as the
-- always-present fallback; age-banded markers add narrower bands on top.
--
-- Resolution (in code, src/db/queries.rs): pick the default row matching the
-- profile sex (falling back to 'any') and age (narrowest matching band; widest
-- when age is unknown). The legacy biomarkers range columns are dropped in the
-- migration runner (guarded ALTER TABLE ... DROP COLUMN).

CREATE TABLE IF NOT EXISTS reference_ranges (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    biomarker_id  INTEGER NOT NULL REFERENCES biomarkers(id) ON DELETE CASCADE,
    sex           TEXT NOT NULL DEFAULT 'any' CHECK (sex IN ('any','male','female')),
    age_min       INTEGER NOT NULL DEFAULT 0,    -- inclusive, years
    age_max       INTEGER NOT NULL DEFAULT 200,  -- inclusive, years
    low           REAL,                          -- NULL = open-ended below
    high          REAL,                          -- NULL = open-ended above
    source        TEXT NOT NULL,                 -- e.g. 'Mayo Clinic Labs 2024'
    source_url    TEXT,
    notes         TEXT,
    is_default    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    UNIQUE(biomarker_id, sex, age_min, age_max, source)
);

CREATE TABLE IF NOT EXISTS optimal_ranges (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    biomarker_id  INTEGER NOT NULL REFERENCES biomarkers(id) ON DELETE CASCADE,
    sex           TEXT NOT NULL DEFAULT 'any' CHECK (sex IN ('any','male','female')),
    age_min       INTEGER NOT NULL DEFAULT 0,
    age_max       INTEGER NOT NULL DEFAULT 200,
    low           REAL,
    high          REAL,
    source        TEXT NOT NULL,
    source_url    TEXT,
    notes         TEXT,
    is_default    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    UNIQUE(biomarker_id, sex, age_min, age_max, source)
);

-- At most one default range per (biomarker, sex, age band).
CREATE UNIQUE INDEX IF NOT EXISTS idx_ref_ranges_default
    ON reference_ranges(biomarker_id, sex, age_min, age_max) WHERE is_default = 1;
CREATE UNIQUE INDEX IF NOT EXISTS idx_opt_ranges_default
    ON optimal_ranges(biomarker_id, sex, age_min, age_max) WHERE is_default = 1;

CREATE INDEX IF NOT EXISTS idx_ref_ranges_biomarker ON reference_ranges(biomarker_id);
CREATE INDEX IF NOT EXISTS idx_opt_ranges_biomarker ON optimal_ranges(biomarker_id);

-- Simple key/value application settings. First use: the user's profile sex and
-- date of birth, which drive sex/age range resolution.
CREATE TABLE IF NOT EXISTS app_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

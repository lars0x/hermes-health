-- Hermes Health - Initial Schema

CREATE TABLE IF NOT EXISTS biomarkers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    loinc_code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    aliases TEXT NOT NULL DEFAULT '[]',  -- JSON array of alternate names
    unit TEXT NOT NULL,                   -- UCUM canonical unit
    category TEXT NOT NULL,               -- e.g. Lipid Panel, Metabolic, etc.
    reference_low REAL,
    reference_high REAL,
    optimal_low REAL,
    optimal_high REAL,
    source TEXT NOT NULL DEFAULT 'measured'  -- 'measured' or 'calculated'
);

CREATE TABLE IF NOT EXISTS observations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    biomarker_id INTEGER NOT NULL REFERENCES biomarkers(id),
    value REAL NOT NULL,                      -- normalized to canonical unit
    original_value TEXT NOT NULL,              -- verbatim from lab report
    original_unit TEXT NOT NULL,               -- unit as reported
    precision INTEGER NOT NULL DEFAULT 0,     -- decimal places in original
    observed_at TEXT NOT NULL,                 -- ISO date (YYYY-MM-DD)
    lab_name TEXT,
    report_id INTEGER REFERENCES reports(id),
    fasting INTEGER,                          -- 0/1/NULL (SQLite boolean)
    notes TEXT,
    detection_limit TEXT,                      -- '<', '>', or NULL
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS reports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    filename TEXT NOT NULL,
    file_hash TEXT NOT NULL UNIQUE,
    file_path TEXT NOT NULL,
    format TEXT NOT NULL,                      -- 'pdf' or 'csv'
    imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    extraction_status TEXT NOT NULL DEFAULT 'pending',  -- pending/extracted/reviewed/failed
    raw_extraction TEXT                        -- JSON blob of extraction output
);

CREATE TABLE IF NOT EXISTS interventions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    category TEXT NOT NULL,                    -- supplement/medication/diet/exercise/fasting/other
    dosage TEXT,
    frequency TEXT,
    started_at TEXT NOT NULL,                  -- ISO date
    ended_at TEXT,                             -- ISO date, NULL = ongoing
    notes TEXT
);

CREATE TABLE IF NOT EXISTS intervention_biomarker_targets (
    intervention_id INTEGER NOT NULL REFERENCES interventions(id),
    biomarker_id INTEGER NOT NULL REFERENCES biomarkers(id),
    expected_effect TEXT NOT NULL,             -- increase/decrease/stabilize
    PRIMARY KEY (intervention_id, biomarker_id)
);

CREATE TABLE IF NOT EXISTS unit_conversions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    biomarker_id INTEGER NOT NULL REFERENCES biomarkers(id),
    from_unit TEXT NOT NULL,
    to_unit TEXT NOT NULL,                     -- always the biomarker's canonical unit
    factor REAL NOT NULL,
    offset REAL NOT NULL DEFAULT 0.0,
    UNIQUE(biomarker_id, from_unit)
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_observations_biomarker ON observations(biomarker_id);
CREATE INDEX IF NOT EXISTS idx_observations_date ON observations(observed_at);
CREATE INDEX IF NOT EXISTS idx_observations_report ON observations(report_id);
CREATE INDEX IF NOT EXISTS idx_biomarkers_loinc ON biomarkers(loinc_code);
CREATE INDEX IF NOT EXISTS idx_biomarkers_category ON biomarkers(category);
CREATE INDEX IF NOT EXISTS idx_interventions_dates ON interventions(started_at, ended_at);
CREATE INDEX IF NOT EXISTS idx_unit_conversions_lookup ON unit_conversions(biomarker_id, from_unit);

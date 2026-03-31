-- Split reports into reports (files) + imports (extraction runs)
-- Observations now link to imports, not reports directly

CREATE TABLE IF NOT EXISTS imports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    report_id INTEGER NOT NULL REFERENCES reports(id),
    model_used TEXT,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending/extracting/extracted/committed/failed
    raw_extraction TEXT,                      -- JSON blob of ExtractionResult
    agent_turns INTEGER DEFAULT 0,
    extracted_count INTEGER DEFAULT 0,
    unresolved_count INTEGER DEFAULT 0,
    test_date TEXT,                           -- extracted date of specimen collection
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_imports_report ON imports(report_id);
CREATE INDEX IF NOT EXISTS idx_imports_status ON imports(status);

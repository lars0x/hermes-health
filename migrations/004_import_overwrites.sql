CREATE TABLE IF NOT EXISTS import_overwrites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    import_id INTEGER NOT NULL REFERENCES imports(id),
    loinc_code TEXT NOT NULL,
    chosen_idx INTEGER NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(import_id, loinc_code)
);

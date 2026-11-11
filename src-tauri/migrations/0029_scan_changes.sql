-- Latest physical-index delta. Deliberately separate from source metadata and scan baselines.
CREATE TABLE library_scan_changes (
    root_id INTEGER PRIMARY KEY REFERENCES library_roots(id) ON DELETE CASCADE,
    summary_json TEXT NOT NULL
);

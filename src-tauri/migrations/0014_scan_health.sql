CREATE TABLE library_scan_health (
    library_root_id INTEGER PRIMARY KEY REFERENCES library_roots(id) ON DELETE CASCADE,
    last_auto_attempt_at TEXT,
    last_success_at TEXT,
    outcome TEXT NOT NULL,
    error_count INTEGER NOT NULL DEFAULT 0,
    detail TEXT
);

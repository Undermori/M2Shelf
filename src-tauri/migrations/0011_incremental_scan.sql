CREATE TABLE IF NOT EXISTS library_scan_snapshots (
    library_root_id INTEGER PRIMARY KEY REFERENCES library_roots(id) ON DELETE CASCADE,
    snapshot_json TEXT NOT NULL
);

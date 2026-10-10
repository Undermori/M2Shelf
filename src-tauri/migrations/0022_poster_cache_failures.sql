-- Application-owned diagnostics; no media, binding or reader tables are rebuilt.
CREATE TABLE IF NOT EXISTS poster_cache_failures (
    node_id INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    detail TEXT NOT NULL
);

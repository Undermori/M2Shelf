-- User confirmations are distinct from playback history and collection status.
-- Root deletion removes only its records. Index replacement retains unresolved confirmations.
CREATE TABLE episode_watch_states (
    library_root_id INTEGER NOT NULL REFERENCES library_roots(id) ON DELETE CASCADE,
    identity_key TEXT NOT NULL CHECK(length(identity_key)=64),
    scope_key TEXT NOT NULL CHECK(length(scope_key)=64),
    identity_kind TEXT NOT NULL CHECK(identity_kind IN ('EPISODE','FILE')),
    source_node_id INTEGER REFERENCES nodes(id) ON DELETE SET NULL,
    source_file_id INTEGER REFERENCES media_files(id) ON DELETE SET NULL,
    source_name TEXT NOT NULL,
    season INTEGER,
    episode INTEGER,
    watched INTEGER NOT NULL CHECK(watched IN (0,1)),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY(library_root_id,identity_key),
    CHECK((identity_kind='EPISODE' AND episode IS NOT NULL AND episode>=0)
       OR (identity_kind='FILE' AND season IS NULL AND episode IS NULL))
);
CREATE INDEX episode_watch_scope ON episode_watch_states(library_root_id,scope_key);
CREATE INDEX episode_watch_source ON episode_watch_states(library_root_id,source_node_id);

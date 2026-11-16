-- Project sources remain read-only; only registrations and managed playback evidence live here.
CREATE TABLE workspace_folders (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 path TEXT NOT NULL UNIQUE COLLATE NOCASE,
 alias TEXT NOT NULL CHECK(length(alias)<=240),
 note TEXT NOT NULL DEFAULT '' CHECK(length(note)<=10000),
 node_id INTEGER REFERENCES nodes(id) ON DELETE SET NULL,
 book_id INTEGER REFERENCES comic_books(id) ON DELETE SET NULL,
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 CHECK(node_id IS NULL OR book_id IS NULL)
);
CREATE TABLE mpv_playback_sessions (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 session_token TEXT NOT NULL,
 node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
 media_file_id INTEGER NOT NULL REFERENCES media_files(id) ON DELETE CASCADE,
 started_at INTEGER NOT NULL,
 ended_at INTEGER,
 heartbeat_at INTEGER NOT NULL,
 screenshot_directory TEXT,
 video_title TEXT NOT NULL DEFAULT '',
 CHECK(started_at>=0 AND heartbeat_at>=started_at),
 CHECK(ended_at IS NULL OR ended_at>=started_at)
);
CREATE INDEX mpv_playback_sessions_time ON mpv_playback_sessions(started_at,heartbeat_at,ended_at);
CREATE INDEX mpv_playback_sessions_token ON mpv_playback_sessions(session_token,ended_at);

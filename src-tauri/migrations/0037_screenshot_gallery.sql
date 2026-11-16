-- Screenshot sources remain read-only. Only registration and curation live here.
CREATE TABLE screenshot_folders (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 path TEXT NOT NULL UNIQUE COLLATE NOCASE,
 alias TEXT NOT NULL CHECK(length(alias)<=240),
 note TEXT NOT NULL DEFAULT '' CHECK(length(note)<=10000),
 node_id INTEGER REFERENCES nodes(id) ON DELETE SET NULL,
 book_id INTEGER REFERENCES comic_books(id) ON DELETE SET NULL,
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 CHECK(node_id IS NULL OR book_id IS NULL)
);
CREATE TABLE screenshot_images (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 folder_id INTEGER NOT NULL REFERENCES screenshot_folders(id) ON DELETE CASCADE,
 relative_path TEXT NOT NULL COLLATE NOCASE,
 file_name TEXT NOT NULL,
 file_size INTEGER NOT NULL CHECK(file_size>0 AND file_size<=67108864),
 modified_at TEXT NOT NULL,
 revision TEXT NOT NULL,
 mime TEXT NOT NULL,
 width INTEGER NOT NULL CHECK(width>0 AND width<=32768),
 height INTEGER NOT NULL CHECK(height>0 AND height<=32768),
 note TEXT NOT NULL DEFAULT '' CHECK(length(note)<=10000),
 tags TEXT NOT NULL DEFAULT '[]' CHECK(length(tags)<=4096),
 starred INTEGER NOT NULL DEFAULT 0 CHECK(starred IN (0,1)),
 node_id INTEGER REFERENCES nodes(id) ON DELETE SET NULL,
 book_id INTEGER REFERENCES comic_books(id) ON DELETE SET NULL,
 available INTEGER NOT NULL DEFAULT 1 CHECK(available IN (0,1)),
 CHECK(node_id IS NULL OR book_id IS NULL),
 UNIQUE(folder_id,relative_path)
);
CREATE INDEX screenshot_images_folder ON screenshot_images(folder_id,available,id);
CREATE INDEX screenshot_images_star ON screenshot_images(starred,available,id);
CREATE TABLE private_note_updates (
 note_id INTEGER PRIMARY KEY REFERENCES private_notes(id) ON DELETE CASCADE,
 updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TRIGGER private_note_updates_insert AFTER INSERT ON private_notes
 BEGIN INSERT OR REPLACE INTO private_note_updates(note_id) VALUES(NEW.id); END;
CREATE TRIGGER private_note_updates_save AFTER UPDATE OF body,revision ON private_notes
 BEGIN INSERT OR REPLACE INTO private_note_updates(note_id) VALUES(NEW.id); END;

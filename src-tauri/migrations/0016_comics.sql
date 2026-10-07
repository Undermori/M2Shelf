ALTER TABLE nodes ADD COLUMN direct_comic_book_count INTEGER NOT NULL DEFAULT 0 CHECK(direct_comic_book_count>=0);
ALTER TABLE nodes ADD COLUMN child_comic_branch_count INTEGER NOT NULL DEFAULT 0 CHECK(child_comic_branch_count>=0);
ALTER TABLE nodes ADD COLUMN total_comic_book_count INTEGER NOT NULL DEFAULT 0 CHECK(total_comic_book_count>=0);
CREATE TABLE comic_books (
    revision TEXT NOT NULL DEFAULT '',
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
 source_path TEXT NOT NULL COLLATE NOCASE,
 source_kind TEXT NOT NULL CHECK(source_kind IN ('IMAGE_FOLDER','ZIP_ARCHIVE')),
 display_name TEXT NOT NULL,
 file_size INTEGER NOT NULL DEFAULT 0 CHECK(file_size>=0),
 modified_at TEXT NOT NULL,
 page_count INTEGER NOT NULL DEFAULT 0 CHECK(page_count>=0),
 index_error TEXT,
 last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 UNIQUE(node_id,source_path)
);
CREATE INDEX idx_comic_books_node ON comic_books(node_id);
CREATE TABLE comic_pages (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 comic_book_id INTEGER NOT NULL REFERENCES comic_books(id) ON DELETE CASCADE,
 page_index INTEGER NOT NULL CHECK(page_index>=0),
 page_name TEXT NOT NULL,
 source_locator TEXT NOT NULL,
 file_size INTEGER NOT NULL CHECK(file_size>=0),
 crc32 INTEGER,
 modified_at TEXT NOT NULL,
 UNIQUE(comic_book_id,page_index)
);
CREATE INDEX idx_comic_pages_book ON comic_pages(comic_book_id,page_index);
CREATE TABLE comic_reading_progress (
 comic_book_id INTEGER PRIMARY KEY REFERENCES comic_books(id) ON DELETE CASCADE,
 last_page_index INTEGER NOT NULL CHECK(last_page_index>=0),
 last_read_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_comic_progress_last_read ON comic_reading_progress(last_read_at DESC);
CREATE TABLE comic_bookmarks (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 comic_book_id INTEGER NOT NULL REFERENCES comic_books(id) ON DELETE CASCADE,
 page_index INTEGER NOT NULL CHECK(page_index>=0),
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 UNIQUE(comic_book_id,page_index)
);

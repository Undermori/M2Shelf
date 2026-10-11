-- Application-owned imported evidence. It never renames or writes to sources.
CREATE TABLE indexed_media_metadata (
 source_key TEXT PRIMARY KEY COLLATE NOCASE,
 node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
 book_id INTEGER REFERENCES comic_books(id) ON DELETE CASCADE,
 resource_id INTEGER REFERENCES resource_files(id) ON DELETE CASCADE,
 revision TEXT NOT NULL, payload_json TEXT NOT NULL CHECK(length(payload_json)<=65536)
);
CREATE INDEX indexed_metadata_node ON indexed_media_metadata(node_id);
CREATE INDEX indexed_metadata_book ON indexed_media_metadata(book_id);

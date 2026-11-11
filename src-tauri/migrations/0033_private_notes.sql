CREATE TABLE private_notes (
    id INTEGER PRIMARY KEY,
    node_id INTEGER REFERENCES nodes(id) ON DELETE CASCADE,
    book_id INTEGER REFERENCES comic_books(id) ON DELETE CASCADE,
    body TEXT NOT NULL CHECK(length(body)<=10000 AND length(CAST(body AS BLOB))<=40000),
    revision INTEGER NOT NULL DEFAULT 1,
    CHECK((node_id IS NOT NULL) <> (book_id IS NOT NULL)),
    UNIQUE(node_id), UNIQUE(book_id)
);
CREATE TABLE book_manual_covers (
    book_id INTEGER PRIMARY KEY REFERENCES comic_books(id) ON DELETE CASCADE,
    cover_cache_path TEXT NOT NULL
);

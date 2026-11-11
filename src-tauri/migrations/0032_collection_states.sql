CREATE TABLE collection_states (
    kind TEXT NOT NULL CHECK(kind IN ('NODE','BOOK')),
    node_id INTEGER UNIQUE REFERENCES nodes(id) ON DELETE CASCADE,
    book_id INTEGER UNIQUE REFERENCES comic_books(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK(state IN ('WANT','IN_PROGRESS','COMPLETED','PAUSED','DROPPED')),
    CHECK((kind='NODE' AND node_id IS NOT NULL AND book_id IS NULL)
       OR (kind='BOOK' AND book_id IS NOT NULL AND node_id IS NULL))
);

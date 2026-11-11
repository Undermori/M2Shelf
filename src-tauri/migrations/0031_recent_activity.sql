CREATE TABLE recent_activity (
    entry_seq INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK(kind IN ('NODE','BOOK')),
    node_id INTEGER UNIQUE REFERENCES nodes(id) ON DELETE CASCADE,
    book_id INTEGER UNIQUE REFERENCES comic_books(id) ON DELETE CASCADE,
    opened_at TEXT NOT NULL,
    CHECK((kind='NODE' AND node_id IS NOT NULL AND book_id IS NULL)
       OR (kind='BOOK' AND book_id IS NOT NULL AND node_id IS NULL))
);
INSERT INTO recent_activity(kind,node_id,book_id,opened_at)
SELECT kind,node_id,book_id,stamp FROM (
    SELECT 'NODE' AS kind,node_id,NULL AS book_id,last_watched_at AS stamp FROM watch_history
    UNION ALL
    SELECT 'BOOK',NULL,comic_book_id,last_read_at FROM comic_reading_progress
) ORDER BY julianday(stamp) DESC,kind,COALESCE(node_id,book_id) DESC LIMIT 30;

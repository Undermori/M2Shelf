CREATE TABLE library_all_resources_visibility (
    library_root_id INTEGER PRIMARY KEY REFERENCES library_roots(id) ON DELETE CASCADE,
    hidden INTEGER NOT NULL CHECK (hidden IN (0, 1))
);

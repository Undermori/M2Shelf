CREATE TABLE metadata_bindings_comic (
 id INTEGER PRIMARY KEY AUTOINCREMENT, node_id INTEGER NOT NULL,
 provider TEXT NOT NULL CHECK(provider='BANGUMI'), provider_subject_id INTEGER NOT NULL,
 provider_title TEXT NOT NULL, provider_title_cn TEXT, provider_date TEXT, provider_image_url TEXT,
 bound_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 cover_download_error TEXT, provider_title_en TEXT, provider_title_ja TEXT, provider_title_ko TEXT,
 provider_subject_type INTEGER NOT NULL DEFAULT 2 CHECK(provider_subject_type IN (1,2,6)),
 provider_aliases_json TEXT NOT NULL DEFAULT '[]', UNIQUE(node_id,provider),
 FOREIGN KEY(node_id) REFERENCES nodes(id) ON DELETE CASCADE
);
INSERT INTO metadata_bindings_comic SELECT id,node_id,provider,provider_subject_id,provider_title,
 provider_title_cn,provider_date,provider_image_url,bound_at,updated_at,cover_download_error,
 provider_title_en,provider_title_ja,provider_title_ko,provider_subject_type,provider_aliases_json FROM metadata_bindings;
DROP TABLE metadata_bindings;
ALTER TABLE metadata_bindings_comic RENAME TO metadata_bindings;
CREATE INDEX idx_metadata_bindings_node ON metadata_bindings(node_id);
CREATE TABLE confirmed_title_aliases_comic (
 normalized_alias TEXT NOT NULL CHECK(length(normalized_alias) BETWEEN 1 AND 200),
 original_alias TEXT NOT NULL CHECK(length(original_alias) BETWEEN 1 AND 200),
 subject_id INTEGER NOT NULL, subject_type INTEGER NOT NULL CHECK(subject_type IN (1,2,6)),
 source_node_id INTEGER NOT NULL, confirmed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 PRIMARY KEY(source_node_id,normalized_alias), FOREIGN KEY(source_node_id) REFERENCES nodes(id) ON DELETE CASCADE
);
INSERT INTO confirmed_title_aliases_comic SELECT * FROM confirmed_title_aliases;
DROP TABLE confirmed_title_aliases;
ALTER TABLE confirmed_title_aliases_comic RENAME TO confirmed_title_aliases;
CREATE INDEX idx_confirmed_title_aliases_normalized ON confirmed_title_aliases(normalized_alias);

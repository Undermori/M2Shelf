ALTER TABLE library_roots ADD COLUMN book_organization_strategy TEXT NOT NULL DEFAULT 'LEGACY'
 CHECK(book_organization_strategy IN ('LEGACY','SMART_MIXED'));
CREATE TRIGGER book_strategy_insert BEFORE INSERT ON library_roots
 WHEN NEW.book_organization_strategy='SMART_MIXED' AND (NEW.media_kind<>'COMIC' OR NEW.recognition_mode<>'FOLDER')
 BEGIN SELECT RAISE(ABORT,'SMART_MIXED_REQUIRES_BOOK_FOLDER_INDEX'); END;
CREATE TRIGGER book_strategy_immutable BEFORE UPDATE OF book_organization_strategy ON library_roots
 WHEN NEW.book_organization_strategy<>OLD.book_organization_strategy
 BEGIN SELECT RAISE(ABORT,'BOOK_ORGANIZATION_IMMUTABLE'); END;
CREATE TABLE book_organization_state (
 root_id INTEGER PRIMARY KEY REFERENCES library_roots(id) ON DELETE CASCADE,
 revision INTEGER NOT NULL DEFAULT 0, applied_revision INTEGER,
 index_version TEXT, rules_version TEXT, status TEXT NOT NULL DEFAULT 'PENDING',
 directories_json TEXT NOT NULL DEFAULT '[]', updated_at TEXT
);
INSERT INTO book_organization_state(root_id) SELECT id FROM library_roots;
CREATE TRIGGER book_organization_root AFTER INSERT ON library_roots
 BEGIN INSERT INTO book_organization_state(root_id) VALUES(NEW.id); END;
CREATE TABLE book_logical_groups (
 root_id INTEGER NOT NULL REFERENCES library_roots(id) ON DELETE CASCADE,
 group_id TEXT NOT NULL, title TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('WORK','SERIES')),
 relative_path TEXT NOT NULL, decision TEXT NOT NULL,
 PRIMARY KEY(root_id,group_id)
);
CREATE TABLE book_logical_members (
 root_id INTEGER NOT NULL, group_id TEXT NOT NULL,
 book_id INTEGER NOT NULL REFERENCES comic_books(id) ON DELETE CASCADE,
 ordinal INTEGER NOT NULL, role TEXT NOT NULL,
 PRIMARY KEY(root_id,group_id,book_id), UNIQUE(root_id,book_id),
 FOREIGN KEY(root_id,group_id) REFERENCES book_logical_groups(root_id,group_id) ON DELETE CASCADE
);
CREATE TABLE book_organization_overrides (
 root_id INTEGER NOT NULL REFERENCES library_roots(id) ON DELETE CASCADE,
 relative_path TEXT NOT NULL, override_json TEXT NOT NULL,
 PRIMARY KEY(root_id,relative_path)
);
CREATE TRIGGER book_override_insert AFTER INSERT ON book_organization_overrides
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=NEW.root_id; END;
CREATE TRIGGER book_override_update AFTER UPDATE ON book_organization_overrides
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=NEW.root_id; END;
CREATE TRIGGER book_override_delete AFTER DELETE ON book_organization_overrides
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=OLD.root_id; END;
CREATE TRIGGER book_node_insert AFTER INSERT ON nodes
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=NEW.library_root_id; END;
CREATE TRIGGER book_node_update AFTER UPDATE ON nodes
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=NEW.library_root_id; END;
CREATE TRIGGER book_node_delete BEFORE DELETE ON nodes
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=OLD.library_root_id; END;
CREATE TRIGGER book_index_insert AFTER INSERT ON comic_books
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=(SELECT library_root_id FROM nodes WHERE id=NEW.node_id); END;
CREATE TRIGGER book_index_update AFTER UPDATE ON comic_books
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=(SELECT library_root_id FROM nodes WHERE id=NEW.node_id); END;
CREATE TRIGGER book_index_delete BEFORE DELETE ON comic_books
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=(SELECT library_root_id FROM nodes WHERE id=OLD.node_id); END;
CREATE TRIGGER book_binding_insert AFTER INSERT ON metadata_bindings
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=(SELECT library_root_id FROM nodes WHERE id=NEW.node_id); END;
CREATE TRIGGER book_binding_update AFTER UPDATE ON metadata_bindings
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=(SELECT library_root_id FROM nodes WHERE id=NEW.node_id); END;
CREATE TRIGGER book_binding_delete BEFORE DELETE ON metadata_bindings
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=(SELECT library_root_id FROM nodes WHERE id=OLD.node_id); END;

CREATE TABLE tmdb_movie_bindings (
 node_id INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
 movie_id INTEGER NOT NULL CHECK(movie_id>0), payload_json TEXT NOT NULL,
 cover_cache_path TEXT, cover_error TEXT, active INTEGER NOT NULL DEFAULT 1 CHECK(active IN(0,1)),
 bound_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER tmdb_movie_scope BEFORE INSERT ON tmdb_movie_bindings
 WHEN NOT EXISTS(SELECT 1 FROM nodes n JOIN library_roots r ON r.id=n.library_root_id WHERE n.id=NEW.node_id AND r.media_kind='VIDEO' AND r.video_subject_scope IN('LIVE_ACTION','MIXED') AND n.total_video_count>0 AND n.node_type IN('WORK','AUTO_WORK'))
 BEGIN SELECT RAISE(ABORT,'TMDB_REQUIRES_MOVIE_WORK'); END;

CREATE TRIGGER book_page_insert AFTER INSERT ON comic_pages
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=(SELECT n.library_root_id FROM comic_books b JOIN nodes n ON n.id=b.node_id WHERE b.id=NEW.comic_book_id); END;
CREATE TRIGGER book_page_update AFTER UPDATE ON comic_pages
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=(SELECT n.library_root_id FROM comic_books b JOIN nodes n ON n.id=b.node_id WHERE b.id=NEW.comic_book_id); END;
CREATE TRIGGER book_page_delete BEFORE DELETE ON comic_pages
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=(SELECT n.library_root_id FROM comic_books b JOIN nodes n ON n.id=b.node_id WHERE b.id=OLD.comic_book_id); END;
CREATE TRIGGER tmdb_movie_update_scope BEFORE UPDATE OF node_id,movie_id,active ON tmdb_movie_bindings
 WHEN NEW.active=1 AND NOT EXISTS(SELECT 1 FROM nodes n JOIN library_roots r ON r.id=n.library_root_id WHERE n.id=NEW.node_id AND r.media_kind='VIDEO' AND r.video_subject_scope IN('LIVE_ACTION','MIXED') AND n.total_video_count>0 AND n.node_type IN('WORK','AUTO_WORK'))
 BEGIN SELECT RAISE(ABORT,'TMDB_REQUIRES_MOVIE_WORK'); END;

CREATE TRIGGER provider_media_files_insert AFTER INSERT ON media_files
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=(SELECT library_root_id FROM nodes WHERE id=NEW.node_id); END;

CREATE TRIGGER provider_media_files_update AFTER UPDATE ON media_files
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=(SELECT library_root_id FROM nodes WHERE id=NEW.node_id); END;

CREATE TRIGGER provider_media_files_delete BEFORE DELETE ON media_files
 BEGIN UPDATE book_organization_state SET revision=revision+1 WHERE root_id=(SELECT library_root_id FROM nodes WHERE id=OLD.node_id); END;

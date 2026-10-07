-- Preserve the existing Root table and all incoming foreign keys. EBOOK is a
-- distinct immutable subtype of the existing book indexing/storage family.
ALTER TABLE library_roots ADD COLUMN book_library_kind TEXT NOT NULL DEFAULT 'COMIC'
    CHECK(book_library_kind IN ('COMIC','EBOOK'));
CREATE TRIGGER book_library_kind_immutable BEFORE UPDATE OF book_library_kind ON library_roots
WHEN NEW.book_library_kind<>OLD.book_library_kind
BEGIN SELECT RAISE(ABORT,'LIBRARY_MEDIA_KIND_IMMUTABLE'); END;
CREATE TRIGGER ebook_library_kind_insert BEFORE INSERT ON library_roots
WHEN NEW.book_library_kind='EBOOK' AND NEW.media_kind<>'COMIC'
BEGIN SELECT RAISE(ABORT,'EBOOK_REQUIRES_BOOK_LIBRARY'); END;
ALTER TABLE library_roots ADD COLUMN video_subject_scope TEXT NOT NULL DEFAULT 'MIXED'
    CHECK(video_subject_scope IN ('MIXED','ANIMATION','LIVE_ACTION'));
CREATE TRIGGER video_subject_scope_immutable BEFORE UPDATE OF video_subject_scope ON library_roots
WHEN NEW.video_subject_scope<>OLD.video_subject_scope
BEGIN SELECT RAISE(ABORT,'LIBRARY_MEDIA_KIND_IMMUTABLE'); END;
CREATE TRIGGER video_subject_scope_insert BEFORE INSERT ON library_roots
WHEN NEW.video_subject_scope<>'MIXED' AND NEW.media_kind<>'VIDEO'
BEGIN SELECT RAISE(ABORT,'VIDEO_SCOPE_REQUIRES_VIDEO_LIBRARY'); END;

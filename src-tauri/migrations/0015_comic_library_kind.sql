ALTER TABLE library_roots ADD COLUMN media_kind TEXT NOT NULL DEFAULT 'VIDEO'
    CHECK(media_kind IN ('VIDEO','COMIC'));
CREATE TRIGGER library_media_kind_immutable BEFORE UPDATE OF media_kind ON library_roots
WHEN NEW.media_kind <> OLD.media_kind
BEGIN SELECT RAISE(ABORT,'LIBRARY_MEDIA_KIND_IMMUTABLE'); END;
CREATE TRIGGER comic_library_mode_insert BEFORE INSERT ON library_roots
WHEN NEW.media_kind='COMIC' AND NEW.recognition_mode<>'FOLDER'
BEGIN SELECT RAISE(ABORT,'COMIC_REQUIRES_FOLDER'); END;
CREATE TRIGGER comic_library_mode_update BEFORE UPDATE OF recognition_mode ON library_roots
WHEN NEW.media_kind='COMIC' AND NEW.recognition_mode<>'FOLDER'
BEGIN SELECT RAISE(ABORT,'COMIC_REQUIRES_FOLDER'); END;

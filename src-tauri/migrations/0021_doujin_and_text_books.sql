-- Add discriminators without rebuilding Roots/books or their incoming foreign keys.
ALTER TABLE library_roots ADD COLUMN doujin_library INTEGER NOT NULL DEFAULT 0 CHECK(doujin_library IN (0,1));
CREATE TRIGGER doujin_library_immutable BEFORE UPDATE OF doujin_library ON library_roots
WHEN NEW.doujin_library<>OLD.doujin_library
BEGIN SELECT RAISE(ABORT,'LIBRARY_MEDIA_KIND_IMMUTABLE'); END;
CREATE TRIGGER doujin_library_insert BEFORE INSERT ON library_roots
WHEN NEW.doujin_library=1 AND (NEW.media_kind<>'COMIC' OR NEW.book_library_kind<>'COMIC')
BEGIN SELECT RAISE(ABORT,'DOUJIN_REQUIRES_COMIC_LIBRARY'); END;
ALTER TABLE comic_books ADD COLUMN text_encoding TEXT CHECK(text_encoding IN ('UTF-8','UTF-16LE','UTF-16BE','GB18030'));
CREATE TRIGGER doujin_binding_insert BEFORE INSERT ON metadata_bindings
WHEN EXISTS(SELECT 1 FROM nodes n JOIN library_roots r ON r.id=n.library_root_id WHERE n.id=NEW.node_id AND r.doujin_library=1)
BEGIN SELECT RAISE(ABORT,'BANGUMI_MEDIA_KIND_CONFLICT'); END;
CREATE TRIGGER doujin_binding_update BEFORE UPDATE ON metadata_bindings
WHEN EXISTS(SELECT 1 FROM nodes n JOIN library_roots r ON r.id=n.library_root_id WHERE n.id=NEW.node_id AND r.doujin_library=1)
BEGIN SELECT RAISE(ABORT,'BANGUMI_MEDIA_KIND_CONFLICT'); END;

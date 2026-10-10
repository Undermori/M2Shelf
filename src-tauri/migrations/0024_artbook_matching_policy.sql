-- Additive subtypes preserve every Root/Node/FK and existing immutable mode.
ALTER TABLE library_roots ADD COLUMN artbook_library INTEGER NOT NULL DEFAULT 0 CHECK(artbook_library IN (0,1));
ALTER TABLE library_roots ADD COLUMN auto_bangumi INTEGER NOT NULL DEFAULT 1 CHECK(auto_bangumi IN (0,1));
UPDATE library_roots SET auto_bangumi=0 WHERE doujin_library=1;
CREATE TRIGGER artbook_library_immutable BEFORE UPDATE OF artbook_library ON library_roots
WHEN NEW.artbook_library<>OLD.artbook_library
BEGIN SELECT RAISE(ABORT,'LIBRARY_MEDIA_KIND_IMMUTABLE'); END;
CREATE TRIGGER artbook_library_insert BEFORE INSERT ON library_roots
WHEN NEW.artbook_library=1 AND (NEW.media_kind<>'COMIC' OR NEW.book_library_kind<>'COMIC' OR NEW.doujin_library<>0)
BEGIN SELECT RAISE(ABORT,'ARTBOOK_REQUIRES_BOOK_LIBRARY'); END;
-- Explicit product change: Doujin permits scoped manual binding and opt-in auto matching.
DROP TRIGGER doujin_binding_insert;
DROP TRIGGER doujin_binding_update;

-- VIDEO_FILE is the compatible storage value for individual-file recognition.
-- Existing Root identities and modes remain unchanged; book Roots may now choose it too.
DROP TRIGGER comic_library_mode_insert;
DROP TRIGGER comic_library_mode_update;
CREATE TRIGGER library_recognition_mode_immutable BEFORE UPDATE OF recognition_mode ON library_roots
WHEN NEW.recognition_mode <> OLD.recognition_mode
BEGIN SELECT RAISE(ABORT,'LIBRARY_RECOGNITION_MODE_IMMUTABLE'); END;

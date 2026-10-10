-- Content positions supplement stable chapter/segment indexes; existing rows remain valid.
ALTER TABLE comic_reading_progress ADD COLUMN text_block_index INTEGER CHECK(text_block_index BETWEEN 0 AND 100000);
ALTER TABLE comic_reading_progress ADD COLUMN text_character_offset INTEGER NOT NULL DEFAULT 0 CHECK(text_character_offset BETWEEN 0 AND 4194304);
ALTER TABLE comic_bookmarks ADD COLUMN text_block_index INTEGER CHECK(text_block_index BETWEEN 0 AND 100000);
ALTER TABLE comic_bookmarks ADD COLUMN text_character_offset INTEGER NOT NULL DEFAULT 0 CHECK(text_character_offset BETWEEN 0 AND 4194304);

-- Preserve legacy document_format constraints/rows while extending the reader.
ALTER TABLE comic_books ADD COLUMN reader_format TEXT CHECK(reader_format IN ('PDF','EPUB','TXT','MOBI','AZW3'));
-- Reading an attachment adds only reader state, retaining its original index identity.
ALTER TABLE comic_books ADD COLUMN source_resource_id INTEGER REFERENCES resource_files(id) ON DELETE CASCADE;
ALTER TABLE comic_books ADD COLUMN source_resource_stamp TEXT;
CREATE UNIQUE INDEX idx_readable_resource_identity ON comic_books(source_resource_id) WHERE source_resource_id IS NOT NULL;

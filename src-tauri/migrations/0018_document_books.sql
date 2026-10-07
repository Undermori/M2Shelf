-- Additive document format discriminator. The existing file-container transport kind is retained
-- for compatibility; document readers dispatch by this explicit format, never by that legacy kind.
ALTER TABLE comic_books ADD COLUMN document_format TEXT CHECK(document_format IN ('PDF','EPUB'));

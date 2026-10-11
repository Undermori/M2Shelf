//! Bounded read-only import, invoked by scanning only. Matching and UI read the SQLite snapshot.
use crate::{db::AppResult, media_scraper::Parsed};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
const XML_LIMIT: u64 = 1024 * 1024;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata {
    pub source: String,
    pub title: Option<String>,
    pub series: Option<String>,
    pub authors: Vec<String>,
    pub isbn: Option<String>,
    pub language: Option<String>,
    pub year: Option<i32>,
    pub volume: Option<u32>,
    pub ids: m2shelf_media_parser::ProviderIds,
    pub warnings: Vec<String>,
}
fn clean(s: &str) -> Option<String> {
    let s = s
        .chars()
        .filter(|c| !c.is_control())
        .take(300)
        .collect::<String>();
    let s = s.trim();
    (!s.is_empty()).then(|| s.into())
}
fn xml(s: &str) -> AppResult<roxmltree::Document<'_>> {
    if s.len() as u64 > XML_LIMIT {
        return Err("METADATA_LIMIT".into());
    }
    roxmltree::Document::parse_with_options(
        s,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 20_000,
        },
    )
    .map_err(|_| "METADATA_XML_INVALID".into())
}
pub(crate) fn from_xml(s: &str, source: &str) -> AppResult<Metadata> {
    let doc = xml(s)?;
    let root = doc.root_element();
    let mut m = Metadata {
        source: source.into(),
        ..Default::default()
    };
    for n in root.descendants().filter(|n| n.is_element()) {
        let key = n.tag_name().name().to_ascii_lowercase();
        let text = n.text().unwrap_or_default().trim();
        if text.is_empty() {
            continue;
        }
        match key.as_str() {
            "title" => {
                if m.title.is_none() {
                    m.title = clean(text)
                }
            }
            "series" => m.series = clean(text),
            "creator" | "writer" | "author" | "penciller" | "inker" | "colorist" => {
                for v in text.split(',').take(16) {
                    if let Some(v) = clean(v) {
                        if !m.authors.contains(&v) && m.authors.len() < 16 {
                            m.authors.push(v)
                        }
                    }
                }
            }
            "language" | "languageiso" => m.language = clean(text),
            "year" => m.year = text.parse().ok().filter(|y| (1800..=2200).contains(y)),
            "date" | "releasedate" | "premiered" => {
                if m.year.is_none() {
                    m.year = text
                        .get(..4)
                        .and_then(|v| v.parse().ok())
                        .filter(|y| (1800..=2200).contains(y))
                }
            }
            "number" | "volume" => m.volume = m2shelf_media_parser::number(text),
            "isbn" | "identifier" => {
                let v = text.strip_prefix("urn:isbn:").unwrap_or(text);
                if m2shelf_media_parser::valid_isbn(v) {
                    m.isbn = Some(v.replace('-', ""))
                }
            }
            "uniqueid" => {
                if let Ok(id) = text.parse::<i64>() {
                    if id > 0 {
                        match n
                            .attribute("type")
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                            .as_str()
                        {
                            "tmdb" => {
                                if root.tag_name().name().eq_ignore_ascii_case("tvshow") {
                                    m.ids.tmdb_tv = Some(id)
                                } else {
                                    m.ids.tmdb_movie = Some(id)
                                }
                            }
                            "bangumi" | "bgm" => m.ids.bangumi = Some(id),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    m.ids.isbn = m.isbn.clone();
    Ok(m)
}
fn entry<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, name: &str) -> AppResult<String> {
    if name.starts_with('/')
        || name.contains('\\')
        || name.split('/').any(|v| v == ".." || v == ".")
    {
        return Err("METADATA_PATH_INVALID".into());
    }
    let mut f = archive
        .by_name(name)
        .map_err(|_| "METADATA_ENTRY_MISSING")?;
    if !f.is_file() || f.size() > XML_LIMIT || f.encrypted() {
        return Err("METADATA_LIMIT".into());
    }
    let mut bytes = Vec::new();
    f.by_ref()
        .take(XML_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "METADATA_CRC_INVALID")?;
    if bytes.len() as u64 > XML_LIMIT {
        return Err("METADATA_LIMIT".into());
    }
    String::from_utf8(bytes).map_err(|_| "METADATA_ENCODING_INVALID".into())
}
fn zipped(file: File, epub: bool) -> AppResult<Metadata> {
    let mut archive = zip::ZipArchive::new(file).map_err(|_| "METADATA_ARCHIVE_INVALID")?;
    if archive.len() > 20_000 {
        return Err("METADATA_LIMIT".into());
    }
    if !epub {
        return from_xml(&entry(&mut archive, "ComicInfo.xml")?, "ComicInfo");
    }
    let container = entry(&mut archive, "META-INF/container.xml")?;
    let doc = xml(&container)?;
    let path = doc
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .ok_or("METADATA_OPF_MISSING")?;
    from_xml(&entry(&mut archive, path)?, "EPUB/OPF")
}
fn be(bytes: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize)
}
fn kindle(mut file: File) -> AppResult<Metadata> {
    // Metadata only: no text decoding, decompression, encryption support or worker bypass.
    let size = file.metadata().map_err(|_| "METADATA_READ_FAILED")?.len();
    if size > 128 * 1024 * 1024 {
        return Err("METADATA_LIMIT".into());
    }
    let mut header = [0u8; 86];
    file.read_exact(&mut header)
        .map_err(|_| "METADATA_READ_FAILED")?;
    if header.get(60..68) != Some(b"BOOKMOBI") {
        return Err("METADATA_KINDLE_INVALID".into());
    }
    let start = be(&header, 78).ok_or("METADATA_KINDLE_INVALID")?;
    if start < 86 || start as u64 >= size {
        return Err("METADATA_KINDLE_INVALID".into());
    }
    file.seek(SeekFrom::Start(start as u64))
        .map_err(|_| "METADATA_READ_FAILED")?;
    let mut bytes = Vec::new();
    file.take(XML_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "METADATA_READ_FAILED")?;
    if bytes.get(12..14) != Some(&[0, 0]) || bytes.get(16..20) != Some(b"MOBI") {
        return Err("METADATA_KINDLE_ENCRYPTED_OR_INVALID".into());
    }
    let encoding = be(&bytes, 28).ok_or("METADATA_KINDLE_INVALID")?;
    let decode = |v: &[u8]| -> Option<String> {
        if encoding == 65001 {
            std::str::from_utf8(v).ok().and_then(clean)
        } else if encoding == 1252 {
            clean(&encoding_rs::WINDOWS_1252.decode(v).0)
        } else {
            None
        }
    };
    let mut m = Metadata {
        source: "MOBI/EXTH".into(),
        ..Default::default()
    };
    if let (Some(offset), Some(len)) = (be(&bytes, 84), be(&bytes, 88)) {
        m.title = offset
            .checked_add(len)
            .and_then(|end| bytes.get(offset..end))
            .and_then(decode)
    }
    let at = be(&bytes, 20)
        .and_then(|n| n.checked_add(16))
        .ok_or("METADATA_KINDLE_INVALID")?;
    if bytes.get(at..at + 4) == Some(b"EXTH") {
        let length = be(&bytes, at + 4).ok_or("METADATA_KINDLE_INVALID")?;
        let end = at
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .ok_or("METADATA_LIMIT")?;
        let count = be(&bytes, at + 8)
            .filter(|n| *n <= 1024)
            .ok_or("METADATA_LIMIT")?;
        let mut pos = at + 12;
        for _ in 0..count {
            let tag = be(&bytes, pos).ok_or("METADATA_KINDLE_INVALID")?;
            let len = be(&bytes, pos + 4)
                .filter(|n| *n >= 8)
                .ok_or("METADATA_KINDLE_INVALID")?;
            let next = pos
                .checked_add(len)
                .filter(|n| *n <= end)
                .ok_or("METADATA_LIMIT")?;
            if let Some(value) = decode(&bytes[pos + 8..next]) {
                match tag {
                    100 => {
                        if m.authors.len() < 16 {
                            m.authors.push(value)
                        }
                    }
                    104 => {
                        if m2shelf_media_parser::valid_isbn(&value) {
                            m.isbn = Some(value.replace('-', ""))
                        }
                    }
                    503 => m.title = Some(value),
                    524 => m.language = Some(value),
                    _ => {}
                }
            }
            pos = next;
        }
    }
    m.ids.isbn = m.isbn.clone();
    Ok(m)
}
fn extract(file: File, extension: &str) -> AppResult<Metadata> {
    match extension {
        "epub" => zipped(file, true),
        "cbz" => zipped(file, false),
        "mobi" | "azw3" => kindle(file),
        "nfo" | "xml" => {
            let mut value = String::new();
            file.take(XML_LIMIT + 1)
                .read_to_string(&mut value)
                .map_err(|_| "METADATA_ENCODING_INVALID")?;
            from_xml(
                &value,
                if extension == "nfo" {
                    "NFO"
                } else {
                    "ComicInfo"
                },
            )
        }
        "pdf" => {
            let size = file.metadata().map_err(|_| "METADATA_READ_FAILED")?.len();
            if size > 16 * 1024 * 1024 {
                return Err("METADATA_PDF_DEFERRED".into());
            }
            let mut bytes = Vec::new();
            file.take(16 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "METADATA_READ_FAILED")?;
            let doc = lopdf::Document::load_mem(&bytes).map_err(|_| "METADATA_PDF_INVALID")?;
            if doc.is_encrypted() {
                return Err("METADATA_ENCRYPTED".into());
            }
            let mut m = Metadata {
                source: "PDF/Info".into(),
                ..Default::default()
            };
            if let Ok(info) = doc
                .trailer
                .get(b"Info")
                .and_then(|o| o.as_reference())
                .and_then(|id| doc.get_dictionary(id))
            {
                let mut author = None;
                for (key, target) in [
                    (b"Title".as_slice(), &mut m.title),
                    (b"Author".as_slice(), &mut author),
                ] {
                    if let Ok(lopdf::Object::String(bytes, _)) = info.get(key) {
                        *target = clean(&if bytes.starts_with(&[0xfe, 0xff]) {
                            String::from_utf16_lossy(
                                &bytes[2..]
                                    .chunks_exact(2)
                                    .map(|v| u16::from_be_bytes([v[0], v[1]]))
                                    .collect::<Vec<_>>(),
                            )
                        } else {
                            String::from_utf8_lossy(bytes).into_owned()
                        })
                    }
                }
                if let Some(author) = author {
                    m.authors.push(author)
                }
            }
            Ok(m)
        }
        _ => Err("METADATA_NOT_AVAILABLE".into()),
    }
}
#[allow(clippy::too_many_arguments)]
pub fn import(
    c: &Connection,
    node: i64,
    path: &Path,
    filesystem: &Path,
    root: &Path,
    size: u64,
    stamp: &str,
    book: Option<i64>,
    resource: Option<i64>,
) -> AppResult<()> {
    let ext = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "epub" | "cbz" | "mobi" | "azw3" | "pdf" | "nfo"
    ) && !path
        .file_name()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case("ComicInfo.xml"))
    {
        return Ok(());
    }
    let key = path.to_string_lossy();
    let revision = format!("1:{size}:{stamp}");
    if c.query_row(
        "SELECT revision FROM indexed_media_metadata WHERE source_key=?1",
        [key.as_ref()],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .map_err(|e| e.to_string())?
    .as_deref()
        == Some(&revision)
    {
        return Ok(());
    }
    let result = crate::comic_reader::checked_file(filesystem, root, size, stamp)
        .and_then(|file| extract(file, &ext));
    let value = result.unwrap_or_else(|warning| Metadata {
        warnings: vec![warning],
        ..Default::default()
    });
    let json = serde_json::to_string(&value).map_err(|_| "METADATA_INVALID")?;
    c.execute("INSERT INTO indexed_media_metadata(source_key,node_id,book_id,resource_id,revision,payload_json) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(source_key) DO UPDATE SET node_id=excluded.node_id,book_id=excluded.book_id,resource_id=excluded.resource_id,revision=excluded.revision,payload_json=excluded.payload_json",params![key,node,book,resource,revision,json]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn enrich(c: &Connection, node: i64, parsed: &mut Parsed) -> AppResult<()> {
    let mut stmt=c.prepare("SELECT payload_json FROM indexed_media_metadata WHERE node_id=?1 ORDER BY source_key COLLATE NOCASE LIMIT 32").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map([node], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut titles = Vec::new();
    let mut series = Vec::new();
    let mut ids = parsed.explicit_provider_ids.clone();
    for row in rows {
        let m: Metadata = serde_json::from_str(&row.map_err(|e| e.to_string())?)
            .map_err(|_| "METADATA_INVALID")?;
        parsed.source_evidence.push(m.source.clone());
        parsed.warnings.extend(m.warnings);
        for author in m.authors {
            if parsed.authors.len() < 16 && !parsed.authors.contains(&author) {
                parsed.authors.push(author);
            }
        }
        if let Some(language) = m.language {
            if !parsed.languages.contains(&language) {
                parsed.languages.push(language);
            }
        }
        for (from, to) in [(m.title, &mut titles), (m.series, &mut series)] {
            if let Some(value) = from {
                if !to.contains(&value) {
                    to.push(value)
                }
            }
        }
        for (from, to) in [
            (m.ids.tmdb_movie, &mut ids.tmdb_movie),
            (m.ids.tmdb_tv, &mut ids.tmdb_tv),
            (m.ids.bangumi, &mut ids.bangumi),
        ] {
            if let Some(id) = from {
                if to.is_some_and(|old| old != id) {
                    parsed.warnings.push("PROVIDER_ID_CONFLICT".into())
                } else {
                    *to = Some(id)
                }
            }
        }
        if let Some(year) = m.year {
            if parsed.year.is_some_and(|y| y != year) {
                parsed.warnings.push("YEAR_CONFLICT".into())
            } else {
                parsed.year = Some(year)
            }
        }
        if m.isbn.is_some() {
            ids.isbn = m.isbn
        }
    }
    let title = if series.len() == 1 {
        series.first()
    } else if titles.len() == 1 {
        titles.first()
    } else {
        None
    };
    if let Some(title) = title {
        parsed.search_titles.insert(
            0,
            m2shelf_media_parser::TitleCandidate {
                title: title.clone(),
                source: "embedded".into(),
            },
        );
        parsed.display_title = title.clone();
    }
    if !parsed.warnings.iter().any(|v| v == "PROVIDER_ID_CONFLICT") {
        if ids.tmdb_movie.is_some() {
            parsed.explicit_provider_ids.tmdb_movie = ids.tmdb_movie
        }
        if ids.tmdb_tv.is_some() {
            parsed.explicit_provider_ids.tmdb_tv = ids.tmdb_tv;
            parsed.subtype = crate::media_scraper::Subtype::Tv
        }
        if ids.bangumi.is_some() {
            parsed.explicit_provider_ids.bangumi = ids.bangumi
        }
    }
    parsed.explicit_provider_ids.isbn = ids.isbn;
    parsed.search_titles.truncate(8);
    Ok(())
}

/// Metadata belongs to the indexed book, never to its author/category folder.
pub fn organization_metadata(
    c: &Connection,
    book: i64,
) -> AppResult<Option<m2shelf_smart_mixed_lab::model::Metadata>> {
    let json: Option<String> = c.query_row("SELECT payload_json FROM indexed_media_metadata WHERE book_id=?1 ORDER BY source_key LIMIT 1",[book],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
    let Some(json) = json else { return Ok(None) };
    let m: Metadata = serde_json::from_str(&json).map_err(|_| "METADATA_INVALID")?;
    if !m.warnings.is_empty() {
        return Ok(None);
    }
    Ok(Some(m2shelf_smart_mixed_lab::model::Metadata {
        title: m.title,
        series: m.series,
        volume: m.volume,
        chapter: None,
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_revision_is_reused_and_foreign_sources_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let media = temp.path().join("Library");
        std::fs::create_dir(&media).unwrap();
        let root = std::fs::canonicalize(&media).unwrap();
        let path = media.join("movie.nfo");
        let original=b"<movie><title>Native Name</title><year>2007</year><uniqueid type='tmdb'>42</uniqueid></movie>";
        std::fs::write(&path, original).unwrap();
        let db = crate::db::Database::new(temp.path().join("test.sqlite"));
        db.migrate().unwrap();
        let library = db.add_root(&media, None).unwrap();
        let c = db.connect().unwrap();
        c.execute("INSERT INTO nodes(library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(?1,?2,'Source','Manual Name','WORK')",params![library.id,media.to_string_lossy()]).unwrap();
        let node = c.last_insert_rowid();
        let meta = std::fs::metadata(&path).unwrap();
        let stamp = crate::comics::modified(&meta);
        import(
            &c,
            node,
            &path,
            &path,
            &root,
            meta.len(),
            &stamp,
            None,
            None,
        )
        .unwrap();
        let first: String = c
            .query_row("SELECT payload_json FROM indexed_media_metadata", [], |r| {
                r.get(0)
            })
            .unwrap();
        import(
            &c,
            node,
            &path,
            &path,
            &root,
            meta.len(),
            &stamp,
            None,
            None,
        )
        .unwrap();
        let second: String = c
            .query_row("SELECT payload_json FROM indexed_media_metadata", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(first, second);
        let mut parsed =
            m2shelf_media_parser::parse("Source", m2shelf_media_parser::Kind::LiveAction);
        enrich(&c, node, &mut parsed).unwrap();
        assert_eq!(parsed.display_title, "Native Name");
        assert_eq!(parsed.explicit_provider_ids.tmdb_movie, Some(42));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let outside = temp.path().join("foreign.nfo");
        std::fs::write(&outside, original).unwrap();
        assert!(crate::comic_reader::checked_file(&outside, &root, meta.len(), &stamp).is_err());
        assert_eq!(
            c.query_row("SELECT display_name FROM nodes WHERE id=?1", [node], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "Manual Name"
        );
    }
    #[test]
    fn comicinfo_roles_and_identifiers() {
        let m=from_xml("<ComicInfo><Title>一卷</Title><Series>作品</Series><Number>2</Number><Writer>作者</Writer><ISBN>9780306406157</ISBN><LanguageISO>ja</LanguageISO></ComicInfo>","ComicInfo").unwrap();
        assert_eq!(m.title.as_deref(), Some("一卷"));
        assert_eq!(m.series.as_deref(), Some("作品"));
        assert_eq!(m.volume, Some(2));
        assert_eq!(m.authors, vec!["作者"]);
        assert!(m.isbn.is_some());
    }
    #[test]
    fn nfo_tv_namespace_and_entities() {
        let m = from_xml(
            "<tvshow><title>Series</title><uniqueid type='tmdb'>42</uniqueid></tvshow>",
            "NFO",
        )
        .unwrap();
        assert_eq!(m.ids.tmdb_tv, Some(42));
        assert_eq!(m.ids.tmdb_movie, None);
        assert!(from_xml(
            "<!DOCTYPE x [<!ENTITY e SYSTEM 'file:///secret'>]><x>&e;</x>",
            "NFO"
        )
        .is_err());
    }
    #[test]
    fn epub_namespaces() {
        let m=from_xml("<metadata xmlns:dc='http://purl.org/dc/elements/1.1/'><dc:title>Native title</dc:title><dc:creator>Writer</dc:creator><dc:identifier>urn:isbn:9780306406157</dc:identifier></metadata>","EPUB").unwrap();
        assert_eq!(m.title.as_deref(), Some("Native title"));
        assert_eq!(m.authors, vec!["Writer"]);
        assert!(m.isbn.is_some());
    }
}

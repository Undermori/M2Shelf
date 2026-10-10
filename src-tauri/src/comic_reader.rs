//! Read one indexed comic page into a bounded binary IPC response. Never extracts to disk.
use crate::{
    comics::{self, ComicBook, ComicOpenResult, ComicPage},
    db::{self, AppResult, Database},
};
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

static ACTIVE_READS: AtomicUsize = AtomicUsize::new(0);
struct ReadPermit;
impl ReadPermit {
    fn acquire() -> AppResult<Self> {
        ACTIVE_READS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 4).then_some(n + 1)
            })
            .map_err(|_| "COMIC_READER_BUSY".to_string())?;
        Ok(Self)
    }
}
impl Drop for ReadPermit {
    fn drop(&mut self) {
        ACTIVE_READS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn validate_book(
    connection: &Connection,
    id: i64,
) -> AppResult<(ComicBook, PathBuf, String, u64, String, PathBuf)> {
    let (node_id,source,kind,size,stamp):(i64,String,String,u64,String)=connection.query_row("SELECT node_id,source_path,source_kind,file_size,modified_at FROM comic_books WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(|_|"COMIC_BOOK_NOT_FOUND".to_string())?;
    db::ensure_node_visible_conn(connection, node_id)?;
    let (root_path,valid):(String,bool)=connection.query_row("SELECT r.path,CASE WHEN b.source_resource_id IS NULL THEN r.media_kind='COMIC' ELSE EXISTS(SELECT 1 FROM resource_files f WHERE f.id=b.source_resource_id AND f.node_id=b.node_id AND f.absolute_path=b.source_path COLLATE NOCASE AND f.file_size=b.file_size AND f.modified_at=b.source_resource_stamp) END FROM comic_books b JOIN nodes n ON n.id=b.node_id JOIN library_roots r ON r.id=n.library_root_id WHERE b.id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|"COMIC_BOOK_NOT_FOUND".to_string())?;
    if !valid {
        return Err("COMIC_BOOK_NOT_FOUND".into());
    }
    let book = comics::books(connection, node_id)?
        .into_iter()
        .find(|b| b.id == id)
        .ok_or_else(|| "COMIC_BOOK_NOT_FOUND".to_string())?;
    if let Some(error) = &book.index_error {
        return Err(error.clone());
    }
    if book.page_count == 0 {
        return Err("COMIC_NO_PAGES".into());
    }
    Ok((
        book,
        PathBuf::from(source),
        kind,
        size,
        stamp,
        PathBuf::from(root_path),
    ))
}

pub fn detail(database: &Database, node_id: i64) -> AppResult<Vec<ComicBook>> {
    database.read_snapshot(|c| {
        db::ensure_node_visible_conn(c, node_id)?;
        let node = db::get_node_conn(c, node_id)?;
        if !node.media_kind.is_book() {
            return Err("COMIC_BOOK_NOT_FOUND".into());
        }
        comics::owned_books(c, node_id)
    })
}

/// Covers use the first indexed page through the same Root/revision/handle checks as reading.
pub fn first_image_cover(database: &Database, node_id: i64) -> AppResult<Option<String>> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let candidate = first_image_cover_source(database, node_id)?;
    let Some((id, revision, _name)) = candidate else {
        return Ok(None);
    };
    let Some(bytes) = read_book_cover(database, id, &revision)? else {
        return Ok(None);
    };
    if bytes.len() > 8 * 1024 * 1024 {
        return Ok(None);
    }
    let mime = image_mime(&bytes)?;
    Ok(Some(format!(
        "data:{mime};base64,{}",
        STANDARD.encode(bytes)
    )))
}

pub(crate) fn first_image_cover_source(
    database: &Database,
    node_id: i64,
) -> AppResult<Option<(i64, String, String)>> {
    let candidate = database.read_snapshot(|c| {
        db::ensure_node_visible_conn(c, node_id)?;
        if !db::get_node_conn(c, node_id)?.media_kind.is_book() { return Ok(None); }
        c.query_row("WITH RECURSIVE visible(id) AS (SELECT id FROM nodes WHERE id=?1 AND node_type<>'IGNORED' UNION ALL SELECT n.id FROM nodes n JOIN visible v ON n.parent_node_id=v.id WHERE n.node_type<>'IGNORED') SELECT b.id,b.revision,p.page_name FROM comic_books b JOIN visible v ON v.id=b.node_id JOIN comic_pages p ON p.comic_book_id=b.id AND p.page_index=0 WHERE b.source_resource_id IS NULL AND b.index_error IS NULL AND (b.source_kind='IMAGE_FOLDER' OR lower(b.source_path) LIKE '%.cbz' OR COALESCE(b.reader_format,b.document_format) IN ('PDF','EPUB','MOBI','AZW3')) ORDER BY b.source_path COLLATE NOCASE,b.id LIMIT 1", [node_id], |r| Ok((r.get::<_,i64>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?)))
            .optional().map_err(|e| e.to_string())
    })?;
    Ok(candidate)
}

pub(crate) fn read_book_cover(
    database: &Database,
    id: i64,
    revision: &str,
) -> AppResult<Option<Vec<u8>>> {
    let format = database.read_snapshot(|c| {
        let (book, _, _, _, _, _) = validate_book(c, id)?;
        if book.revision != revision {
            return Err("COMIC_PAGE_CHANGED".into());
        }
        Ok((book.document_format, book.page_count))
    })?;
    let bytes = if let Some(format) = format.0 {
        let _permit = ReadPermit::acquire()?;
        database.read_snapshot(|c| {
            let (book, source, _, size, stamp, root) = validate_book(c, id)?;
            if book.revision != revision {
                return Err("COMIC_PAGE_CHANGED".into());
            }
            let root = fs::canonicalize(root).map_err(|_| "COMIC_READ_FAILED")?;
            let mut file = checked_file(&source, &root, size, &stamp)?;
            let bytes = if matches!(format.as_str(), "MOBI" | "AZW3") {
                crate::kindle_books::cover(&mut file)?
            } else {
                crate::ebooks::cover(file.try_clone().map_err(|_| "COMIC_READ_FAILED")?, &format)?
            };
            let after = file.metadata().map_err(|_| "COMIC_READ_FAILED")?;
            if after.len() != size || comics::modified(&after) != stamp {
                return Err("COMIC_PAGE_CHANGED".into());
            }
            Ok(bytes)
        })?
    } else {
        let mut found = None;
        for page in 0..format.1.min(8) {
            if let Ok(bytes) = read_page_at_revision(database, id, page, Some(revision)) {
                if bytes.len() <= 8 * 1024 * 1024 {
                    found = Some(bytes);
                    break;
                }
            }
        }
        found
    };
    Ok(bytes.filter(|bytes| bytes.len() <= 8 * 1024 * 1024 && validate_image(bytes).is_ok()))
}

pub(crate) fn cover_identity(
    database: &Database,
    id: i64,
    revision: &str,
) -> AppResult<Option<String>> {
    database.read_snapshot(|c| {
        let (book, _, _, _, _, _) = validate_book(c, id)?;
        if book.revision != revision {
            return Err("COMIC_PAGE_CHANGED".into());
        }
        if book.index_error.is_some() || book.page_count == 0 {
            return Ok(None);
        }
        c.query_row(
            "SELECT page_name FROM comic_pages WHERE comic_book_id=?1 AND page_index=0",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())
    })
}

/// Reveal only an indexed source within its current Root, including unreadable books.
pub fn reveal(database: &Database, id: i64) -> AppResult<()> {
    let (source, kind, size, stamp, root) = database.read_snapshot(|c| {
        let (node, source, kind, size, stamp, root): (i64, String, String, u64, String, String) = c.query_row(
            "SELECT b.node_id,b.source_path,b.source_kind,b.file_size,b.modified_at,r.path FROM comic_books b JOIN nodes n ON n.id=b.node_id JOIN library_roots r ON r.id=n.library_root_id WHERE b.id=?1 AND (r.media_kind IN ('COMIC','EBOOK','DOUJIN','ARTBOOK') OR EXISTS(SELECT 1 FROM resource_files f WHERE f.id=b.source_resource_id AND f.node_id=b.node_id AND f.absolute_path=b.source_path COLLATE NOCASE AND f.file_size=b.file_size AND f.modified_at=b.source_resource_stamp))",
            [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))
        ).map_err(|_| "COMIC_BOOK_NOT_FOUND".to_string())?;
        db::ensure_node_visible_conn(c, node)?;
        Ok((source, kind, size, stamp, root))
    })?;
    let root = fs::canonicalize(root).map_err(|_| "COMIC_READ_FAILED")?;
    let source = Path::new(&source);
    let resolved = canonical(source, &root)?;
    if kind == "IMAGE_FOLDER" && resolved.is_dir() {
        if !resolved.is_dir() {
            return Err("COMIC_PAGE_CHANGED".into());
        }
    } else {
        let _file = checked_file(source, &root, size, &stamp)?;
    }
    crate::player::reveal(&resolved)
}
pub fn open(database: &Database, id: i64) -> AppResult<ComicOpenResult> {
    let resource = database.read_snapshot(|c| {
        c.query_row(
            "SELECT source_resource_id FROM comic_books WHERE id=?1",
            [id],
            |r| r.get::<_, Option<i64>>(0),
        )
        .map_err(|_| "COMIC_BOOK_NOT_FOUND".to_string())
    })?;
    if let Some(resource) = resource {
        open_resource(database, resource)?;
    }
    database.read_snapshot(|c|{
        let (book,source,_,size,stamp,root)=validate_book(c,id)?;
        let mut s=c.prepare("SELECT page_index,page_name,source_locator FROM comic_pages WHERE comic_book_id=?1 ORDER BY page_index").map_err(|e|e.to_string())?;
        let indexed=s.query_map([id],|r|Ok((ComicPage{page_index:r.get(0)?,page_name:r.get(1)?},r.get::<_,String>(2)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        let navigation = if book.document_format.as_deref()==Some("EPUB") {
            let canonical=fs::canonicalize(root).map_err(|_|"COMIC_READ_FAILED")?;
            let file=checked_file(&source,&canonical,size,&stamp)?;
            let navigation=crate::ebooks::navigation(file.try_clone().map_err(|_|"COMIC_READ_FAILED")?).unwrap_or_default();
            let after=file.metadata().map_err(|_|"COMIC_READ_FAILED")?;
            if after.len()!=size||comics::modified(&after)!=stamp{return Err("COMIC_PAGE_CHANGED".into());}
            navigation.into_iter().filter_map(|(locator,title,fragment)|indexed.iter().find(|(_,path)|*path==locator).map(|(page,_)|crate::comics::BookNavigation{page_index:page.page_index,title,fragment})).collect::<Vec<_>>()
        } else {Vec::new()};
        let pages=indexed.into_iter().map(|(mut page,_)|{if let Some(item)=navigation.iter().find(|n|n.page_index==page.page_index){page.page_name=item.title.clone();}page}).collect();
        let mut positions=c.prepare("SELECT page_index,text_block_index,text_character_offset FROM comic_bookmarks WHERE comic_book_id=?1 AND text_block_index IS NOT NULL").map_err(|e|e.to_string())?;
        let bookmark_positions=positions.query_map([id],|r|Ok((r.get(0)?,crate::comics::TextPosition{block_index:r.get(1)?,character_offset:r.get(2)?}))).map_err(|e|e.to_string())?.collect::<Result<std::collections::BTreeMap<_,_>,_>>().map_err(|e|e.to_string())?;
        Ok(ComicOpenResult{book,pages,bookmarks:bookmarks_conn(c,id)?,bookmark_positions,navigation})
    })
}

/// The single opening policy for indexed attachments. Unknown formats return
/// None to the existing Shell opener; readable formats keep the ResourceFile FK.
pub fn open_resource(database: &Database, resource_id: i64) -> AppResult<Option<ComicBook>> {
    let resource = database.get_resource_file(resource_id)?;
    let source = Path::new(&resource.absolute_path);
    let format = crate::ebooks::format(source);
    if format.is_none() && !comics::is_image(source) && !comics::is_archive(source) {
        return Ok(None);
    }
    let root = database.read_snapshot(|c| {
        db::ensure_node_visible_conn(c, resource.node_id)?;
        c.query_row("SELECT r.path FROM nodes n JOIN library_roots r ON r.id=n.library_root_id WHERE n.id=?1", [resource.node_id], |r| r.get::<_,String>(0)).map_err(|_| "COMIC_BOOK_NOT_FOUND".to_string())
    })?;
    let root = fs::canonicalize(root).map_err(|_| "COMIC_READ_FAILED")?;
    // Legacy ResourceFile timestamps have second precision. Preserve that index
    // identity, while each opened reader revision records the full handle stamp.
    let metadata = fs::metadata(source).map_err(|_| "COMIC_READ_FAILED")?;
    let resource_stamp = metadata
        .modified()
        .ok()
        .map(chrono::DateTime::<chrono::Utc>::from)
        .map(|stamp| stamp.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default();
    let book_stamp = comics::modified(&metadata);
    if metadata.len() != resource.file_size as u64 || resource_stamp != resource.modified_at {
        return Err("COMIC_PAGE_CHANGED".into());
    }
    let file = checked_file(source, &root, resource.file_size as u64, &book_stamp)?;
    // Kindle parsing is needed for reading anyway. Reuse its bounded one-book
    // cache, and refresh a changed chapter adapter without trusting stale locators.
    let kindle_pages = if matches!(format, Some("MOBI" | "AZW3")) {
        Some(crate::ebooks::index_file(
            file.try_clone().map_err(|_| "COMIC_READ_FAILED")?,
            format.unwrap(),
        )?)
    } else {
        None
    };
    let connection = database.connect()?;
    if let Some(book) = comics::books(&connection, resource.node_id)?
        .into_iter()
        .find(|b| {
            b.source_path.eq_ignore_ascii_case(&resource.absolute_path)
                && b.source_size == resource.file_size as u64
                && b.modified_at == book_stamp
                && b.index_error.is_none()
        })
    {
        let matches_index = if let Some(pages) = &kindle_pages {
            let mut statement = connection.prepare("SELECT source_locator FROM comic_pages WHERE comic_book_id=?1 ORDER BY page_index").map_err(|e|e.to_string())?;
            let old = statement
                .query_map([book.id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            old.iter()
                .map(String::as_str)
                .eq(pages.iter().map(|p| p.locator.as_str()))
        } else {
            true
        };
        if matches_index {
            return Ok(Some(book));
        }
    }
    let kind = if comics::is_image(source) {
        "IMAGE_FOLDER"
    } else {
        "ZIP_ARCHIVE"
    };
    let pages = if let Some(pages) = kindle_pages {
        pages
    } else if let Some(format) = format {
        crate::ebooks::index_file(file.try_clone().map_err(|_| "COMIC_READ_FAILED")?, format)?
    } else if comics::is_archive(source) {
        comics::archive_pages_file(file.try_clone().map_err(|_| "COMIC_READ_FAILED")?, None)?
    } else {
        let mut bytes = Vec::new();
        file.try_clone()
            .map_err(|_| "COMIC_READ_FAILED")?
            .take(comics::MAX_PAGE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "COMIC_READ_FAILED")?;
        if bytes.len() as u64 > comics::MAX_PAGE_BYTES {
            return Err("COMIC_PAGE_LIMIT".into());
        }
        validate_image(&bytes)?;
        vec![comics::IndexedPage {
            name: resource.file_name.clone(),
            locator: resource.absolute_path.clone(),
            size: resource.file_size as u64,
            modified: book_stamp.clone(),
            crc: None,
        }]
    };
    let after = file.metadata().map_err(|_| "COMIC_READ_FAILED")?;
    if after.len() != resource.file_size as u64 || comics::modified(&after) != book_stamp {
        return Err("COMIC_PAGE_CHANGED".into());
    }
    // Only the final short write transaction holds SQLite's writer reservation.
    // Revalidate identity/visibility after parsing so a scan/remove cannot be undone.
    let tx = connection
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    db::ensure_node_visible_conn(&tx, resource.node_id)?;
    let current: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM resource_files WHERE id=?1 AND node_id=?2 AND absolute_path=?3 COLLATE NOCASE AND file_size=?4 AND modified_at=?5)", params![resource.id,resource.node_id,resource.absolute_path,resource.file_size,resource.modified_at], |r| r.get(0)).map_err(|e|e.to_string())?;
    if !current {
        return Err("COMIC_PAGE_CHANGED".into());
    }
    comics::store_book_rows(
        &tx,
        resource.node_id,
        source,
        kind,
        &resource.file_name,
        resource.file_size as u64,
        &book_stamp,
        Ok(pages),
        &resource.last_seen_at,
    )?;
    tx.execute("UPDATE comic_books SET reader_format=?1,source_resource_id=?2,source_resource_stamp=?5 WHERE node_id=?3 AND source_path=?4 COLLATE NOCASE",params![format,resource.id,resource.node_id,resource.absolute_path,resource.modified_at]).map_err(|e|e.to_string())?;
    let book = comics::books(&tx, resource.node_id)?
        .into_iter()
        .find(|b| b.source_path.eq_ignore_ascii_case(&resource.absolute_path))
        .ok_or("COMIC_BOOK_NOT_FOUND")?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Some(book))
}
fn canonical(path: &Path, root: &Path) -> AppResult<PathBuf> {
    let value = fs::canonicalize(path).map_err(|_| "COMIC_READ_FAILED".to_string())?;
    if !value.starts_with(root) {
        return Err("COMIC_PATH_OUTSIDE_ROOT".into());
    }
    Ok(value)
}
fn checked_file(path: &Path, root: &Path, size: u64, stamp: &str) -> AppResult<File> {
    let resolved = canonical(path, root)?;
    let file = File::open(&resolved).map_err(|_| "COMIC_READ_FAILED".to_string())?;
    // On Windows validate the actual opened handle, not just the pre-open path.
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;
        let mut buffer = vec![0u16; 32_768];
        let length = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                0,
            )
        };
        if length == 0 || length as usize >= buffer.len() {
            return Err("COMIC_READ_FAILED".into());
        }
        let actual = PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize]));
        if !actual.starts_with(root) {
            return Err("COMIC_PATH_OUTSIDE_ROOT".into());
        }
    }
    let meta = file
        .metadata()
        .map_err(|_| "COMIC_READ_FAILED".to_string())?;
    if !meta.is_file() || meta.len() != size || comics::modified(&meta) != stamp {
        return Err("COMIC_PAGE_CHANGED".into());
    }
    canonical(path, root)?;
    Ok(file)
}
#[cfg(test)]
pub fn read_page(database: &Database, id: i64, index: i64) -> AppResult<Vec<u8>> {
    read_page_at_revision(database, id, index, None)
}
pub fn read_page_at_revision(
    database: &Database,
    id: i64,
    index: i64,
    expected: Option<&str>,
) -> AppResult<Vec<u8>> {
    let _permit = ReadPermit::acquire()?;
    database.read_snapshot(|c|{
        let (book,source,kind,size,stamp,root_path)=validate_book(c,id)?;
        if expected.is_some_and(|revision|revision!=book.revision){return Err("COMIC_PAGE_CHANGED".into());}
        if index<0 || index>=book.page_count {return Err("COMIC_PAGE_CHANGED".into());}
        let root=fs::canonicalize(root_path).map_err(|_|"COMIC_READ_FAILED".to_string())?;
        let (locator,page_size,page_stamp,crc):(String,u64,String,Option<u32>)=c.query_row("SELECT source_locator,file_size,modified_at,crc32 FROM comic_pages WHERE comic_book_id=?1 AND page_index=?2",params![id,index],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(|_|"COMIC_PAGE_CHANGED".to_string())?;
        if page_size>comics::MAX_PAGE_BYTES {return Err("COMIC_PAGE_LIMIT".into());}
        let bytes=if kind=="IMAGE_FOLDER" {
            canonical(&source,&root)?;
            let mut file=checked_file(Path::new(&locator),&root,page_size,&page_stamp)?;
            let mut bytes=Vec::new();file.by_ref().take(comics::MAX_PAGE_BYTES+1).read_to_end(&mut bytes).map_err(|_|"COMIC_READ_FAILED".to_string())?;
            let after=file.metadata().map_err(|_|"COMIC_READ_FAILED".to_string())?;
            if after.len()!=page_size || comics::modified(&after)!=page_stamp {return Err("COMIC_PAGE_CHANGED".into());} bytes
        } else {
            if !comics::valid_entry(&locator) {return Err("COMIC_ARCHIVE_PATH".into());}
            let file=checked_file(&source,&root,size,&stamp)?;
            let mut archive=comics::open_archive(file)?;
            if archive.len()>20_000 {return Err("COMIC_PAGE_LIMIT".into());}
            let mut entry=archive.by_name(&locator).map_err(|e|if e.to_string().to_lowercase().contains("password"){"COMIC_ARCHIVE_ENCRYPTED".to_string()}else{"COMIC_PAGE_CHANGED".to_string()})?;
            if entry.size()!=page_size || Some(entry.crc32())!=crc {return Err("COMIC_PAGE_CHANGED".into());}
            let mut bytes=Vec::new();entry.by_ref().take(comics::MAX_PAGE_BYTES+1).read_to_end(&mut bytes).map_err(|_|"COMIC_ARCHIVE_DAMAGED".to_string())?;drop(entry);
            let after=archive.into_inner().metadata().map_err(|_|"COMIC_READ_FAILED".to_string())?;
            if after.len()!=size || comics::modified(&after)!=stamp {return Err("COMIC_PAGE_CHANGED".into());} bytes
        };
        if bytes.len() as u64!=page_size || bytes.len() as u64>comics::MAX_PAGE_BYTES {return Err("COMIC_PAGE_LIMIT".into());}
        validate_image(&bytes)?;
        Ok(bytes)
    })
}

pub(crate) fn validate_image(bytes: &[u8]) -> AppResult<()> {
    image_dimensions(bytes).map(|_| ())
}

/// Some collections contain JPEG pages named .png (or the reverse). The indexed suffix
/// selects pages, but the supported binary signature determines their actual format.
pub(crate) fn image_mime(bytes: &[u8]) -> AppResult<&'static str> {
    if bytes.starts_with(&[255, 216, 255]) {
        Ok("image/jpeg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok("image/png")
    } else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Ok("image/webp")
    } else if bytes.len() >= 10 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        Ok("image/gif")
    } else if bytes.len() >= 26 && bytes.starts_with(b"BM") {
        Ok("image/bmp")
    } else if bytes.len() >= 24
        && &bytes[4..8] == b"ftyp"
        && bytes[8..32.min(bytes.len())]
            .chunks_exact(4)
            .any(|b| b == b"avif" || b == b"avis")
    {
        Ok("image/avif")
    } else {
        Err("COMIC_IMAGE_INVALID".into())
    }
}

pub(crate) fn image_dimensions(bytes: &[u8]) -> AppResult<(u32, u32)> {
    let bad = || "COMIC_IMAGE_INVALID".to_string();
    let (width, height) = match image_mime(bytes)? {
        "image/jpeg" | "image/png" | "image/webp" => {
            return crate::cache::cover_payload_dimensions(bytes).map_err(|_| bad());
        }
        "image/gif" => (
            u32::from(u16::from_le_bytes([bytes[6], bytes[7]])),
            u32::from(u16::from_le_bytes([bytes[8], bytes[9]])),
        ),
        "image/bmp" => {
            let width = i32::from_le_bytes(bytes[18..22].try_into().map_err(|_| bad())?)
                .checked_abs()
                .ok_or_else(bad)? as u32;
            let height = i32::from_le_bytes(bytes[22..26].try_into().map_err(|_| bad())?)
                .checked_abs()
                .ok_or_else(bad)? as u32;
            (width, height)
        }
        "image/avif" => avif_dimensions(bytes, 0)?.ok_or_else(bad)?,
        _ => return Err(bad()),
    };
    if width == 0
        || height == 0
        || width > 32768
        || height > 32768
        || u64::from(width) * u64::from(height) > 40_000_000
    {
        return Err("COMIC_PAGE_LIMIT".into());
    }
    Ok((width, height))
}

pub fn read_document(
    database: &Database,
    id: i64,
    index: i64,
    expected: &str,
) -> AppResult<Vec<u8>> {
    let _permit = ReadPermit::acquire()?;
    database.read_snapshot(|c|{
  let (book,source,_,size,stamp,root)=validate_book(c,id)?;
  if book.revision!=expected||index<0||index>=book.page_count{return Err("COMIC_PAGE_CHANGED".into());}
  let root=fs::canonicalize(root).map_err(|_|"COMIC_READ_FAILED")?;
  let mut file=checked_file(&source,&root,size,&stamp)?;
  let result=match book.document_format.as_deref(){
   Some("PDF")=>{if size>comics::MAX_PAGE_BYTES{return Err("COMIC_PAGE_LIMIT".into());}let mut bytes=Vec::new();file.by_ref().take(comics::MAX_PAGE_BYTES+1).read_to_end(&mut bytes).map_err(|_|"COMIC_READ_FAILED")?;if bytes.len() as u64!=size{return Err("COMIC_PAGE_CHANGED".into());}Ok(bytes)},
   Some("TXT")=>{let locator:String=c.query_row("SELECT source_locator FROM comic_pages WHERE comic_book_id=?1 AND page_index=?2",params![id,index],|r|r.get(0)).map_err(|_|"COMIC_PAGE_CHANGED")?;serde_json::to_vec(&crate::text_books::read(&mut file,&locator,size)?).map_err(|_|"COMIC_DOCUMENT_INVALID".to_string())},
   Some("EPUB")=>{let (locator,page_size,crc):(String,u64,Option<u32>)=c.query_row("SELECT source_locator,file_size,crc32 FROM comic_pages WHERE comic_book_id=?1 AND page_index=?2",params![id,index],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_|"COMIC_PAGE_CHANGED")?;serde_json::to_vec(&crate::ebooks::chapter(file.try_clone().map_err(|_|"COMIC_READ_FAILED")?,&locator,page_size,crc)?).map_err(|_|"COMIC_DOCUMENT_INVALID".to_string())},
   Some("MOBI"|"AZW3")=>{let locator:String=c.query_row("SELECT source_locator FROM comic_pages WHERE comic_book_id=?1 AND page_index=?2",params![id,index],|r|r.get(0)).map_err(|_|"COMIC_PAGE_CHANGED")?;serde_json::to_vec(&crate::kindle_books::chapter(&mut file,&locator)?).map_err(|_|"COMIC_DOCUMENT_INVALID".to_string())},
   _=>Err("COMIC_DOCUMENT_INVALID".into()),
  }?;
  let after=file.metadata().map_err(|_|"COMIC_READ_FAILED")?;
  if after.len()!=size||comics::modified(&after)!=stamp{return Err("COMIC_PAGE_CHANGED".into());}Ok(result)
 })
}
/// Lazy EPUB illustration transport, bounded independently of the containing chapter.
pub fn read_epub_image(
    database: &Database,
    id: i64,
    index: i64,
    expected: &str,
    block: usize,
) -> AppResult<Vec<u8>> {
    let _permit = ReadPermit::acquire()?;
    database.read_snapshot(|c| {
        let (book,source,_,size,stamp,root)=validate_book(c,id)?;
        if book.revision!=expected || book.document_format.as_deref()!=Some("EPUB") || index<0 || index>=book.page_count || block>30_000 {return Err("COMIC_PAGE_CHANGED".into());}
        let root=fs::canonicalize(root).map_err(|_|"COMIC_READ_FAILED")?;
        let file=checked_file(&source,&root,size,&stamp)?;
        let (locator,page_size,crc):(String,u64,Option<u32>)=c.query_row("SELECT source_locator,file_size,crc32 FROM comic_pages WHERE comic_book_id=?1 AND page_index=?2",params![id,index],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_|"COMIC_PAGE_CHANGED")?;
        let bytes=crate::ebooks::illustration(file.try_clone().map_err(|_|"COMIC_READ_FAILED")?,&locator,page_size,crc,block)?;
        let after=file.metadata().map_err(|_|"COMIC_READ_FAILED")?;
        if after.len()!=size || comics::modified(&after)!=stamp {return Err("COMIC_PAGE_CHANGED".into());}
        Ok(bytes)
    })
}
/// Random access PDF transport: never sends or duplicates an entire large PDF in IPC.
pub fn read_pdf_range(
    database: &Database,
    id: i64,
    begin: u64,
    end: u64,
    expected: &str,
) -> AppResult<Vec<u8>> {
    let _permit = ReadPermit::acquire()?;
    database.read_snapshot(|c| {
        let (book, source, _, size, stamp, root) = validate_book(c, id)?;
        if book.revision != expected || book.document_format.as_deref() != Some("PDF") {
            return Err("COMIC_PAGE_CHANGED".into());
        }
        if size > crate::ebooks::MAX_PDF_BYTES
            || begin >= end
            || end > size
            || end - begin > crate::ebooks::PDF_CHUNK_BYTES
        {
            return Err("COMIC_PAGE_LIMIT".into());
        }
        let root = fs::canonicalize(root).map_err(|_| "COMIC_READ_FAILED")?;
        let mut file = checked_file(&source, &root, size, &stamp)?;
        file.seek(SeekFrom::Start(begin))
            .map_err(|_| "COMIC_READ_FAILED")?;
        let mut bytes = vec![0; (end - begin) as usize];
        file.read_exact(&mut bytes)
            .map_err(|_| "COMIC_READ_FAILED")?;
        let after = file.metadata().map_err(|_| "COMIC_READ_FAILED")?;
        if after.len() != size || comics::modified(&after) != stamp {
            return Err("COMIC_PAGE_CHANGED".into());
        }
        Ok(bytes)
    })
}
fn avif_dimensions(bytes: &[u8], depth: u8) -> AppResult<Option<(u32, u32)>> {
    if depth > 5 {
        return Err("COMIC_IMAGE_INVALID".into());
    }
    let mut offset = 0;
    let mut count = 0;
    while offset + 8 <= bytes.len() {
        count += 1;
        if count > 20_000 {
            return Err("COMIC_IMAGE_INVALID".into());
        }
        let size = u32::from_be_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .map_err(|_| "COMIC_IMAGE_INVALID".to_string())?,
        ) as usize;
        if size < 8 || size > bytes.len() - offset {
            return Err("COMIC_IMAGE_INVALID".into());
        }
        let kind = &bytes[offset + 4..offset + 8];
        let payload = &bytes[offset + 8..offset + size];
        if kind == b"ispe" && payload.len() >= 12 {
            return Ok(Some((
                u32::from_be_bytes(payload[4..8].try_into().unwrap()),
                u32::from_be_bytes(payload[8..12].try_into().unwrap()),
            )));
        }
        if matches!(kind, b"meta" | b"iprp" | b"ipco") {
            let skip = if kind == b"meta" { 4 } else { 0 };
            if payload.len() < skip {
                return Err("COMIC_IMAGE_INVALID".into());
            }
            if let Some(size) = avif_dimensions(&payload[skip..], depth + 1)? {
                return Ok(Some(size));
            }
        }
        offset += size;
    }
    Ok(None)
}

fn validate_page(connection: &Connection, id: i64, index: i64) -> AppResult<()> {
    let (book, _, _, _, _, _) = validate_book(connection, id)?;
    if index < 0 || index >= book.page_count {
        return Err("COMIC_PAGE_CHANGED".into());
    }
    Ok(())
}
#[cfg(test)]
pub fn progress(database: &Database, id: i64, index: i64) -> AppResult<()> {
    progress_at_revision(database, id, index, None)
}
#[cfg(test)]
pub fn progress_at_revision(
    database: &Database,
    id: i64,
    index: i64,
    expected: Option<&str>,
) -> AppResult<()> {
    progress_with_position_at_revision(database, id, index, expected, None)
}
pub fn progress_with_position_at_revision(
    database: &Database,
    id: i64,
    index: i64,
    expected: Option<&str>,
    position: Option<crate::comics::TextPosition>,
) -> AppResult<()> {
    let mut c = database.connect()?;
    let tx = c.transaction().map_err(|e| e.to_string())?;
    validate_page(&tx, id, index)?;
    if let Some(revision) = expected {
        if validate_book(&tx, id)?.0.revision != revision {
            return Err("COMIC_PAGE_CHANGED".into());
        }
    }
    validate_position(&tx, id, position.as_ref())?;
    tx.execute("INSERT INTO comic_reading_progress(comic_book_id,last_page_index,text_block_index,text_character_offset) VALUES(?1,?2,?3,?4) ON CONFLICT(comic_book_id) DO UPDATE SET last_page_index=excluded.last_page_index,text_block_index=excluded.text_block_index,text_character_offset=excluded.text_character_offset,last_read_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",params![id,index,position.as_ref().map(|p|p.block_index),position.as_ref().map_or(0,|p|p.character_offset)]).map_err(|_|"COMIC_PROGRESS_FAILED".to_string())?;
    tx.commit().map_err(|_| "COMIC_PROGRESS_FAILED".to_string())
}
fn bookmarks_conn(c: &Connection, id: i64) -> AppResult<Vec<i64>> {
    let mut s = c
        .prepare(
            "SELECT page_index FROM comic_bookmarks WHERE comic_book_id=?1 ORDER BY page_index",
        )
        .map_err(|e| e.to_string())?;
    let items = s
        .query_map([id], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(items)
}
pub fn bookmarks(database: &Database, id: i64) -> AppResult<Vec<i64>> {
    database.read_snapshot(|c| {
        validate_book(c, id)?;
        bookmarks_conn(c, id)
    })
}
#[cfg(test)]
pub fn bookmark(database: &Database, id: i64, index: i64, add: bool) -> AppResult<Vec<i64>> {
    bookmark_at_revision(database, id, index, add, None)
}
pub fn bookmark_at_revision(
    database: &Database,
    id: i64,
    index: i64,
    add: bool,
    expected: Option<&str>,
) -> AppResult<Vec<i64>> {
    bookmark_with_position_at_revision(database, id, index, add, expected, None)
}
pub fn bookmark_with_position_at_revision(
    database: &Database,
    id: i64,
    index: i64,
    add: bool,
    expected: Option<&str>,
    position: Option<crate::comics::TextPosition>,
) -> AppResult<Vec<i64>> {
    let mut c = database.connect()?;
    let tx = c.transaction().map_err(|e| e.to_string())?;
    validate_page(&tx, id, index)?;
    if let Some(revision) = expected {
        if validate_book(&tx, id)?.0.revision != revision {
            return Err("COMIC_PAGE_CHANGED".into());
        }
    }
    tx.execute(
        if add {
            "INSERT OR IGNORE INTO comic_bookmarks(comic_book_id,page_index) VALUES(?1,?2)"
        } else {
            "DELETE FROM comic_bookmarks WHERE comic_book_id=?1 AND page_index=?2"
        },
        params![id, index],
    )
    .map_err(|_| "COMIC_BOOKMARK_FAILED".to_string())?;
    if add {
        validate_position(&tx, id, position.as_ref())?;
        tx.execute("UPDATE comic_bookmarks SET text_block_index=?3,text_character_offset=?4 WHERE comic_book_id=?1 AND page_index=?2",params![id,index,position.as_ref().map(|p|p.block_index),position.as_ref().map_or(0,|p|p.character_offset)]).map_err(|_|"COMIC_BOOKMARK_FAILED".to_string())?;
    }
    let result = bookmarks_conn(&tx, id)?;
    tx.commit()
        .map_err(|_| "COMIC_BOOKMARK_FAILED".to_string())?;
    Ok(result)
}

fn validate_position(
    c: &Connection,
    id: i64,
    position: Option<&crate::comics::TextPosition>,
) -> AppResult<()> {
    if let Some(p) = position {
        let book = validate_book(c, id)?.0;
        if !matches!(
            book.document_format.as_deref(),
            Some("EPUB" | "TXT" | "MOBI" | "AZW3")
        ) || !(0..=100000).contains(&p.block_index)
            || !(0..=4194304).contains(&p.character_offset)
        {
            return Err("COMIC_PAGE_CHANGED".into());
        }
    }
    Ok(())
}

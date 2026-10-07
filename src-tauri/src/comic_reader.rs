//! Read one indexed comic page into a bounded binary IPC response. Never extracts to disk.
use crate::{
    comics::{self, ComicBook, ComicOpenResult, ComicPage},
    db::{self, AppResult, Database},
    models::LibraryMediaKind,
};
use rusqlite::{params, Connection};
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
    let (root_path,media_kind):(String,String)=connection.query_row("SELECT r.path,r.media_kind FROM nodes n JOIN library_roots r ON r.id=n.library_root_id WHERE n.id=?1",[node_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|"COMIC_BOOK_NOT_FOUND".to_string())?;
    if LibraryMediaKind::from_db(&media_kind) != LibraryMediaKind::Comic {
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

/// Reveal only an indexed source within its current Root, including unreadable books.
pub fn reveal(database: &Database, id: i64) -> AppResult<()> {
    let (source, kind, size, stamp, root) = database.read_snapshot(|c| {
        let (node, source, kind, size, stamp, root): (i64, String, String, u64, String, String) = c.query_row(
            "SELECT b.node_id,b.source_path,b.source_kind,b.file_size,b.modified_at,r.path FROM comic_books b JOIN nodes n ON n.id=b.node_id JOIN library_roots r ON r.id=n.library_root_id WHERE b.id=?1 AND r.media_kind='COMIC'",
            [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))
        ).map_err(|_| "COMIC_BOOK_NOT_FOUND".to_string())?;
        db::ensure_node_visible_conn(c, node)?;
        Ok((source, kind, size, stamp, root))
    })?;
    let root = fs::canonicalize(root).map_err(|_| "COMIC_READ_FAILED")?;
    let source = Path::new(&source);
    let resolved = canonical(source, &root)?;
    if kind == "IMAGE_FOLDER" {
        if !resolved.is_dir() {
            return Err("COMIC_PAGE_CHANGED".into());
        }
    } else {
        let _file = checked_file(source, &root, size, &stamp)?;
    }
    crate::player::reveal(&resolved)
}
pub fn open(database: &Database, id: i64) -> AppResult<ComicOpenResult> {
    database.read_snapshot(|c|{
        let (book,_,_,_,_,_)=validate_book(c,id)?;
        let mut s=c.prepare("SELECT page_index,page_name FROM comic_pages WHERE comic_book_id=?1 ORDER BY page_index").map_err(|e|e.to_string())?;
        let pages=s.query_map([id],|r|Ok(ComicPage{page_index:r.get(0)?,page_name:r.get(1)?})).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        Ok(ComicOpenResult{book,pages,bookmarks:bookmarks_conn(c,id)?})
    })
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
        validate_image(&bytes,&comics::extension(Path::new(&locator)))?;
        Ok(bytes)
    })
}

pub(crate) fn validate_image(bytes: &[u8], extension: &str) -> AppResult<()> {
    image_dimensions(bytes, extension).map(|_| ())
}

pub(crate) fn image_dimensions(bytes: &[u8], extension: &str) -> AppResult<(u32, u32)> {
    let bad = || "COMIC_IMAGE_INVALID".to_string();
    let (width, height) = match extension {
        "jpg" | "jpeg" if bytes.starts_with(&[255, 216, 255]) => {
            return crate::cache::cover_payload_dimensions(bytes).map_err(|_| bad());
        }
        "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => {
            return crate::cache::cover_payload_dimensions(bytes).map_err(|_| bad());
        }
        "webp" if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" => {
            return crate::cache::cover_payload_dimensions(bytes).map_err(|_| bad());
        }
        "gif"
            if bytes.len() >= 10
                && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) =>
        {
            (
                u32::from(u16::from_le_bytes([bytes[6], bytes[7]])),
                u32::from(u16::from_le_bytes([bytes[8], bytes[9]])),
            )
        }
        "bmp" if bytes.len() >= 26 && bytes.starts_with(b"BM") => {
            let width = i32::from_le_bytes(bytes[18..22].try_into().map_err(|_| bad())?)
                .checked_abs()
                .ok_or_else(bad)? as u32;
            let height = i32::from_le_bytes(bytes[22..26].try_into().map_err(|_| bad())?)
                .checked_abs()
                .ok_or_else(bad)? as u32;
            (width, height)
        }
        "avif"
            if bytes.len() >= 24
                && &bytes[4..8] == b"ftyp"
                && bytes[8..32.min(bytes.len())]
                    .chunks_exact(4)
                    .any(|b| b == b"avif" || b == b"avis") =>
        {
            avif_dimensions(bytes, 0)?.ok_or_else(bad)?
        }
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
   Some("EPUB")=>{let (locator,page_size,crc):(String,u64,Option<u32>)=c.query_row("SELECT source_locator,file_size,crc32 FROM comic_pages WHERE comic_book_id=?1 AND page_index=?2",params![id,index],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_|"COMIC_PAGE_CHANGED")?;serde_json::to_vec(&crate::ebooks::chapter(file.try_clone().map_err(|_|"COMIC_READ_FAILED")?,&locator,page_size,crc)?).map_err(|_|"COMIC_DOCUMENT_INVALID".to_string())},
   _=>Err("COMIC_DOCUMENT_INVALID".into()),
  }?;
  let after=file.metadata().map_err(|_|"COMIC_READ_FAILED")?;
  if after.len()!=size||comics::modified(&after)!=stamp{return Err("COMIC_PAGE_CHANGED".into());}Ok(result)
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
pub fn progress_at_revision(
    database: &Database,
    id: i64,
    index: i64,
    expected: Option<&str>,
) -> AppResult<()> {
    let mut c = database.connect()?;
    let tx = c.transaction().map_err(|e| e.to_string())?;
    validate_page(&tx, id, index)?;
    if let Some(revision) = expected {
        if validate_book(&tx, id)?.0.revision != revision {
            return Err("COMIC_PAGE_CHANGED".into());
        }
    }
    tx.execute("INSERT INTO comic_reading_progress(comic_book_id,last_page_index) VALUES(?1,?2) ON CONFLICT(comic_book_id) DO UPDATE SET last_page_index=excluded.last_page_index,last_read_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",params![id,index]).map_err(|_|"COMIC_PROGRESS_FAILED".to_string())?;
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
    let result = bookmarks_conn(&tx, id)?;
    tx.commit()
        .map_err(|_| "COMIC_BOOKMARK_FAILED".to_string())?;
    Ok(result)
}

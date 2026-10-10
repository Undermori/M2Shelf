//! Application-owned comic indexing; source trees are always read-only.
use crate::{
    db::{self, AppResult},
    models::NodeType,
    scanner::{self, ScanAbort, ScanControl, ScanTarget},
};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use tauri::AppHandle;

pub const MAX_PAGE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PAGES: usize = 10_000;
/// Book volumes and named continuations are not animation seasons.
pub fn book_volume(raw: &str) -> Option<u16> {
    static VOLUME: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = VOLUME.get_or_init(|| regex::Regex::new(r"(?i)(?:\bvol(?:ume)?[.\s_-]*|第?\s*)([0-9０-９零〇一二三四五六七八九十百两兩]{1,6})\s*[卷巻册冊]|\bvol(?:ume)?[.\s_-]*([0-9]{1,3})|[（(]\s*([0-9０-９]{1,3})\s*[）)]|[卷巻册冊]\s*([0-9０-９零〇一二三四五六七八九十百两兩]{1,6})").expect("book volume pattern"));
    let captures = pattern.captures(raw)?;
    let value = (1..=4).find_map(|i| captures.get(i))?.as_str();
    let normalized = value
        .chars()
        .map(|c| {
            if ('０'..='９').contains(&c) {
                char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap_or(c)
            } else {
                c
            }
        })
        .collect::<String>();
    if normalized.chars().all(|c| c.is_ascii_digit()) {
        return normalized.parse().ok();
    }
    let (mut total, mut pending) = (0_u16, 0_u16);
    for c in normalized.chars() {
        match c {
            '十' | '百' => {
                total = total.checked_add(pending.max(1).checked_mul(if c == '十' {
                    10
                } else {
                    100
                })?)?;
                pending = 0;
            }
            _ => {
                let digit = match c {
                    '零' | '〇' => 0,
                    '一' => 1,
                    '二' | '两' | '兩' => 2,
                    '三' => 3,
                    '四' => 4,
                    '五' => 5,
                    '六' => 6,
                    '七' => 7,
                    '八' => 8,
                    '九' => 9,
                    _ => return None,
                };
                pending = pending.checked_mul(10)?.checked_add(digit)?;
            }
        }
    }
    total.checked_add(pending).filter(|value| *value <= 999)
}
pub fn book_is_re(raw: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"(?i)(?:^|[:：_\s-]|[\p{Han}\p{Hiragana}\p{Katakana}])re(?:$|[\s_.:：(（0-9])",
        )
        .expect("book continuation pattern")
    })
    .is_match(raw)
}
pub fn clean_title(raw: &str) -> String {
    let stem = if matches!(extension(Path::new(raw)).as_str(), "cbz" | "zip")
        || crate::ebooks::format(Path::new(raw)).is_some()
    {
        Path::new(raw)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(raw)
    } else {
        raw
    };
    let mut title = stem.to_string();
    // Comic release qualifiers are not anime seasons or parts of the work's title.
    static QUALIFIERS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let qualifiers = QUALIFIERS.get_or_init(|| regex::Regex::new(
        r"(?i)[（(\[【](?:完结|完結|全集|全巻|全卷|complete|completed|end|[0-9０-９]{1,3})[）)\]】]|(?:[_\s-]*第?\s*[0-9０-９一二三四五六七八九十百]+\s*(?:话|話|回|册|冊))\s*$|(?:\s+(?:漫画|漫畫|manga|comic))\s*$"
    ).expect("fixed comic title pattern"));
    title = qualifiers.replace_all(&title, "").into_owned();
    let lower = title.to_ascii_lowercase();
    for marker in ["volume", "vol"] {
        if let Some(i) = lower.find(marker) {
            let tail = lower[i + marker.len()..]
                .trim_start_matches(|c: char| c.is_whitespace() || "._-[(".contains(c));
            if (i == 0 || lower[..i].ends_with([' ', '_', '-', '[', '(']))
                && tail
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit() || "ivx一二三四五六七八九十".contains(c))
            {
                title.truncate(i);
                break;
            }
        }
    }
    for marker in ['卷', '巻'] {
        if let Some(end) = title.find(marker) {
            let prefix = &title[..end];
            let start = prefix.rfind('第').unwrap_or_else(|| {
                prefix
                    .char_indices()
                    .rev()
                    .take_while(|(_, c)| {
                        c.is_ascii_digit() || "一二三四五六七八九十百上下 ".contains(*c)
                    })
                    .last()
                    .map_or(end, |(i, _)| i)
            });
            let marker_end = end + marker.len_utf8();
            let following = title[marker_end..]
                .chars()
                .take_while(|c| c.is_ascii_digit() || "一二三四五六七八九十百上下".contains(*c))
                .map(char::len_utf8)
                .sum::<usize>();
            if start < end || following > 0 {
                title.replace_range(start..marker_end + following, "");
            }
        }
    }
    crate::title_extractor::extract_search_keyword(
        title.trim_matches(|c: char| c.is_whitespace() || "_-.[]()".contains(c)),
    )
}
#[cfg(test)]
pub fn match_evidence(
    folder: &str,
    display: &str,
    parent: Option<&str>,
) -> crate::title_extractor::MatchEvidence {
    match_evidence_with_books(folder, display, parent, &[])
}
pub fn match_evidence_with_books(
    folder: &str,
    display: &str,
    parent: Option<&str>,
    books: &[String],
) -> crate::title_extractor::MatchEvidence {
    let mut volume = book_volume(folder).or_else(|| book_volume(display));
    let mut continuation = book_is_re(folder) || book_is_re(display);
    let folder = clean_title(folder);
    let display = clean_title(display);
    let parent = parent.map(clean_title);
    let generic = |s: &str| {
        crate::title_extractor::is_generic_title(s)
            || matches!(
                s.to_ascii_lowercase().as_str(),
                "manga"
                    | "comic"
                    | "comics"
                    | "漫画"
                    | "漫画库"
                    | "漫畫"
                    | "小说"
                    | "小說"
                    | "电子书"
                    | "電子書籍"
                    | "ebooks"
                    | "books"
                    | "novels"
            )
    };
    let cleaned_books = books
        .iter()
        .take(32)
        .map(|s| clean_title(s))
        .filter(|s| !generic(s) && !s.is_empty())
        .collect::<Vec<_>>();
    // A generic single-book directory may rely on its file name, not its parent series.
    // Multiple editions in one directory must not inherit the first book's volume.
    if (generic(&folder) || folder.is_empty()) && books.len() == 1 {
        volume = volume.or_else(|| book_volume(&books[0]));
        continuation |= book_is_re(&books[0]);
    }
    let primary = if generic(&folder) || folder.is_empty() {
        cleaned_books
            .first()
            .map(String::as_str)
            .or(parent.as_deref())
            .unwrap_or(&folder)
    } else {
        &folder
    };
    let display = if generic(&display) || display.is_empty() {
        primary
    } else {
        &display
    };
    let mut evidence = crate::title_extractor::build_match_evidence(
        primary,
        display,
        parent.as_deref(),
        &cleaned_books,
    );
    evidence.season_number = None;
    evidence.edition_kind = crate::title_extractor::EditionKind::Unknown;
    // Collection/download years are not publication years for serialized books.
    evidence.year_is_strong = false;
    // Do not let a parent-series exact match overwhelm the actual child edition.
    if volume.is_some() || continuation {
        let mut specific = if primary.eq_ignore_ascii_case("re") {
            parent.as_deref().unwrap_or(primary).to_string()
        } else {
            primary.to_string()
        };
        if continuation && !book_is_re(&specific) {
            specific.push_str(" :re");
        }
        // Generic animation extraction may have discarded a continuation or volume.
        if let Some(volume) = volume {
            specific = format!("{} ({volume})", specific.trim());
        }
        evidence.primary_title = specific.clone();
        evidence.alternate_titles = vec![specific];
        evidence.parent_title = None;
        evidence.frequent_file_title = None;
        evidence.season_number = None;
        evidence.evidence_quality = evidence.evidence_quality.max(60);
    }
    evidence
}
const MAX_ARCHIVE_ENTRIES: usize = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPosition {
    pub block_index: i64,
    pub character_offset: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicProgress {
    pub comic_book_id: i64,
    pub last_page_index: i64,
    pub last_read_at: String,
    pub text_position: Option<TextPosition>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicBook {
    pub source_size: u64,
    pub source_path: String,
    pub document_format: Option<String>,
    pub revision: String,
    pub id: i64,
    pub node_id: i64,
    pub source_kind: String,
    pub display_name: String,
    pub page_count: i64,
    pub modified_at: String,
    pub index_error: Option<String>,
    pub progress: Option<ComicProgress>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicPage {
    pub page_index: i64,
    pub page_name: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicOpenResult {
    pub book: ComicBook,
    pub pages: Vec<ComicPage>,
    pub bookmarks: Vec<i64>,
    pub bookmark_positions: std::collections::BTreeMap<i64, TextPosition>,
    pub navigation: Vec<BookNavigation>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookNavigation {
    pub page_index: i64,
    pub title: String,
    pub fragment: Option<String>,
}

pub(crate) struct IndexedPage {
    pub name: String,
    pub locator: String,
    pub size: u64,
    pub modified: String,
    pub crc: Option<u32>,
}

pub fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|p| p.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}
pub fn is_image(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif" | "bmp"
    )
}
pub fn is_archive(path: &Path) -> bool {
    extension(path) == "cbz"
}
pub fn modified(metadata: &fs::Metadata) -> String {
    metadata
        .modified()
        .map(DateTime::<Utc>::from)
        .map(|d| d.to_rfc3339_opts(SecondsFormat::Nanos, true))
        .unwrap_or_default()
}
pub fn valid_entry(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 2048
        && !name.starts_with('/')
        && !name.contains(['\\', ':', '\0'])
        && name.split('/').all(|p| p != ".." && p != ".")
}
fn sort_pages(pages: &mut [IndexedPage]) {
    pages.sort_by(|a, b| db::natural_cmp(&a.name, &b.name).then_with(|| a.locator.cmp(&b.locator)));
}

/// Bound central-directory allocation before the ZIP library parses entry metadata.
pub(crate) fn open_archive(mut file: File) -> AppResult<zip::ZipArchive<File>> {
    let damaged = || "COMIC_ARCHIVE_DAMAGED".to_string();
    let length = file.metadata().map_err(|_| damaged())?.len();
    let tail_size = length.min(65_557) as usize;
    file.seek(SeekFrom::End(-(tail_size as i64)))
        .map_err(|_| damaged())?;
    let mut tail = vec![0; tail_size];
    file.read_exact(&mut tail).map_err(|_| damaged())?;
    let end = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| {
            &tail[i..i + 4] == b"PK\x05\x06"
                && i + 22 + usize::from(u16::from_le_bytes([tail[i + 20], tail[i + 21]]))
                    == tail.len()
        })
        .ok_or_else(damaged)?;
    let eocd = &tail[end..];
    let word = |i| u16::from_le_bytes([eocd[i], eocd[i + 1]]);
    let dword = |i| u32::from_le_bytes(eocd[i..i + 4].try_into().unwrap());
    if word(4) != 0 || word(6) != 0 || word(8) != word(10) {
        return Err(damaged());
    }
    let end_offset = length - tail_size as u64 + end as u64;
    let (count, central_size, central_offset, central_end) =
        if word(10) == u16::MAX || dword(12) == u32::MAX || dword(16) == u32::MAX {
            let locator_offset = end_offset.checked_sub(20).ok_or_else(damaged)?;
            file.seek(SeekFrom::Start(locator_offset))
                .map_err(|_| damaged())?;
            let mut locator = [0; 20];
            file.read_exact(&mut locator).map_err(|_| damaged())?;
            if &locator[..4] != b"PK\x06\x07"
                || locator[4..8] != [0; 4]
                || locator[16..20] != [1, 0, 0, 0]
            {
                return Err(damaged());
            }
            let record_offset = u64::from_le_bytes(locator[8..16].try_into().unwrap());
            if record_offset
                .checked_add(56)
                .is_none_or(|end| end > locator_offset)
            {
                return Err(damaged());
            }
            file.seek(SeekFrom::Start(record_offset))
                .map_err(|_| damaged())?;
            let mut record = [0; 56];
            file.read_exact(&mut record).map_err(|_| damaged())?;
            let value = |i| u64::from_le_bytes(record[i..i + 8].try_into().unwrap());
            if &record[..4] != b"PK\x06\x06"
                || value(4) < 44
                || record_offset
                    .checked_add(12)
                    .and_then(|v| v.checked_add(value(4)))
                    != Some(locator_offset)
                || record[16..24] != [0; 8]
                || value(24) != value(32)
            {
                return Err(damaged());
            }
            (value(32), value(40), value(48), record_offset)
        } else {
            (
                u64::from(word(10)),
                u64::from(dword(12)),
                u64::from(dword(16)),
                end_offset,
            )
        };
    if count > MAX_ARCHIVE_ENTRIES as u64 || central_size > 16 * 1024 * 1024 {
        return Err("COMIC_PAGE_LIMIT".into());
    }
    if central_offset
        .checked_add(central_size)
        .is_none_or(|end| end > central_end)
    {
        return Err(damaged());
    }
    // A forged small declared size must not hide large extra fields from the parser.
    let limit = central_offset + central_size;
    let mut position = central_offset;
    for _ in 0..count {
        if position.checked_add(46).is_none_or(|end| end > limit) {
            return Err(damaged());
        }
        file.seek(SeekFrom::Start(position))
            .map_err(|_| damaged())?;
        let mut header = [0; 46];
        file.read_exact(&mut header).map_err(|_| damaged())?;
        if &header[..4] != b"PK\x01\x02" {
            return Err(damaged());
        }
        let lengths = [28, 30, 32]
            .into_iter()
            .map(|i| u64::from(u16::from_le_bytes([header[i], header[i + 1]])))
            .sum::<u64>();
        position = position
            .checked_add(46 + lengths)
            .filter(|end| *end <= limit)
            .ok_or_else(damaged)?;
    }
    file.seek(SeekFrom::Start(0)).map_err(|_| damaged())?;
    let archive = zip::ZipArchive::new(file).map_err(|_| damaged())?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("COMIC_PAGE_LIMIT".into());
    }
    Ok(archive)
}
pub(crate) fn archive_pages(
    path: &Path,
    control: Option<&ScanControl>,
) -> AppResult<Vec<IndexedPage>> {
    let file = File::open(path).map_err(|_| "COMIC_READ_FAILED".to_string())?;
    archive_pages_file(file, control)
}
pub(crate) fn archive_pages_file(
    file: File,
    control: Option<&ScanControl>,
) -> AppResult<Vec<IndexedPage>> {
    let mut zip = open_archive(file)?;
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        return Err("COMIC_PAGE_LIMIT".into());
    }
    let mut pages = Vec::new();
    let mut names = HashSet::new();
    for index in 0..zip.len() {
        if control.is_some_and(|c| c.cancel.load(std::sync::atomic::Ordering::Relaxed)) {
            return Err("COMIC_SCAN_CANCELLED".into());
        }
        let entry = zip
            .by_index_raw(index)
            .map_err(|_| "COMIC_ARCHIVE_DAMAGED".to_string())?;
        let name = entry.name();
        if !valid_entry(name)
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("COMIC_ARCHIVE_PATH".into());
        }
        if entry.encrypted() {
            return Err("COMIC_ARCHIVE_ENCRYPTED".into());
        }
        if entry.is_dir()
            || name.split('/').any(|s| s.eq_ignore_ascii_case("__MACOSX"))
            || !is_image(Path::new(name))
        {
            continue;
        }
        if !names.insert(name.to_lowercase()) {
            return Err("COMIC_ARCHIVE_DAMAGED".into());
        }
        if entry.size() > MAX_PAGE_BYTES {
            return Err("COMIC_PAGE_LIMIT".into());
        }
        pages.push(IndexedPage {
            name: name.to_string(),
            locator: name.to_string(),
            size: entry.size(),
            modified: String::new(),
            crc: Some(entry.crc32()),
        });
        if pages.len() > MAX_PAGES {
            return Err("COMIC_PAGE_LIMIT".into());
        }
    }
    if pages.is_empty() {
        return Err("COMIC_NO_PAGES".into());
    }
    sort_pages(&mut pages);
    Ok(pages)
}

struct ComicScan<'a> {
    app: Option<&'a AppHandle>,
    connection: &'a Connection,
    root_id: i64,
    canonical_root: &'a Path,
    control: &'a ScanControl,
    token: &'a str,
    visited: HashSet<PathBuf>,
    flat_root: Option<i64>,
}

pub(crate) fn scan_library(
    app: Option<&AppHandle>,
    connection: &Connection,
    target: &ScanTarget,
    root: &Path,
    control: &ScanControl,
    token: &str,
) -> Result<(), ScanAbort> {
    let mut scan = ComicScan {
        app,
        connection,
        root_id: target.root.id,
        canonical_root: root,
        control,
        token,
        visited: HashSet::new(),
        flat_root: None,
    };
    if matches!(
        target.root.recognition_mode,
        crate::models::LibraryRecognitionMode::VideoFile
    ) {
        scan.flat_root = Some(scanner::upsert_node(
            connection,
            target.root.id,
            None,
            &target.root.path,
            &target.root.display_name,
            token,
        )?);
    }
    let before = control.progress().errors;
    scan.directory(&target.path, target.parent_node_id)?;
    if let Some(root_id) = scan.flat_root {
        scanner::check_cancel(control)?;
        // Flat mode has one cleanup boundary. Failed/cancelled traversals retain unread rows.
        if before == control.progress().errors {
            connection.execute("DELETE FROM comic_books WHERE source_resource_id IS NULL AND node_id IN (SELECT id FROM nodes WHERE library_root_id=?1) AND last_seen_at<>?2 AND node_id NOT IN (SELECT id FROM nodes WHERE node_type='IGNORED')", params![target.root.id,token]).map_err(|e|e.to_string())?;
            connection
                .execute(
                    "DELETE FROM resource_files WHERE node_id IN (SELECT id FROM nodes WHERE library_root_id=?1 AND node_type<>'IGNORED') AND last_seen_at<>?2",
                    params![target.root.id, token],
                )
                .map_err(|e| e.to_string())?;
            connection
                .execute(
                    "DELETE FROM nodes WHERE library_root_id=?1 AND id<>?2 AND last_seen_at<>?3",
                    params![target.root.id, root_id, token],
                )
                .map_err(|e| e.to_string())?;
        }
        refresh_counts(connection, root_id)?;
    }
    let mut parent = target.parent_node_id;
    while let Some(id) = parent {
        refresh_counts(connection, id)?;
        parent = connection
            .query_row("SELECT parent_node_id FROM nodes WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

impl ComicScan<'_> {
    fn directory(&mut self, path: &Path, parent: Option<i64>) -> Result<(), ScanAbort> {
        scanner::check_cancel(self.control)?;
        let canonical = scanner::canonicalize_within_library_root(path, self.canonical_root)?;
        if !canonical.is_dir() || !self.visited.insert(canonical.clone()) {
            return Ok(());
        }
        if self.flat_root.is_some()
            && self
                .control
                .unchanged_directories
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains(path)
        {
            self.retain_flat_subtree(path)?;
            return Ok(());
        }
        let existing=self.connection.query_row("SELECT id,node_type FROM nodes WHERE library_root_id=?1 AND absolute_path=?2 COLLATE NOCASE",params![self.root_id,path.to_string_lossy()],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).optional().map_err(|e|e.to_string())?;
        if let Some((id, kind)) = &existing {
            if kind == "IGNORED"
                || (self.flat_root.is_none()
                    && self
                        .control
                        .unchanged_directories
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .contains(path))
            {
                self.connection
                    .execute(
                        "UPDATE nodes SET last_seen_at=?1 WHERE id=?2",
                        params![self.token, id],
                    )
                    .map_err(|e| e.to_string())?;
                if self.flat_root.is_some() {
                    self.retain_flat_subtree(path)?;
                }
                return Ok(());
            }
        }
        // Observe the complete directory before pruning anything. Failed reads retain old rows.
        let entries = fs::read_dir(&canonical)
            .map_err(|_| "COMIC_READ_FAILED".to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "COMIC_READ_FAILED".to_string())?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Library");
        let image_directory = entries.iter().any(|entry| {
            is_image(&entry.path()) && entry.file_type().is_ok_and(|kind| kind.is_file())
        });
        let id = if let Some(root_id) = self
            .flat_root
            .filter(|_| !image_directory || canonical == self.canonical_root)
        {
            root_id
        } else {
            scanner::upsert_node(
                self.connection,
                self.root_id,
                self.flat_root.or(parent),
                &path.to_string_lossy(),
                name,
                self.token,
            )?
        };
        scanner::update_progress(self.app, self.control, path, |p| p.folders_scanned += 1);
        let before = self.control.progress().errors;
        let mut pages = Vec::new();
        let mut directories = Vec::new();
        let mut archives = Vec::new();
        for entry in entries {
            scanner::check_cancel(self.control)?;
            if entry
                .file_type()
                .map_err(|_| "COMIC_READ_FAILED".to_string())?
                .is_symlink()
            {
                continue;
            }
            let entry_name = entry.file_name().to_string_lossy().to_lowercase();
            if matches!(
                entry_name.as_str(),
                "__macosx" | ".ds_store" | "thumbs.db" | "desktop.ini"
            ) {
                continue;
            }
            let logical = path.join(entry.file_name());
            let filesystem =
                match scanner::canonicalize_within_library_root(&logical, self.canonical_root) {
                    Ok(value) => value,
                    Err(_) => {
                        self.error(&logical, "COMIC_PATH_OUTSIDE_ROOT");
                        continue;
                    }
                };
            let metadata =
                fs::metadata(&filesystem).map_err(|_| "COMIC_READ_FAILED".to_string())?;
            if metadata.is_dir() {
                directories.push(logical);
            } else if metadata.is_file() && is_image(&logical) {
                if metadata.len() > MAX_PAGE_BYTES || pages.len() >= MAX_PAGES {
                    self.error(path, "COMIC_PAGE_LIMIT");
                    continue;
                }
                pages.push(IndexedPage {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    locator: logical.to_string_lossy().into_owned(),
                    size: metadata.len(),
                    modified: modified(&metadata),
                    crc: None,
                });
            } else if metadata.is_file()
                && (is_archive(&logical) || crate::ebooks::format(&logical).is_some())
            {
                archives.push((logical, filesystem, metadata));
            } else if metadata.is_file() {
                scanner::index_resource_file(
                    self.connection,
                    id,
                    &logical,
                    &filesystem,
                    self.token,
                )?;
            }
        }
        if !pages.is_empty() && before == self.control.progress().errors {
            sort_pages(&mut pages);
            let latest = pages
                .iter()
                .map(|p| p.modified.as_str())
                .max()
                .unwrap_or_default()
                .to_string();
            let size = pages.iter().map(|p| p.size).sum();
            store_book(
                self.connection,
                id,
                path,
                "IMAGE_FOLDER",
                name,
                size,
                &latest,
                Ok(pages),
                self.token,
            )?;
            scanner::update_progress(self.app, self.control, path, |p| p.comic_books_found += 1);
        }
        for (logical, filesystem, metadata) in archives {
            let book_node = if let Some(root_id) = self.flat_root {
                let node = scanner::upsert_node(
                    self.connection,
                    self.root_id,
                    Some(root_id),
                    &logical.to_string_lossy(),
                    logical
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default(),
                    self.token,
                )?;
                if db::get_node_conn(self.connection, node)?.node_type == NodeType::Ignored {
                    continue;
                }
                node
            } else {
                id
            };
            scanner::update_progress(self.app, self.control, &logical, |p| {
                p.comic_books_found += 1
            });
            scanner::check_cancel(self.control)?;
            let stamp = modified(&metadata);
            // Previously unsupported files may have an attachment reader under
            // the hidden Root. Individual-file recognition now owns that same
            // book through its file Node; keep its ID before attachment cleanup.
            self.connection.execute("UPDATE comic_books SET node_id=?1,source_resource_id=NULL,source_resource_stamp=NULL WHERE source_path=?2 COLLATE NOCASE AND source_resource_id IS NOT NULL AND node_id IN (SELECT id FROM nodes WHERE library_root_id=?3)",params![book_node,logical.to_string_lossy(),self.root_id]).map_err(|e|e.to_string())?;
            let unchanged=self.connection.query_row("SELECT id FROM comic_books WHERE node_id=?1 AND source_path=?2 COLLATE NOCASE AND file_size=?3 AND modified_at=?4 AND index_error IS NULL",params![book_node,logical.to_string_lossy(),metadata.len() as i64,stamp],|r|r.get::<_,i64>(0)).optional().map_err(|e|e.to_string())?;
            if let Some(book) = unchanged {
                self.connection
                    .execute(
                        "UPDATE comic_books SET last_seen_at=?1,source_resource_id=NULL,source_resource_stamp=NULL WHERE id=?2",
                        params![self.token, book],
                    )
                    .map_err(|e| e.to_string())?;
            } else {
                let document_format = crate::ebooks::format(&logical);
                let indexed = if document_format.is_some() {
                    crate::ebooks::index(&filesystem)
                } else {
                    archive_pages(&filesystem, Some(self.control))
                };
                scanner::check_cancel(self.control)?;
                if let Err(error) = &indexed {
                    self.error(&logical, error);
                }
                store_book(
                    self.connection,
                    book_node,
                    &logical,
                    "ZIP_ARCHIVE",
                    logical
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default(),
                    metadata.len(),
                    &stamp,
                    indexed,
                    self.token,
                )?;
                self.connection.execute("UPDATE comic_books SET document_format=?1,reader_format=?4,source_resource_id=NULL,source_resource_stamp=NULL WHERE node_id=?2 AND source_path=?3 COLLATE NOCASE",params![document_format.filter(|format| matches!(*format,"PDF"|"EPUB")),book_node,logical.to_string_lossy(),document_format]).map_err(|e|e.to_string())?;
            }
            if crate::ebooks::format(&logical) == Some("TXT") {
                self.connection.execute("UPDATE comic_books SET text_encoding=COALESCE((SELECT substr(source_locator,1,instr(source_locator,':')-1) FROM comic_pages WHERE comic_book_id=comic_books.id AND page_index=0),'UTF-8') WHERE node_id=?1 AND source_path=?2 COLLATE NOCASE", params![book_node,logical.to_string_lossy()]).map_err(|e|e.to_string())?;
            }
            if self.flat_root.is_some() {
                refresh_counts(self.connection, book_node)?;
            }
        }
        directories.sort_by(|a, b| {
            db::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()).then_with(|| a.cmp(b))
        });
        for child in directories {
            if let Err(error) = self.directory(&child, Some(id)) {
                if matches!(error, ScanAbort::Cancelled) {
                    return Err(error);
                }
                self.error(&child, "COMIC_READ_FAILED");
            }
        }
        scanner::check_cancel(self.control)?;
        if self.flat_root.is_none() && before == self.control.progress().errors {
            for (table, column) in [
                ("comic_books", "node_id"),
                ("resource_files", "node_id"),
                ("nodes", "parent_node_id"),
            ] {
                self.connection
                    .execute(
                        &format!("DELETE FROM {table} WHERE {column}=?1 AND last_seen_at<>?2"),
                        params![id, self.token],
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
        refresh_counts(self.connection, id)?;
        Ok(())
    }
    fn error(&self, path: &Path, message: &str) {
        scanner::update_progress(self.app, self.control, path, |p| {
            p.errors += 1;
            p.message = Some(message.into());
        });
    }
    fn retain_flat_subtree(&self, path: &Path) -> AppResult<()> {
        let value = path.to_string_lossy().replace('\\', "/");
        let prefix = format!("{}/", value.trim_end_matches('/'));
        for (table, column, scope) in [
            ("nodes", "absolute_path", "library_root_id=?4"),
            (
                "comic_books",
                "source_path",
                "node_id IN (SELECT id FROM nodes WHERE library_root_id=?4)",
            ),
            (
                "resource_files",
                "absolute_path",
                "node_id IN (SELECT id FROM nodes WHERE library_root_id=?4)",
            ),
        ] {
            self.connection.execute(&format!("UPDATE {table} SET last_seen_at=?1 WHERE {scope} AND (replace({column},'\\','/')=?2 COLLATE NOCASE OR lower(substr(replace({column},'\\','/'),1,length(?3)))=lower(?3))"),params![self.token,value,prefix,self.root_id]).map_err(|e|e.to_string())?;
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn store_book(
    connection: &Connection,
    node_id: i64,
    path: &Path,
    kind: &str,
    name: &str,
    size: u64,
    modified_at: &str,
    pages: AppResult<Vec<IndexedPage>>,
    token: &str,
) -> AppResult<()> {
    let tx = connection
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    store_book_rows(
        &tx,
        node_id,
        path,
        kind,
        name,
        size,
        modified_at,
        pages,
        token,
    )?;
    tx.commit().map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn store_book_rows(
    tx: &Connection,
    node_id: i64,
    path: &Path,
    kind: &str,
    name: &str,
    size: u64,
    modified_at: &str,
    pages: AppResult<Vec<IndexedPage>>,
    token: &str,
) -> AppResult<()> {
    tx.execute("INSERT INTO comic_books(node_id,source_path,source_kind,display_name,file_size,modified_at,last_seen_at,index_error) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(node_id,source_path) DO UPDATE SET display_name=excluded.display_name,file_size=excluded.file_size,modified_at=excluded.modified_at,last_seen_at=excluded.last_seen_at,index_error=excluded.index_error",params![node_id,path.to_string_lossy(),kind,name,size as i64,modified_at,token,pages.as_ref().err()]).map_err(|e|e.to_string())?;
    let id: i64 = tx
        .query_row(
            "SELECT id FROM comic_books WHERE node_id=?1 AND source_path=?2 COLLATE NOCASE",
            params![node_id, path.to_string_lossy()],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if let Ok(pages) = pages {
        use sha2::{Digest, Sha256};
        // Kindle chapter identity is the original source locator. Adapter fixes
        // may remove metadata-only chapters without moving the reader's content.
        let kindle = pages
            .first()
            .is_some_and(|page| page.locator.starts_with("kindle:"));
        let old_progress: Option<String> = if kindle {
            tx.query_row("SELECT p.source_locator FROM comic_reading_progress s JOIN comic_pages p ON p.comic_book_id=s.comic_book_id AND p.page_index=s.last_page_index WHERE s.comic_book_id=?1", [id], |r|r.get(0)).optional().map_err(|e|e.to_string())?
        } else {
            None
        };
        let old_marks: Vec<(i64, String, String, Option<i64>, i64)> = if kindle {
            let mut statement=tx.prepare("SELECT b.page_index,p.source_locator,b.created_at,b.text_block_index,b.text_character_offset FROM comic_bookmarks b JOIN comic_pages p ON p.comic_book_id=b.comic_book_id AND p.page_index=b.page_index WHERE b.comic_book_id=?1").map_err(|e|e.to_string())?;
            let marks = statement
                .query_map([id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            marks
        } else {
            Vec::new()
        };
        let mut hash = Sha256::new();
        hash.update(serde_json::to_vec(&(size, modified_at)).map_err(|e| e.to_string())?);
        for page in &pages {
            let value = serde_json::to_vec(&(
                page.locator.as_str(),
                page.size,
                page.modified.as_str(),
                page.crc,
            ))
            .map_err(|e| e.to_string())?;
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value);
        }
        let revision = format!("{:x}", hash.finalize());
        tx.execute("DELETE FROM comic_pages WHERE comic_book_id=?1", [id])
            .map_err(|e| e.to_string())?;
        for (index, page) in pages.iter().enumerate() {
            tx.execute("INSERT INTO comic_pages(comic_book_id,page_index,page_name,source_locator,file_size,modified_at,crc32) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,index as i64,page.name,page.locator,page.size as i64,page.modified,page.crc.map(i64::from)]).map_err(|e|e.to_string())?;
        }
        let count = pages.len() as i64;
        tx.execute(
            "UPDATE comic_books SET page_count=?1,revision=?3 WHERE id=?2",
            params![count, id, revision],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("UPDATE comic_reading_progress SET last_page_index=MIN(last_page_index,?1) WHERE comic_book_id=?2",params![(count-1).max(0),id]).map_err(|e|e.to_string())?;
        tx.execute(
            "DELETE FROM comic_bookmarks WHERE comic_book_id=?1 AND page_index>=?2",
            params![id, count],
        )
        .map_err(|e| e.to_string())?;
        if kindle {
            let locations: std::collections::HashMap<_, _> = pages
                .iter()
                .enumerate()
                .map(|(index, page)| (page.locator.as_str(), index as i64))
                .collect();
            if let Some(index) = old_progress
                .as_deref()
                .and_then(|locator| locations.get(locator))
            {
                tx.execute(
                    "UPDATE comic_reading_progress SET last_page_index=?1 WHERE comic_book_id=?2",
                    params![index, id],
                )
                .map_err(|e| e.to_string())?;
            }
            if !old_marks.is_empty() {
                tx.execute("DELETE FROM comic_bookmarks WHERE comic_book_id=?1", [id])
                    .map_err(|e| e.to_string())?;
                for (old_index, locator, created, block, offset) in old_marks {
                    let index = locations
                        .get(locator.as_str())
                        .copied()
                        .unwrap_or(old_index);
                    if index < count {
                        tx.execute("INSERT OR IGNORE INTO comic_bookmarks(comic_book_id,page_index,created_at,text_block_index,text_character_offset) VALUES(?1,?2,?3,?4,?5)",params![id,index,created,block,offset]).map_err(|e|e.to_string())?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn refresh_counts(connection: &Connection, id: i64) -> AppResult<()> {
    let direct: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM comic_books WHERE node_id=?1 AND source_resource_id IS NULL",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let (branches,children):(i64,i64)=connection.query_row("SELECT COUNT(*),COALESCE(SUM(total_comic_book_count),0) FROM nodes WHERE parent_node_id=?1 AND node_type<>'IGNORED' AND total_comic_book_count>0",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;
    let automatic = if direct > 0 && branches > 0 {
        NodeType::Mixed
    } else if direct > 0 {
        NodeType::AutoWork
    } else {
        NodeType::Container
    };
    connection.execute("UPDATE nodes SET node_type=CASE WHEN manual_type_override=1 THEN node_type ELSE ?1 END,direct_comic_book_count=?2,child_comic_branch_count=?3,total_comic_book_count=?4 WHERE id=?5",params![automatic.as_db(),direct,branches,direct+children,id]).map_err(|e|e.to_string())?;
    Ok(())
}

pub(crate) fn refresh_related(connection: &Connection, ids: &[i64]) -> AppResult<()> {
    for id in ids {
        let node = db::get_node_conn(connection, *id)?;
        if !node.media_kind.is_book() {
            continue;
        }
        refresh_counts(connection, *id)?;
        let mut parent = node.parent_node_id;
        while let Some(id) = parent {
            refresh_counts(connection, id)?;
            parent = db::get_node_conn(connection, id)?.parent_node_id;
        }
    }
    Ok(())
}

/// Detail tables flatten owned books from the indexed tree, never from disk.
/// Ignored and explicitly independent/bound editions remain separate destinations.
/// This follows the same anchor-binding/manual boundary rules as `owned_nodes`, starting
/// at each visible Root child. Collapsed volumes/format folders need no separate cover task.
/// Doujin libraries keep their per-image-folder first-page covers; ebooks are unchanged.
pub(crate) const PRESENTATION_CTE: &str = "WITH RECURSIVE comic_presentation(id,anchor) AS (
    SELECT n.id,NULL FROM nodes n JOIN library_roots r ON r.id=n.library_root_id
    WHERE n.parent_node_id IS NULL AND n.node_type<>'IGNORED'
      AND r.media_kind='COMIC' AND r.book_library_kind='COMIC' AND r.doujin_library=0 AND r.artbook_library=0
    UNION ALL
    SELECT n.id,CASE WHEN p.anchor IS NULL OR n.manual_type_override=1 OR
      (b.node_id IS NOT NULL AND (a.node_id IS NULL OR b.provider_subject_id<>a.provider_subject_id
        OR b.provider_subject_type<>a.provider_subject_type)) THEN n.id ELSE p.anchor END
    FROM nodes n JOIN comic_presentation p ON n.parent_node_id=p.id
    LEFT JOIN metadata_bindings b ON b.node_id=n.id
    LEFT JOIN metadata_bindings a ON a.node_id=p.anchor
    WHERE n.node_type<>'IGNORED'
)";

pub(crate) const POSTER_ELIGIBILITY: &str = "n.node_type<>'IGNORED'
    AND n.parent_node_id IS NOT NULL
    AND (n.cover_cache_path IS NOT NULL OR n.total_comic_book_count>0 OR EXISTS(SELECT 1 FROM tmdb_movie_bindings t WHERE t.node_id=n.id AND t.active=1 AND t.cover_cache_path IS NOT NULL))
    AND n.id NOT IN (SELECT id FROM comic_presentation WHERE anchor IS NOT NULL AND id<>anchor)";

pub fn owned_books(connection: &Connection, node_id: i64) -> AppResult<Vec<ComicBook>> {
    books_for_nodes(
        connection,
        &owned_nodes(connection, node_id)?
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        false,
    )
}

fn owned_nodes(
    connection: &Connection,
    node_id: i64,
) -> AppResult<std::collections::HashMap<i64, Option<i64>>> {
    let mut statement = connection.prepare("WITH RECURSIVE owned(id,parent_node_id) AS (
        SELECT id,parent_node_id FROM nodes WHERE id=?1 UNION ALL SELECT n.id,n.parent_node_id FROM nodes n JOIN owned o ON n.parent_node_id=o.id
        WHERE n.node_type<>'IGNORED' AND n.manual_type_override=0 AND (
          NOT EXISTS(SELECT 1 FROM metadata_bindings b WHERE b.node_id=n.id) OR
          EXISTS(SELECT 1 FROM metadata_bindings b JOIN metadata_bindings p ON p.node_id=?1
            WHERE b.node_id=n.id AND b.provider_subject_id=p.provider_subject_id AND b.provider_subject_type=p.provider_subject_type)))
        SELECT id,parent_node_id FROM owned").map_err(|e|e.to_string())?;
    let ids = statement
        .query_map([node_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<std::collections::HashMap<_, _>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(ids)
}

fn books_for_nodes(
    connection: &Connection,
    ids: &[i64],
    attachments: bool,
) -> AppResult<Vec<ComicBook>> {
    let mut result = Vec::new();
    for chunk in ids.chunks(500) {
        let sql = format!(
            "{} WHERE b.node_id IN ({}) AND lower(b.source_path) NOT LIKE '%.zip' {}",
            book_select(),
            vec!["?"; chunk.len()].join(","),
            if attachments {
                ""
            } else {
                "AND b.source_resource_id IS NULL"
            },
        );
        let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
        result.extend(
            statement
                .query_map(rusqlite::params_from_iter(chunk), book_from_row)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?,
        );
    }
    result.sort_by(|a, b| {
        db::natural_cmp(&a.source_path, &b.source_path).then_with(|| a.id.cmp(&b.id))
    });
    Ok(result)
}

/// Build the book table and remaining folders/resources in the caller's SQLite snapshot.
/// Expanded image collections and format folders are represented by their books once;
/// independent editions and non-book folders stay available as navigation destinations.
pub(crate) fn populate_detail(
    connection: &Connection,
    detail: &mut crate::models::NodeDetail,
) -> AppResult<()> {
    let node_id = detail.node.id;
    let owned = owned_nodes(connection, node_id)?;
    let books = books_for_nodes(connection, &owned.keys().copied().collect::<Vec<_>>(), true)?;
    let mut expanded = HashSet::from([node_id]);
    for book in &books {
        let mut next = Some(book.node_id);
        while let Some(id) = next {
            if !owned.contains_key(&id) || !expanded.insert(id) {
                break;
            }
            next = owned[&id];
        }
    }
    let expanded_ids = expanded.into_iter().collect::<Vec<_>>();
    let mut folders = db::list_remaining_children_conn(connection, &expanded_ids)?;
    let mut resources = db::list_resources_for_nodes_conn(connection, &expanded_ids)?;
    folders.sort_by(|a, b| db::natural_cmp(&a.folder_name, &b.folder_name).then(a.id.cmp(&b.id)));
    resources
        .sort_by(|a, b| db::natural_cmp(&a.absolute_path, &b.absolute_path).then(a.id.cmp(&b.id)));
    detail.children = folders;
    detail.resource_files = resources;
    detail.comic_books = Some(books);
    Ok(())
}

/// Include already opened file books without changing their ResourceFile ownership.
pub(crate) fn books_for_resources(
    connection: &Connection,
    resources: &[crate::models::ResourceFile],
) -> AppResult<Vec<ComicBook>> {
    let mut books = Vec::new();
    for chunk in resources.chunks(500) {
        let sql = format!(
            "{} WHERE b.source_resource_id IN ({})",
            book_select(),
            vec!["?"; chunk.len()].join(",")
        );
        let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
        books.extend(
            statement
                .query_map(
                    rusqlite::params_from_iter(chunk.iter().map(|file| file.id)),
                    book_from_row,
                )
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?,
        );
    }
    Ok(books)
}

pub fn books(connection: &Connection, node_id: i64) -> AppResult<Vec<ComicBook>> {
    let mut books = books_for_nodes(connection, &[node_id], true)?;
    books.sort_by(|a, b| {
        db::natural_cmp(&a.display_name, &b.display_name).then_with(|| a.id.cmp(&b.id))
    });
    Ok(books)
}

pub(crate) fn book_select() -> &'static str {
    "SELECT b.id,b.node_id,b.source_kind,b.display_name,b.page_count,b.modified_at,b.index_error,p.last_page_index,p.last_read_at,b.revision,COALESCE(b.reader_format,CASE WHEN b.text_encoding IS NOT NULL THEN 'TXT' ELSE b.document_format END),b.file_size,b.source_path,p.text_block_index,p.text_character_offset FROM comic_books b LEFT JOIN comic_reading_progress p ON p.comic_book_id=b.id"
}

pub(crate) fn book_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ComicBook> {
    let id = r.get(0)?;
    let page: Option<i64> = r.get(7)?;
    Ok(ComicBook {
        document_format: r.get(10)?,
        source_size: r.get(11)?,
        source_path: r.get(12)?,
        revision: r.get(9)?,
        id,
        node_id: r.get(1)?,
        source_kind: r.get(2)?,
        display_name: r.get(3)?,
        page_count: r.get(4)?,
        modified_at: r.get(5)?,
        index_error: r.get(6)?,
        progress: page.map(|last_page_index| ComicProgress {
            comic_book_id: id,
            last_page_index,
            last_read_at: r.get(8).unwrap_or_default(),
            text_position: r
                .get::<_, Option<i64>>(13)
                .ok()
                .flatten()
                .map(|block_index| TextPosition {
                    block_index,
                    character_offset: r.get(14).unwrap_or(0),
                }),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        comic_reader,
        db::Database,
        models::{LibraryMediaKind, LibraryRecognitionMode, ScanProgress},
        scanner::{self, ScanControl, ScanTarget},
    };
    use base64::Engine;
    use std::{
        io::Write,
        sync::{atomic::AtomicBool, Arc, Mutex},
    };

    fn image() -> Vec<u8> {
        base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jH3sAAAAASUVORK5CYII=").unwrap()
    }
    fn archive(path: &Path, names: &[&str]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for name in names {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&image()).unwrap();
        }
        zip.finish().unwrap();
    }
    fn fixture() -> (tempfile::TempDir, Database, crate::models::LibraryRoot) {
        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("媒体 Manga");
        fs::create_dir(&root_path).unwrap();
        let db = Database::new(temp.path().join("test.sqlite"));
        db.migrate().unwrap();
        let root = db
            .add_root_with_policy(
                &root_path,
                None,
                LibraryMediaKind::Comic,
                LibraryRecognitionMode::Folder,
                true,
            )
            .unwrap();
        (temp, db, root)
    }
    fn control(root_id: i64) -> ScanControl {
        let progress:ScanProgress=serde_json::from_value(serde_json::json!({"scanId":"comic-test","rootId":root_id,"currentPath":"","foldersScanned":0,"videosFound":0,"status":"RUNNING","errors":0,"message":null})).unwrap();
        ScanControl {
            unchanged_directories: Arc::new(Mutex::new(HashSet::new())),
            scan_id: "comic-test".into(),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(progress)),
        }
    }
    fn scan(db: &Database, root: &crate::models::LibraryRoot) -> ScanControl {
        let control = control(root.id);
        scanner::run_scan(
            None,
            db,
            vec![ScanTarget {
                root: root.clone(),
                path: PathBuf::from(&root.path),
                parent_node_id: None,
            }],
            &control,
            &["mp4".into()],
        );
        control
    }
    fn root_node(db: &Database, root_id: i64) -> i64 {
        db.connect()
            .unwrap()
            .query_row(
                "SELECT id FROM nodes WHERE library_root_id=?1 AND parent_node_id IS NULL",
                [root_id],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn doujin_uses_first_page_without_bangumi_and_txt_survives_rescan() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("同人 [Unicode]");
        fs::create_dir(&source).unwrap();
        let folder = source.join("Collection");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("10.png"), image()).unwrap();
        image::RgbaImage::from_pixel(32, 48, image::Rgba([20, 60, 90, 255]))
            .save(folder.join("2.png"))
            .unwrap();
        let first_page_bytes = fs::read(folder.join("2.png")).unwrap();
        let db = Database::new(temp.path().join("books.sqlite"));
        db.migrate().unwrap();
        let root = db
            .add_root_with_kind(
                &source,
                None,
                LibraryMediaKind::Doujin,
                LibraryRecognitionMode::Folder,
            )
            .unwrap();
        scan(&db, &root);
        assert_eq!(
            db.list_roots().unwrap()[0].media_kind,
            LibraryMediaKind::Doujin
        );
        let child = db.list_children(root_node(&db, root.id)).unwrap()[0].clone();
        assert!(child.can_bind_bangumi());
        assert!(!root.auto_bangumi);
        assert!(db
            .list_bangumi_match_candidates(Some(&[child.id]), false)
            .unwrap()
            .is_empty());
        assert_eq!(
            db.list_bangumi_match_candidates(Some(&[child.id]), true)
                .unwrap()
                .len(),
            1
        );
        let book = comic_reader::detail(&db, child.id).unwrap().remove(0);
        assert_eq!(
            comic_reader::open(&db, book.id).unwrap().pages[0].page_name,
            "2.png"
        );
        assert!(comic_reader::first_image_cover(&db, child.id)
            .unwrap()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        let poster_cache = temp.path().join("poster-cache");
        crate::cache::ensure_directories(&poster_cache).unwrap();
        let old_poster = crate::poster_cache::cover_data_url(&db, &poster_cache, child.id, 256)
            .unwrap()
            .unwrap();
        assert!(old_poster.starts_with("data:image/jpeg;base64,"));
        assert_eq!(
            crate::poster_cache::cover_data_url(
                &Database::new(db.path().into()),
                &poster_cache,
                child.id,
                256,
            )
            .unwrap()
            .unwrap(),
            old_poster
        );
        assert_eq!(fs::read(folder.join("2.png")).unwrap(), first_page_bytes);
        let changed_page = image::RgbaImage::from_pixel(48, 72, image::Rgba([180, 20, 30, 255]));
        changed_page.save(folder.join("2.png")).unwrap();
        let changed_bytes = fs::read(folder.join("2.png")).unwrap();
        scan(&db, &root);
        assert_ne!(
            crate::poster_cache::cover_data_url(&db, &poster_cache, child.id, 256)
                .unwrap()
                .unwrap(),
            old_poster
        );
        assert_eq!(fs::read(folder.join("2.png")).unwrap(), changed_bytes);
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 2);
        assert!(db
            .connect()
            .unwrap()
            .execute(
                "UPDATE library_roots SET doujin_library=0 WHERE id=?1",
                [root.id]
            )
            .is_err());

        let novels = temp.path().join("Novels");
        fs::create_dir(&novels).unwrap();
        let path = novels.join("长篇小说 [书].txt");
        let original = "小说段落，安全文本<script>文字</script>。\n".repeat(900);
        fs::write(&path, &original).unwrap();
        let ebook = db
            .add_root_with_kind(
                &novels,
                None,
                LibraryMediaKind::Ebook,
                LibraryRecognitionMode::VideoFile,
            )
            .unwrap();
        scan(&db, &ebook);
        let work = db
            .list_children(root_node(&db, ebook.id))
            .unwrap()
            .remove(0);
        let book = comic_reader::detail(&db, work.id).unwrap().remove(0);
        assert_eq!(book.document_format.as_deref(), Some("TXT"));
        assert!(book.page_count > 1);
        let bytes = comic_reader::read_document(&db, book.id, 1, &book.revision).unwrap();
        assert!(bytes.len() < 64 * 1024);
        let blocks: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(blocks[0]["kind"], "text");
        comic_reader::progress_at_revision(&db, book.id, 1, Some(&book.revision)).unwrap();
        comic_reader::bookmark_at_revision(&db, book.id, 1, true, Some(&book.revision)).unwrap();
        scan(&db, &ebook);
        let again = comic_reader::detail(&db, work.id).unwrap().remove(0);
        assert_eq!(again.id, book.id);
        assert_eq!(again.revision, book.revision);
        assert_eq!(again.progress.unwrap().last_page_index, 1);
        assert_eq!(comic_reader::open(&db, book.id).unwrap().bookmarks, vec![1]);
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert!(comic_reader::read_document(&db, book.id, 1, "stale").is_err());
        assert!(crate::works::node_detail(&db, work.id)
            .unwrap()
            .resource_files
            .is_empty());
    }

    #[test]
    fn individual_books_flatten_and_preserve_identity_progress_and_failures() {
        for kind in [
            LibraryMediaKind::Comic,
            LibraryMediaKind::Ebook,
            LibraryMediaKind::Doujin,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("Books [合集]");
            fs::create_dir(&path).unwrap();
            let db = Database::new(temp.path().join("flat.sqlite"));
            db.migrate().unwrap();
            let root = db
                .add_root_with_kind(&path, None, kind, LibraryRecognitionMode::VideoFile)
                .unwrap();
            let path = Path::new(&root.path);
            fs::create_dir_all(path.join("nested/pictures")).unwrap();
            archive(&path.join("nested/第01册.cbz"), &["1.png", "2.png"]);
            archive(&path.join("第02册.cbz"), &["1.png"]);
            fs::write(path.join("nested/pictures/1.png"), image()).unwrap();
            fs::write(path.join("nested/note.md"), "attachment").unwrap();
            assert_eq!(scan(&db, &root).progress().errors, 0);
            let root_id = root_node(&db, root.id);
            let c = db.connect().unwrap();
            let nodes: Vec<i64> = c
                .prepare("SELECT id FROM nodes WHERE parent_node_id=?1 ORDER BY id")
                .unwrap()
                .query_map([root_id], |r| r.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            assert_eq!(nodes.len(), 3);
            assert!(nodes
                .iter()
                .all(|id| db.get_node(*id).unwrap().node_type == NodeType::AutoWork));
            let books = owned_books(&c, root_id).unwrap();
            assert_eq!(books.len(), 3);
            let first = books.iter().find(|b| b.page_count == 2).unwrap();
            comic_reader::progress_at_revision(&db, first.id, 1, Some(&first.revision)).unwrap();
            comic_reader::bookmark_at_revision(&db, first.id, 1, true, Some(&first.revision))
                .unwrap();
            db.set_display_name(first.node_id, Some("My independent book".into()))
                .unwrap();
            assert_eq!(scan(&db, &root).progress().errors, 0);
            let again = comic_reader::open(&db, first.id).unwrap();
            assert_eq!(again.book.node_id, first.node_id);
            assert_eq!(again.book.progress.unwrap().last_page_index, 1);
            assert_eq!(again.bookmarks, vec![1]);
            assert_eq!(
                db.get_node(first.node_id).unwrap().display_name,
                "My independent book"
            );
            // A cancelled traversal cannot prune indexed sources.
            let cancelled = control(root.id);
            cancelled
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
            scanner::run_scan(
                None,
                &db,
                vec![ScanTarget {
                    root: root.clone(),
                    path: PathBuf::from(&root.path),
                    parent_node_id: None,
                }],
                &cancelled,
                &[],
            );
            assert_eq!(owned_books(&c, root_id).unwrap().len(), 3);
            fs::write(path.join("broken.epub"), "broken").unwrap();
            assert!(scan(&db, &root).progress().errors > 0);
            assert_eq!(
                comic_reader::open(&db, first.id).unwrap().bookmarks,
                vec![1]
            );
            assert!(nodes.iter().all(|id| db.get_node(*id).is_ok()));
        }
    }

    #[test]
    fn book_detail_flattens_owned_subfolders_but_respects_explicit_boundaries() {
        let (_temp, db, root) = fixture();
        let path = Path::new(&root.path);
        fs::create_dir_all(path.join("Series/ing_jpg")).unwrap();
        fs::create_dir_all(path.join("Series/ing_cbz")).unwrap();
        fs::write(path.join("Series/ing_jpg/1.png"), image()).unwrap();
        archive(&path.join("Series/ing_cbz/01.cbz"), &["1.png"]);
        assert_eq!(scan(&db, &root).progress().errors, 0);
        let c = db.connect().unwrap();
        let series: i64 = c
            .query_row("SELECT id FROM nodes WHERE folder_name='Series'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let child: i64 = c
            .query_row(
                "SELECT id FROM nodes WHERE folder_name='ing_jpg'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(comic_reader::detail(&db, series).unwrap().len(), 2);
        db.set_node_type(child, NodeType::Work).unwrap();
        assert_eq!(comic_reader::detail(&db, series).unwrap().len(), 1);
        db.set_node_type(child, NodeType::Ignored).unwrap();
        assert_eq!(comic_reader::detail(&db, series).unwrap().len(), 1);
    }

    #[test]
    fn complete_book_detail_keeps_counts_books_and_independent_continuations_consistent() {
        let (_temp, db, root) = fixture();
        let path = Path::new(&root.path);
        for (branch, count) in [("Original", 14), ("Original/Original re", 194)] {
            for index in 0..count {
                let volume = path.join(branch).join(format!("Volume {index:03}"));
                fs::create_dir_all(&volume).unwrap();
                fs::write(volume.join("1.png"), image()).unwrap();
            }
        }
        assert_eq!(scan(&db, &root).progress().errors, 0);
        let c = db.connect().unwrap();
        let node = |name: &str| {
            c.query_row("SELECT id FROM nodes WHERE folder_name=?1", [name], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
        };
        let original = node("Original");
        let continuation = node("Original re");
        db.set_node_type(continuation, NodeType::Work).unwrap();
        let detail = crate::works::node_detail(&db, original).unwrap();
        assert_eq!(detail.node.total_comic_book_count, 208);
        assert_eq!(detail.comic_books.as_ref().unwrap().len(), 14);
        assert_eq!(
            detail.children.iter().map(|n| n.id).collect::<Vec<_>>(),
            [continuation]
        );
        assert!(detail.resource_files.is_empty());
        let continued = crate::works::node_detail(&db, continuation).unwrap();
        assert_eq!(continued.comic_books.unwrap().len(), 194);
        assert!(continued.children.is_empty());
        // Opening details must continue to work entirely from the existing index offline.
        fs::rename(path.join("Original"), path.join("Offline original")).unwrap();
        assert_eq!(
            crate::works::node_detail(&db, original)
                .unwrap()
                .comic_books
                .unwrap()
                .len(),
            14
        );
    }

    #[test]
    fn book_details_remove_expanded_format_folders_but_keep_nested_attachments_and_hidden_boundaries(
    ) {
        let (_temp, db, root) = fixture();
        let path = Path::new(&root.path);
        let books_dir = path.join("Series/ing_cbz/Parts");
        fs::create_dir_all(&books_dir).unwrap();
        archive(&books_dir.join("01.cbz"), &["1.png"]);
        fs::write(books_dir.join("notes.md"), b"attached notes").unwrap();
        fs::create_dir_all(path.join("Series/ing_jpg")).unwrap();
        fs::write(path.join("Series/ing_jpg/images.7z"), b"archive attachment").unwrap();
        fs::create_dir_all(path.join("Series/Hidden")).unwrap();
        archive(&path.join("Series/Hidden/secret.cbz"), &["1.png"]);
        assert_eq!(scan(&db, &root).progress().errors, 0);
        let c = db.connect().unwrap();
        let series = c
            .query_row("SELECT id FROM nodes WHERE folder_name='Series'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let hidden = c
            .query_row("SELECT id FROM nodes WHERE folder_name='Hidden'", [], |r| {
                r.get(0)
            })
            .unwrap();
        db.set_node_type(hidden, NodeType::Ignored).unwrap();
        let detail = crate::works::node_detail(&db, series).unwrap();
        assert_eq!(detail.comic_books.unwrap().len(), 1);
        assert_eq!(
            detail
                .children
                .iter()
                .map(|n| n.folder_name.as_str())
                .collect::<Vec<_>>(),
            ["ing_jpg"]
        );
        assert_eq!(
            detail
                .resource_files
                .iter()
                .map(|f| f.file_name.as_str())
                .collect::<Vec<_>>(),
            ["notes.md"]
        );
        let jpg = crate::works::node_detail(&db, detail.children[0].id).unwrap();
        assert!(jpg.comic_books.unwrap().is_empty());
        assert_eq!(jpg.resource_files[0].file_name, "images.7z");
    }

    #[test]
    fn large_book_detail_batches_keep_sources_progress_and_remaining_folders() {
        let (_temp, db, root) = fixture();
        let path = Path::new(&root.path);
        for index in 0..505 {
            let volume = path.join(format!("Volume {index:03}"));
            fs::create_dir(&volume).unwrap();
            fs::write(volume.join("1.png"), image()).unwrap();
        }
        fs::create_dir(path.join("Attachments")).unwrap();
        fs::write(path.join("Attachments/notes.md"), b"attachment").unwrap();
        assert_eq!(scan(&db, &root).progress().errors, 0);
        let id = root_node(&db, root.id);
        let before = crate::works::node_detail(&db, id).unwrap();
        let first = &before.comic_books.as_ref().unwrap()[0];
        comic_reader::progress_at_revision(&db, first.id, 0, Some(&first.revision)).unwrap();
        comic_reader::bookmark_at_revision(&db, first.id, 0, true, Some(&first.revision)).unwrap();
        let after = crate::works::node_detail(&db, id).unwrap();
        let books = after.comic_books.unwrap();
        assert_eq!(books.len(), 505);
        assert!(books.windows(2).all(|pair| db::natural_cmp(
            &pair[0].source_path,
            &pair[1].source_path
        )
        .is_lt()));
        assert_eq!(books[0].id, first.id);
        assert_eq!(books[0].revision, first.revision);
        assert_eq!(books[0].progress.as_ref().unwrap().last_page_index, 0);
        assert_eq!(comic_reader::open(&db, first.id).unwrap().bookmarks, [0]);
        assert!(after.resource_files.is_empty());
        assert_eq!(
            after
                .children
                .iter()
                .map(|node| node.folder_name.as_str())
                .collect::<Vec<_>>(),
            ["Attachments"]
        );
        let attachment = crate::works::node_detail(&db, after.children[0].id).unwrap();
        assert_eq!(attachment.resource_files[0].file_name, "notes.md");
    }

    #[test]
    #[ignore = "Requires an explicitly supplied read-only book directory for detail timing"]
    fn owner_book_detail_timing() {
        let source = PathBuf::from(std::env::var("M2SHELF_BOOK_DETAIL_TIMING_SOURCE").unwrap());
        let temp = tempfile::tempdir().unwrap();
        let db = Database::new(temp.path().join("timing.sqlite"));
        let id = if let Ok(index) = std::env::var("M2SHELF_BOOK_DETAIL_TIMING_INDEX") {
            let original =
                Connection::open_with_flags(index, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .unwrap();
            let mut copied = Connection::open(db.path()).unwrap();
            let backup = rusqlite::backup::Backup::new(&original, &mut copied).unwrap();
            backup
                .run_to_completion(100, std::time::Duration::from_millis(1), None)
                .unwrap();
            db.connect()
                .unwrap()
                .query_row(
                    "SELECT id FROM nodes WHERE folder_name=?1",
                    [source.file_name().unwrap().to_str().unwrap()],
                    |r| r.get(0),
                )
                .unwrap()
        } else {
            db.migrate().unwrap();
            let root = db
                .add_root_with_kind(
                    &source,
                    None,
                    LibraryMediaKind::Comic,
                    LibraryRecognitionMode::Folder,
                )
                .unwrap();
            assert_eq!(scan(&db, &root).progress().errors, 0);
            root_node(&db, root.id)
        };
        let mut count = 0;
        let mut times = Vec::new();
        for _ in 0..5 {
            let started = std::time::Instant::now();
            let detail = crate::works::node_detail(&db, id).unwrap();
            times.push(started.elapsed().as_millis());
            count = detail.comic_books.as_ref().unwrap().len();
            assert!(detail.children.is_empty());
            assert!(detail.resource_files.is_empty());
        }
        println!("Native indexed detail: {count} books, milliseconds {times:?}");
        let original = crate::works::node_detail(&db, id)
            .unwrap()
            .comic_books
            .unwrap();
        // Rename only the temporary index label to prove directory names do not
        // select a different loading path. No source path or live index is changed.
        db.connect().unwrap().execute("UPDATE nodes SET folder_name='Ordinary directory',display_name='Ordinary directory' WHERE id=?1", [id]).unwrap();
        let started = std::time::Instant::now();
        let ordinary = crate::works::node_detail(&db, id)
            .unwrap()
            .comic_books
            .unwrap();
        assert_eq!(
            serde_json::to_value(original).unwrap(),
            serde_json::to_value(ordinary).unwrap()
        );
        println!(
            "Same indexed content under an ordinary label: {} ms",
            started.elapsed().as_millis()
        );
    }

    #[test]
    #[ignore = "Requires owner-supplied read-only directories and a private snapshot output directory"]
    fn owner_book_details_use_actual_sources_without_modifying_media() {
        use sha2::{Digest, Sha256};
        fn hashes(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
            let mut pending = vec![root.to_path_buf()];
            let mut result = Vec::new();
            while let Some(dir) = pending.pop() {
                for entry in fs::read_dir(dir).unwrap() {
                    let entry = entry.unwrap();
                    let kind = entry.file_type().unwrap();
                    if kind.is_dir() {
                        pending.push(entry.path());
                    } else if kind.is_file() {
                        let mut file = File::open(entry.path()).unwrap();
                        let mut hasher = Sha256::new();
                        let mut buffer = [0u8; 64 * 1024];
                        loop {
                            let count = file.read(&mut buffer).unwrap();
                            if count == 0 {
                                break;
                            }
                            hasher.update(&buffer[..count]);
                        }
                        result.push((entry.path(), hasher.finalize().to_vec()));
                        assert!(result.len() <= 50_000);
                    }
                }
            }
            result.sort_by(|a, b| a.0.cmp(&b.0));
            result
        }
        let fixtures: serde_json::Value = serde_json::from_slice(
            &fs::read(
                std::env::var("M2SHELF_BOOK_DETAIL_FIXTURES").expect("explicit fixture list"),
            )
            .unwrap(),
        )
        .unwrap();
        let output = PathBuf::from(
            std::env::var("M2SHELF_BOOK_DETAIL_OUTPUT").expect("private snapshot output directory"),
        );
        let reveal = std::env::var("M2SHELF_BOOK_DETAIL_REVEAL").as_deref() == Ok("1");
        for (index, input) in fixtures.as_array().unwrap().iter().enumerate() {
            let source = Path::new(input["path"].as_str().unwrap());
            let before = hashes(source);
            let (_temp, db, _) = fixture();
            let root = db
                .add_root_with_kind(
                    source,
                    None,
                    LibraryMediaKind::Comic,
                    LibraryRecognitionMode::Folder,
                )
                .unwrap();
            let progress = scan(&db, &root).progress();
            assert_eq!(progress.errors, 0, "owner fixture {} scan", index + 1);
            if let Some(name) = input["independent"].as_str() {
                let id = db
                    .connect()
                    .unwrap()
                    .query_row(
                        "SELECT id FROM nodes WHERE folder_name=?1 AND library_root_id=?2",
                        params![name, root.id],
                        |r| r.get(0),
                    )
                    .unwrap();
                db.set_node_type(id, NodeType::Work).unwrap();
            }
            let detail = crate::works::node_detail(&db, root_node(&db, root.id)).unwrap();
            let books = detail.comic_books.as_ref().unwrap();
            assert_eq!(books.len() as u64, input["owned"].as_u64().unwrap());
            assert_eq!(
                detail.children.len() as u64,
                input["folders"].as_u64().unwrap()
            );
            for book in books {
                comic_reader::open(&db, book.id).unwrap();
                if book.document_format.as_deref() == Some("PDF") {
                    assert_eq!(
                        &comic_reader::read_pdf_range(&db, book.id, 0, 8, &book.revision).unwrap()
                            [..5],
                        b"%PDF-"
                    );
                }
            }
            if reveal {
                comic_reader::reveal(&db, books[0].id).unwrap();
            }
            fs::write(
                output.join(format!("detail-{}.json", index + 1)),
                serde_json::to_vec(&detail).unwrap(),
            )
            .unwrap();
            for child in &detail.children {
                let child_detail = crate::works::node_detail(&db, child.id).unwrap();
                fs::write(
                    output.join(format!("detail-{}-child-{}.json", index + 1, child.id)),
                    serde_json::to_vec(&child_detail).unwrap(),
                )
                .unwrap();
            }
            assert_eq!(
                before,
                hashes(source),
                "media source paths and SHA-256 must remain unchanged"
            );
            println!("Owner fixture {}: {} owned books, {} remaining folders, native reads and source hashes verified", index + 1, books.len(), detail.children.len());
        }
    }

    #[test]
    fn library_kind_is_immutable_and_old_roots_are_video() {
        let (temp, db, root) = fixture();
        let other = temp.path().join("video");
        fs::create_dir(&other).unwrap();
        let old = db.add_root(&other, None).unwrap();
        assert_eq!(old.media_kind, LibraryMediaKind::Video);
        assert_eq!(
            db.get_root(root.id).unwrap().media_kind,
            LibraryMediaKind::Comic
        );
        assert!(db
            .connect()
            .unwrap()
            .execute(
                "UPDATE library_roots SET media_kind='VIDEO' WHERE id=?1",
                [root.id]
            )
            .is_err());
        assert!(db
            .connect()
            .unwrap()
            .execute(
                "UPDATE library_roots SET recognition_mode='VIDEO_FILE' WHERE id=?1",
                [root.id]
            )
            .is_err());
        assert!(LibraryMediaKind::Comic.accepts_subject(1));
        assert!(!LibraryMediaKind::Comic.accepts_subject(2));
        assert!(!LibraryMediaKind::Video.accepts_subject(1));
    }
    #[test]
    fn folders_archives_natural_order_progress_bookmarks_and_sources_survive_rescan() {
        let (_temp, db, root) = fixture();
        let path = Path::new(&root.path);
        for name in ["10.png", "2.png", "1.png"] {
            fs::write(path.join(name), image()).unwrap();
        }
        fs::write(path.join("note.md"), b"attachment").unwrap();
        archive(
            &path.join("Vol.02.cbz"),
            &["10.png", "2.png", "1.png", "__MACOSX/1.png"],
        );
        let before = fs::read(path.join("Vol.02.cbz")).unwrap();
        let status = scan(&db, &root);
        assert_eq!(status.progress().errors, 0);
        let id = root_node(&db, root.id);
        let books = books(&db.connect().unwrap(), id).unwrap();
        assert_eq!(books.len(), 2);
        assert!(books.iter().all(|b| b.page_count == 3));
        let book = books
            .iter()
            .find(|b| b.source_kind == "ZIP_ARCHIVE")
            .unwrap();
        let opened = comic_reader::open(&db, book.id).unwrap();
        assert_eq!(
            opened
                .pages
                .iter()
                .map(|p| p.page_name.as_str())
                .collect::<Vec<_>>(),
            vec!["1.png", "2.png", "10.png"]
        );
        assert_eq!(comic_reader::read_page(&db, book.id, 0).unwrap(), image());
        assert!(comic_reader::read_page(&db, book.id, 3).is_err());
        comic_reader::progress(&db, book.id, 1).unwrap();
        comic_reader::bookmark(&db, book.id, 1, true).unwrap();
        scan(&db, &root);
        let reopened = comic_reader::open(&db, book.id).unwrap();
        assert_eq!(reopened.book.progress.unwrap().last_page_index, 1);
        assert_eq!(reopened.bookmarks, vec![1]);
        assert_eq!(db.list_resources(id).unwrap().len(), 1);
        assert_eq!(fs::read(path.join("Vol.02.cbz")).unwrap(), before);
        assert_eq!(db.get_node(id).unwrap().direct_video_count, 0);
        assert_eq!(db.get_node(id).unwrap().direct_comic_book_count, 2);
        fs::remove_file(path.join("Vol.02.cbz")).unwrap();
        scan(&db, &root);
        assert!(comic_reader::open(&db, book.id).is_err());
        let c = db.connect().unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM comic_bookmarks", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn library_subtypes_are_immutable_and_plain_zip_is_only_an_attachment() {
        let (temp, db, root) = fixture();
        let path = Path::new(&root.path);
        archive(&path.join("ordinary.zip"), &["1.png"]);
        archive(&path.join("readable.cbz"), &["1.png"]);
        assert_eq!(scan(&db, &root).progress().errors, 0);
        let id = root_node(&db, root.id);
        assert_eq!(books(&db.connect().unwrap(), id).unwrap().len(), 1);
        let resources = db.list_resources(id).unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].file_name, "ordinary.zip");
        db.connect().unwrap().execute(
            "INSERT INTO resource_files(node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(?1,?2,'readable.cbz','cbz',1,'2026-10-07T00:00:00Z')",
            params![id,path.join("readable.cbz").to_string_lossy()],
        ).unwrap();
        assert_eq!(db.list_resources(id).unwrap().len(), 1);
        for kind in [
            LibraryMediaKind::Ebook,
            LibraryMediaKind::Animation,
            LibraryMediaKind::LiveAction,
        ] {
            let path = temp.path().join(kind.as_db());
            fs::create_dir(&path).unwrap();
            let library = db
                .add_root_with_kind(&path, None, kind, LibraryRecognitionMode::Folder)
                .unwrap();
            assert_eq!(library.media_kind, kind);
            assert_eq!(
                db.list_roots()
                    .unwrap()
                    .iter()
                    .find(|r| r.id == library.id)
                    .unwrap()
                    .media_kind,
                kind
            );
            scan(&db, &library);
            assert_eq!(
                db.get_node(root_node(&db, library.id)).unwrap().media_kind,
                kind
            );
            let column = if kind == LibraryMediaKind::Ebook {
                "book_library_kind"
            } else {
                "video_subject_scope"
            };
            let value = if kind == LibraryMediaKind::Ebook {
                "COMIC"
            } else {
                "MIXED"
            };
            assert!(db
                .connect()
                .unwrap()
                .execute(
                    &format!("UPDATE library_roots SET {column}=?1 WHERE id=?2"),
                    params![value, library.id]
                )
                .is_err());
        }
        assert!(LibraryMediaKind::Animation.accepts_subject(2));
        assert!(!LibraryMediaKind::Animation.accepts_subject(6));
        assert!(LibraryMediaKind::LiveAction.accepts_subject(6));
        assert!(!LibraryMediaKind::LiveAction.accepts_subject(2));
        assert!(LibraryMediaKind::Ebook.accepts_subject(1));
        assert!(!LibraryMediaKind::Ebook.accepts_subject(2));
    }
    #[test]
    fn nested_volumes_partial_refresh_cancellation_and_deleted_pages() {
        let (_temp, db, root) = fixture();
        let series = Path::new(&root.path).join("作品");
        let volume = series.join("第02巻");
        fs::create_dir_all(&volume).unwrap();
        fs::write(volume.join("1.png"), image()).unwrap();
        fs::write(volume.join("2.png"), image()).unwrap();
        scan(&db, &root);
        let c = db.connect().unwrap();
        let parent: i64 = c
            .query_row(
                "SELECT id FROM nodes WHERE absolute_path=?1",
                [series.to_string_lossy().as_ref()],
                |r| r.get(0),
            )
            .unwrap();
        let child: i64 = c
            .query_row(
                "SELECT id FROM nodes WHERE absolute_path=?1",
                [volume.to_string_lossy().as_ref()],
                |r| r.get(0),
            )
            .unwrap();
        drop(c);
        let book = books(&db.connect().unwrap(), child).unwrap().remove(0);
        comic_reader::progress(&db, book.id, 1).unwrap();
        let ctrl = control(root.id);
        ctrl.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        scanner::run_scan(
            None,
            &db,
            vec![ScanTarget {
                root: root.clone(),
                path: volume.clone(),
                parent_node_id: Some(parent),
            }],
            &ctrl,
            &[],
        );
        assert_eq!(comic_reader::open(&db, book.id).unwrap().book.page_count, 2);
        fs::remove_file(volume.join("2.png")).unwrap();
        let ctrl = control(root.id);
        scanner::run_scan(
            None,
            &db,
            vec![ScanTarget {
                root: root.clone(),
                path: volume,
                parent_node_id: Some(parent),
            }],
            &ctrl,
            &[],
        );
        assert_eq!(
            comic_reader::open(&db, book.id)
                .unwrap()
                .book
                .progress
                .unwrap()
                .last_page_index,
            0
        );
        assert_eq!(db.get_node(parent).unwrap().total_comic_book_count, 1);
    }
    #[test]
    fn unsafe_archive_corruption_empty_and_encryption_are_not_readable() {
        let (_temp, db, root) = fixture();
        let path = Path::new(&root.path);
        for (name, entry) in [
            ("parent.cbz", "../1.png"),
            ("absolute.cbz", "/1.png"),
            ("drive.cbz", "C:/1.png"),
        ] {
            archive(&path.join(name), &[entry]);
            assert_eq!(
                archive_pages(&path.join(name), None).err().unwrap(),
                "COMIC_ARCHIVE_PATH"
            );
        }
        archive(&path.join("empty.cbz"), &[]);
        assert_eq!(
            archive_pages(&path.join("empty.cbz"), None).err().unwrap(),
            "COMIC_NO_PAGES"
        );
        fs::write(path.join("broken.cbz"), b"broken").unwrap();
        assert_eq!(
            archive_pages(&path.join("broken.cbz"), None).err().unwrap(),
            "COMIC_ARCHIVE_DAMAGED"
        );
        archive(&path.join("encrypted.cbz"), &["1.png"]);
        let mut bytes = fs::read(path.join("encrypted.cbz")).unwrap();
        let central = bytes.windows(4).position(|b| b == b"PK\x01\x02").unwrap();
        bytes[central + 8] |= 1;
        bytes[6] |= 1;
        fs::write(path.join("encrypted.cbz"), bytes).unwrap();
        assert_eq!(
            archive_pages(&path.join("encrypted.cbz"), None)
                .err()
                .unwrap(),
            "COMIC_ARCHIVE_ENCRYPTED"
        );
        scan(&db, &root);
        let id = root_node(&db, root.id);
        assert!(books(&db.connect().unwrap(), id)
            .unwrap()
            .iter()
            .all(|b| b.index_error.is_some()));
    }
    #[test]
    fn source_changed_and_fake_extension_fail_closed() {
        let (_temp, db, root) = fixture();
        let path = Path::new(&root.path);
        fs::write(path.join("fake.png"), b"<html>not a page</html>").unwrap();
        scan(&db, &root);
        let id = root_node(&db, root.id);
        let book = books(&db.connect().unwrap(), id).unwrap().remove(0);
        assert!(comic_reader::read_page(&db, book.id, 0).is_err());
        fs::write(path.join("fake.png"), image()).unwrap();
        assert!(comic_reader::read_page(&db, book.id, 0).is_err());
        scan(&db, &root);
        assert_eq!(comic_reader::read_page(&db, book.id, 0).unwrap(), image());
    }

    #[test]
    fn png_folders_and_mislabeled_supported_images_read_by_content() {
        let (temp, db, root) = fixture();
        let folder = Path::new(&root.path).join("漫画 [PNG]");
        fs::create_dir(&folder).unwrap();
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode_image(&image::RgbImage::from_pixel(
                32,
                48,
                image::Rgb([20, 60, 90]),
            ))
            .unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(40, 60, image::Rgba([20, 60, 90, 180]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();
        let originals = [("01.PNG", jpeg), ("02.png", png.clone()), ("10.jpg", png)];
        for (name, bytes) in &originals {
            fs::write(folder.join(name), bytes).unwrap();
        }
        let archive_path = folder.join("Mixed.cbz");
        let mut zip = zip::ZipWriter::new(File::create(&archive_path).unwrap());
        for (name, bytes) in &originals {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        assert_eq!(scan(&db, &root).progress().errors, 0);
        let node = db.list_children(root_node(&db, root.id)).unwrap().remove(0);
        let books = comic_reader::detail(&db, node.id).unwrap();
        assert_eq!(books.len(), 2);
        for book in &books {
            assert_eq!(book.page_count, 3);
            for (index, (_, original)) in originals.iter().enumerate() {
                let read = comic_reader::read_page_at_revision(
                    &db,
                    book.id,
                    index as i64,
                    Some(&book.revision),
                )
                .unwrap();
                assert_eq!(&read, original);
                image::load_from_memory(&read).unwrap();
            }
            comic_reader::progress_at_revision(&db, book.id, 1, Some(&book.revision)).unwrap();
            comic_reader::bookmark_at_revision(&db, book.id, 1, true, Some(&book.revision))
                .unwrap();
        }
        assert!(comic_reader::first_image_cover(&db, node.id)
            .unwrap()
            .unwrap()
            .starts_with("data:image/jpeg;base64,"));
        let cache = temp.path().join("cover-cache");
        crate::cache::ensure_directories(&cache).unwrap();
        assert!(
            crate::poster_cache::cover_data_url(&db, &cache, node.id, 256)
                .unwrap()
                .is_some()
        );
        scan(&db, &root);
        for book in &books {
            let reopened = comic_reader::open(&db, book.id).unwrap();
            assert_eq!(reopened.book.revision, book.revision);
            assert_eq!(reopened.book.progress.unwrap().last_page_index, 1);
            assert_eq!(reopened.bookmarks, [1]);
        }
        for (name, bytes) in originals {
            assert_eq!(fs::read(folder.join(name)).unwrap(), bytes);
        }
    }

    /// A copied index is required; this probe performs only the normal bounded page read.
    #[test]
    #[ignore = "requires a disposable index with recorded source-read failure"]
    fn disposable_failed_image_read_probe() {
        let db = Database::new(
            std::env::var_os("M2SHELF_IMAGE_PROBE_INDEX")
                .expect("disposable index required")
                .into(),
        );
        let (book, revision):(i64,String)=db.connect().unwrap().query_row(
            "SELECT b.id,b.revision FROM poster_cache_failures f JOIN comic_books b ON b.node_id=f.node_id WHERE f.detail='COMIC_IMAGE_INVALID' AND b.source_kind='IMAGE_FOLDER' LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        let bytes = comic_reader::read_page_at_revision(&db, book, 0, Some(&revision)).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        eprintln!(
            "source image probe passed: {} bytes, {} x {}",
            bytes.len(),
            decoded.width(),
            decoded.height()
        );
    }

    #[test]
    fn comic_titles_strip_volumes_without_video_seasons_or_page_evidence() {
        for title in [
            "作品 Vol. 02.cbz",
            "作品 Volume 2.zip",
            "作品 Vol02",
            "作品 第2卷",
            "作品 02巻",
            "作品 卷一",
            "作品 上巻",
            "作品 下巻",
            "作品 第02册.pdf",
            "作品（完结） 第12話.epub",
            "作品_1话",
        ] {
            assert_eq!(clean_title(title), "作品", "{title}");
        }
        assert_eq!(clean_title("Voltron"), "Voltron");
        assert_eq!(clean_title("卷轴"), "卷轴");
        let e = match_evidence("Vol. 02", "Vol. 02", Some("独立漫画"));
        assert_eq!(e.primary_title, "独立漫画 (2)");
        assert!(e.season_number.is_none());
        let e =
            match_evidence_with_books("Manga", "Manga", None, &["魔法使いの嫁 Vol. 1.cbz".into()]);
        assert_eq!(e.primary_title, "魔法使いの嫁 (1)");
        for (title, number) in [
            ("作品 卷一", 1),
            ("作品 第二十三卷", 23),
            ("作品 第１２冊", 12),
            ("作品 一百零二巻", 102),
        ] {
            assert_eq!(book_volume(title), Some(number));
        }
        assert_eq!(
            match_evidence("作品 第二十三卷", "作品 第二十三卷", None).primary_title,
            "作品 (23)"
        );
        assert_eq!(
            match_evidence_with_books(
                "Books",
                "Books",
                Some("东京喰种"),
                &["东京喰种:re 第02卷.pdf".into()]
            )
            .primary_title,
            "东京喰种:re (2)"
        );
        let e = match_evidence_with_books(
            "Manga",
            "Manga",
            None,
            &["作品 Vol. 1.cbz".into(), "作品 Vol. 2.cbz".into()],
        );
        assert_eq!(e.primary_title, "作品");
    }

    #[test]
    fn readable_comic_series_excludes_collapsed_volumes_after_partial_scan() {
        let (_temp, db, root) = fixture();
        let series = Path::new(&root.path).join("漫画作品");
        let volume = series.join("第01卷");
        fs::create_dir_all(&volume).unwrap();
        fs::write(volume.join("1.png"), image()).unwrap();
        let bad = Path::new(&root.path).join("没有页面");
        fs::create_dir(&bad).unwrap();
        archive(&bad.join("bad.cbz"), &["note.md"]);
        assert!(scan(&db, &root).progress().errors > 0);
        let conn = db.connect().unwrap();
        let series_id: i64 = conn
            .query_row(
                "SELECT id FROM nodes WHERE folder_name='漫画作品'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let volume_id: i64 = conn
            .query_row(
                "SELECT id FROM nodes WHERE folder_name='第01卷'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let ids: Vec<_> = db
            .list_unbound_bangumi_candidates(root.id)
            .unwrap()
            .iter()
            .map(|n| n.id)
            .collect();
        assert!(ids.contains(&series_id));
        assert!(!ids.contains(&volume_id));
        assert!(db
            .list_bangumi_match_candidates(Some(&[volume_id]), true)
            .unwrap()
            .is_empty());
        assert_eq!(
            ids.len(),
            1,
            "hidden root, unreadable folders and collapsed volumes are excluded"
        );
    }

    #[test]
    fn matching_and_poster_scope_share_detail_boundaries_without_changing_books() {
        let (_temp, db, root) = fixture();
        let path = Path::new(&root.path);
        for dir in ["Series/Volume 1", "Series/formats/Chapter 2", "Standalone"] {
            fs::create_dir_all(path.join(dir)).unwrap();
            fs::write(path.join(dir).join("1.png"), image()).unwrap();
        }
        scan(&db, &root);
        let c = db.connect().unwrap();
        let id = |name: &str| {
            c.query_row("SELECT id FROM nodes WHERE folder_name=?1", [name], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
        };
        let (series, volume, chapter, standalone) = (
            id("Series"),
            id("Volume 1"),
            id("Chapter 2"),
            id("Standalone"),
        );
        let check = |expected: &[i64]| {
            let matching = db
                .list_bangumi_match_candidates(None, true)
                .unwrap()
                .iter()
                .map(|n| n.id)
                .collect::<HashSet<_>>();
            assert_eq!(matching, expected.iter().copied().collect());
            assert_eq!(db.poster_node_count().unwrap(), expected.len() as u64);
            assert_eq!(
                db.poster_nodes_after(0)
                    .unwrap()
                    .into_iter()
                    .collect::<HashSet<_>>(),
                matching
            );
        };
        check(&[series, standalone]);
        assert_eq!(comic_reader::detail(&db, series).unwrap().len(), 2);
        let book = comic_reader::detail(&db, volume).unwrap().remove(0);
        comic_reader::progress(&db, book.id, 0).unwrap();
        comic_reader::bookmark(&db, book.id, 0, true).unwrap();
        // Historical errors on collapsed rows no longer appear or enter retries.
        db.save_poster_checkpoint(
            "{}",
            Some((volume, Some(("SOURCE_READ", "COMIC_IMAGE_INVALID")))),
            false,
        )
        .unwrap();
        assert_eq!(db.poster_failure_count().unwrap(), 0);
        assert!(db.poster_failed_nodes_after(0).unwrap().is_empty());
        assert!(db.poster_failures().unwrap().is_empty());
        db.set_node_type(volume, NodeType::Work).unwrap();
        check(&[series, standalone, volume]);
        assert_eq!(db.poster_failure_count().unwrap(), 1);
        assert_eq!(comic_reader::detail(&db, series).unwrap().len(), 1);
        db.reset_node_type(volume).unwrap();
        // A different existing edition binding remains an independent destination.
        let subject = crate::models::BangumiSubject {
            subject_id: 777,
            title: "Independent edition".into(),
            title_cn: None,
            title_en: None,
            title_ja: None,
            title_ko: None,
            date: None,
            image_url: None,
            summary: None,
            match_aliases: vec![],
            subject_type: 1,
        };
        db.save_binding_if_absent(series, &subject).unwrap();
        db.save_binding_if_absent(
            chapter,
            &crate::models::BangumiSubject {
                subject_id: 778,
                ..subject.clone()
            },
        )
        .unwrap();
        check(&[series, standalone, chapter]);
        let format_folder = crate::works::node_detail(&db, series).unwrap().children[0].id;
        assert_eq!(
            crate::works::node_detail(&db, format_folder)
                .unwrap()
                .children[0]
                .id,
            chapter
        );
        // Same-Subject bindings follow the existing detail flattening rule.
        db.save_confirmed_binding(chapter, &subject).unwrap();
        check(&[series, standalone]);
        assert_eq!(comic_reader::detail(&db, series).unwrap().len(), 2);
        assert_eq!(comic_reader::open(&db, book.id).unwrap().bookmarks, [0]);
        scan(&db, &root);
        check(&[series, standalone]);
        assert_eq!(
            comic_reader::open(&db, book.id)
                .unwrap()
                .book
                .progress
                .unwrap()
                .last_page_index,
            0
        );
    }

    #[test]
    fn artbook_modes_persist_independent_matching_policy_and_reader_metadata() {
        for mode in [
            LibraryRecognitionMode::Folder,
            LibraryRecognitionMode::VideoFile,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("設定資料 [Unicode]");
            fs::create_dir_all(source.join("Art Pages")).unwrap();
            fs::write(source.join("Art Pages/10.png"), image()).unwrap();
            fs::write(source.join("Art Pages/2.png"), image()).unwrap();
            fs::write(source.join("Notes.txt"), "第一章\n原画集\n").unwrap();
            let db = Database::new(temp.path().join("art.sqlite"));
            db.migrate().unwrap();
            let root = db
                .add_root_with_kind(&source, None, LibraryMediaKind::Artbook, mode.clone())
                .unwrap();
            assert!(!root.auto_bangumi);
            assert_eq!(scan(&db, &root).progress().errors, 0);
            let c = db.connect().unwrap();
            let node: i64 = c
                .query_row(
                    "SELECT node_id FROM comic_books WHERE source_kind='IMAGE_FOLDER' LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                db.get_node(node).unwrap().media_kind,
                LibraryMediaKind::Artbook
            );
            assert!(db
                .list_bangumi_match_candidates(Some(&[node]), false)
                .unwrap()
                .is_empty());
            let book = books(&c, node).unwrap().remove(0);
            assert_eq!(
                comic_reader::open(&db, book.id).unwrap().pages[0].page_name,
                "2.png"
            );
            assert!(comic_reader::first_image_cover(&db, node)
                .unwrap()
                .is_some());
            let subject = crate::models::BangumiSubject {
                subject_id: 987,
                title: "Art Pages".into(),
                title_cn: None,
                title_en: None,
                title_ja: None,
                title_ko: None,
                date: None,
                image_url: None,
                summary: None,
                match_aliases: vec![],
                subject_type: 1,
            };
            assert!(!db.save_binding_if_absent(node, &subject).unwrap());
            db.save_confirmed_binding(node, &subject).unwrap();
            assert!(db
                .save_confirmed_binding(
                    node,
                    &crate::models::BangumiSubject {
                        subject_type: 2,
                        ..subject.clone()
                    }
                )
                .is_err());
            comic_reader::progress(&db, book.id, 1).unwrap();
            comic_reader::bookmark(&db, book.id, 1, true).unwrap();
            let tag = db.create_or_assign_user_tag(node, "資料").unwrap();
            let favorite = db.create_favorite_folder("Art").unwrap();
            db.batch_add_nodes_to_favorite(favorite.id, &[node])
                .unwrap();
            db.set_root_auto_bangumi(root.id, true).unwrap();
            assert!(Database::new(db.path().into()).list_roots().unwrap()[0].auto_bangumi);
            db.set_root_auto_bangumi(root.id, false).unwrap();
            scan(&db, &root);
            db.migrate().unwrap();
            assert_eq!(
                db.get_binding(node).unwrap().unwrap().provider_subject_id,
                987
            );
            assert_eq!(comic_reader::open(&db, book.id).unwrap().bookmarks, [1]);
            assert_eq!(db.get_node(node).unwrap().user_tags[0].id, tag.id);
            assert_eq!(
                db.list_favorite_folder_nodes(favorite.id).unwrap()[0].id,
                node
            );
            assert!(c
                .execute(
                    "UPDATE library_roots SET artbook_library=0 WHERE id=?1",
                    [root.id]
                )
                .is_err());
            assert!(c
                .execute(
                    "UPDATE library_roots SET recognition_mode=?1 WHERE id=?2",
                    params![
                        if matches!(mode, LibraryRecognitionMode::Folder) {
                            "VIDEO_FILE"
                        } else {
                            "FOLDER"
                        },
                        root.id
                    ]
                )
                .is_err());
        }
    }

    #[test]
    fn video_attachments_read_internally_without_changing_ownership_or_counts() {
        for kind in [
            LibraryMediaKind::Video,
            LibraryMediaKind::Animation,
            LibraryMediaKind::LiveAction,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("Video");
            let work = source.join("Movie");
            fs::create_dir_all(&work).unwrap();
            fs::write(work.join("1.mp4"), b"video fixture").unwrap();
            let mut epub = zip::ZipWriter::new(File::create(work.join("novel.epub")).unwrap());
            for (name,text) in [("mimetype","application/epub+zip"),("META-INF/container.xml","<container><rootfiles><rootfile full-path='book.opf'/></rootfiles></container>"),("book.opf","<package><manifest><item id='c' href='chapter.xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='c'/></spine></package>"),("chapter.xhtml","<html><body><h1>第一章</h1><p>Safe local attachment.</p></body></html>")] {
                epub.start_file(name,zip::write::SimpleFileOptions::default()).unwrap();epub.write_all(text.as_bytes()).unwrap();
            }
            epub.finish().unwrap();
            let mut pdf = lopdf::Document::with_version("1.5");
            let pages = pdf.new_object_id();
            let page=pdf.add_object(lopdf::dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),200.into(),300.into()]});
            pdf.objects.insert(pages,lopdf::dictionary!{"Type"=>"Pages","Kids"=>vec![lopdf::Object::Reference(page)],"Count"=>1}.into());
            let catalog = pdf.add_object(lopdf::dictionary! {"Type"=>"Catalog","Pages"=>pages});
            pdf.trailer.set("Root", catalog);
            pdf.save(work.join("art.pdf")).unwrap();
            let text = "第一章\nAttachment reading.\n".repeat(700);
            fs::write(work.join("Read me.txt"), &text).unwrap();
            fs::write(work.join("other.txt"), &text).unwrap();
            fs::write(work.join("cover.png"), image()).unwrap();
            archive(&work.join("comic.cbz"), &["2.png", "10.png"]);
            archive(&work.join("ordinary.zip"), &["1.png"]);
            let db = Database::new(temp.path().join("video.sqlite"));
            db.migrate().unwrap();
            let root = db
                .add_root_with_kind(&source, None, kind, LibraryRecognitionMode::Folder)
                .unwrap();
            scan(&db, &root);
            let node = db.list_children(root_node(&db, root.id)).unwrap().remove(0);
            let original_resources = db.list_resources(node.id).unwrap();
            let before_count = db.get_node(node.id).unwrap().total_comic_book_count;
            let find = |name: &str| {
                original_resources
                    .iter()
                    .find(|f| f.file_name == name)
                    .unwrap()
                    .id
            };
            assert!(comic_reader::open_resource(&db, find("ordinary.zip"))
                .unwrap()
                .is_none());
            let txt = comic_reader::open_resource(&db, find("Read me.txt"))
                .unwrap()
                .unwrap();
            assert_eq!(txt.document_format.as_deref(), Some("TXT"));
            assert!(txt.page_count > 1);
            let blocks = comic_reader::read_document(&db, txt.id, 0, &txt.revision).unwrap();
            assert!(String::from_utf8(blocks).unwrap().contains("第一章"));
            comic_reader::progress(&db, txt.id, 1).unwrap();
            comic_reader::bookmark(&db, txt.id, 1, true).unwrap();
            let other = comic_reader::open_resource(&db, find("other.txt"))
                .unwrap()
                .unwrap();
            assert_ne!(other.id, txt.id);
            assert!(other.progress.is_none());
            for (name, format) in [("novel.epub", "EPUB"), ("art.pdf", "PDF")] {
                let b = comic_reader::open_resource(&db, find(name))
                    .unwrap()
                    .unwrap();
                assert_eq!(b.document_format.as_deref(), Some(format));
                if format == "PDF" {
                    assert!(comic_reader::read_pdf_range(&db, b.id, 0, 20, &b.revision)
                        .unwrap()
                        .starts_with(b"%PDF"));
                } else {
                    assert!(String::from_utf8(
                        comic_reader::read_document(&db, b.id, 0, &b.revision).unwrap()
                    )
                    .unwrap()
                    .contains("第一章"));
                }
            }
            for name in ["cover.png", "comic.cbz"] {
                let b = comic_reader::open_resource(&db, find(name))
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    comic_reader::read_page_at_revision(&db, b.id, 0, Some(&b.revision)).unwrap(),
                    image()
                );
            }
            scan(&db, &root);
            let reopened = comic_reader::open_resource(&db, find("Read me.txt"))
                .unwrap()
                .unwrap();
            assert_eq!(reopened.id, txt.id);
            assert_eq!(reopened.progress.unwrap().last_page_index, 1);
            assert_eq!(comic_reader::open(&db, txt.id).unwrap().bookmarks, [1]);
            assert_eq!(db.get_node(node.id).unwrap().media_kind, kind);
            assert_eq!(
                db.get_node(node.id).unwrap().total_comic_book_count,
                before_count
            );
            assert_eq!(
                db.list_resources(node.id)
                    .unwrap()
                    .iter()
                    .map(|f| f.id)
                    .collect::<Vec<_>>(),
                original_resources.iter().map(|f| f.id).collect::<Vec<_>>()
            );
            assert!(owned_books(&db.connect().unwrap(), node.id)
                .unwrap()
                .is_empty());
            let detail = crate::works::node_detail(&db, node.id).unwrap();
            assert_eq!(detail.resource_files.len(), original_resources.len());
            let presented = detail.comic_books.unwrap();
            assert_eq!(presented.len(), 6);
            assert_eq!(
                presented
                    .iter()
                    .find(|book| book.id == txt.id)
                    .unwrap()
                    .progress
                    .as_ref()
                    .unwrap()
                    .last_page_index,
                1
            );
            assert_eq!(fs::read_to_string(work.join("Read me.txt")).unwrap(), text);
            fs::write(work.join("Read me.txt"), "changed revision").unwrap();
            assert!(comic_reader::read_document(&db, txt.id, 0, &txt.revision).is_err());
        }
    }

    #[test]
    fn rescan_promotes_previously_opened_book_attachment_without_cascading_state() {
        for mode in [
            LibraryRecognitionMode::Folder,
            LibraryRecognitionMode::VideoFile,
        ] {
            let (_temp, db, mut root) = fixture();
            if matches!(mode, LibraryRecognitionMode::VideoFile) {
                let directory = Path::new(&root.path).parent().unwrap().join("Individual");
                fs::create_dir(&directory).unwrap();
                root = db
                    .add_root_with_kind(&directory, None, LibraryMediaKind::Ebook, mode.clone())
                    .unwrap();
            }
            let path = Path::new(&root.path).join("Novel.txt");
            fs::write(&path, "第一章\n".repeat(4000)).unwrap();
            scan(&db, &root);
            let node = root_node(&db, root.id);
            let owner = if matches!(mode, LibraryRecognitionMode::VideoFile) {
                db.list_children(node).unwrap().remove(0).id
            } else {
                node
            };
            let original = books(&db.connect().unwrap(), owner).unwrap().remove(0);
            comic_reader::progress(&db, original.id, 1).unwrap();
            comic_reader::bookmark(&db, original.id, 1, true).unwrap();
            let c = db.connect().unwrap();
            crate::scanner::index_resource_file(
                &c,
                node,
                &path,
                &fs::canonicalize(&path).unwrap(),
                "old-index",
            )
            .unwrap();
            let (resource_id, resource_stamp): (i64, String) = c
                .query_row(
                    "SELECT id,modified_at FROM resource_files WHERE node_id=?1",
                    [node],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            c.execute(
            "UPDATE comic_books SET node_id=?4,source_resource_id=?1,source_resource_stamp=?2 WHERE id=?3",
            params![resource_id, resource_stamp, original.id,node],
        )
        .unwrap();
            scan(&db, &root);
            let current = books(&db.connect().unwrap(), owner).unwrap().remove(0);
            assert_eq!(current.id, original.id);
            assert_eq!(current.progress.unwrap().last_page_index, 1);
            assert_eq!(comic_reader::open(&db, original.id).unwrap().bookmarks, [1]);
            assert!(db.list_resources(node).unwrap().is_empty());
            assert_eq!(db.get_node(owner).unwrap().direct_comic_book_count, 1);
        }
    }

    #[test]
    fn kindle_reindex_preserves_chapter_locators_in_progress_and_bookmarks() {
        let (_temp, db, root) = fixture();
        scan(&db, &root);
        let c = db.connect().unwrap();
        let node = root_node(&db, root.id);
        let path = Path::new(&root.path).join("Novel.mobi");
        let pages = |locators: &[&str]| {
            locators
                .iter()
                .map(|locator| IndexedPage {
                    name: locator.to_string(),
                    locator: locator.to_string(),
                    size: 1,
                    modified: "stamp".into(),
                    crc: None,
                })
                .collect()
        };
        store_book(
            &c,
            node,
            &path,
            "ZIP_ARCHIVE",
            "Book",
            1,
            "stamp",
            Ok(pages(&[
                "kindle:0:0:sha",
                "kindle:0:10:sha",
                "kindle:0:20:sha",
            ])),
            "scan1",
        )
        .unwrap();
        let id = books(&c, node).unwrap()[0].id;
        c.execute(
            "INSERT INTO comic_reading_progress(comic_book_id,last_page_index) VALUES(?1,1)",
            [id],
        )
        .unwrap();
        c.execute("INSERT INTO comic_bookmarks(comic_book_id,page_index,created_at) VALUES(?1,2,'2026-01-01')",[id]).unwrap();
        c.execute("UPDATE comic_reading_progress SET text_block_index=4,text_character_offset=120 WHERE comic_book_id=?1",[id]).unwrap();
        c.execute("UPDATE comic_bookmarks SET text_block_index=9,text_character_offset=80 WHERE comic_book_id=?1",[id]).unwrap();
        store_book(
            &c,
            node,
            &path,
            "ZIP_ARCHIVE",
            "Book",
            1,
            "stamp",
            Ok(pages(&["kindle:0:10:sha", "kindle:0:20:sha"])),
            "scan2",
        )
        .unwrap();
        let book = books(&c, node).unwrap().remove(0);
        assert_eq!(book.id, id);
        assert_eq!(
            book.progress
                .as_ref()
                .unwrap()
                .text_position
                .as_ref()
                .unwrap()
                .character_offset,
            120
        );
        assert_eq!(
            c.query_row(
                "SELECT text_character_offset FROM comic_bookmarks WHERE comic_book_id=?1",
                [id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            80
        );
        assert_eq!(book.progress.unwrap().last_page_index, 0);
        assert_eq!(
            c.query_row(
                "SELECT page_index,created_at FROM comic_bookmarks WHERE comic_book_id=?1",
                [id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            )
            .unwrap(),
            (1, "2026-01-01".into())
        );
    }

    #[test]
    #[ignore = "requires explicitly provided public-domain MOBI6/KF8 fixtures"]
    fn real_kindle_libraries_and_video_attachments_share_reader_and_state() {
        let samples = PathBuf::from(
            std::env::var_os("M2SHELF_KINDLE_SAMPLES").expect("M2SHELF_KINDLE_SAMPLES"),
        );
        let mobi = fs::read(samples.join("public-domain-mobi6.mobi")).unwrap();
        let azw3 = fs::read(samples.join("public-domain-kf8.azw3")).unwrap();
        for kind in [
            LibraryMediaKind::Animation,
            LibraryMediaKind::LiveAction,
            LibraryMediaKind::Comic,
            LibraryMediaKind::Ebook,
            LibraryMediaKind::Doujin,
            LibraryMediaKind::Artbook,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("Library");
            let work = source.join("Books");
            fs::create_dir_all(&work).unwrap();
            fs::write(work.join("English.mobi"), &mobi).unwrap();
            fs::write(work.join("English.azw3"), &azw3).unwrap();
            let db = Database::new(temp.path().join("test.sqlite"));
            db.migrate().unwrap();
            let root = db
                .add_root_with_kind(&source, None, kind, LibraryRecognitionMode::Folder)
                .unwrap();
            assert_eq!(scan(&db, &root).progress().errors, 0);
            let node = db.list_children(root_node(&db, root.id)).unwrap().remove(0);
            let book_ids = if kind.is_book() {
                books(&db.connect().unwrap(), node.id)
                    .unwrap()
                    .into_iter()
                    .map(|b| b.id)
                    .collect::<Vec<_>>()
            } else {
                db.list_resources(node.id)
                    .unwrap()
                    .iter()
                    .map(|f| comic_reader::open_resource(&db, f.id).unwrap().unwrap().id)
                    .collect()
            };
            assert_eq!(book_ids.len(), 2);
            for id in book_ids {
                let opened = comic_reader::open(&db, id).unwrap();
                assert!(opened.book.page_count > 3);
                assert!(String::from_utf8(
                    comic_reader::read_document(&db, id, 1, &opened.book.revision).unwrap()
                )
                .unwrap()
                .contains("text"));
                comic_reader::progress_at_revision(&db, id, 1, Some(&opened.book.revision))
                    .unwrap();
                comic_reader::bookmark_at_revision(&db, id, 1, true, Some(&opened.book.revision))
                    .unwrap();
                assert!(
                    comic_reader::read_book_cover(&db, id, &opened.book.revision)
                        .unwrap()
                        .is_some()
                );
                scan(&db, &root);
                let after = comic_reader::open(&db, id).unwrap();
                assert_eq!(after.book.progress.unwrap().last_page_index, 1);
                assert_eq!(after.bookmarks, [1]);
            }
            assert_eq!(db.get_node(node.id).unwrap().media_kind, kind);
            assert_eq!(fs::read(work.join("English.mobi")).unwrap(), mobi);
            assert_eq!(fs::read(work.join("English.azw3")).unwrap(), azw3);
            assert_eq!(fs::read_dir(work).unwrap().count(), 2);
            println!(
                "native_reader_root={} MOBI/AZW3 revision/progress/bookmarks/cover/rescan passed",
                kind.as_db()
            );
        }
    }
    #[test]
    fn pdf_scan_read_and_rescan_preserve_identity_progress_and_bookmarks() {
        let (_temp, db, root) = fixture();
        let path = Path::new(&root.path).join("作品.pdf");
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let pages = (0..2).map(|_| doc.add_object(lopdf::dictionary! {
            "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(),0.into(),200.into(),300.into()]
        })).collect::<Vec<_>>();
        doc.objects.insert(pages_id, lopdf::dictionary! {"Type" => "Pages", "Kids" => pages.iter().map(|id| lopdf::Object::Reference(*id)).collect::<Vec<_>>(), "Count" => 2}.into());
        let catalog = doc.add_object(lopdf::dictionary! {"Type" => "Catalog", "Pages" => pages_id});
        doc.trailer.set("Root", catalog);
        doc.save(&path).unwrap();
        let original = fs::read(&path).unwrap();
        assert_eq!(scan(&db, &root).progress().errors, 0);
        let conn = db.connect().unwrap();
        let book = books(&conn, root_node(&db, root.id)).unwrap().remove(0);
        assert_eq!(book.document_format.as_deref(), Some("PDF"));
        assert_eq!(book.page_count, 2);
        assert_eq!(
            comic_reader::read_pdf_range(&db, book.id, 0, 20, &book.revision).unwrap(),
            original[..20]
        );
        assert!(comic_reader::read_pdf_range(&db, book.id, 20, 10, &book.revision).is_err());
        assert!(comic_reader::read_pdf_range(
            &db,
            book.id,
            0,
            original.len() as u64 + 1,
            &book.revision
        )
        .is_err());
        assert!(comic_reader::read_pdf_range(&db, book.id, 0, 20, "stale").is_err());
        assert_eq!(
            comic_reader::read_document(&db, book.id, 0, &book.revision).unwrap(),
            original
        );
        comic_reader::progress_at_revision(&db, book.id, 1, Some(&book.revision)).unwrap();
        comic_reader::bookmark_at_revision(&db, book.id, 1, true, Some(&book.revision)).unwrap();
        scan(&db, &root);
        db.migrate().unwrap();
        let refreshed = books(&conn, book.node_id).unwrap().remove(0);
        assert_eq!(book.id, refreshed.id);
        assert_eq!(refreshed.progress.unwrap().last_page_index, 1);
        assert_eq!(comic_reader::open(&db, book.id).unwrap().bookmarks, vec![1]);
        assert_eq!(
            fs::read(&path).unwrap(),
            original,
            "source remains byte-for-byte unchanged"
        );
        assert!(comic_reader::read_document(&db, book.id, 2, &book.revision).is_err());
        assert!(comic_reader::read_document(&db, book.id, 0, "stale").is_err());
    }
    #[test]
    fn pdf_over_64_mib_indexes_and_reads_only_requested_ranges() {
        let (_temp, db, root) = fixture();
        let path = Path::new(&root.path).join("large.pdf");
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page = doc.add_object(lopdf::dictionary! {"Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(),0.into(),200.into(),300.into()]});
        doc.objects.insert(pages_id, lopdf::dictionary! {"Type" => "Pages", "Kids" => vec![lopdf::Object::Reference(page)], "Count" => 1}.into());
        let catalog = doc.add_object(lopdf::dictionary! {"Type" => "Catalog", "Pages" => pages_id});
        doc.trailer.set("Root", catalog);
        doc.add_object(lopdf::Stream::new(
            lopdf::Dictionary::new(),
            vec![b'x'; 65 * 1024 * 1024],
        ));
        doc.save(&path).unwrap();
        drop(doc);
        assert!(fs::metadata(&path).unwrap().len() > MAX_PAGE_BYTES);
        assert_eq!(scan(&db, &root).progress().errors, 0);
        let book = books(&db.connect().unwrap(), root_node(&db, root.id))
            .unwrap()
            .remove(0);
        assert_eq!(book.page_count, 1);
        assert_eq!(
            &comic_reader::read_pdf_range(&db, book.id, 0, 8, &book.revision).unwrap(),
            b"%PDF-1.5"
        );
        assert_eq!(
            comic_reader::read_pdf_range(
                &db,
                book.id,
                book.source_size - 32,
                book.source_size,
                &book.revision
            )
            .unwrap()
            .len(),
            32
        );
        assert!(comic_reader::read_pdf_range(
            &db,
            book.id,
            0,
            crate::ebooks::PDF_CHUNK_BYTES + 1,
            &book.revision
        )
        .is_err());
    }

    #[test]
    fn mixed_browse_search_binding_activity_and_curation() {
        let (temp, db, root) = fixture();
        let volume = Path::new(&root.path).join("漫画作品");
        fs::create_dir(&volume).unwrap();
        archive(&volume.join("单行本.cbz"), &["secret-page.png"]);
        scan(&db, &root);
        let conn = db.connect().unwrap();
        let node_id: i64 = conn
            .query_row(
                "SELECT id FROM nodes WHERE folder_name='漫画作品'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let book = books(&conn, node_id).unwrap().remove(0);
        drop(conn);
        let comic = crate::models::BangumiSubject {
            subject_id: 777,
            title: "漫画作品".into(),
            title_cn: None,
            title_en: None,
            title_ja: None,
            title_ko: None,
            date: None,
            image_url: None,
            summary: None,
            match_aliases: vec![],
            subject_type: 1,
        };
        assert!(db.save_binding_if_absent(node_id, &comic).unwrap());
        assert_eq!(
            db.get_binding(node_id)
                .unwrap()
                .unwrap()
                .provider_subject_type,
            1
        );
        assert!(db
            .save_confirmed_binding(
                node_id,
                &crate::models::BangumiSubject {
                    subject_type: 2,
                    ..comic.clone()
                }
            )
            .is_err());
        assert_eq!(db.search("单行本", None).unwrap().len(), 1);
        assert!(db.search("secret-page", None).unwrap().is_empty());
        assert_eq!(db.list_all_resources().unwrap().comic_nodes.len(), 1);
        let tag = db.create_or_assign_user_tag(node_id, "漫画收藏").unwrap();
        let favorite = db.create_favorite_folder("漫画").unwrap();
        db.batch_add_nodes_to_favorite(favorite.id, &[node_id])
            .unwrap();
        comic_reader::progress(&db, book.id, 0).unwrap();
        let entries = db.list_recently_watched().unwrap();
        assert_eq!(entries[0].comic_book_id, Some(book.id));
        assert_eq!(entries[0].node.id, node_id);
        assert!(entries[0].node.last_watched_at.is_some());
        let video_path = temp.path().join("Video");
        fs::create_dir(&video_path).unwrap();
        fs::write(video_path.join("Movie.mp4"), b"video fixture").unwrap();
        let video_root = db
            .add_root_with_mode(&video_path, None, LibraryRecognitionMode::Folder)
            .unwrap();
        scan(&db, &video_root);
        let video_node = root_node(&db, video_root.id);
        db.record_node_watched(video_node).unwrap();
        assert_eq!(db.list_recently_watched().unwrap().len(), 2);
        assert!(db.save_binding_if_absent(video_node, &comic).is_err());
        scan(&db, &root);
        assert_eq!(db.get_node(node_id).unwrap().user_tags[0].id, tag.id);
        assert_eq!(
            db.list_favorite_folder_nodes(favorite.id).unwrap()[0].id,
            node_id
        );
        db.set_node_type(node_id, NodeType::Ignored).unwrap();
        assert_eq!(
            db.get_node(root_node(&db, root.id))
                .unwrap()
                .total_comic_book_count,
            0
        );
        assert_eq!(db.list_recently_watched().unwrap().len(), 1);
        db.reset_node_type(node_id).unwrap();
        assert_eq!(
            db.get_node(root_node(&db, root.id))
                .unwrap()
                .total_comic_book_count,
            1
        );
    }

    #[test]
    fn oversized_pages_and_book_revisions_are_bounded() {
        let (_temp, db, root) = fixture();
        let path = Path::new(&root.path);
        fs::write(path.join("1.png"), image()).unwrap();
        scan(&db, &root);
        let id = root_node(&db, root.id);
        let book = books(&db.connect().unwrap(), id).unwrap().remove(0);
        fs::write(path.join("2.png"), image()).unwrap();
        scan(&db, &root);
        assert!(
            comic_reader::read_page_at_revision(&db, book.id, 0, Some(&book.revision)).is_err()
        );
        assert!(comic_reader::progress_at_revision(&db, book.id, 0, Some(&book.revision)).is_err());
        assert!(
            comic_reader::bookmark_at_revision(&db, book.id, 0, true, Some(&book.revision))
                .is_err()
        );
        let too_big = File::create(path.join("3.png")).unwrap();
        too_big.set_len(MAX_PAGE_BYTES + 1).unwrap();
        assert!(scan(&db, &root).progress().errors > 0);
        assert_eq!(books(&db.connect().unwrap(), id).unwrap()[0].page_count, 2);
    }

    #[test]
    fn unchanged_archive_reuses_pages_and_keeps_progress() {
        let (_temp, db, root) = fixture();
        archive(&Path::new(&root.path).join("Book.cbz"), &["1.png", "2.png"]);
        scan(&db, &root);
        let conn = db.connect().unwrap();
        let book = books(&conn, root_node(&db, root.id)).unwrap().remove(0);
        comic_reader::progress(&db, book.id, 1).unwrap();
        let page_ids = || {
            conn.prepare("SELECT id FROM comic_pages ORDER BY id")
                .unwrap()
                .query_map([], |r| r.get::<_, i64>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        let original = page_ids();
        scan(&db, &root);
        assert_eq!(page_ids(), original);
        assert_eq!(
            books(&conn, book.node_id).unwrap()[0]
                .progress
                .as_ref()
                .unwrap()
                .last_page_index,
            1
        );
    }

    #[cfg(windows)]
    #[test]
    fn reader_rejects_junction_escape_after_indexing() {
        let (temp, db, root) = fixture();
        let source = Path::new(&root.path).join("Pages");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("1.png"), image()).unwrap();
        scan(&db, &root);
        let id: i64 = db
            .connect()
            .unwrap()
            .query_row("SELECT id FROM nodes WHERE folder_name='Pages'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let book = books(&db.connect().unwrap(), id).unwrap().remove(0);
        let outside = temp.path().join("Outside");
        fs::rename(&source, &outside).unwrap();
        let result = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&source)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(comic_reader::read_page(&db, book.id, 0)
            .unwrap_err()
            .contains("OUTSIDE_ROOT"));
        fs::remove_dir(&source).unwrap();
    }

    #[test]
    fn comic_incremental_scan_reuses_baseline_and_preserves_it_on_failure() {
        let (_temp, db, root) = fixture();
        let source = Path::new(&root.path).join("Book");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("1.png"), image()).unwrap();
        let background = || {
            let control = control(root.id);
            control.progress.lock().unwrap().background = true;
            scanner::run_scan(
                None,
                &db,
                vec![ScanTarget {
                    root: root.clone(),
                    path: PathBuf::from(&root.path),
                    parent_node_id: None,
                }],
                &control,
                &["mp4".into()],
            );
            control
        };
        assert_eq!(background().progress().library_changed, Some(true));
        let c = db.connect().unwrap();
        let node = c
            .query_row("SELECT id FROM nodes WHERE folder_name='Book'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let book = books(&c, node).unwrap().remove(0);
        comic_reader::progress(&db, book.id, 0).unwrap();
        assert_eq!(background().progress().library_changed, Some(false));
        let baseline = || {
            c.query_row(
                "SELECT snapshot_json FROM library_scan_snapshots WHERE library_root_id=?1",
                [root.id],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
        };
        let original = baseline();
        let failure = crate::incremental::fail_directory_read(source.clone());
        assert!(background().progress().errors > 0);
        drop(failure);
        assert_eq!(baseline(), original);
        assert_eq!(books(&c, node).unwrap()[0].id, book.id);
        fs::write(source.join("2.png"), image()).unwrap();
        assert_eq!(background().progress().library_changed, Some(true));
        assert_eq!(books(&c, node).unwrap()[0].page_count, 2);
        assert_eq!(
            books(&c, node).unwrap()[0]
                .progress
                .as_ref()
                .unwrap()
                .last_page_index,
            0
        );
    }

    #[test]
    fn central_directory_is_bounded_before_metadata_allocation() {
        let (_temp, _db, root) = fixture();
        let path = Path::new(&root.path).join("Book.cbz");
        archive(&path, &["1.png"]);
        let original = fs::read(&path).unwrap();
        let end = original
            .windows(4)
            .rposition(|b| b == b"PK\x05\x06")
            .unwrap();
        let mut many = original.clone();
        many[end + 8..end + 12].copy_from_slice(&[0xfe, 0xff, 0xfe, 0xff]);
        fs::write(&path, many).unwrap();
        assert_eq!(
            archive_pages(&path, None).err().unwrap(),
            "COMIC_PAGE_LIMIT"
        );
        let mut huge = original.clone();
        huge[end + 12..end + 16].copy_from_slice(&(17u32 * 1024 * 1024).to_le_bytes());
        fs::write(&path, huge).unwrap();
        assert_eq!(
            archive_pages(&path, None).err().unwrap(),
            "COMIC_PAGE_LIMIT"
        );
        let mut zip64 = original.clone();
        zip64[end + 8..end + 12].fill(255);
        fs::write(&path, zip64).unwrap();
        assert_eq!(
            archive_pages(&path, None).err().unwrap(),
            "COMIC_ARCHIVE_DAMAGED"
        );
        fs::write(&path, original).unwrap();
        assert_eq!(archive_pages(&path, None).unwrap().len(), 1);
    }

    #[test]
    fn bounded_zip64_archive_is_supported() {
        let (_temp, _db, root) = fixture();
        let path = Path::new(&root.path).join("Book.cbz");
        archive(&path, &["1.png"]);
        let bytes = fs::read(&path).unwrap();
        let end = bytes.windows(4).rposition(|b| b == b"PK\x05\x06").unwrap();
        let mut record = [0u8; 56];
        record[..4].copy_from_slice(b"PK\x06\x06");
        record[4..12].copy_from_slice(&44u64.to_le_bytes());
        record[12..16].copy_from_slice(&[45, 0, 45, 0]);
        record[24..32].copy_from_slice(&1u64.to_le_bytes());
        record[32..40].copy_from_slice(&1u64.to_le_bytes());
        for (from, to) in [(12, 40), (16, 48)] {
            record[to..to + 8].copy_from_slice(
                &u64::from(u32::from_le_bytes(
                    bytes[end + from..end + from + 4].try_into().unwrap(),
                ))
                .to_le_bytes(),
            );
        }
        let mut locator = [0u8; 20];
        locator[..4].copy_from_slice(b"PK\x06\x07");
        locator[8..16].copy_from_slice(&(end as u64).to_le_bytes());
        locator[16..20].copy_from_slice(&1u32.to_le_bytes());
        let mut tail = bytes[end..].to_vec();
        tail[8..20].fill(255);
        let result = [&bytes[..end], &record, &locator, &tail].concat();
        fs::write(&path, &result).unwrap();
        assert_eq!(archive_pages(&path, None).unwrap().len(), 1);
        let mut forged = result;
        forged[end + 24..end + 32].copy_from_slice(&20_001u64.to_le_bytes());
        forged[end + 32..end + 40].copy_from_slice(&20_001u64.to_le_bytes());
        fs::write(&path, forged).unwrap();
        assert_eq!(
            archive_pages(&path, None).err().unwrap(),
            "COMIC_PAGE_LIMIT"
        );
    }
    #[test]
    fn independent_epubs_share_persistent_book_covers_and_content_position_guards() {
        use crate::poster_cache;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("synthetic-library");
        fs::create_dir(&source).unwrap();
        let db = Database::new(temp.path().join("test.sqlite"));
        db.migrate().unwrap();
        let root = db
            .add_root_with_strategy(
                &source,
                None,
                LibraryMediaKind::Ebook,
                LibraryRecognitionMode::Folder,
                false,
                "SMART_MIXED",
            )
            .unwrap();
        let mut originals = Vec::new();
        for i in 0..4 {
            let path = Path::new(&root.path).join(format!("Independent {i}.epub"));
            let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
            for (name,text) in [("META-INF/container.xml","<container><rootfiles><rootfile full-path='book.opf'/></rootfiles></container>"),("book.opf","<package><metadata><meta name='cover' content='cover'/></metadata><manifest><item id='one' href='one.xhtml' media-type='application/xhtml+xml'/><item id='cover' href='cover.png' media-type='image/png' properties='cover-image'/><item id='nav' href='nav.xhtml' media-type='application/xhtml+xml' properties='nav'/></manifest><spine><itemref idref='one'/></spine></package>"),("nav.xhtml","<html xmlns:epub='http://www.idpf.org/2007/ops'><nav epub:type='toc'><a href='one.xhtml'>A semantic chapter</a><a href='one.xhtml#section'>Second section</a></nav></html>"),("one.xhtml","<html><body><p>First paragraph</p><h2 id='section'>Second section</h2><p>Last paragraph</p></body></html>")] {zip.start_file(name,zip::write::SimpleFileOptions::default()).unwrap();zip.write_all(text.as_bytes()).unwrap();}
            zip.start_file("cover.png", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&image()).unwrap();
            zip.finish().unwrap();
            originals.push((path.clone(), fs::read(path).unwrap()));
        }
        scan(&db, &root);
        let node = root_node(&db, root.id);
        let books = books(&db.connect().unwrap(), node).unwrap();
        assert_eq!(books.len(), 4);
        crate::smart_mixed::rebuild(&db, root.id, Arc::new(AtomicBool::new(false))).unwrap();
        let catalogue = crate::smart_mixed::catalogue(&db, root.id).unwrap();
        assert_eq!(catalogue.groups.len(), 4);
        assert!(catalogue.groups.iter().all(|g| g.cover_node.is_none()));
        let cache_root = temp.path().join("covers");
        crate::cache::ensure_directories(&cache_root).unwrap();
        for book in &books {
            let first =
                poster_cache::book_cover_data_url(&db, &cache_root, book.id, &book.revision, 384)
                    .unwrap()
                    .unwrap();
            assert!(first.starts_with("data:image/"));
            let reopened = Database::new(db.path().into());
            assert_eq!(
                poster_cache::book_cover_data_url(
                    &reopened,
                    &cache_root,
                    book.id,
                    &book.revision,
                    384
                )
                .unwrap(),
                Some(first)
            );
            assert!(
                poster_cache::book_cover_data_url(&db, &cache_root, book.id, "stale", 384).is_err()
            );
        }
        let book = &books[0];
        let position = TextPosition {
            block_index: 2,
            character_offset: 7,
        };
        comic_reader::progress_with_position_at_revision(
            &db,
            book.id,
            0,
            Some(&book.revision),
            Some(position.clone()),
        )
        .unwrap();
        comic_reader::bookmark_with_position_at_revision(
            &db,
            book.id,
            0,
            true,
            Some(&book.revision),
            Some(position.clone()),
        )
        .unwrap();
        let opened = comic_reader::open(&db, book.id).unwrap();
        assert_eq!(opened.navigation.len(), 2);
        assert_eq!(opened.pages[0].page_name, "A semantic chapter");
        assert_eq!(opened.navigation[1].fragment.as_deref(), Some("section"));
        assert_eq!(
            opened
                .book
                .progress
                .unwrap()
                .text_position
                .unwrap()
                .character_offset,
            7
        );
        assert_eq!(opened.bookmark_positions[&0].block_index, 2);
        assert!(comic_reader::progress_with_position_at_revision(
            &db,
            book.id,
            0,
            Some("stale"),
            Some(position)
        )
        .is_err());
        for invalid in [
            TextPosition {
                block_index: -1,
                character_offset: 0,
            },
            TextPosition {
                block_index: 0,
                character_offset: 4194305,
            },
        ] {
            assert!(comic_reader::bookmark_with_position_at_revision(
                &db,
                book.id,
                0,
                true,
                Some(&book.revision),
                Some(invalid)
            )
            .is_err());
        }
        assert_eq!(
            comic_reader::open(&db, book.id).unwrap().bookmark_positions[&0].character_offset,
            7
        );
        scan(&db, &root);
        assert_eq!(
            comic_reader::open(&db, book.id)
                .unwrap()
                .book
                .progress
                .unwrap()
                .text_position
                .unwrap()
                .block_index,
            2
        );
        for (path, bytes) in originals {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }
}

//! Read-only screenshot sources, application-owned registration and lightweight curation.
//! No source mutation, filesystem URLs or base64 transport. Thumbnails use an isolated application cache.
use crate::{
    collection::Target,
    db::{AppResult, Database},
};
use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
    sync::{Condvar, Mutex},
    time::{Duration, Instant},
};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
const HEADER_BYTES: u64 = 256 * 1024;
const MAX_ENTRIES: usize = 20_000;
const MAX_IMAGES: usize = 10_000;
const MAX_FOLDERS: i64 = 128;
static MUTATION: Mutex<()> = Mutex::new(());
static READS: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());
struct Permit;
impl Permit {
    fn acquire() -> AppResult<Self> {
        let count = READS.0.lock().map_err(|_| "SCREENSHOT_READ_FAILED")?;
        let (mut count, timeout) = READS
            .1
            .wait_timeout_while(count, Duration::from_secs(20), |n| *n >= 4)
            .map_err(|_| "SCREENSHOT_READ_FAILED")?;
        if timeout.timed_out() && *count >= 4 {
            return Err("SCREENSHOT_LIMIT".into());
        }
        *count += 1;
        Ok(Self)
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        if let Ok(mut count) = READS.0.lock() {
            *count = count.saturating_sub(1);
            READS.1.notify_one();
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotFolder {
    pub id: i64,
    pub path: String,
    pub alias: String,
    pub note: String,
    pub target: Option<Target>,
    pub node_id: Option<i64>,
    pub created_at: String,
    pub image_count: i64,
    pub auto_match: AutoMatchScreenshotsResult,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoMatchScreenshotsResult {
    pub matched_count: i64,
    pub matched_by_playback: i64,
    pub matched_by_title: i64,
    pub matched_ids: Vec<i64>,
    pub limited: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotImage {
    pub id: i64,
    pub folder_id: i64,
    pub file_name: String,
    pub relative_path: String,
    pub revision: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub note: String,
    pub tags: Vec<String>,
    pub starred: bool,
    pub target: Option<Target>,
    pub own_target: Option<Target>,
    pub node_id: Option<i64>,
    pub target_title: Option<String>,
}
#[derive(Serialize)]
pub struct ScreenshotPage {
    pub items: Vec<ScreenshotImage>,
    pub total: i64,
}
#[derive(Serialize)]
pub struct ScreenshotLocation {
    pub image: ScreenshotImage,
    pub offset: i64,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScreenshotSort {
    #[default]
    Modified,
    Name,
    Size,
}
impl ScreenshotSort {
    fn expression(self) -> &'static str {
        match self {
            Self::Modified => "i.modified_at",
            Self::Name => "i.file_name COLLATE NOCASE",
            Self::Size => "i.file_size",
        }
    }
}
pub fn locate(
    db: &Database,
    id: i64,
    sort: Option<ScreenshotSort>,
    descending: Option<bool>,
) -> AppResult<ScreenshotLocation> {
    db.read_snapshot(|c| {
        let image = image(c, id)?;
        let expression = sort.unwrap_or_default().expression();
        let direction = if descending.unwrap_or(true) { "DESC" } else { "ASC" };
        let preceding: i64 = sql(c.query_row(&format!(
            "SELECT position FROM (SELECT i.id, row_number() OVER (ORDER BY {expression} {direction}, i.id {direction})-1 AS position FROM screenshot_images i WHERE i.available=1 AND i.folder_id=?1) WHERE id=?2"
        ), params![image.folder_id,id], |r|r.get(0)))?;
        Ok(ScreenshotLocation { image, offset: preceding / 48 * 48 })
    })
}
fn sql<T>(v: rusqlite::Result<T>) -> AppResult<T> {
    v.map_err(|e| e.to_string())
}
fn target(n: Option<i64>, b: Option<i64>) -> Option<Target> {
    n.map(Target::Node).or_else(|| b.map(Target::Book))
}
fn ids(t: Option<Target>) -> (Option<i64>, Option<i64>) {
    match t {
        Some(Target::Node(n)) => (Some(n), None),
        Some(Target::Book(b)) => (None, Some(b)),
        None => (None, None),
    }
}
fn validate_target(c: &Connection, t: Option<Target>) -> AppResult<()> {
    if let Some(t) = t {
        let n = crate::collection::validate(c, t)?;
        if matches!(t, Target::Node(_)) {
            crate::collection::validate_metadata_node(c, n)?;
        }
    }
    Ok(())
}
fn visible_target(c: &Connection, t: Option<Target>) -> Option<Target> {
    t.filter(|t| validate_target(c, Some(*t)).is_ok())
}
fn target_title(c: &Connection, t: Option<Target>) -> AppResult<Option<String>> {
    match t {
        None => Ok(None),
        Some(Target::Book(id)) => sql(c
            .query_row(
                "SELECT display_name FROM comic_books WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()),
        Some(Target::Node(id)) => {
            let mut s = sql(c.prepare_cached("SELECT CASE WHEN n.manual_display_name=1 THEN n.display_name ELSE COALESCE(NULLIF(json_extract(CASE WHEN json_valid(tm.payload_json) THEN tm.payload_json ELSE '{}' END,'$.title'),''),NULLIF(CASE coalesce((SELECT value FROM settings WHERE key='language'),'zh-CN') WHEN 'en-US' THEN m.provider_title_en WHEN 'ja-JP' THEN m.provider_title_ja WHEN 'ko-KR' THEN m.provider_title_ko ELSE m.provider_title_cn END,''),NULLIF(m.provider_title_cn,''),NULLIF(m.provider_title,''),NULLIF(n.display_name,''),n.folder_name) END FROM nodes n LEFT JOIN metadata_bindings m ON m.id=(SELECT id FROM metadata_bindings WHERE node_id=n.id ORDER BY id DESC LIMIT 1) LEFT JOIN tmdb_movie_bindings tm ON tm.node_id=n.id AND tm.active=1 WHERE n.id=?1"))?;
            sql(s.query_row([id], |r| r.get(0)).optional())
        }
    }
}
fn folder_row(r: &Row<'_>) -> rusqlite::Result<ScreenshotFolder> {
    Ok(ScreenshotFolder {
        id: r.get(0)?,
        path: r.get(1)?,
        alias: r.get(2)?,
        note: r.get(3)?,
        target: target(r.get(4)?, r.get(5)?),
        node_id: None,
        created_at: r.get(6)?,
        image_count: r.get(7)?,
        auto_match: AutoMatchScreenshotsResult::default(),
    })
}
const FOLDERS:&str="SELECT f.id,f.path,f.alias,f.note,f.node_id,f.book_id,f.created_at,(SELECT count(*) FROM screenshot_images i WHERE i.folder_id=f.id AND i.available=1) FROM screenshot_folders f";
pub fn folders(db: &Database) -> AppResult<Vec<ScreenshotFolder>> {
    db.read_snapshot(|c| {
        let mut s = sql(c.prepare(&format!("{FOLDERS} ORDER BY f.created_at DESC,f.id DESC")))?;
        let mut values =
            sql(sql(s.query_map([], folder_row))?.collect::<rusqlite::Result<Vec<_>>>())?;
        for f in &mut values {
            f.target = visible_target(c, f.target);
            f.node_id = f
                .target
                .and_then(|t| crate::collection::validate(c, t).ok());
        }
        Ok(values)
    })
}
fn folder(c: &Connection, id: i64) -> AppResult<ScreenshotFolder> {
    let mut f = sql(c
        .query_row(&format!("{FOLDERS} WHERE f.id=?1"), [id], folder_row)
        .optional())?
    .ok_or("SCREENSHOT_NOT_FOUND")?;
    f.target = visible_target(c, f.target);
    f.node_id = f
        .target
        .and_then(|t| crate::collection::validate(c, t).ok());
    Ok(f)
}

fn plain(path: &Path, is_dir: bool) -> AppResult<()> {
    let m = fs::symlink_metadata(path).map_err(|_| "SCREENSHOT_PATH")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("SCREENSHOT_PATH".into());
        }
    }
    if m.file_type().is_symlink() || (is_dir && !m.is_dir()) || (!is_dir && !m.is_file()) {
        return Err("SCREENSHOT_PATH".into());
    }
    Ok(())
}
fn sensitive_component(path: &Path) -> bool {
    path.components()
        .filter_map(|p| match p {
            Component::Normal(n) => Some(n.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .any(|n| {
            matches!(
                n.as_str(),
                "windows"
                    | "program files"
                    | "program files (x86)"
                    | "programdata"
                    | "appdata"
                    | ".ssh"
                    | ".aws"
                    | ".azure"
                    | ".gnupg"
                    | ".codex"
                    | ".git"
                    | ".git-codex-local"
                    | "system volume information"
                    | "$recycle.bin"
                    | "m2shelf-production-key"
            )
        })
}
fn safe_root(db: &Database, path: &Path) -> AppResult<PathBuf> {
    if !path.is_absolute()
        || path.as_os_str().len() > 32768
        || path
            .components()
            .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
        || sensitive_component(path)
    {
        return Err("SCREENSHOT_PATH".into());
    }
    // Reject both filesystem roots and broad profile registrations, including their ancestors.
    #[cfg(windows)]
    if !matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(),std::path::Prefix::Disk(_)|std::path::Prefix::VerbatimDisk(_)|std::path::Prefix::UNC(_,_)|std::path::Prefix::VerbatimUNC(_,_)))
    {
        return Err("SCREENSHOT_PATH".into());
    }
    if path.parent().is_none()
        || path
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count()
            < 1
    {
        return Err("SCREENSHOT_PATH".into());
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        if Path::new(&profile).starts_with(path) {
            return Err("SCREENSHOT_PATH".into());
        }
    }
    for p in path.ancestors() {
        plain(p, true)?;
    }
    let root = fs::canonicalize(path).map_err(|_| "SCREENSHOT_PATH")?;
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        let normalized_root = crate::db::display_path(&root)
            .replace('/', "\\")
            .to_lowercase();
        let normalized_profile = profile.to_string_lossy().replace('/', "\\").to_lowercase();
        if normalized_profile == normalized_root
            || normalized_profile
                .starts_with(&(normalized_root.trim_end_matches('\\').to_owned() + "\\"))
        {
            return Err("SCREENSHOT_PATH".into());
        }
    }
    if sensitive_component(&root) {
        return Err("SCREENSHOT_PATH".into());
    }
    if let Some(parent) = db.path().parent() {
        if let Ok(app) = fs::canonicalize(parent) {
            if root.starts_with(&app) || app.starts_with(&root) {
                return Err("SCREENSHOT_PATH".into());
            }
        }
    }
    if crate::workspace_files::overlaps_cover_cache(db, &root).map_err(|_| "SCREENSHOT_PATH")? {
        return Err("SCREENSHOT_PATH".into());
    }
    Ok(root)
}
fn safe_file(root: &Path, relative: &str) -> AppResult<PathBuf> {
    let rel = Path::new(relative);
    if rel.is_absolute()
        || relative.is_empty()
        || relative.len() > 4096
        || relative.contains(['\0', ':'])
        || rel.components().any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err("SCREENSHOT_PATH".into());
    }
    if sensitive_component(&root.join(rel)) {
        return Err("SCREENSHOT_PATH".into());
    }
    let mut path = root.to_path_buf();
    for part in rel.components() {
        // Check each ancestor before touching a child through it.
        plain(&path, true)?;
        path.push(part.as_os_str());
    }
    plain(&path, false)?;
    let resolved = fs::canonicalize(&path).map_err(|_| "SCREENSHOT_PATH")?;
    if !resolved.starts_with(root) {
        return Err("SCREENSHOT_PATH".into());
    }
    Ok(resolved)
}
fn supported(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "avif" | "bmp"
        )
    })
}
fn avif_sizes(bytes: &[u8], depth: u8, sizes: &mut Vec<(u32, u32)>) -> AppResult<()> {
    if depth > 8 {
        return Err("SCREENSHOT_FORMAT".into());
    }
    let mut offset = 0;
    let mut count = 0;
    while offset < bytes.len() {
        if bytes.len() - offset < 8 {
            return Err("SCREENSHOT_FORMAT".into());
        }
        count += 1;
        if count > 2048 {
            return Err("SCREENSHOT_LIMIT".into());
        }
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        if size < 8 || size > bytes.len() - offset {
            return Err("SCREENSHOT_FORMAT".into());
        }
        let kind = &bytes[offset + 4..offset + 8];
        let payload = &bytes[offset + 8..offset + size];
        if kind == b"ispe" {
            if payload.len() != 12 || sizes.len() >= 16 {
                return Err("SCREENSHOT_FORMAT".into());
            }
            let w = u32::from_be_bytes(payload[4..8].try_into().unwrap());
            let h = u32::from_be_bytes(payload[8..12].try_into().unwrap());
            if w == 0
                || h == 0
                || w > 32768
                || h > 32768
                || u64::from(w) * u64::from(h) > 20_000_000
            {
                return Err("SCREENSHOT_LIMIT".into());
            }
            sizes.push((w, h));
        }
        if matches!(kind, b"meta" | b"iprp" | b"ipco") {
            let skip = if kind == b"meta" { 4 } else { 0 };
            if payload.len() < skip {
                return Err("SCREENSHOT_FORMAT".into());
            }
            avif_sizes(&payload[skip..], depth + 1, sizes)?;
        }
        offset += size;
    }
    Ok(())
}
pub(crate) fn dimensions(bytes: &[u8]) -> AppResult<(&'static str, u32, u32)> {
    let mime = crate::comic_reader::image_mime(bytes).map_err(|_| "SCREENSHOT_FORMAT")?;
    if !matches!(
        mime,
        "image/png" | "image/jpeg" | "image/webp" | "image/avif" | "image/bmp"
    ) {
        return Err("SCREENSHOT_FORMAT".into());
    }
    let (w, h) = if mime == "image/avif" {
        let mut sizes = Vec::new();
        avif_sizes(bytes, 0, &mut sizes)?;
        if sizes
            .iter()
            .map(|(w, h)| u64::from(*w) * u64::from(*h))
            .sum::<u64>()
            > 40_000_000
        {
            return Err("SCREENSHOT_LIMIT".into());
        }
        sizes
            .into_iter()
            .max_by_key(|(w, h)| u64::from(*w) * u64::from(*h))
            .ok_or("SCREENSHOT_FORMAT")?
    } else {
        crate::comic_reader::image_dimensions(bytes).map_err(|_| "SCREENSHOT_FORMAT")?
    };
    if u64::from(w) * u64::from(h) > 20_000_000 {
        return Err("SCREENSHOT_LIMIT".into());
    }
    Ok((mime, w, h))
}
fn revision(size: u64, stamp: &str, header: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(size.to_le_bytes());
    h.update(stamp.as_bytes());
    h.update(header);
    format!("{:x}", h.finalize())
}
struct Indexed {
    relative: String,
    name: String,
    size: u64,
    stamp: String,
    revision: String,
    mime: String,
    w: u32,
    h: u32,
    captured_at: Option<i64>,
}
fn capture_time(meta: &fs::Metadata) -> Option<i64> {
    // Copying an old screenshot often gives it a new creation time while retaining
    // its old modification time. Prefer the older evidence so a copy never looks
    // like a screenshot taken during the currently playing work.
    let modified = meta.modified().ok()?;
    let stable = meta
        .created()
        .ok()
        .map_or(modified, |created| created.min(modified));
    stable
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
}
fn inspect(root: &Path, relative: &str) -> AppResult<Indexed> {
    let path = safe_file(root, relative)?;
    let meta = fs::metadata(&path).map_err(|_| "SCREENSHOT_PATH")?;
    if meta.len() == 0 || meta.len() > MAX_BYTES {
        return Err("SCREENSHOT_LIMIT".into());
    }
    let stamp = crate::comics::modified(&meta);
    let mut file = crate::comic_reader::checked_file(&path, root, meta.len(), &stamp)
        .map_err(|_| "SCREENSHOT_CHANGED")?;
    let mut header = Vec::new();
    file.by_ref()
        .take(HEADER_BYTES)
        .read_to_end(&mut header)
        .map_err(|_| "SCREENSHOT_READ_FAILED")?;
    // AVIF properties can follow a large media box. Read this one bounded file to inspect
    // every image-size property; never trust only the first ispe or truncated metadata.
    if crate::comic_reader::image_mime(&header).ok() == Some("image/avif") {
        file.by_ref()
            .take(MAX_BYTES + 1 - HEADER_BYTES.min(header.len() as u64))
            .read_to_end(&mut header)
            .map_err(|_| "SCREENSHOT_READ_FAILED")?;
        if header.len() as u64 != meta.len() {
            return Err("SCREENSHOT_CHANGED".into());
        }
    }
    crate::comic_reader::validate_file_handle(&file, &path, root, meta.len(), &stamp)
        .map_err(|_| "SCREENSHOT_CHANGED")?;
    let (mime, w, h) = dimensions(&header)?;
    Ok(Indexed {
        relative: relative.into(),
        name: path
            .file_name()
            .ok_or("SCREENSHOT_PATH")?
            .to_string_lossy()
            .into_owned(),
        size: meta.len(),
        stamp: stamp.clone(),
        revision: revision(
            meta.len(),
            &stamp,
            &header[..header.len().min(HEADER_BYTES as usize)],
        ),
        mime: mime.into(),
        w,
        h,
        captured_at: capture_time(&file.metadata().map_err(|_| "SCREENSHOT_READ_FAILED")?),
    })
}
fn scan(db: &Database, path: &Path) -> AppResult<(PathBuf, Vec<Indexed>)> {
    let root = safe_root(db, path)?;
    let mut stack = vec![root.clone()];
    let mut images = Vec::new();
    let mut count = 0;
    let start = Instant::now();
    while let Some(dir) = stack.pop() {
        plain(&dir, true)?;
        let canonical = fs::canonicalize(&dir).map_err(|_| "SCREENSHOT_PATH")?;
        if !canonical.starts_with(&root) {
            return Err("SCREENSHOT_PATH".into());
        }
        for entry in fs::read_dir(&dir).map_err(|_| "SCREENSHOT_READ_FAILED")? {
            count += 1;
            if count > MAX_ENTRIES || start.elapsed() > Duration::from_secs(30) {
                return Err("SCREENSHOT_LIMIT".into());
            }
            let entry = entry.map_err(|_| "SCREENSHOT_READ_FAILED")?;
            let path = entry.path();
            let m = fs::symlink_metadata(&path).map_err(|_| "SCREENSHOT_READ_FAILED")?;
            // Links and sensitive descendants are never followed, including in an otherwise valid folder.
            if sensitive_component(&path) || plain(&path, m.is_dir()).is_err() {
                continue;
            }
            if m.is_dir() {
                if stack.len() >= 1024 {
                    return Err("SCREENSHOT_LIMIT".into());
                }
                stack.push(path);
                continue;
            }
            if !supported(&path) {
                continue;
            }
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| "SCREENSHOT_PATH")?
                .to_string_lossy()
                .into_owned();
            match inspect(&root, &relative) {
                Ok(i) => images.push(i),
                Err(e) if e == "SCREENSHOT_FORMAT" || e == "SCREENSHOT_LIMIT" => continue,
                Err(e) => return Err(e),
            }
            if images.len() > MAX_IMAGES {
                return Err("SCREENSHOT_LIMIT".into());
            }
        }
    }
    if safe_root(db, path)? != root {
        return Err("SCREENSHOT_CHANGED".into());
    }
    images.sort_by(|a, b| crate::db::natural_cmp(&a.relative, &b.relative));
    Ok((root, images))
}
fn store_scan(c: &Connection, id: i64, images: &[Indexed]) -> AppResult<()> {
    // Missing files become unavailable; their IDs, links, notes and stars survive a later return.
    sql(c.execute(
        "UPDATE screenshot_images SET available=0 WHERE folder_id=?1",
        [id],
    ))?;
    for i in images {
        sql(c.execute("INSERT INTO screenshot_images(folder_id,relative_path,file_name,file_size,modified_at,revision,mime,width,height) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(folder_id,relative_path) DO UPDATE SET file_name=excluded.file_name,file_size=excluded.file_size,modified_at=excluded.modified_at,revision=excluded.revision,mime=excluded.mime,width=excluded.width,height=excluded.height,available=1",params![id,i.relative,i.name,i.size,i.stamp,i.revision,i.mime,i.w,i.h]))?;
    }
    Ok(())
}
pub fn import(db: &Database, path: String) -> AppResult<ScreenshotFolder> {
    let _lock = MUTATION.lock().map_err(|_| "SCREENSHOT_READ_FAILED")?;
    let (root, images) = scan(db, Path::new(&path))?;
    let path = crate::db::display_path(&root);
    let mut c = db.connect()?;
    let tx = sql(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
    let existing: Option<i64> = sql(tx
        .query_row(
            "SELECT id FROM screenshot_folders WHERE path=?1 COLLATE NOCASE",
            [&path],
            |r| r.get(0),
        )
        .optional())?;
    let id = if let Some(id) = existing {
        id
    } else {
        if sql(
            tx.query_row("SELECT count(*) FROM screenshot_folders", [], |r| {
                r.get::<_, i64>(0)
            }),
        )? >= MAX_FOLDERS
        {
            return Err("SCREENSHOT_LIMIT".into());
        }
        let alias = root
            .file_name()
            .ok_or("SCREENSHOT_PATH")?
            .to_string_lossy()
            .chars()
            .take(240)
            .collect::<String>();
        sql(tx.execute(
            "INSERT INTO screenshot_folders(path,alias) VALUES(?1,?2)",
            params![path, alias],
        ))?;
        tx.last_insert_rowid()
    };
    store_scan(&tx, id, &images)?;
    let mut result = folder(&tx, id)?;
    result.auto_match = auto_match_conn(db, &tx, Some(id), Some(&images))?;
    sql(tx.commit())?;
    Ok(result)
}
pub fn refresh(db: &Database, id: i64) -> AppResult<ScreenshotFolder> {
    let _lock = MUTATION.lock().map_err(|_| "SCREENSHOT_READ_FAILED")?;
    let f = folder(&db.connect()?, id)?;
    let (_, images) = scan(db, Path::new(&f.path))?;
    let mut c = db.connect()?;
    let tx = sql(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
    folder(&tx, id)?;
    store_scan(&tx, id, &images)?;
    let mut result = folder(&tx, id)?;
    result.auto_match = auto_match_conn(db, &tx, Some(id), Some(&images))?;
    sql(tx.commit())?;
    Ok(result)
}

enum PlaybackMatch {
    None,
    Ambiguous,
    Node(i64),
}
fn directory_key(path: &str) -> String {
    let path = path.replace('/', "\\").to_lowercase();
    let plain = if let Some(unc) = path.strip_prefix(r"\\?\unc\") {
        format!(r"\\{unc}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(&path).to_owned()
    };
    plain.trim_end_matches('\\').to_owned()
}
fn playback_match(c: &Connection, captured: i64, parent: &Path) -> AppResult<PlaybackMatch> {
    // A stale open session has at most its last heartbeat, never an infinite end.
    // Matching uses the original indexed file Root and currently visible owner.
    let mut s = sql(c.prepare("SELECT p.node_id,p.screenshot_directory FROM mpv_playback_sessions p JOIN media_files m ON m.id=p.media_file_id JOIN nodes source ON source.id=m.node_id JOIN nodes owner ON owner.id=p.node_id WHERE source.library_root_id=owner.library_root_id AND p.started_at<=?1+3000 AND min(coalesce(p.ended_at,p.heartbeat_at),p.heartbeat_at+15000)>=?1-15000 ORDER BY p.started_at DESC LIMIT 257"))?;
    let rows = sql(sql(s.query_map([captured], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
    }))?
    .collect::<rusqlite::Result<Vec<_>>>())?;
    if rows.len() > 256 {
        return Ok(PlaybackMatch::Ambiguous);
    }
    let directory = directory_key(&crate::db::display_path(parent));
    let mut exact = std::collections::BTreeSet::new();
    let mut unspecified = std::collections::BTreeSet::new();
    for (id, path) in rows {
        if validate_target(c, Some(Target::Node(id))).is_err() {
            continue;
        }
        match path {
            Some(path) if directory_key(&path) == directory => {
                exact.insert(id);
            }
            Some(_) => {} // Known other destinations cannot corroborate this source.
            None => {
                unspecified.insert(id);
            }
        }
    }
    let candidates = if exact.is_empty() { unspecified } else { exact };
    Ok(match candidates.len() {
        0 => PlaybackMatch::None,
        1 => PlaybackMatch::Node(*candidates.first().expect("one candidate")),
        _ => PlaybackMatch::Ambiguous,
    })
}
fn generic_screenshot_title(raw: &str) -> bool {
    let compact = crate::title_extractor::normalize_title_for_match(raw);
    if matches!(
        compact.as_str(),
        "screenshot"
            | "screenshots"
            | "shots"
            | "images"
            | "pictures"
            | "截图"
            | "截圖"
            | "スクリーンショット"
            | "스크린샷"
    ) {
        return true;
    }
    ["mpvshot", "screenshot", "snapshot", "image", "img", "shot"]
        .iter()
        .any(|prefix| {
            compact
                .strip_prefix(prefix)
                .is_some_and(|tail| tail.is_empty() || tail.chars().all(|c| c.is_ascii_digit()))
        })
}
struct TitleCandidate {
    id: i64,
    signals: crate::title_extractor::TitleSignals,
    guards: Vec<crate::title_extractor::TitleSignals>,
}
type TitleIndex = std::collections::HashMap<String, Vec<TitleCandidate>>;
fn title_index(c: &Connection) -> AppResult<TitleIndex> {
    let mut s = sql(c.prepare("SELECT n.id,n.folder_name,n.display_name,b.provider_title,b.provider_title_cn,b.provider_title_en,b.provider_title_ja,b.provider_title_ko,json_extract(CASE WHEN json_valid(tm.payload_json) THEN tm.payload_json ELSE '{}' END,'$.title'),json_extract(CASE WHEN json_valid(tm.payload_json) THEN tm.payload_json ELSE '{}' END,'$.originalTitle'),b.provider_date,json_extract(CASE WHEN json_valid(tm.payload_json) THEN tm.payload_json ELSE '{}' END,'$.releaseDate') FROM nodes n JOIN library_roots r ON r.id=n.library_root_id LEFT JOIN metadata_bindings b ON b.node_id=n.id LEFT JOIN tmdb_movie_bindings tm ON tm.node_id=n.id AND tm.active=1 WHERE r.media_kind='VIDEO' AND n.parent_node_id IS NOT NULL AND n.node_type IN ('WORK','AUTO_WORK') AND n.total_video_count>0 ORDER BY n.id LIMIT 10001"))?;
    let rows = sql(sql(s.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            (1..10)
                .map(|i| r.get::<_, Option<String>>(i))
                .collect::<rusqlite::Result<Vec<_>>>()?,
            (10..12)
                .map(|i| r.get::<_, Option<String>>(i))
                .collect::<rusqlite::Result<Vec<_>>>()?,
        ))
    }))?
    .collect::<rusqlite::Result<Vec<_>>>())?;
    // Do not pick a unique winner from a truncated catalogue.
    if rows.len() > 10_000 {
        return Ok(TitleIndex::new());
    }
    let mut index = TitleIndex::new();
    for (id, titles, dates) in rows {
        let mut guards: Vec<_> = titles
            .iter()
            .take(2)
            .flatten()
            .map(|title| crate::title_extractor::extract_title_signals(title))
            .collect();
        for date in dates.into_iter().flatten() {
            if let Some(year) = date
                .get(..4)
                .and_then(|s| s.parse::<i32>().ok())
                .filter(|year| (1900..=2099).contains(year))
            {
                let mut guard = crate::title_extractor::extract_title_signals("");
                guard.year = Some(year);
                guards.push(guard);
            }
        }
        for title in titles.into_iter().flatten().filter(|s| s.len() <= 2048) {
            let signals = crate::title_extractor::extract_title_signals(&title);
            if generic_screenshot_title(&signals.cleaned_title)
                || !crate::title_extractor::is_safe_movie_query(&signals.cleaned_title)
            {
                continue;
            }
            let key = crate::title_extractor::normalize_title_for_match(&signals.cleaned_title);
            index.entry(key).or_default().push(TitleCandidate {
                id,
                signals,
                guards: guards.clone(),
            });
        }
    }
    Ok(index)
}
fn title_match(c: &Connection, index: &TitleIndex, path: &Path, root: &Path) -> Option<i64> {
    let mut names = Vec::new();
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        names.push(stem);
    }
    for ancestor in path.parent().into_iter().flat_map(Path::ancestors) {
        if !ancestor.starts_with(root) {
            break;
        }
        if let Some(name) = ancestor.file_name().and_then(|s| s.to_str()) {
            names.push(name);
        }
        if ancestor == root {
            break;
        }
    }
    for raw in names {
        if generic_screenshot_title(raw) {
            continue;
        }
        let signals = crate::title_extractor::extract_title_signals(raw);
        if !crate::title_extractor::is_safe_movie_query(&signals.cleaned_title) {
            continue;
        }
        let key = crate::title_extractor::normalize_title_for_match(&signals.cleaned_title);
        let Some(candidates) = index.get(&key) else {
            continue;
        };
        let valid: std::collections::BTreeSet<_> = candidates.iter().filter(|candidate| {
            let other = &candidate.signals;
            !matches!((signals.year, other.year), (Some(a),Some(b)) if a!=b)
                && signals.season_number == other.season_number
                && (signals.edition_kind == crate::title_extractor::EditionKind::Unknown
                    || other.edition_kind == crate::title_extractor::EditionKind::Unknown
                    || signals.edition_kind == other.edition_kind)
                && candidate.guards.iter().all(|guard| {
                    !matches!((signals.year,guard.year), (Some(a),Some(b)) if a!=b)
                        && !matches!(guard.season_number, Some(season) if signals.season_number!=Some(season))
                        && (signals.edition_kind == crate::title_extractor::EditionKind::Unknown
                            || guard.edition_kind == crate::title_extractor::EditionKind::Unknown
                            || signals.edition_kind == guard.edition_kind)
                })
                && validate_target(c, Some(Target::Node(candidate.id))).is_ok()
        }).map(|candidate|candidate.id).collect();
        // An ambiguous explicit filename/folder is never overridden by an ancestor.
        if valid.len() > 1 {
            return None;
        }
        if let Some(id) = valid.first() {
            return Some(*id);
        }
        // An explicit recognized title with conflicting year/season/edition is
        // evidence against this assignment, not permission to try a parent name.
        return None;
    }
    None
}
fn auto_match_conn(
    db: &Database,
    c: &Connection,
    folder_id: Option<i64>,
    scanned: Option<&[Indexed]>,
) -> AppResult<AutoMatchScreenshotsResult> {
    if let Some(id) = folder_id {
        folder(c, id)?;
    }
    let mut s = sql(c.prepare("SELECT i.id,i.folder_id,i.relative_path,i.revision,f.path FROM screenshot_images i JOIN screenshot_folders f ON f.id=i.folder_id WHERE i.available=1 AND i.node_id IS NULL AND i.book_id IS NULL AND (?1 IS NULL OR i.folder_id=?1) ORDER BY i.id LIMIT 10001"))?;
    let rows = sql(sql(s.query_map([folder_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    }))?
    .collect::<rusqlite::Result<Vec<_>>>())?;
    let titles = title_index(c)?;
    let mut roots = std::collections::HashMap::<i64, Option<PathBuf>>::new();
    let fresh: std::collections::HashMap<_, _> = scanned
        .unwrap_or(&[])
        .iter()
        .map(|i| (i.relative.as_str(), i))
        .collect();
    let mut result = AutoMatchScreenshotsResult {
        limited: rows.len() > MAX_IMAGES,
        ..AutoMatchScreenshotsResult::default()
    };
    let start = Instant::now();
    for (id, folder_id, relative, revision, folder_path) in rows.into_iter().take(MAX_IMAGES) {
        if start.elapsed() > Duration::from_secs(30) {
            result.limited = true;
            break;
        }
        let Some(root) = roots
            .entry(folder_id)
            .or_insert_with(|| safe_root(db, Path::new(&folder_path)).ok())
            .as_ref()
        else {
            continue;
        };
        let Ok(path) = safe_file(root, &relative) else {
            continue;
        };
        let inspected;
        let indexed = if let Some(indexed) = fresh.get(relative.as_str()) {
            // Reuse this scan's validated header and capture timestamp. Recheck
            // the open handle and revision metadata without decoding AVIF twice.
            if crate::comic_reader::checked_file(&path, root, indexed.size, &indexed.stamp).is_err()
            {
                continue;
            }
            *indexed
        } else {
            inspected = match inspect(root, &relative) {
                Ok(value) => value,
                Err(_) => continue,
            };
            &inspected
        };
        if indexed.revision != revision {
            continue;
        }
        let playback = match (indexed.captured_at, path.parent()) {
            (Some(stamp), Some(parent)) => playback_match(c, stamp, parent)?,
            _ => PlaybackMatch::None,
        };
        let (node, by_playback) = match playback {
            PlaybackMatch::Node(id) => (id, true),
            PlaybackMatch::Ambiguous => continue,
            PlaybackMatch::None => match title_match(c, &titles, &path, root) {
                Some(id) => (id, false),
                None => continue,
            },
        };
        let count = sql(c.execute("UPDATE screenshot_images SET node_id=?1 WHERE id=?2 AND revision=?3 AND available=1 AND node_id IS NULL AND book_id IS NULL",params![node,id,revision]))?;
        if count == 1 {
            result.matched_count += 1;
            if by_playback {
                result.matched_by_playback += 1;
            } else {
                result.matched_by_title += 1;
            }
            result.matched_ids.push(id);
        }
    }
    Ok(result)
}
pub fn auto_match(db: &Database, folder_id: Option<i64>) -> AppResult<AutoMatchScreenshotsResult> {
    let _lock = MUTATION.lock().map_err(|_| "SCREENSHOT_READ_FAILED")?;
    let mut c = db.connect()?;
    let tx = sql(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
    let result = auto_match_conn(db, &tx, folder_id, None)?;
    sql(tx.commit())?;
    Ok(result)
}
fn text_limit(value: &str, max: usize) -> AppResult<()> {
    if value.chars().count() > max || value.len() > max * 4 || value.contains('\0') {
        Err("SCREENSHOT_LIMIT".into())
    } else {
        Ok(())
    }
}
pub fn update_folder(
    db: &Database,
    id: i64,
    alias: String,
    note: String,
    t: Option<Target>,
) -> AppResult<ScreenshotFolder> {
    text_limit(&alias, 240)?;
    text_limit(&note, 10000)?;
    if alias.trim().is_empty() {
        return Err("SCREENSHOT_LIMIT".into());
    }
    let mut c = db.connect()?;
    let tx = sql(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
    validate_target(&tx, t)?;
    let (n, b) = ids(t);
    if sql(tx.execute(
        "UPDATE screenshot_folders SET alias=?1,note=?2,node_id=?3,book_id=?4 WHERE id=?5",
        params![alias, note, n, b, id],
    ))? != 1
    {
        return Err("SCREENSHOT_NOT_FOUND".into());
    }
    let result = folder(&tx, id)?;
    sql(tx.commit())?;
    Ok(result)
}
pub fn remove(db: &Database, id: i64) -> AppResult<()> {
    let _lock = MUTATION.lock().map_err(|_| "SCREENSHOT_READ_FAILED")?;
    sql(db
        .connect()?
        .execute("DELETE FROM screenshot_folders WHERE id=?1", [id]))?;
    Ok(())
}
const IMAGES:&str="SELECT i.id,i.folder_id,i.file_name,i.relative_path,i.revision,i.mime,i.width,i.height,i.note,i.tags,i.starred,CASE WHEN i.node_id IS NULL AND i.book_id IS NULL THEN f.node_id ELSE i.node_id END,CASE WHEN i.node_id IS NULL AND i.book_id IS NULL THEN f.book_id ELSE i.book_id END,i.node_id,i.book_id FROM screenshot_images i JOIN screenshot_folders f ON f.id=i.folder_id";
fn image_row(r: &Row<'_>) -> rusqlite::Result<ScreenshotImage> {
    Ok(ScreenshotImage {
        id: r.get(0)?,
        folder_id: r.get(1)?,
        file_name: r.get(2)?,
        relative_path: r.get(3)?,
        revision: r.get(4)?,
        mime: r.get(5)?,
        width: r.get(6)?,
        height: r.get(7)?,
        note: r.get(8)?,
        tags: serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or_default(),
        starred: r.get(10)?,
        target: target(r.get(11)?, r.get(12)?),
        own_target: target(r.get(13)?, r.get(14)?),
        node_id: None,
        target_title: None,
    })
}
fn image(c: &Connection, id: i64) -> AppResult<ScreenshotImage> {
    let mut i = sql(c
        .query_row(
            &format!("{IMAGES} WHERE i.id=?1 AND i.available=1"),
            [id],
            image_row,
        )
        .optional())?
    .ok_or("SCREENSHOT_NOT_FOUND")?;
    i.target = visible_target(c, i.target);
    i.node_id = i
        .target
        .and_then(|t| crate::collection::validate(c, t).ok());
    i.target_title = target_title(c, i.target)?;
    Ok(i)
}
#[cfg(test)]
pub fn list(
    db: &Database,
    folder_id: Option<i64>,
    query: String,
    t: Option<Target>,
    starred: bool,
    offset: i64,
    limit: i64,
) -> AppResult<ScreenshotPage> {
    list_filtered(
        db, folder_id, query, t, starred, offset, limit, None, None, None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn list_filtered(
    db: &Database,
    folder_id: Option<i64>,
    query: String,
    t: Option<Target>,
    starred: bool,
    offset: i64,
    limit: i64,
    tag: Option<String>,
    sort: Option<ScreenshotSort>,
    descending: Option<bool>,
) -> AppResult<ScreenshotPage> {
    text_limit(&query, 500)?;
    if let Some(tag) = &tag {
        text_limit(tag, 80)?;
    }
    if offset < 0 || !(1..=100).contains(&limit) {
        return Err("SCREENSHOT_LIMIT".into());
    }
    let order = sort.unwrap_or_default().expression();
    let direction = if descending.unwrap_or(true) {
        "DESC"
    } else {
        "ASC"
    };
    db.read_snapshot(|c|{validate_target(c,t)?;let(n,b)=ids(t);
        let filter=" WHERE i.available=1 AND (?1 IS NULL OR i.folder_id=?1) AND (?2='' OR instr(lower(i.file_name||' '||i.note||' '||i.tags||' '||f.alias||' '||f.note),lower(?2))>0) AND (?3=0 OR i.starred=1) AND (?4 IS NULL OR CASE WHEN i.node_id IS NULL AND i.book_id IS NULL THEN f.node_id ELSE i.node_id END=?4) AND (?5 IS NULL OR CASE WHEN i.node_id IS NULL AND i.book_id IS NULL THEN f.book_id ELSE i.book_id END=?5)";
        let filter = format!("{filter} AND (?8 IS NULL OR EXISTS (SELECT 1 FROM json_each(i.tags) WHERE value=?8))");
        let total=sql(c.query_row(&format!("SELECT count(*) FROM screenshot_images i JOIN screenshot_folders f ON f.id=i.folder_id {filter}"),params![folder_id,query,starred,n,b,limit,offset,tag],|r|r.get(0)))?;
        let mut s=sql(c.prepare(&format!("{IMAGES}{filter} ORDER BY {order} {direction},i.id {direction} LIMIT ?6 OFFSET ?7")))?;
        let mut items=sql(sql(s.query_map(params![folder_id,query,starred,n,b,limit,offset,tag],image_row))?.collect::<rusqlite::Result<Vec<_>>>())?;
        for item in &mut items{item.target=visible_target(c,item.target);item.node_id=item.target.and_then(|t|crate::collection::validate(c,t).ok());item.target_title=target_title(c,item.target)?;}
        Ok(ScreenshotPage{items,total})
    })
}

/// A curation snapshot is independent of the source-file revision. Compare both so
/// a concurrent note/tag/star edit cannot be silently overwritten by a batch action.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScreenshotSelection {
    pub id: i64,
    pub revision: String,
    pub note: String,
    pub tags: Vec<String>,
    pub starred: bool,
    pub own_target: Option<Target>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ScreenshotBatchAction {
    Star { starred: bool },
    Link { target: Option<Target> },
    AddTags { tags: Vec<String> },
    RemoveTags { tags: Vec<String> },
}

fn clean_tags(tags: Vec<String>) -> AppResult<Vec<String>> {
    if tags.len() > 20 {
        return Err("SCREENSHOT_LIMIT".into());
    }
    let mut cleaned = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        text_limit(tag, 80)?;
        if !tag.is_empty() && !cleaned.iter().any(|value| value == tag) {
            cleaned.push(tag.to_owned());
        }
    }
    if serde_json::to_string(&cleaned)
        .map_err(|_| "SCREENSHOT_LIMIT")?
        .len()
        > 4096
    {
        return Err("SCREENSHOT_LIMIT".into());
    }
    Ok(cleaned)
}

pub fn batch_update(
    db: &Database,
    selection: Vec<ScreenshotSelection>,
    action: ScreenshotBatchAction,
) -> AppResult<Vec<ScreenshotImage>> {
    if selection.is_empty() || selection.len() > 100 {
        return Err("SCREENSHOT_LIMIT".into());
    }
    let mut seen = std::collections::HashSet::new();
    for item in &selection {
        if item.id <= 0 || !seen.insert(item.id) {
            return Err("SCREENSHOT_CHANGED".into());
        }
        text_limit(&item.revision, 200)?;
        text_limit(&item.note, 10000)?;
        clean_tags(item.tags.clone())?;
    }
    let tags = match &action {
        ScreenshotBatchAction::AddTags { tags } | ScreenshotBatchAction::RemoveTags { tags } => {
            clean_tags(tags.clone())?
        }
        _ => Vec::new(),
    };
    let mut c = db.connect()?;
    let tx = sql(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
    if let ScreenshotBatchAction::Link { target } = &action {
        validate_target(&tx, *target)?;
    }
    let mut result = Vec::with_capacity(selection.len());
    for expected in selection {
        let current = image(&tx, expected.id).map_err(|_| "SCREENSHOT_CHANGED")?;
        if current.revision != expected.revision
            || current.note != expected.note
            || current.tags != expected.tags
            || current.starred != expected.starred
            || current.own_target != expected.own_target
        {
            return Err("SCREENSHOT_CHANGED".into());
        }
        let mut next_tags = current.tags;
        let mut starred = current.starred;
        let mut link = current.own_target;
        match &action {
            ScreenshotBatchAction::Star { starred: value } => starred = *value,
            ScreenshotBatchAction::Link { target } => link = *target,
            ScreenshotBatchAction::AddTags { .. } => {
                for tag in &tags {
                    if !next_tags.contains(tag) {
                        next_tags.push(tag.clone());
                    }
                }
            }
            ScreenshotBatchAction::RemoveTags { .. } => next_tags.retain(|tag| !tags.contains(tag)),
        }
        let next_tags =
            serde_json::to_string(&clean_tags(next_tags)?).map_err(|_| "SCREENSHOT_LIMIT")?;
        let (n, b) = ids(link);
        sql(tx.execute(
            "UPDATE screenshot_images SET tags=?1,starred=?2,node_id=?3,book_id=?4 WHERE id=?5",
            params![next_tags, starred, n, b, expected.id],
        ))?;
        result.push(image(&tx, expected.id)?);
    }
    sql(tx.commit())?;
    Ok(result)
}
pub fn update_image(
    db: &Database,
    id: i64,
    expected: String,
    note: String,
    tags: Vec<String>,
    starred: bool,
    t: Option<Target>,
) -> AppResult<ScreenshotImage> {
    text_limit(&note, 10000)?;
    if tags.len() > 20 {
        return Err("SCREENSHOT_LIMIT".into());
    }
    let mut cleaned = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        text_limit(tag, 80)?;
        if !tag.is_empty() && !cleaned.contains(&tag.to_owned()) {
            cleaned.push(tag.to_owned());
        }
    }
    let tags = serde_json::to_string(&cleaned).map_err(|_| "SCREENSHOT_LIMIT")?;
    if tags.len() > 4096 {
        return Err("SCREENSHOT_LIMIT".into());
    }
    let mut c = db.connect()?;
    let tx = sql(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
    validate_target(&tx, t)?;
    let (n, b) = ids(t);
    if sql(tx.execute("UPDATE screenshot_images SET note=?1,tags=?2,starred=?3,node_id=?4,book_id=?5 WHERE id=?6 AND revision=?7 AND available=1",params![note,tags,starred,n,b,id,expected]))?!=1{return Err("SCREENSHOT_CHANGED".into());}
    let result = image(&tx, id)?;
    sql(tx.commit())?;
    Ok(result)
}

struct Source {
    root: PathBuf,
    path: PathBuf,
    size: u64,
    stamp: String,
    revision: String,
}
fn source(db: &Database, id: i64, expected: &str) -> AppResult<Source> {
    let (path,relative,size,stamp,revision):(String,String,u64,String,String)=sql(db.connect()?.query_row("SELECT f.path,i.relative_path,i.file_size,i.modified_at,i.revision FROM screenshot_images i JOIN screenshot_folders f ON f.id=i.folder_id WHERE i.id=?1 AND i.available=1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional())?.ok_or("SCREENSHOT_NOT_FOUND")?;
    if revision != expected || size > MAX_BYTES {
        return Err("SCREENSHOT_CHANGED".into());
    }
    let root = safe_root(db, Path::new(&path))?;
    let path = safe_file(&root, &relative)?;
    Ok(Source {
        root,
        path,
        size,
        stamp,
        revision,
    })
}
pub fn read(db: &Database, id: i64, expected: String, thumbnail: bool) -> AppResult<Vec<u8>> {
    let _permit = Permit::acquire()?;
    let s = source(db, id, &expected)?;
    let mut f = crate::comic_reader::checked_file(&s.path, &s.root, s.size, &s.stamp)
        .map_err(|_| "SCREENSHOT_CHANGED")?;
    let mut bytes = Vec::new();
    f.by_ref()
        .take(HEADER_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|_| "SCREENSHOT_READ_FAILED")?;
    if revision(s.size, &s.stamp, &bytes) != s.revision {
        return Err("SCREENSHOT_CHANGED".into());
    }
    let cache = if thumbnail {
        crate::workbench_cache::Cache::open(db).ok()
    } else {
        None
    };
    let cache_key = crate::workbench_cache::key(
        "screenshot",
        &format!("{id}:{}", s.path.display()),
        &expected,
    );
    if thumbnail && dimensions(&bytes).is_ok() {
        if let Some(output) = cache.as_ref().and_then(|cache| cache.get(&cache_key)) {
            crate::comic_reader::validate_file_handle(&f, &s.path, &s.root, s.size, &s.stamp)
                .map_err(|_| "SCREENSHOT_CHANGED")?;
            source(db, id, &expected)?;
            return Ok(output);
        }
    }
    f.by_ref()
        .take(MAX_BYTES + 1 - bytes.len() as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "SCREENSHOT_READ_FAILED")?;
    if bytes.len() as u64 != s.size
        || revision(
            s.size,
            &s.stamp,
            &bytes[..bytes.len().min(HEADER_BYTES as usize)],
        ) != s.revision
    {
        return Err("SCREENSHOT_CHANGED".into());
    }
    crate::comic_reader::validate_file_handle(&f, &s.path, &s.root, s.size, &s.stamp)
        .map_err(|_| "SCREENSHOT_CHANGED")?;
    let (mime, _, _) = dimensions(&bytes)?;
    // Recheck registration after IO so an in-flight read cannot revive a removed folder.
    source(db, id, &expected)?;
    let result = if thumbnail
        && matches!(
            mime,
            "image/png" | "image/jpeg" | "image/webp" | "image/bmp"
        ) {
        let mut reader = image::ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|_| "SCREENSHOT_FORMAT")?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(96 * 1024 * 1024);
        limits.max_image_width = Some(32768);
        limits.max_image_height = Some(32768);
        reader.limits(limits);
        let image = reader.decode().map_err(|_| "SCREENSHOT_FORMAT")?;
        crate::workbench_cache::encode(image).map_err(|_| "SCREENSHOT_FORMAT")?
    } else {
        bytes
    };
    source(db, id, &expected)?;
    if let Some(cache) = cache {
        let _ = cache.put(&cache_key, &result);
    }
    Ok(result)
}
pub fn reveal(db: &Database, id: i64, expected: String) -> AppResult<()> {
    let s = source(db, id, &expected)?;
    let _f = crate::comic_reader::checked_file(&s.path, &s.root, s.size, &s.stamp)
        .map_err(|_| "SCREENSHOT_CHANGED")?;
    crate::player::reveal(Path::new(&crate::db::display_path(&s.path)))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DragSelection {
    pub id: i64,
    pub revision: String,
}

pub(crate) fn drag_sources(
    db: &Database,
    selection: Vec<DragSelection>,
) -> AppResult<Vec<crate::workspace_drag::Source>> {
    if selection.is_empty() || selection.len() > crate::workspace_drag::MAX_FILES {
        return Err("SCREENSHOT_LIMIT".into());
    }
    let mut sources = Vec::with_capacity(selection.len());
    for item in &selection {
        if item.revision.len() != 64 || !item.revision.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("SCREENSHOT_CHANGED".into());
        }
        let s = source(db, item.id, &item.revision)?;
        let pinned =
            crate::workspace_drag::Source::checked(s.path.clone(), &s.root, s.size, &s.stamp)
                .map_err(|_| "SCREENSHOT_CHANGED")?;
        // The indexed revision already includes the validated signature/dimensions
        // header. Recheck that digest with <=256 KiB per image, without decoding
        // or reading 100 full-size images merely to start a native drag.
        let mut file = crate::comic_reader::checked_file(&s.path, &s.root, s.size, &s.stamp)
            .map_err(|_| "SCREENSHOT_CHANGED")?;
        let mut header = Vec::new();
        file.by_ref()
            .take(HEADER_BYTES)
            .read_to_end(&mut header)
            .map_err(|_| "SCREENSHOT_READ_FAILED")?;
        if revision(s.size, &s.stamp, &header) != item.revision {
            return Err("SCREENSHOT_CHANGED".into());
        }
        let latest = source(db, item.id, &item.revision)?;
        if latest.path != s.path || latest.root != s.root {
            return Err("SCREENSHOT_CHANGED".into());
        }
        sources.push(pinned);
    }
    // A removal while checking a later selected item invalidates the whole drag.
    for item in &selection {
        source(db, item.id, &item.revision)?;
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    fn fixture() -> (TempDir, Database, PathBuf) {
        // Gallery rejects AppData, including Windows TEMP: use only our generated workspace fixture.
        let parent = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("screenshot-gallery-tests");
        fs::create_dir_all(&parent).unwrap();
        let temp = tempfile::Builder::new()
            .prefix("synthetic-")
            .tempdir_in(parent)
            .unwrap();
        let app = temp.path().join("app");
        fs::create_dir(&app).unwrap();
        let db = Database::new(app.join("collection.db"));
        db.migrate().unwrap();
        let shots = temp.path().join("screenshots");
        fs::create_dir(&shots).unwrap();
        (temp, db, shots)
    }
    fn png(path: &Path, color: u8) {
        let img = image::RgbaImage::from_pixel(24, 16, image::Rgba([color, 80, 120, 255]));
        img.save_with_format(path, image::ImageFormat::Png).unwrap();
    }
    #[test]
    fn round11_sorts_all_columns_directions_ties_and_filtered_pages() {
        let (temp, db, shots) = fixture();
        for name in ["A.png", "beta.png", "old.png"] {
            png(&shots.join(name), 50);
        }
        let folder = import(&db, shots.to_string_lossy().into_owned()).unwrap();
        let second = temp.path().join("second");
        fs::create_dir(&second).unwrap();
        png(&second.join("a.png"), 60);
        let other = import(&db, second.to_string_lossy().into_owned()).unwrap();
        let c = db.connect().unwrap();
        c.execute("UPDATE screenshot_images SET modified_at='2026-01-01T00:00:00.000000000Z',file_size=50 WHERE file_name='old.png'",[]).unwrap();
        c.execute("UPDATE screenshot_images SET modified_at='2026-01-02T00:00:00.000000000Z',file_size=100,starred=1,tags='[\"tag\"]' WHERE file_name IN ('A.png','a.png')",[]).unwrap();
        c.execute("UPDATE screenshot_images SET modified_at='2026-01-03T00:00:00.000000000Z',file_size=200,starred=1,tags='[\"tag\"]' WHERE file_name='beta.png'",[]).unwrap();
        let rows = list(&db, None, "".into(), None, false, 0, 48)
            .unwrap()
            .items;
        assert_eq!(
            rows.iter()
                .map(|i| i.file_name.as_str())
                .collect::<Vec<_>>(),
            ["beta.png", "a.png", "A.png", "old.png"]
        );
        let query = |sort, desc, folder_id, starred, offset, limit, tag| {
            list_filtered(
                &db,
                folder_id,
                "".into(),
                None,
                starred,
                offset,
                limit,
                tag,
                Some(sort),
                Some(desc),
            )
            .unwrap()
        };
        for sort in [
            ScreenshotSort::Modified,
            ScreenshotSort::Name,
            ScreenshotSort::Size,
        ] {
            let asc = query(sort, false, None, false, 0, 48, None).items;
            let desc = query(sort, true, None, false, 0, 48, None).items;
            assert_eq!(
                asc.iter().map(|i| i.id).collect::<Vec<_>>(),
                desc.iter().rev().map(|i| i.id).collect::<Vec<_>>()
            );
        }
        let names = query(ScreenshotSort::Name, false, None, false, 0, 48, None).items;
        assert_eq!(
            names
                .iter()
                .map(|i| i.file_name.as_str())
                .collect::<Vec<_>>(),
            ["A.png", "a.png", "beta.png", "old.png"]
        );
        let sizes = query(ScreenshotSort::Size, false, None, false, 0, 48, None).items;
        assert_eq!(
            sizes
                .iter()
                .map(|i| i.file_name.as_str())
                .collect::<Vec<_>>(),
            ["old.png", "A.png", "a.png", "beta.png"]
        );
        let page = query(
            ScreenshotSort::Modified,
            true,
            Some(folder.id),
            true,
            1,
            1,
            Some("tag".into()),
        );
        assert_eq!(page.total, 2);
        assert_eq!(page.items[0].file_name, "A.png");
        let page = query(ScreenshotSort::Name, true, Some(other.id), true, 0, 1, None);
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].file_name, "a.png");
        let search = list_filtered(
            &db,
            None,
            "beta".into(),
            None,
            false,
            0,
            1,
            None,
            Some(ScreenshotSort::Size),
            Some(false),
        )
        .unwrap();
        assert_eq!(search.total, 1);
        assert_eq!(search.items[0].file_name, "beta.png");
        assert!(
            serde_json::from_str::<ScreenshotSort>("\"id;DROP TABLE screenshot_images\"").is_err()
        );
    }

    #[test]
    fn round11_thumbnail_cache_does_not_decode_again_and_rechecks_source_revision() {
        let (_temp, db, shots) = fixture();
        let path = shots.join("small.png");
        png(&path, 40);
        let original = fs::read(&path).unwrap();
        let folder = import(&db, shots.to_string_lossy().into_owned()).unwrap();
        let item = list(&db, Some(folder.id), "".into(), None, false, 0, 48)
            .unwrap()
            .items
            .remove(0);
        let before = crate::workbench_cache::encode_count();
        let first = read(&db, item.id, item.revision.clone(), true).unwrap();
        assert_eq!(crate::workbench_cache::encode_count(), before + 1);
        assert_eq!(dimensions(&first).unwrap().0, "image/jpeg");
        assert_eq!(
            read(&db, item.id, item.revision.clone(), true).unwrap(),
            first
        );
        assert_eq!(crate::workbench_cache::encode_count(), before + 1);
        assert_eq!(fs::read(&path).unwrap(), original);
        png(&path, 90);
        assert!(read(&db, item.id, item.revision, true).is_err());
        refresh(&db, folder.id).unwrap();
        let current = list(&db, Some(folder.id), "".into(), None, false, 0, 48)
            .unwrap()
            .items
            .remove(0);
        assert_ne!(
            read(&db, current.id, current.revision.clone(), true).unwrap(),
            first
        );
        remove(&db, folder.id).unwrap();
        assert!(read(&db, current.id, current.revision, true).is_err());
    }
    fn matching_nodes(db: &Database) {
        db.connect().unwrap().execute_batch("INSERT INTO library_roots(id,path,display_name) VALUES(1,'synthetic-library','Library'); INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,total_video_count) VALUES(1,1,NULL,'synthetic-library','Root','Root','CONTAINER',2),(2,1,1,'synthetic-library/a','Silver Garden','Silver Garden','WORK',1),(3,1,1,'synthetic-library/b','Copper River','Copper River','WORK',1); INSERT INTO media_files(id,node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(1,2,'synthetic-library/a/01.mkv','01.mkv','mkv',1,'synthetic'),(2,3,'synthetic-library/b/01.mkv','01.mkv','mkv',1,'synthetic');").unwrap();
    }
    fn playback(db: &Database, node: i64, at: i64, directory: Option<String>) {
        db.connect().unwrap().execute("INSERT INTO mpv_playback_sessions(session_token,node_id,media_file_id,started_at,heartbeat_at,ended_at,screenshot_directory,video_title) VALUES('test',?1,?2,?3-1000,?3+1000,?3+1000,?4,'Synthetic')",params![node,node-1,at,directory]).unwrap();
    }
    #[test]
    fn round10_open_playback_window_matches_before_player_exit_with_utc_milliseconds() {
        let (_temp, db, shots) = fixture();
        matching_nodes(&db);
        let folder = import(&db, shots.to_string_lossy().into_owned()).unwrap();
        let path = shots.join("mpv-shot0001.png");
        png(&path, 40);
        let captured = capture_time(&fs::metadata(&path).unwrap()).unwrap();
        // FILETIME -> SystemTime -> Unix milliseconds is timezone independent.
        let now = chrono::Utc::now().timestamp_millis();
        assert!((now - captured).abs() < 10_000);
        db.connect().unwrap().execute("INSERT INTO mpv_playback_sessions(session_token,node_id,media_file_id,started_at,heartbeat_at,ended_at,screenshot_directory,video_title) VALUES('live',2,1,?1-20000,?1-4000,NULL,?2,'Synthetic')",params![captured,shots.to_string_lossy()]).unwrap();
        let result = refresh(&db, folder.id).unwrap();
        assert_eq!(result.auto_match.matched_by_playback, 1);
        assert_eq!(
            image(&db.connect().unwrap(), result.auto_match.matched_ids[0])
                .unwrap()
                .own_target,
            Some(Target::Node(2))
        );
        assert_eq!(
            db.connect()
                .unwrap()
                .query_row(
                    "SELECT ended_at FROM mpv_playback_sessions WHERE session_token='live'",
                    [],
                    |r| r.get::<_, Option<i64>>(0)
                )
                .unwrap(),
            None
        );
        assert_eq!(auto_match(&db, Some(folder.id)).unwrap().matched_count, 0);
    }

    #[test]
    fn round10_drag_batch_rechecks_available_revision_and_scope_without_source_writes() {
        let (_temp, db, shots) = fixture();
        png(&shots.join("图 01.png"), 40);
        png(&shots.join("图 02.png"), 50);
        let folder = import(&db, shots.to_string_lossy().into_owned()).unwrap();
        let rows = list(&db, Some(folder.id), String::new(), None, false, 0, 48)
            .unwrap()
            .items;
        let original = fs::read(shots.join("图 01.png")).unwrap();
        let selection = || {
            rows.iter()
                .map(|r| DragSelection {
                    id: r.id,
                    revision: r.revision.clone(),
                })
                .collect()
        };
        assert_eq!(drag_sources(&db, selection()).unwrap().len(), 2);
        assert!(drag_sources(&db, vec![]).is_err());
        assert!(drag_sources(
            &db,
            (0..101)
                .map(|_| DragSelection {
                    id: rows[0].id,
                    revision: rows[0].revision.clone()
                })
                .collect()
        )
        .is_err());
        assert!(drag_sources(
            &db,
            vec![DragSelection {
                id: rows[0].id,
                revision: "0".repeat(64)
            }]
        )
        .is_err());
        db.connect()
            .unwrap()
            .execute(
                "UPDATE screenshot_images SET available=0 WHERE id=?1",
                [rows[0].id],
            )
            .unwrap();
        assert!(drag_sources(&db, selection()).is_err());
        db.connect().unwrap().execute("UPDATE screenshot_images SET available=1,relative_path='../outside.png' WHERE id=?1",[rows[0].id]).unwrap();
        assert!(drag_sources(&db, selection()).is_err());
        remove(&db, folder.id).unwrap();
        assert!(drag_sources(&db, selection()).is_err());
        assert_eq!(fs::read(shots.join("图 01.png")).unwrap(), original);
    }
    #[test]
    fn round9_mpv_generic_screenshots_import_refresh_and_manual_link_priority_are_read_only() {
        let (_temp, db, shots) = fixture();
        matching_nodes(&db);
        let path = shots.join("mpv-shot0001 (2).jpg");
        png(&path, 40);
        let original = fs::read(&path).unwrap();
        let stamp = capture_time(&fs::metadata(&path).unwrap()).unwrap();
        playback(&db, 2, stamp, Some(shots.to_string_lossy().into_owned()));
        let f = import(&db, shots.to_string_lossy().into_owned()).unwrap();
        assert_eq!(f.auto_match.matched_count, 1);
        assert_eq!(f.auto_match.matched_by_playback, 1);
        assert_eq!(f.auto_match.matched_by_title, 0);
        let i = list(&db, None, "".into(), None, false, 0, 48)
            .unwrap()
            .items
            .remove(0);
        assert_eq!(i.own_target, Some(Target::Node(2)));
        assert_eq!(f.auto_match.matched_ids, vec![i.id]);
        update_image(
            &db,
            i.id,
            i.revision.clone(),
            "manual".into(),
            vec![],
            false,
            Some(Target::Node(3)),
        )
        .unwrap();
        assert_eq!(refresh(&db, f.id).unwrap().auto_match.matched_count, 0);
        assert_eq!(auto_match(&db, None).unwrap().matched_count, 0);
        assert_eq!(
            image(&db.connect().unwrap(), i.id).unwrap().own_target,
            Some(Target::Node(3))
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        // A folder-wide association is inherited presentation, not an own-image edit.
        update_folder(&db, f.id, "Shots".into(), "".into(), Some(Target::Node(3))).unwrap();
        png(&shots.join("mpv-shot0002.png"), 50);
        let refreshed = refresh(&db, f.id).unwrap();
        assert_eq!(refreshed.auto_match.matched_by_playback, 1);
        assert_eq!(
            list(&db, None, "mpv-shot0002".into(), None, false, 0, 48)
                .unwrap()
                .items[0]
                .own_target,
            Some(Target::Node(2))
        );
    }
    #[test]
    fn round9_overlapping_windows_never_fall_back_to_title_and_exact_directory_disambiguates() {
        let (_temp, db, shots) = fixture();
        matching_nodes(&db);
        let path = shots.join("Silver Garden.png");
        png(&path, 40);
        let stamp = capture_time(&fs::metadata(&path).unwrap()).unwrap();
        playback(&db, 2, stamp, None);
        playback(&db, 3, stamp, None);
        let f = import(&db, shots.to_string_lossy().into_owned()).unwrap();
        assert_eq!(f.auto_match.matched_count, 0);
        assert_eq!(auto_match(&db, Some(f.id)).unwrap().matched_count, 0);
        db.connect()
            .unwrap()
            .execute(
                "UPDATE mpv_playback_sessions SET screenshot_directory=?1 WHERE node_id=2",
                [shots.to_string_lossy()],
            )
            .unwrap();
        let matched = auto_match(&db, Some(f.id)).unwrap();
        assert_eq!(matched.matched_by_playback, 1);
        assert_eq!(matched.matched_by_title, 0);
    }
    #[test]
    fn round9_copied_history_and_crashed_windows_do_not_claim_current_screenshots() {
        let (_temp, db, shots) = fixture();
        matching_nodes(&db);
        let path = shots.join("mpv-shot0001.png");
        png(&path, 40);
        let now = chrono::Utc::now().timestamp_millis();
        playback(&db, 2, now, None);
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_times(
            fs::FileTimes::new()
                .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_000_000)),
        )
        .unwrap();
        drop(file);
        let metadata = fs::metadata(&path).unwrap();
        assert!(metadata.created().unwrap() > metadata.modified().unwrap());
        assert_eq!(capture_time(&metadata), Some(1_000_000_000));
        assert_eq!(
            import(&db, shots.to_string_lossy().into_owned())
                .unwrap()
                .auto_match
                .matched_count,
            0
        );
        let path = shots.join("mpv-shot0002.png");
        png(&path, 50);
        db.connect()
            .unwrap()
            .execute(
                "UPDATE mpv_playback_sessions SET started_at=?1,heartbeat_at=?1,ended_at=NULL",
                [now - 120_000],
            )
            .unwrap();
        assert_eq!(
            refresh(&db, folders(&db).unwrap()[0].id)
                .unwrap()
                .auto_match
                .matched_count,
            0
        );
    }
    #[test]
    fn round9_title_fallback_requires_unique_exact_evidence_and_respects_source_revision() {
        let (_temp, db, shots) = fixture();
        matching_nodes(&db);
        let child = shots.join("Silver Garden");
        fs::create_dir(&child).unwrap();
        png(&child.join("mpv-shot0001 (2).png"), 40);
        png(&shots.join("Copper River.png"), 40);
        png(&shots.join("almost Copper River maybe.png"), 40);
        let f = import(&db, shots.to_string_lossy().into_owned()).unwrap();
        assert_eq!(f.auto_match.matched_by_title, 2);
        assert_eq!(f.auto_match.matched_by_playback, 0);
        assert_eq!(
            list(&db, None, "almost".into(), None, false, 0, 48)
                .unwrap()
                .items[0]
                .own_target,
            None
        );
        db.connect().unwrap().execute("UPDATE nodes SET display_name='Silver Garden',folder_name='Silver Garden' WHERE id=3",[]).unwrap();
        png(&child.join("mpv-shot0002.png"), 50);
        assert_eq!(refresh(&db, f.id).unwrap().auto_match.matched_count, 0);
        db.connect().unwrap().execute("UPDATE nodes SET display_name='Copper River',folder_name='Copper River' WHERE id=3",[]).unwrap();
        png(&child.join("mpv-shot0002.png"), 80);
        assert_eq!(auto_match(&db, Some(f.id)).unwrap().matched_count, 0);
        assert_eq!(refresh(&db, f.id).unwrap().auto_match.matched_by_title, 1);
        for generic in [
            "mpv-shot0001",
            "mpv-shot0001 (2)",
            "Screenshots",
            "img_0099",
        ] {
            assert!(generic_screenshot_title(generic));
        }
    }
    #[test]
    fn round9_playback_time_tolerance_is_bounded_and_other_directories_are_not_evidence() {
        let (_temp, db, shots) = fixture();
        matching_nodes(&db);
        playback(&db, 2, 100_000, None);
        let c = db.connect().unwrap();
        assert!(matches!(
            playback_match(&c, 96_000, &shots).unwrap(),
            PlaybackMatch::Node(2)
        ));
        assert!(matches!(
            playback_match(&c, 95_999, &shots).unwrap(),
            PlaybackMatch::None
        ));
        assert!(matches!(
            playback_match(&c, 116_000, &shots).unwrap(),
            PlaybackMatch::Node(2)
        ));
        assert!(matches!(
            playback_match(&c, 116_001, &shots).unwrap(),
            PlaybackMatch::None
        ));
        c.execute(
            "UPDATE mpv_playback_sessions SET screenshot_directory='C:/Synthetic/Other'",
            [],
        )
        .unwrap();
        assert!(matches!(
            playback_match(&c, 100_000, &shots).unwrap(),
            PlaybackMatch::None
        ));
    }
    #[test]
    fn round9_official_alias_never_erases_node_year_season_or_explicit_filename_conflicts() {
        let (_temp, db, shots) = fixture();
        matching_nodes(&db);
        let c = db.connect().unwrap();
        c.execute("UPDATE nodes SET folder_name='Silver Garden (1990)',display_name='Silver Garden (1990)' WHERE id=2",[]).unwrap();
        c.execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_title) VALUES(2,'BANGUMI',123,'Silver Garden')",[]).unwrap();
        let index = title_index(&c).unwrap();
        assert_eq!(
            title_match(&c, &index, &shots.join("Silver Garden (1990).png"), &shots),
            Some(2)
        );
        assert_eq!(
            title_match(
                &c,
                &index,
                &shots.join("Copper River").join("Silver Garden (2020).png"),
                &shots
            ),
            None
        );
        c.execute("UPDATE nodes SET folder_name='Silver Garden Season 2',display_name='Silver Garden Season 2' WHERE id=2",[]).unwrap();
        let index = title_index(&c).unwrap();
        assert_eq!(
            title_match(&c, &index, &shots.join("Silver Garden.png"), &shots),
            None
        );
    }
    #[test]
    fn round9_active_tmdb_titles_share_exact_match_and_known_year_guards() {
        let (_temp, db, shots) = fixture();
        matching_nodes(&db);
        let c = db.connect().unwrap();
        c.execute("INSERT INTO tmdb_movie_bindings(node_id,movie_id,payload_json) VALUES(2,42,?1)",[r#"{"id":42,"title":"银色花园","originalTitle":"The Silver Garden","releaseDate":"1990-01-01"}"#]).unwrap();
        let index = title_index(&c).unwrap();
        assert_eq!(
            title_match(&c, &index, &shots.join("银色花园.png"), &shots),
            Some(2)
        );
        assert_eq!(
            title_match(
                &c,
                &index,
                &shots.join("The Silver Garden (2020).png"),
                &shots
            ),
            None
        );
        c.execute(
            "UPDATE tmdb_movie_bindings SET active=0 WHERE node_id=2",
            [],
        )
        .unwrap();
        assert_eq!(
            title_match(
                &c,
                &title_index(&c).unwrap(),
                &shots.join("银色花园.png"),
                &shots
            ),
            None
        );
    }
    #[test]
    fn round9_screenshot_source_cannot_contain_or_be_an_active_cover_cache() {
        let (_temp, db, shots) = fixture();
        let cache = shots.join("cache");
        fs::create_dir(&cache).unwrap();
        db.connect()
            .unwrap()
            .execute(
                "INSERT OR REPLACE INTO settings(key,value) VALUES('cover_cache_directory',?1)",
                [cache.to_string_lossy()],
            )
            .unwrap();
        assert_eq!(safe_root(&db, &shots).unwrap_err(), "SCREENSHOT_PATH");
        assert_eq!(safe_root(&db, &cache).unwrap_err(), "SCREENSHOT_PATH");
        let child = cache.join("nested");
        fs::create_dir(&child).unwrap();
        assert_eq!(safe_root(&db, &child).unwrap_err(), "SCREENSHOT_PATH");
    }
    fn selection(i: &ScreenshotImage) -> ScreenshotSelection {
        ScreenshotSelection {
            id: i.id,
            revision: i.revision.clone(),
            note: i.note.clone(),
            tags: i.tags.clone(),
            starred: i.starred,
            own_target: i.own_target,
        }
    }
    #[test]
    fn round8_batch_curation_is_atomic_and_rejects_metadata_and_source_conflicts() {
        let (_temp, db, shots) = fixture();
        for n in 1..=3 {
            png(&shots.join(format!("{n}.png")), 40);
        }
        import(&db, shots.to_string_lossy().into_owned()).unwrap();
        let before = list(&db, None, "".into(), None, false, 0, 48).unwrap();
        let original = fs::read(shots.join("1.png")).unwrap();
        let selected: Vec<_> = before.items.iter().map(selection).collect();
        let updated = batch_update(
            &db,
            selected.clone(),
            ScreenshotBatchAction::AddTags {
                tags: vec![" 作画 ".into(), "作画".into()],
            },
        )
        .unwrap();
        assert!(updated.iter().all(|i| i.tags == vec!["作画"]));
        assert!(
            batch_update(&db, selected, ScreenshotBatchAction::Star { starred: true }).is_err()
        );
        assert_eq!(
            list(&db, None, "".into(), None, true, 0, 48).unwrap().total,
            0
        );
        let mut stale: Vec<_> = updated.iter().map(selection).collect();
        stale[2].revision = "stale".into();
        assert!(batch_update(&db, stale, ScreenshotBatchAction::Star { starred: true }).is_err());
        assert_eq!(
            list(&db, None, "".into(), None, true, 0, 48).unwrap().total,
            0
        );
        let updated = batch_update(
            &db,
            updated.iter().map(selection).collect(),
            ScreenshotBatchAction::Star { starred: true },
        )
        .unwrap();
        assert!(updated.iter().all(|i| i.starred));
        assert!(batch_update(
            &db,
            updated.iter().map(selection).collect(),
            ScreenshotBatchAction::Link {
                target: Some(Target::Node(9999))
            }
        )
        .is_err());
        let removed = batch_update(
            &db,
            updated.iter().map(selection).collect(),
            ScreenshotBatchAction::RemoveTags {
                tags: vec!["作画".into()],
            },
        )
        .unwrap();
        assert!(removed.iter().all(|i| i.tags.is_empty() && i.starred));
        assert_eq!(fs::read(shots.join("1.png")).unwrap(), original);
        assert_eq!(fs::read_dir(&shots).unwrap().count(), 3);
    }
    #[test]
    fn round8_exact_tag_filter_and_locator_keep_page_order_and_bounds() {
        let (_temp, db, shots) = fixture();
        for n in 1..=50 {
            png(&shots.join(format!("{n:03}.png")), 40);
        }
        import(&db, shots.to_string_lossy().into_owned()).unwrap();
        let p = list(&db, None, "".into(), None, false, 0, 100).unwrap();
        batch_update(
            &db,
            vec![selection(&p.items[0])],
            ScreenshotBatchAction::AddTags {
                tags: vec!["作画".into()],
            },
        )
        .unwrap();
        batch_update(
            &db,
            vec![selection(&p.items[1])],
            ScreenshotBatchAction::AddTags {
                tags: vec!["作画集".into()],
            },
        )
        .unwrap();
        let exact = list_filtered(
            &db,
            None,
            "".into(),
            None,
            false,
            0,
            48,
            Some("作画".into()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(exact.total, 1);
        assert_eq!(exact.items[0].id, p.items[0].id);
        assert_eq!(locate(&db, p.items[47].id, None, None).unwrap().offset, 0);
        assert_eq!(locate(&db, p.items[48].id, None, None).unwrap().offset, 48);
        assert!(batch_update(&db, vec![], ScreenshotBatchAction::Star { starred: true }).is_err());
        assert!(batch_update(
            &db,
            vec![selection(&p.items[2]); 101],
            ScreenshotBatchAction::Star { starred: true }
        )
        .is_err());
        assert!(batch_update(
            &db,
            vec![selection(&p.items[2]); 2],
            ScreenshotBatchAction::Star { starred: true }
        )
        .is_err());
        assert!(batch_update(
            &db,
            vec![selection(&p.items[2])],
            ScreenshotBatchAction::AddTags {
                tags: vec!["a".repeat(81)]
            }
        )
        .is_err());
    }
    #[test]
    fn gallery_roundtrip_is_read_only_preserves_metadata_ids_and_revisions() {
        let (_temp, db, shots) = fixture();
        let path = shots.join("动画 [01].png");
        png(&path, 40);
        let original = fs::read(&path).unwrap();
        let f = import(&db, shots.to_string_lossy().into_owned()).unwrap();
        assert_eq!(f.image_count, 1);
        let before_hash = format!("{:x}", Sha256::digest(&original));
        let p = list(&db, Some(f.id), "".into(), None, false, 0, 48).unwrap();
        let i = &p.items[0];
        let id = i.id;
        let rev = i.revision.clone();
        assert_eq!(read(&db, id, rev.clone(), false).unwrap(), original);
        let thumbnail = read(&db, id, rev.clone(), true).unwrap();
        assert_eq!(dimensions(&thumbnail).unwrap().1, 24);
        update_image(
            &db,
            id,
            rev.clone(),
            "A quiet scene".into(),
            vec![" blue ".into(), "blue".into()],
            true,
            None,
        )
        .unwrap();
        refresh(&db, f.id).unwrap();
        let p = list(&db, None, "quiet".into(), None, true, 0, 48).unwrap();
        assert_eq!(p.total, 1);
        assert_eq!(p.items[0].id, id);
        assert_eq!(p.items[0].tags, vec!["blue"]);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(&path).unwrap())),
            before_hash
        );
        assert_eq!(fs::read_dir(&shots).unwrap().count(), 1);
        // Fixture changes emulate external edits; production import/refresh never writes sources.
        fs::rename(&path, shots.join("temporarily-away.dat")).unwrap();
        refresh(&db, f.id).unwrap();
        assert_eq!(
            list(&db, None, "".into(), None, false, 0, 48)
                .unwrap()
                .total,
            0
        );
        fs::rename(shots.join("temporarily-away.dat"), &path).unwrap();
        refresh(&db, f.id).unwrap();
        assert_eq!(
            list(&db, None, "quiet".into(), None, true, 0, 48)
                .unwrap()
                .items[0]
                .id,
            id
        );
        png(&path, 90);
        assert_eq!(
            read(&db, id, rev.clone(), false).unwrap_err(),
            "SCREENSHOT_CHANGED"
        );
        refresh(&db, f.id).unwrap();
        assert!(update_image(&db, id, rev, "stale".into(), vec![], false, None).is_err());
        let latest = fs::read(&path).unwrap();
        remove(&db, f.id).unwrap();
        assert_eq!(fs::read(&path).unwrap(), latest);
        assert_eq!(folders(&db).unwrap().len(), 0);
    }
    #[test]
    fn gallery_validates_signatures_limits_containment_and_source_replacement() {
        let (_temp, db, shots) = fixture();
        png(&shots.join("jpeg-name.jpg"), 80);
        fs::write(shots.join("fake.png"), b"<script>bad</script>").unwrap();
        let f = import(&db, shots.to_string_lossy().into_owned()).unwrap();
        assert_eq!(f.image_count, 1);
        let p = list(&db, None, "".into(), None, false, 0, 10).unwrap();
        assert_eq!(p.items[0].mime, "image/png");
        assert!(safe_file(&shots, "../outside.png").is_err());
        assert!(safe_file(&shots, "file.png:stream").is_err());
        assert!(safe_root(&db, db.path().parent().unwrap()).is_err());
        assert!(safe_root(&db, Path::new("C:\\")).is_err());
        assert!(list(&db, None, "".into(), None, false, 0, 101).is_err());
        assert!(update_folder(&db, f.id, "".into(), "".into(), None).is_err());
        // A forged relative locator cannot turn a persisted registration into an arbitrary file read.
        db.connect()
            .unwrap()
            .execute(
                "UPDATE screenshot_images SET relative_path='../outside.png'",
                [],
            )
            .unwrap();
        assert_eq!(
            read(&db, p.items[0].id, p.items[0].revision.clone(), false).unwrap_err(),
            "SCREENSHOT_PATH"
        );
    }
    #[test]
    fn gallery_refresh_failure_preserves_existing_index_and_links_validate_target() {
        let (_temp, db, shots) = fixture();
        png(&shots.join("01.png"), 40);
        let f = import(&db, shots.to_string_lossy().into_owned()).unwrap();
        assert!(update_folder(
            &db,
            f.id,
            "Shots".into(),
            "".into(),
            Some(Target::Node(9999))
        )
        .is_err());
        let moved = shots.with_file_name("offline");
        fs::rename(&shots, &moved).unwrap();
        assert!(refresh(&db, f.id).is_err());
        assert_eq!(
            list(&db, None, "".into(), None, false, 0, 48)
                .unwrap()
                .total,
            1
        );
        fs::rename(moved, &shots).unwrap();
        assert_eq!(refresh(&db, f.id).unwrap().image_count, 1);
    }
    #[test]
    fn gallery_downsizes_full_hd_bmp_and_validates_every_avif_image_property() {
        let (_temp, db, shots) = fixture();
        image::RgbImage::from_pixel(1920, 1080, image::Rgb([20, 50, 80]))
            .save_with_format(shots.join("full-hd.bmp"), image::ImageFormat::Bmp)
            .unwrap();
        assert!(fs::metadata(shots.join("full-hd.bmp")).unwrap().len() > 4 * 1024 * 1024);
        import(&db, shots.to_string_lossy().into_owned()).unwrap();
        let p = list(&db, None, "".into(), None, false, 0, 10).unwrap();
        let i = &p.items[0];
        assert_eq!(i.mime, "image/bmp");
        let small = read(&db, i.id, i.revision.clone(), true).unwrap();
        assert_eq!(dimensions(&small).unwrap(), ("image/jpeg", 768, 432));
        fn b(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
            let mut v = ((data.len() + 8) as u32).to_be_bytes().to_vec();
            v.extend_from_slice(kind);
            v.extend_from_slice(data);
            v
        }
        fn ispe(w: u32, h: u32) -> Vec<u8> {
            let mut v = vec![0; 4];
            v.extend_from_slice(&w.to_be_bytes());
            v.extend_from_slice(&h.to_be_bytes());
            b(b"ispe", &v)
        }
        let mut valid = b(b"ftyp", b"avif\0\0\0\0avifmif1");
        valid.extend_from_slice(&b(b"ipco", &ispe(1920, 1080)));
        assert_eq!(dimensions(&valid).unwrap(), ("image/avif", 1920, 1080));
        valid.extend_from_slice(&b(b"ipco", &ispe(32768, 32768)));
        assert_eq!(dimensions(&valid).unwrap_err(), "SCREENSHOT_LIMIT");
    }
    #[test]
    fn gallery_image_edits_preserve_inherited_folder_links() {
        let (_temp, db, shots) = fixture();
        png(&shots.join("01.png"), 40);
        db.connect().unwrap().execute_batch("INSERT INTO library_roots(id,path,display_name) VALUES(1,'synthetic-library','Library'); INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,total_video_count) VALUES(1,1,NULL,'synthetic-library','Root','Root','CONTAINER',2),(2,1,1,'synthetic-library/a','A','A','WORK',1),(3,1,1,'synthetic-library/b','B','B','WORK',1);").unwrap();
        let f = import(&db, shots.to_string_lossy().into_owned()).unwrap();
        update_folder(&db, f.id, "Shots".into(), "".into(), Some(Target::Node(2))).unwrap();
        let p = list(&db, None, "".into(), None, false, 0, 10).unwrap();
        let i = &p.items[0];
        assert_eq!(i.target, Some(Target::Node(2)));
        assert_eq!(i.own_target, None);
        assert_eq!(i.node_id, Some(2));
        update_image(
            &db,
            i.id,
            i.revision.clone(),
            "note".into(),
            vec![],
            true,
            i.own_target,
        )
        .unwrap();
        update_folder(&db, f.id, "Shots".into(), "".into(), Some(Target::Node(3))).unwrap();
        let p = list(&db, None, "".into(), None, false, 0, 10).unwrap();
        assert_eq!(p.items[0].target, Some(Target::Node(3)));
        assert_eq!(p.items[0].own_target, None);
    }
    #[cfg(windows)]
    #[test]
    fn gallery_never_follows_directory_junctions() {
        use std::ffi::c_void;
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn DeviceIoControl(
                device: *mut c_void,
                code: u32,
                input: *const c_void,
                input_len: u32,
                output: *mut c_void,
                output_len: u32,
                returned: *mut u32,
                overlapped: *mut c_void,
            ) -> i32;
        }
        let (temp, db, shots) = fixture();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        png(&outside.join("secret.png"), 40);
        // A real mount-point reparse directory needs neither Developer Mode nor symlink privilege.
        // Both source and target are generated under this test's private temporary directory.
        let link = shots.join("linked");
        fs::create_dir(&link).unwrap();
        let print = crate::db::display_path(&fs::canonicalize(&outside).unwrap());
        let substitute = format!(r"\??\{print}");
        let sub: Vec<u16> = substitute.encode_utf16().collect();
        let display: Vec<u16> = print.encode_utf16().collect();
        let sub_len = (sub.len() * 2) as u16;
        let print_len = (display.len() * 2) as u16;
        let data_len = 8 + sub_len + 2 + print_len + 2;
        let mut data = 0xA0000003u32.to_le_bytes().to_vec();
        data.extend_from_slice(&data_len.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        for value in [0, sub_len, sub_len + 2, print_len] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        for value in sub.into_iter().chain([0]).chain(display).chain([0]) {
            data.extend_from_slice(&value.to_le_bytes());
        }
        let handle = fs::OpenOptions::new()
            .write(true)
            .custom_flags(0x02200000)
            .open(&link)
            .unwrap();
        let mut returned = 0;
        let success = unsafe {
            DeviceIoControl(
                handle.as_raw_handle(),
                0x000900A4,
                data.as_ptr().cast(),
                data.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(success, 0, "{}", std::io::Error::last_os_error());
        drop(handle);
        let f = import(&db, shots.to_string_lossy().into_owned()).unwrap();
        assert_eq!(f.image_count, 0);
        assert!(import(&db, link.to_string_lossy().into_owned()).is_err());
        assert!(safe_file(&shots, "linked/secret.png").is_err());
        assert!(outside.join("secret.png").exists());
        assert!(sensitive_component(Path::new(
            "C:\\Users\\Example\\AppData\\images"
        )));
    }
}

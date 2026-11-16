//! Read-only project folders. No recursive indexing, source mutation, shell interpolation,
//! file URLs or external image decoder. Thumbnails use an isolated application cache.
use crate::{
    collection::Target,
    db::{AppResult, Database},
};
use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
    sync::{Condvar, Mutex},
    time::{Duration, Instant},
};

const MAX_FOLDERS: i64 = 128;
const MAX_ENTRIES: usize = 20_000;
const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;
static READS: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());
struct Permit;
impl Permit {
    fn acquire() -> AppResult<Self> {
        let count = READS.0.lock().map_err(|_| "WORKSPACE_READ_FAILED")?;
        let (mut count, wait) = READS
            .1
            .wait_timeout_while(count, Duration::from_secs(20), |n| *n >= 4)
            .map_err(|_| "WORKSPACE_READ_FAILED")?;
        if wait.timed_out() && *count >= 4 {
            return Err("WORKSPACE_LIMIT".into());
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
pub struct WorkspaceFolder {
    pub id: i64,
    pub path: String,
    pub alias: String,
    pub note: String,
    pub target: Option<Target>,
    pub node_id: Option<i64>,
    pub created_at: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntry {
    pub relative_path: String,
    pub file_name: String,
    pub extension: String,
    pub file_size: u64,
    pub modified_at: String,
    pub kind: String,
    pub can_open_default: bool,
    pub revision: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBreadcrumb {
    pub name: String,
    pub relative_path: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceListing {
    pub folder_id: i64,
    pub relative_dir: String,
    pub entries: Vec<WorkspaceEntry>,
    pub breadcrumbs: Vec<WorkspaceBreadcrumb>,
    pub total: usize,
    pub truncated: bool,
}

fn sql<T>(r: rusqlite::Result<T>) -> AppResult<T> {
    r.map_err(|_| "WORKSPACE_READ_FAILED".into())
}
fn target(node: Option<i64>, book: Option<i64>) -> Option<Target> {
    node.map(Target::Node).or_else(|| book.map(Target::Book))
}
fn validate_target(c: &Connection, t: Option<Target>) -> AppResult<()> {
    if let Some(t) = t {
        let node = crate::collection::validate(c, t)?;
        if matches!(t, Target::Node(_)) {
            crate::collection::validate_metadata_node(c, node)?;
        }
    }
    Ok(())
}
fn folder_row(r: &Row<'_>) -> rusqlite::Result<WorkspaceFolder> {
    Ok(WorkspaceFolder {
        id: r.get(0)?,
        path: r.get(1)?,
        alias: r.get(2)?,
        note: r.get(3)?,
        target: target(r.get(4)?, r.get(5)?),
        node_id: None,
        created_at: r.get(6)?,
    })
}
const FOLDERS: &str = "SELECT id,path,alias,note,node_id,book_id,created_at FROM workspace_folders";
fn visible_folder(c: &Connection, mut f: WorkspaceFolder) -> WorkspaceFolder {
    f.target = f.target.filter(|t| validate_target(c, Some(*t)).is_ok());
    f.node_id = f
        .target
        .and_then(|t| crate::collection::validate(c, t).ok());
    f
}
fn folder(c: &Connection, id: i64) -> AppResult<WorkspaceFolder> {
    let f = sql(c
        .query_row(&format!("{FOLDERS} WHERE id=?1"), [id], folder_row)
        .optional())?
    .ok_or("WORKSPACE_NOT_FOUND")?;
    Ok(visible_folder(c, f))
}
pub fn folders(db: &Database) -> AppResult<Vec<WorkspaceFolder>> {
    db.read_snapshot(|c| {
        let mut s = sql(c.prepare(&format!("{FOLDERS} ORDER BY created_at DESC,id DESC")))?;
        let result = sql(sql(s.query_map([], folder_row))?.collect::<rusqlite::Result<Vec<_>>>())?;
        Ok(result.into_iter().map(|f| visible_folder(c, f)).collect())
    })
}
fn plain(path: &Path, directory: bool) -> AppResult<()> {
    crate::collection_backup::plain(path, directory).map_err(|_| "WORKSPACE_PATH".into())
}
fn sensitive(path: &Path) -> bool {
    path.components().any(|c| match c {
        Component::Normal(n) => matches!(
            n.to_string_lossy().to_lowercase().as_str(),
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
        ),
        _ => false,
    })
}
fn safe_root(db: &Database, path: &Path) -> AppResult<PathBuf> {
    if !path.is_absolute()
        || path.as_os_str().len() > 32768
        || sensitive(path)
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        || path
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count()
            < 1
    {
        return Err("WORKSPACE_PATH".into());
    }
    #[cfg(windows)]
    if !matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), std::path::Prefix::Disk(_)|std::path::Prefix::VerbatimDisk(_)|std::path::Prefix::UNC(_,_)|std::path::Prefix::VerbatimUNC(_,_)))
    {
        return Err("WORKSPACE_PATH".into());
    }
    plain(path, true)?;
    let root = fs::canonicalize(path).map_err(|_| "WORKSPACE_PATH")?;
    if sensitive(&root) {
        return Err("WORKSPACE_PATH".into());
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        let root_name = crate::db::display_path(&root)
            .replace('/', "\\")
            .to_lowercase();
        let profile = profile.to_string_lossy().replace('/', "\\").to_lowercase();
        if profile == root_name
            || profile.starts_with(&(root_name.trim_end_matches('\\').to_owned() + "\\"))
        {
            return Err("WORKSPACE_PATH".into());
        }
    }
    if let Some(parent) = db.path().parent() {
        if let Ok(app) = fs::canonicalize(parent) {
            if root.starts_with(&app) || app.starts_with(&root) {
                return Err("WORKSPACE_PATH".into());
            }
        }
    }
    if overlaps_cover_cache(db, &root)? {
        return Err("WORKSPACE_PATH".into());
    }
    Ok(root)
}
pub(crate) fn overlaps_cover_cache(db: &Database, path: &Path) -> AppResult<bool> {
    let parent = db.path().parent().ok_or("WORKSPACE_PATH")?;
    let cache = db.poster_cache_root(&parent.join("cache/covers"))?;
    crate::cache::paths_overlap_checked(path, &cache).map_err(|_| "WORKSPACE_PATH".into())
}
fn relative_parts(relative: &str, allow_root: bool) -> AppResult<Vec<&str>> {
    if relative.is_empty() && allow_root {
        return Ok(Vec::new());
    }
    if relative.len() > 4096 || relative.contains(['\0', ':']) || Path::new(relative).is_absolute()
    {
        return Err("WORKSPACE_PATH".into());
    }
    let parts: Vec<_> = relative.split(['/', '\\']).collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || *p == "." || *p == ".." || p.ends_with(['.', ' ']))
    {
        return Err("WORKSPACE_PATH".into());
    }
    Ok(parts)
}
fn safe_path(root: &Path, relative: &str, directory: bool) -> AppResult<PathBuf> {
    let mut path = root.to_path_buf();
    for part in relative_parts(relative, directory)? {
        // Validate each parent before touching its child, rather than discovering a
        // linked ancestor only after an outside target has already been inspected.
        plain(&path, true)?;
        path.push(part);
        if sensitive(&path) {
            return Err("WORKSPACE_PATH".into());
        }
    }
    if sensitive(&path) {
        return Err("WORKSPACE_PATH".into());
    }
    plain(&path, directory)?;
    let resolved = fs::canonicalize(&path).map_err(|_| "WORKSPACE_PATH")?;
    if !resolved.starts_with(root) {
        return Err("WORKSPACE_PATH".into());
    }
    Ok(resolved)
}
fn safe_any_path(root: &Path, relative: &str) -> AppResult<PathBuf> {
    let mut path = root.to_path_buf();
    for part in relative_parts(relative, true)? {
        plain(&path, true)?;
        path.push(part);
        if sensitive(&path) {
            return Err("WORKSPACE_PATH".into());
        }
    }
    let metadata = fs::symlink_metadata(&path).map_err(|_| "WORKSPACE_PATH")?;
    safe_path(root, relative, metadata.is_dir())
}
fn text_limit(s: &str, max: usize) -> AppResult<()> {
    if s.chars().count() > max || s.len() > max * 4 || s.contains('\0') {
        Err("WORKSPACE_LIMIT".into())
    } else {
        Ok(())
    }
}
pub fn import(db: &Database, path: String) -> AppResult<WorkspaceFolder> {
    let root = safe_root(db, Path::new(&path))?;
    // Check this directory is readable without recursively visiting project contents.
    fs::read_dir(&root).map_err(|_| "WORKSPACE_READ_FAILED")?;
    let path = crate::db::display_path(&root);
    let mut c = db.connect()?;
    let tx = sql(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
    let existing: Option<i64> = sql(tx
        .query_row(
            "SELECT id FROM workspace_folders WHERE path=?1 COLLATE NOCASE",
            [&path],
            |r| r.get(0),
        )
        .optional())?;
    let id = if let Some(id) = existing {
        id
    } else {
        if sql(
            tx.query_row("SELECT count(*) FROM workspace_folders", [], |r| {
                r.get::<_, i64>(0)
            }),
        )? >= MAX_FOLDERS
        {
            return Err("WORKSPACE_LIMIT".into());
        }
        let alias: String = root
            .file_name()
            .ok_or("WORKSPACE_PATH")?
            .to_string_lossy()
            .chars()
            .take(240)
            .collect();
        sql(tx.execute(
            "INSERT INTO workspace_folders(path,alias) VALUES(?1,?2)",
            params![path, alias],
        ))?;
        tx.last_insert_rowid()
    };
    let result = folder(&tx, id)?;
    sql(tx.commit())?;
    Ok(result)
}
pub fn update(
    db: &Database,
    id: i64,
    alias: String,
    note: String,
    t: Option<Target>,
) -> AppResult<WorkspaceFolder> {
    let alias = alias.trim();
    text_limit(alias, 240)?;
    text_limit(&note, 10000)?;
    if alias.is_empty() {
        return Err("WORKSPACE_LIMIT".into());
    }
    let mut c = db.connect()?;
    let tx = sql(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
    validate_target(&tx, t)?;
    let (node, book) = match t {
        Some(Target::Node(n)) => (Some(n), None),
        Some(Target::Book(b)) => (None, Some(b)),
        None => (None, None),
    };
    if sql(tx.execute(
        "UPDATE workspace_folders SET alias=?1,note=?2,node_id=?3,book_id=?4 WHERE id=?5",
        params![alias, note, node, book, id],
    ))? != 1
    {
        return Err("WORKSPACE_NOT_FOUND".into());
    }
    let result = folder(&tx, id)?;
    sql(tx.commit())?;
    Ok(result)
}
pub fn remove(db: &Database, id: i64) -> AppResult<()> {
    if sql(db
        .connect()?
        .execute("DELETE FROM workspace_folders WHERE id=?1", [id]))?
        != 1
    {
        return Err("WORKSPACE_NOT_FOUND".into());
    }
    Ok(())
}
fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}
fn blocked(ext: &str) -> bool {
    matches!(
        ext,
        "" | "exe"
            | "msi"
            | "msp"
            | "mst"
            | "bat"
            | "cmd"
            | "ps1"
            | "msh"
            | "msh1"
            | "msh2"
            | "psm1"
            | "psd1"
            | "ps1xml"
            | "psc1"
            | "psc2"
            | "vbs"
            | "vbe"
            | "vb"
            | "vba"
            | "js"
            | "jse"
            | "ws"
            | "wsc"
            | "wsf"
            | "wsh"
            | "hta"
            | "chm"
            | "php"
            | "lua"
            | "ahk"
            | "au3"
            | "scr"
            | "com"
            | "pif"
            | "reg"
            | "lnk"
            | "cpl"
            | "msc"
            | "inf"
            | "ins"
            | "isp"
            | "scf"
            | "sct"
            | "url"
            | "website"
            | "application"
            | "appref-ms"
            | "appx"
            | "appxbundle"
            | "msix"
            | "msixbundle"
            | "msu"
            | "diagcab"
            | "settingcontent-ms"
            | "search-ms"
            | "searchconnector-ms"
            | "library-ms"
            | "gadget"
            | "jar"
            | "py"
            | "pyw"
            | "pyc"
            | "pyo"
            | "rb"
            | "pl"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "command"
            | "workflow"
            | "webloc"
            | "desktop"
            | "run"
            | "bin"
            | "dll"
            | "ocx"
    )
}
fn kind(ext: &str, directory: bool) -> &'static str {
    if directory {
        return "FOLDER";
    }
    match ext {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "avif" => "IMAGE",
        "psd" | "psb" => "PSD",
        "doc" | "docx" | "odt" | "rtf" | "txt" | "md" | "markdown" | "pdf" | "xls" | "xlsx"
        | "ods" | "ppt" | "pptx" | "odp" | "csv" => "DOCUMENT",
        "ass" | "ssa" | "srt" | "vtt" | "sub" | "idx" => "SUBTITLE",
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus" | "aiff" | "ape" | "alac"
        | "wma" => "AUDIO",
        "mp4" | "mkv" | "avi" | "mov" | "m4v" | "webm" | "ts" | "m2ts" | "wmv" => "VIDEO",
        "ai" | "eps" | "indd" | "aep" | "aepx" | "prproj" | "drp" | "blend" | "c4d" | "sketch"
        | "fig" | "xcf" | "kra" | "aup3" | "sesx" | "veg" | "sln" | "csproj" => "PROJECT",
        "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" => "ARCHIVE",
        _ => "OTHER",
    }
}
fn revision(meta: &fs::Metadata) -> String {
    let mut h = Sha256::new();
    h.update(meta.len().to_le_bytes());
    h.update(crate::comics::modified(meta));
    if let Ok(created) = meta.created() {
        h.update(format!("{created:?}"));
    }
    format!("{:x}", h.finalize())
}
fn registered_root(db: &Database, id: i64) -> AppResult<(WorkspaceFolder, PathBuf)> {
    let f = folder(&db.connect()?, id)?;
    let root = safe_root(db, Path::new(&f.path))?;
    Ok((f, root))
}
pub fn list(
    db: &Database,
    id: i64,
    relative: String,
    sort: String,
    descending: bool,
) -> AppResult<WorkspaceListing> {
    if !matches!(sort.as_str(), "NAME" | "TYPE" | "SIZE" | "MODIFIED") {
        return Err("WORKSPACE_LIMIT".into());
    }
    let (f, root) = registered_root(db, id)?;
    let parts = relative_parts(&relative, true)?;
    let dir = safe_path(&root, &relative, true)?;
    let relative = parts.join("/");
    let mut breadcrumbs = vec![WorkspaceBreadcrumb {
        name: f.alias,
        relative_path: String::new(),
    }];
    let mut current = String::new();
    for part in parts {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(part);
        breadcrumbs.push(WorkspaceBreadcrumb {
            name: part.into(),
            relative_path: current.clone(),
        });
    }
    let start = Instant::now();
    let mut entries = Vec::new();
    for (n, e) in fs::read_dir(&dir)
        .map_err(|_| "WORKSPACE_READ_FAILED")?
        .enumerate()
    {
        if n >= MAX_ENTRIES || start.elapsed() > Duration::from_secs(15) {
            return Err("WORKSPACE_LIMIT".into());
        }
        let e = e.map_err(|_| "WORKSPACE_READ_FAILED")?;
        let path = e.path();
        let meta = fs::symlink_metadata(&path).map_err(|_| "WORKSPACE_READ_FAILED")?;
        if sensitive(&path) || plain(&path, meta.is_dir()).is_err() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let relative_path = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };
        // Reparse points and platform-special filename spellings are not actionable entries.
        if relative_parts(&relative_path, false).is_err() {
            continue;
        }
        let ext = if meta.is_dir() {
            String::new()
        } else {
            extension(&path)
        };
        entries.push(WorkspaceEntry {
            relative_path,
            file_name: name,
            file_size: if meta.is_dir() { 0 } else { meta.len() },
            modified_at: crate::comics::modified(&meta),
            kind: kind(&ext, meta.is_dir()).into(),
            can_open_default: meta.is_file() && !blocked(&ext),
            revision: revision(&meta),
            extension: ext,
        });
    }
    if registered_root(db, id)?.1 != root || safe_path(&root, &relative, true)? != dir {
        return Err("WORKSPACE_CHANGED".into());
    }
    entries.sort_by(|a, b| {
        let folder_order = (b.kind == "FOLDER").cmp(&(a.kind == "FOLDER"));
        if !folder_order.is_eq() {
            return folder_order;
        }
        // In TYPE sort, PSD always stays immediately after folders as the first file type.
        if sort == "TYPE" {
            let psd_order = (b.kind == "PSD").cmp(&(a.kind == "PSD"));
            if !psd_order.is_eq() {
                return psd_order;
            }
        }
        let order = match sort.as_str() {
            "TYPE" => a
                .kind
                .cmp(&b.kind)
                .then_with(|| (a.extension != "psd").cmp(&(b.extension != "psd")))
                .then_with(|| a.extension.cmp(&b.extension)),
            "SIZE" => a.file_size.cmp(&b.file_size),
            "MODIFIED" => a.modified_at.cmp(&b.modified_at),
            _ => crate::db::natural_cmp(&a.file_name, &b.file_name),
        }
        .then_with(|| crate::db::natural_cmp(&a.file_name, &b.file_name))
        .then_with(|| a.relative_path.cmp(&b.relative_path));
        if descending {
            order.reverse()
        } else {
            order
        }
    });
    Ok(WorkspaceListing {
        folder_id: id,
        relative_dir: relative,
        total: entries.len(),
        entries,
        breadcrumbs,
        truncated: false,
    })
}
struct CheckedSource {
    root: PathBuf,
    path: PathBuf,
    metadata: fs::Metadata,
    file: fs::File,
}
fn source(
    db: &Database,
    id: i64,
    relative: &str,
    expected: Option<&str>,
) -> AppResult<CheckedSource> {
    let (_, root) = registered_root(db, id)?;
    let path = safe_path(&root, relative, false)?;
    let metadata = fs::metadata(&path).map_err(|_| "WORKSPACE_PATH")?;
    if expected.is_some_and(|expected| expected != revision(&metadata)) {
        return Err("WORKSPACE_CHANGED".into());
    }
    let file = crate::comic_reader::checked_file(
        &path,
        &root,
        metadata.len(),
        &crate::comics::modified(&metadata),
    )
    .map_err(|_| "WORKSPACE_CHANGED")?;
    Ok(CheckedSource {
        root,
        path,
        metadata,
        file,
    })
}
fn recheck(db: &Database, id: i64, relative: &str, s: &CheckedSource) -> AppResult<()> {
    if registered_root(db, id)?.1 != s.root || safe_path(&s.root, relative, false)? != s.path {
        return Err("WORKSPACE_CHANGED".into());
    }
    crate::comic_reader::validate_file_handle(
        &s.file,
        &s.path,
        &s.root,
        s.metadata.len(),
        &crate::comics::modified(&s.metadata),
    )
    .map_err(|_| "WORKSPACE_CHANGED".into())
}
pub fn thumbnail(db: &Database, id: i64, relative: String, expected: String) -> AppResult<Vec<u8>> {
    let _permit = Permit::acquire()?;
    let mut s = source(db, id, &relative, Some(&expected))?;
    if kind(&extension(&s.path), false) != "IMAGE" {
        return Err("WORKSPACE_FORMAT".into());
    }
    if s.metadata.len() == 0 || s.metadata.len() > MAX_IMAGE_BYTES {
        return Err("WORKSPACE_LIMIT".into());
    }
    // Validate a bounded signature/header and the current opened source even on cache hits.
    let mut bytes = Vec::new();
    s.file
        .by_ref()
        .take(256 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|_| "WORKSPACE_READ_FAILED")?;
    recheck(db, id, &relative, &s)?;
    let cache = crate::workbench_cache::Cache::open(db).ok();
    let cache_key = crate::workbench_cache::key(
        "workspace",
        &format!("{id}:{}", s.path.display()),
        &format!("{expected}:{:x}", Sha256::digest(&bytes)),
    );
    if crate::screenshot_gallery::dimensions(&bytes).is_ok() {
        if let Some(output) = cache.as_ref().and_then(|cache| cache.get(&cache_key)) {
            recheck(db, id, &relative, &s)?;
            return Ok(output);
        }
    }
    s.file
        .by_ref()
        .take(MAX_IMAGE_BYTES + 1 - bytes.len() as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "WORKSPACE_READ_FAILED")?;
    if bytes.len() as u64 != s.metadata.len() {
        return Err("WORKSPACE_CHANGED".into());
    }
    recheck(db, id, &relative, &s)?;
    let (mime, _, _) = crate::screenshot_gallery::dimensions(&bytes).map_err(|e| {
        if e == "SCREENSHOT_LIMIT" {
            "WORKSPACE_LIMIT"
        } else {
            "WORKSPACE_FORMAT"
        }
    })?;
    let output = if mime == "image/avif" {
        // The configured image crate has no AVIF decoder. The same bounded binary and
        // all-ispe validation as the gallery lets the WebView decode its supported AVIF.
        bytes
    } else {
        let mut reader = image::ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|_| "WORKSPACE_FORMAT")?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(96 * 1024 * 1024);
        limits.max_image_width = Some(32768);
        limits.max_image_height = Some(32768);
        reader.limits(limits);
        let decoded = reader.decode().map_err(|_| "WORKSPACE_FORMAT")?;
        crate::workbench_cache::encode(decoded).map_err(|_| "WORKSPACE_FORMAT")?
    };
    recheck(db, id, &relative, &s)?;
    if let Some(cache) = cache {
        let _ = cache.put(&cache_key, &output);
    }
    Ok(output)
}
fn open_checked<F>(db: &Database, id: i64, relative: &str, open: F) -> AppResult<()>
where
    F: FnOnce(&Path) -> AppResult<()>,
{
    let s = source(db, id, relative, None)?;
    if blocked(&extension(&s.path)) {
        return Err("WORKSPACE_OPEN_BLOCKED".into());
    }
    recheck(db, id, relative, &s)?;
    open(Path::new(&crate::db::display_path(&s.path)))
}
pub fn open(db: &Database, id: i64, relative: String) -> AppResult<()> {
    open_checked(
        db,
        id,
        &relative,
        crate::player::open_with_default_application,
    )
    .map_err(|e| {
        if e.starts_with("WORKSPACE_") {
            e
        } else {
            "WORKSPACE_OPEN_FAILED".into()
        }
    })
}
pub fn reveal(db: &Database, id: i64, relative: String) -> AppResult<()> {
    let (_, root) = registered_root(db, id)?;
    let path = safe_any_path(&root, &relative)?;
    if path.is_file() {
        let s = source(db, id, &relative, None)?;
        recheck(db, id, &relative, &s)?;
    }
    crate::player::reveal(Path::new(&crate::db::display_path(&path)))
        .map_err(|_| "WORKSPACE_OPEN_FAILED".into())
}
pub fn copy_path(db: &Database, id: i64, relative: String) -> AppResult<String> {
    let (_, root) = registered_root(db, id)?;
    let path = safe_any_path(&root, &relative)?;
    Ok(crate::db::display_path(&path))
}

pub(crate) fn drag_source(
    db: &Database,
    id: i64,
    relative: &str,
    expected: &str,
) -> AppResult<crate::workspace_drag::Source> {
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("WORKSPACE_CHANGED".into());
    }
    // Unlike opening, exporting accepts every ordinary file, including PSD and
    // project formats. It never executes it, and directories remain excluded.
    let s = source(db, id, relative, Some(expected))?;
    let pinned = crate::workspace_drag::Source::checked(
        s.path.clone(),
        &s.root,
        s.metadata.len(),
        &crate::comics::modified(&s.metadata),
    )?;
    recheck(db, id, relative, &s)?;
    Ok(pinned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use tempfile::TempDir;

    #[test]
    fn round10_drag_checks_registration_revision_and_ordinary_file_without_writes() {
        let (_temp, db, root, id) = fixture();
        let path = root.join("设计 [01].psd");
        fs::write(&path, b"synthetic project").unwrap();
        fs::create_dir(root.join("folder")).unwrap();
        let expected = revision(&fs::metadata(&path).unwrap());
        {
            let _pinned = drag_source(&db, id, "设计 [01].psd", &expected).unwrap();
            assert_eq!(fs::read(&path).unwrap(), b"synthetic project");
            #[cfg(windows)]
            assert!(fs::remove_file(&path).is_err());
        }
        assert!(drag_source(&db, id, "设计 [01].psd", &"0".repeat(64)).is_err());
        assert!(drag_source(&db, id, "folder", &expected).is_err());
        assert!(drag_source(&db, id, "../outside.psd", &expected).is_err());
        assert!(drag_source(&db, id, "design.psd:secret", &expected).is_err());
        remove(&db, id).unwrap();
        assert!(drag_source(&db, id, "设计 [01].psd", &expected).is_err());
        assert_eq!(fs::read(path).unwrap(), b"synthetic project");
    }

    fn fixture() -> (TempDir, Database, PathBuf, i64) {
        // The real user profile, application data and project sources are never fixtures.
        let parent = std::env::current_dir()
            .unwrap()
            .join("target/workspace-files-tests");
        fs::create_dir_all(&parent).unwrap();
        let temp = tempfile::Builder::new()
            .prefix("synthetic-")
            .tempdir_in(parent)
            .unwrap();
        let app = temp.path().join("app");
        let projects = temp.path().join("projects");
        fs::create_dir(&app).unwrap();
        fs::create_dir(&projects).unwrap();
        let db = Database::new(app.join("test.db"));
        db.migrate().unwrap();
        let id = import(&db, projects.to_string_lossy().into_owned())
            .unwrap()
            .id;
        (temp, db, projects, id)
    }
    fn listing(db: &Database, id: i64, relative: &str) -> WorkspaceListing {
        list(db, id, relative.into(), "NAME".into(), false).unwrap()
    }
    fn png(path: &Path, w: u32, h: u32) {
        image::RgbaImage::from_pixel(w, h, image::Rgba([20, 80, 200, 255]))
            .save_with_format(path, image::ImageFormat::Png)
            .unwrap();
    }

    #[test]
    fn round11_thumbnails_cache_without_redecoding_and_reject_changed_or_removed_sources() {
        let (_temp, db, root, id) = fixture();
        let path = root.join("poster.png");
        png(&path, 64, 36);
        let original = fs::read(&path).unwrap();
        let entry = listing(&db, id, "").entries.remove(0);
        let before = crate::workbench_cache::encode_count();
        let first =
            thumbnail(&db, id, entry.relative_path.clone(), entry.revision.clone()).unwrap();
        assert_eq!(
            crate::screenshot_gallery::dimensions(&first).unwrap().0,
            "image/jpeg"
        );
        assert_eq!(crate::workbench_cache::encode_count(), before + 1);
        assert_eq!(
            thumbnail(&db, id, entry.relative_path.clone(), entry.revision.clone()).unwrap(),
            first
        );
        assert_eq!(crate::workbench_cache::encode_count(), before + 1);
        assert_eq!(fs::read(&path).unwrap(), original);
        png(&path, 128, 72);
        assert!(thumbnail(&db, id, entry.relative_path, entry.revision).is_err());
        let current = listing(&db, id, "").entries.remove(0);
        assert_eq!(
            crate::screenshot_gallery::dimensions(
                &thumbnail(
                    &db,
                    id,
                    current.relative_path.clone(),
                    current.revision.clone()
                )
                .unwrap()
            )
            .unwrap()
            .1,
            128
        );
        remove(&db, id).unwrap();
        assert!(thumbnail(&db, id, current.relative_path, current.revision).is_err());
    }

    #[test]
    fn round11_synthetic_4k_triangle_and_warm_cache_timings_preserve_source() {
        let (_temp, db, root, id) = fixture();
        let path = root.join("synthetic-4k.jpg");
        let pixels = image::RgbImage::from_fn(3840, 2160, |x, y| {
            image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8])
        });
        let decoded = image::DynamicImage::ImageRgb8(pixels);
        decoded
            .save_with_format(&path, image::ImageFormat::Jpeg)
            .unwrap();
        let before = Sha256::digest(fs::read(&path).unwrap());
        let started = Instant::now();
        let old = decoded.resize(768, 768, image::imageops::FilterType::Lanczos3);
        let mut old_bytes = Cursor::new(Vec::new());
        old.write_to(&mut old_bytes, image::ImageFormat::Png)
            .unwrap();
        let old_micros = started.elapsed().as_micros();
        let started = Instant::now();
        let encoded = crate::workbench_cache::encode(decoded).unwrap();
        let triangle_micros = started.elapsed().as_micros();
        let entry = listing(&db, id, "").entries.remove(0);
        let started = Instant::now();
        let first =
            thumbnail(&db, id, entry.relative_path.clone(), entry.revision.clone()).unwrap();
        let cold_micros = started.elapsed().as_micros();
        let encodes = crate::workbench_cache::encode_count();
        let mut warm = Vec::new();
        for _ in 0..5 {
            let started = Instant::now();
            assert_eq!(
                thumbnail(&db, id, entry.relative_path.clone(), entry.revision.clone()).unwrap(),
                first
            );
            warm.push(started.elapsed().as_micros());
        }
        warm.sort_unstable();
        assert_eq!(crate::workbench_cache::encode_count(), encodes);
        assert_eq!(Sha256::digest(fs::read(path).unwrap()), before);
        println!("ROUND11_SYNTHETIC_4K debug=true old_lanczos_png_us={old_micros} triangle_jpeg_us={triangle_micros} old_bytes={} new_bytes={} cold_checked_request_us={cold_micros} warm_checked_median_us={} warm_encodes=0 source_sha256_unchanged=true",old_bytes.get_ref().len(),encoded.len(),warm[2]);
    }

    #[test]
    fn direct_children_have_kinds_natural_sort_and_unicode_breadcrumbs() {
        let (_temp, db, root, id) = fixture();
        fs::create_dir(root.join("素材_日本語_한국어")).unwrap();
        fs::create_dir(root.join("BGM")).unwrap();
        for name in [
            "cover10.psd",
            "cover2.psd",
            "稿件.md",
            "sound.flac",
            "字幕.ass",
            "edit.prproj",
            "assets.zip",
            "clip.mkv",
        ] {
            fs::write(root.join(name), name).unwrap();
        }
        png(&root.join("poster.jpg"), 32, 18);
        let page = listing(&db, id, "");
        assert_eq!(page.total, 11);
        assert!(page.entries[..2].iter().all(|e| e.kind == "FOLDER"));
        assert!(
            page.entries
                .iter()
                .position(|e| e.file_name == "cover2.psd")
                .unwrap()
                < page
                    .entries
                    .iter()
                    .position(|e| e.file_name == "cover10.psd")
                    .unwrap()
        );
        for (name, kind) in [
            ("cover2.psd", "PSD"),
            ("poster.jpg", "IMAGE"),
            ("稿件.md", "DOCUMENT"),
            ("sound.flac", "AUDIO"),
            ("字幕.ass", "SUBTITLE"),
            ("edit.prproj", "PROJECT"),
            ("assets.zip", "ARCHIVE"),
            ("clip.mkv", "VIDEO"),
        ] {
            let item = page.entries.iter().find(|e| e.file_name == name).unwrap();
            assert_eq!(item.kind, kind);
            assert!(item.can_open_default);
        }
        fs::create_dir(root.join("素材_日本語_한국어/inner")).unwrap();
        let child = listing(&db, id, "素材_日本語_한국어/inner");
        assert_eq!(child.breadcrumbs.len(), 3);
        assert_eq!(child.breadcrumbs[1].relative_path, "素材_日本語_한국어");
        assert_eq!(child.relative_dir, "素材_日本語_한국어/inner");
        for sort in ["TYPE", "SIZE", "MODIFIED"] {
            let sorted = list(&db, id, "".into(), sort.into(), true).unwrap();
            assert!(sorted.entries[..2].iter().all(|e| e.kind == "FOLDER"));
            assert_eq!(sorted.total, page.total);
        }
        fs::write(root.join("backup.psb"), "psb").unwrap();
        for descending in [false, true] {
            let by_type = list(&db, id, "".into(), "TYPE".into(), descending).unwrap();
            let non_folders: Vec<_> = by_type
                .entries
                .iter()
                .filter(|e| e.kind != "FOLDER")
                .collect();
            assert!(non_folders[..3].iter().all(|e| e.kind == "PSD"));
            assert!(non_folders[3..].iter().all(|e| e.kind != "PSD"));
            if !descending {
                assert_eq!(non_folders[0].file_name, "cover2.psd");
                assert_eq!(non_folders[1].file_name, "cover10.psd");
                assert_eq!(non_folders[2].file_name, "backup.psb");
            }
        }
        assert!(list(&db, id, "".into(), "name; DROP TABLE nodes".into(), false).is_err());
    }
    #[test]
    fn traversal_streams_and_special_spellings_never_reach_open_callback() {
        let (_temp, db, root, id) = fixture();
        fs::write(root.join("safe.txt"), "source").unwrap();
        for relative in [
            "../outside.txt",
            "..\\outside.txt",
            "/outside.txt",
            "\\\\server\\share\\x.txt",
            "C:\\x.txt",
            "safe.txt:stream",
            "safe.txt.",
            "safe.txt ",
            "./safe.txt",
            "inner//safe.txt",
            "",
            "safe.txt\0.exe",
        ] {
            let reached = RefCell::new(false);
            assert!(
                open_checked(&db, id, relative, |_| {
                    *reached.borrow_mut() = true;
                    Ok(())
                })
                .is_err(),
                "{relative}"
            );
            assert!(!*reached.borrow());
            if !relative.is_empty() {
                assert!(copy_path(&db, id, relative.into()).is_err(), "{relative}");
            }
        }
        assert!(source(&db, id, "missing.txt", None).is_err());
        assert!(import(&db, root.parent().unwrap().to_string_lossy().into_owned()).is_err());
    }
    #[test]
    fn executables_and_scripts_are_listed_but_rejected_and_safe_documents_are_literal() {
        let (_temp, db, root, id) = fixture();
        let dangerous = [
            "EXE",
            "msi",
            "msp",
            "bat",
            "cmd",
            "ps1",
            "msh",
            "msh1",
            "msh2",
            "pyc",
            "pyo",
            "psm1",
            "vbs",
            "vbe",
            "js",
            "jse",
            "wsf",
            "wsh",
            "hta",
            "scr",
            "com",
            "pif",
            "reg",
            "lnk",
            "cpl",
            "url",
            "scf",
            "appref-ms",
            "jar",
            "py",
        ];
        for ext in dangerous {
            let name = format!("blocked.{ext}");
            fs::write(root.join(&name), "synthetic inert test").unwrap();
            let reached = RefCell::new(false);
            let result = open_checked(&db, id, &name, |_| {
                *reached.borrow_mut() = true;
                Ok(())
            });
            assert_eq!(result.unwrap_err(), "WORKSPACE_OPEN_BLOCKED");
            assert!(!*reached.borrow());
        }
        assert!(listing(&db, id, "")
            .entries
            .iter()
            .all(|e| !e.can_open_default));
        let name = "构思 [draft] & 日本語 한국어.psd";
        fs::write(root.join(name), "synthetic PSD fixture").unwrap();
        let received = RefCell::new(None);
        open_checked(&db, id, name, |p| {
            *received.borrow_mut() = Some(p.to_path_buf());
            Ok(())
        })
        .unwrap();
        assert_eq!(
            received.into_inner().unwrap(),
            PathBuf::from(copy_path(&db, id, name.into()).unwrap())
        );
        assert_eq!(
            fs::read_to_string(root.join(name)).unwrap(),
            "synthetic PSD fixture"
        );
    }
    #[test]
    fn thumbnails_validate_signature_revision_limits_and_registration_without_source_writes() {
        let (_temp, db, root, id) = fixture();
        let path = root.join("image.jpg");
        png(&path, 1600, 900); // A PNG named JPG must use binary format evidence.
        let original = fs::read(&path).unwrap();
        let item = listing(&db, id, "").entries.remove(0);
        let thumb = thumbnail(&db, id, item.relative_path.clone(), item.revision.clone()).unwrap();
        assert_eq!(
            crate::comic_reader::image_dimensions(&thumb).unwrap(),
            (768, 432)
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(
            thumbnail(&db, id, item.relative_path.clone(), "stale".into()).unwrap_err(),
            "WORKSPACE_CHANGED"
        );
        fs::write(root.join("fake.png"), b"not a picture").unwrap();
        let fake = listing(&db, id, "")
            .entries
            .into_iter()
            .find(|e| e.file_name == "fake.png")
            .unwrap();
        assert_eq!(
            thumbnail(&db, id, fake.relative_path, fake.revision).unwrap_err(),
            "WORKSPACE_FORMAT"
        );
        remove(&db, id).unwrap();
        assert_eq!(
            thumbnail(&db, id, item.relative_path, item.revision).unwrap_err(),
            "WORKSPACE_NOT_FOUND"
        );
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    #[test]
    fn import_is_idempotent_metadata_only_and_removal_keeps_every_source_byte() {
        let (_temp, db, root, id) = fixture();
        fs::write(root.join("script.md"), b"# Creative source\n").unwrap();
        update(
            &db,
            id,
            "Custom project".into(),
            "private context".into(),
            None,
        )
        .unwrap();
        let imported = import(&db, root.to_string_lossy().into_owned()).unwrap();
        assert_eq!(imported.id, id);
        assert_eq!(imported.alias, "Custom project");
        assert_eq!(imported.note, "private context");
        assert_eq!(folders(&db).unwrap().len(), 1);
        remove(&db, id).unwrap();
        assert!(folders(&db).unwrap().is_empty());
        assert_eq!(
            fs::read(root.join("script.md")).unwrap(),
            b"# Creative source\n"
        );
        assert!(root.is_dir());
    }
    #[test]
    fn project_registration_and_cover_cache_exclude_each_other_without_creating_files() {
        let (temp, db, root, id) = fixture();
        let configured = temp.path().join("configured-cache");
        fs::create_dir(&configured).unwrap();
        let child = configured.join("child");
        fs::create_dir(&child).unwrap();
        db.connect()
            .unwrap()
            .execute(
                "INSERT OR REPLACE INTO settings(key,value) VALUES('cover_cache_directory',?1)",
                [configured.to_string_lossy()],
            )
            .unwrap();
        assert!(import(&db, configured.to_string_lossy().into_owned()).is_err());
        assert!(import(&db, child.to_string_lossy().into_owned()).is_err());
        assert_eq!(folders(&db).unwrap()[0].id, id);
        let protected = db.read_only_source_paths().unwrap();
        assert!(crate::cache::validate_cache_location(&root, protected.clone()).is_err());
        assert!(
            crate::cache::validate_cache_location(&root.join("new-cache"), protected.clone())
                .is_err()
        );
        assert!(
            crate::cache::validate_cache_location(root.parent().unwrap(), protected.clone())
                .is_err()
        );
        assert!(crate::cache::validate_cache_location(&configured, protected).is_ok());
        assert!(!root.join("new-cache").exists());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        // Database restoration can introduce an overlap without using the importer.
        // Every active cache operation must reject that state before initialization.
        db.connect()
            .unwrap()
            .execute(
                "INSERT INTO workspace_folders(path,alias) VALUES(?1,'Restored project')",
                [configured.to_string_lossy()],
            )
            .unwrap();
        assert!(crate::cache::validate_cache_location(
            &configured,
            db.read_only_source_paths().unwrap()
        )
        .is_err());
        assert_eq!(fs::read_dir(&configured).unwrap().count(), 1);
        assert!(!configured.join(".m2shelf-cover-cache").exists());
    }
    #[test]
    fn migration38_preserves_existing_gallery_and_settings_and_is_idempotent() {
        let (_temp, db, _root, _id) = fixture();
        let c = db.connect().unwrap();
        c.execute_batch("DROP TABLE workspace_folders; DROP TABLE mpv_playback_sessions; DELETE FROM mediashelf_schema_migrations WHERE version=38;
            INSERT INTO screenshot_folders(path,alias,note) VALUES('synthetic-offline','shots','retained note');
            INSERT INTO screenshot_images(folder_id,relative_path,file_name,file_size,modified_at,revision,mime,width,height,note,tags,starred) VALUES(1,'scene.png','scene.png',100,'stamp','rev','image/png',32,18,'kept','[\"tag\"]',1);
            INSERT OR REPLACE INTO settings(key,value) VALUES('round9-synthetic','keep');").unwrap();
        drop(c);
        db.migrate().unwrap();
        db.migrate().unwrap();
        let c = db.connect().unwrap();
        assert_eq!(
            c.query_row(
                "SELECT note||':'||tags||':'||starred FROM screenshot_images",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "kept:[\"tag\"]:1"
        );
        assert_eq!(
            c.query_row(
                "SELECT value FROM settings WHERE key='round9-synthetic'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "keep"
        );
        assert_eq!(
            c.query_row(
                "SELECT max(version) FROM mediashelf_schema_migrations",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            38
        );
        assert!(c
            .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
            .optional()
            .unwrap()
            .is_none());
    }
    #[test]
    fn descendant_links_are_rejected_without_following_them() {
        let (temp, db, root, id) = fixture();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("private.txt"), "outside fixture").unwrap();
        #[cfg(windows)]
        let link = std::os::windows::fs::symlink_dir(&outside, root.join("linked"));
        #[cfg(not(windows))]
        let link = std::os::unix::fs::symlink(&outside, root.join("linked"));
        if link.is_err() {
            return;
        } // OS privilege may prohibit fixture symlinks.
        assert!(listing(&db, id, "").entries.is_empty());
        assert!(list(&db, id, "linked".into(), "NAME".into(), false).is_err());
        assert!(copy_path(&db, id, "linked/private.txt".into()).is_err());
        let expected = revision(&fs::metadata(outside.join("private.txt")).unwrap());
        assert!(drag_source(&db, id, "linked/private.txt", &expected).is_err());
        assert!(open_checked(&db, id, "linked/private.txt", |_| panic!(
            "must not open link target"
        ))
        .is_err());
        assert_eq!(
            fs::read_to_string(outside.join("private.txt")).unwrap(),
            "outside fixture"
        );
    }
}

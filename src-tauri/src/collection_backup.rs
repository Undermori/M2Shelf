//! User collection backup. Archives never contain media, credentials or derived posters.
//! Restore is armed by the UI and applied BEFORE opening the application's database.
use crate::{
    cache,
    db::{AppResult, Database},
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions as FileOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

const SCHEMA: i64 = 36;
const MAX_DB: u64 = 512 * 1024 * 1024;
const MAX_COVERS: u64 = 256 * 1024 * 1024;
const MAX_COVER: u64 = 15 * 1024 * 1024;
const MAX_COUNT: usize = 2048;
const MAX_ARCHIVE: u64 = MAX_DB + MAX_COVERS + 2 * 1024 * 1024;
const SAFE_SETTINGS: &[&str] = &[
    "comic_reader",
    "text_reader",
    "mpv_path",
    "player_mode",
    "default_view_mode",
    "video_extensions",
    "bangumi_search_enabled",
    "language",
    "theme",
    "auto_check_updates",
    "auto_scan_on_startup",
    "all_resources_flattened",
    "main_window_size",
    "collection_sort_all",
    "collection_sort_browse",
    "collection_sort_favorites",
];

fn err<T>(_: T) -> String {
    "BACKUP_INVALID".into()
}
fn io<T>(result: std::io::Result<T>) -> AppResult<T> {
    result.map_err(|_| "BACKUP_IO".into())
}
fn sql<T>(result: rusqlite::Result<T>) -> AppResult<T> {
    result.map_err(|_| "BACKUP_DATABASE".into())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    name: String,
    bytes: u64,
    sha256: String,
    node_id: Option<i64>,
    #[serde(default)]
    book_id: Option<i64>,
}
impl Entry {
    fn is_cover(&self) -> bool {
        self.node_id.is_some() || self.book_id.is_some()
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    format_version: u32,
    schema_version: i64,
    app_version: String,
    created_at: String,
    entries: Vec<Entry>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub token: String,
    pub created_at: String,
    pub roots: usize,
    pub offline_roots: usize,
    pub manual_covers: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    token: String,
    phase: String,
    rollback_hash: Option<String>,
    rollback_bytes: Option<u64>,
    incoming_hash: String,
    incoming_bytes: u64,
}

/// Reject every existing reparse point, including ancestors, before reads/writes/cleanup.
fn plain(path: &Path, directory: bool) -> AppResult<()> {
    if !path.is_absolute() {
        return Err("BACKUP_UNSAFE_PATH".into());
    }
    for ancestor in path.ancestors() {
        if let Ok(m) = fs::symlink_metadata(ancestor) {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if m.file_attributes() & 0x400 != 0 {
                    return Err("BACKUP_UNSAFE_PATH".into());
                }
            }
            if m.file_type().is_symlink() {
                return Err("BACKUP_UNSAFE_PATH".into());
            }
        } else {
            return Err("BACKUP_UNSAFE_PATH".into());
        }
    }
    let m = io(fs::symlink_metadata(path))?;
    if directory != m.is_dir() || (!directory && !m.is_file()) {
        return Err("BACKUP_UNSAFE_PATH".into());
    }
    Ok(())
}
fn outside(path: &Path, roots: &[PathBuf]) -> AppResult<()> {
    for root in roots {
        if cache::paths_overlap_checked(path, root)? {
            return Err("BACKUP_UNSAFE_PATH".into());
        }
    }
    Ok(())
}
fn owned_root(app: &Path) -> AppResult<PathBuf> {
    plain(app, true)?;
    let root = app.join("collection-backups");
    if !root.exists() {
        io(fs::create_dir(&root))?;
    }
    plain(&root, true)?;
    Ok(root)
}
fn new_stage(app: &Path) -> AppResult<(String, PathBuf)> {
    let token = Uuid::new_v4().to_string();
    let path = owned_root(app)?.join(&token);
    io(fs::create_dir(&path))?;
    Ok((token, path))
}
fn stage(app: &Path, token: &str) -> AppResult<PathBuf> {
    if Uuid::parse_str(token).map_err(err)?.to_string() != token {
        return Err("BACKUP_INVALID".into());
    }
    let path = owned_root(app)?.join(token);
    plain(&path, true)?;
    Ok(path)
}
fn bounded(path: &Path, max: u64) -> AppResult<Vec<u8>> {
    plain(path, false)?;
    let file = io(File::open(path))?;
    if io(file.metadata())?.len() > max {
        return Err("BACKUP_LIMIT".into());
    }
    let mut bytes = Vec::new();
    io(file.take(max + 1).read_to_end(&mut bytes))?;
    if bytes.len() as u64 > max {
        return Err("BACKUP_LIMIT".into());
    }
    Ok(bytes)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn file_hash(path: &Path, max: u64) -> AppResult<(u64, String)> {
    plain(path, false)?;
    let mut f = io(File::open(path))?;
    let size = io(f.metadata())?.len();
    if size > max {
        return Err("BACKUP_LIMIT".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    let mut read = 0;
    loop {
        let n = io(f.read(&mut buffer))?;
        if n == 0 {
            break;
        }
        read += n as u64;
        if read > max {
            return Err("BACKUP_LIMIT".into());
        }
        hash.update(&buffer[..n]);
    }
    if read != size {
        return Err("BACKUP_INVALID".into());
    }
    Ok((size, format!("{:x}", hash.finalize())))
}
fn write_new(path: &Path, bytes: &[u8]) -> AppResult<()> {
    plain(path.parent().ok_or("BACKUP_UNSAFE_PATH")?, true)?;
    let mut f = io(FileOptions::new().create_new(true).write(true).open(path))?;
    io(f.write_all(bytes))?;
    io(f.sync_all())
}
fn replace(source: &Path, target: &Path) -> AppResult<()> {
    plain(source, false)?;
    plain(target.parent().ok_or("BACKUP_UNSAFE_PATH")?, true)?;
    if target.exists() {
        plain(target, false)?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let s: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let t: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            MoveFileExW(
                s.as_ptr(),
                t.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err("BACKUP_IO".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        io(fs::rename(source, target))
    }
}
fn atomic_json<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    let temp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    write_new(&temp, &serde_json::to_vec(value).map_err(err)?)?;
    replace(&temp, path)
}
fn space(path: &Path, needed: u64) -> AppResult<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let p: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut available = 0;
        if unsafe {
            GetDiskFreeSpaceExW(
                p.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err("BACKUP_IO".into());
        }
        if available < needed {
            return Err("BACKUP_SPACE".into());
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (path, needed);
    }
    Ok(())
}
fn readonly(path: &Path) -> AppResult<Connection> {
    plain(path, false)?;
    let c = sql(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ))?;
    sql(c.execute_batch(
        "PRAGMA trusted_schema=OFF; PRAGMA query_only=ON; PRAGMA busy_timeout=15000;",
    ))?;
    Ok(c)
}
fn schema(c: &Connection) -> AppResult<Vec<(String, String, String)>> {
    let mut s=sql(c.prepare("SELECT type,name,sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY type,name"))?;
    let rows = sql(s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))))?;
    let rows: Vec<(String, String, String)> = sql(rows.collect())?;
    // SQLite keeps the original DDL spelling. Historical factories/releases differ in
    // whitespace; normalize only tokens outside quoted values, never the schema objects.
    Ok(rows
        .into_iter()
        .map(|(kind, name, ddl)| (kind, name, normalize_schema_sql(&ddl)))
        .collect())
}
fn normalize_schema_sql(ddl: &str) -> String {
    let mut quote = None;
    let mut normalized = String::new();
    let mut chars = ddl.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(delimiter) = quote {
            normalized.push(ch);
            if ch == delimiter {
                if chars.peek() == Some(&delimiter) {
                    normalized.push(chars.next().unwrap());
                } else {
                    quote = None;
                }
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
            normalized.push(ch);
        } else if !ch.is_whitespace() {
            normalized.push(ch.to_ascii_lowercase());
        }
    }
    normalized
}
// Old backups must retain their exact historical guards, not skip trigger validation.
fn legacy_tmdb_guard(name: &str) -> AppResult<&'static str> {
    if !matches!(name, "tmdb_movie_scope" | "tmdb_movie_update_scope") {
        return Err("BACKUP_SCHEMA".into());
    }
    let migration = include_str!("../migrations/0025_smart_mixed.sql");
    let start = migration
        .find(&format!("CREATE TRIGGER {name} "))
        .ok_or("BACKUP_SCHEMA")?;
    let tail = &migration[start..];
    let end = tail.find(" END;").ok_or("BACKUP_SCHEMA")? + 4;
    Ok(&tail[..end])
}

fn reference(stage: &Path) -> AppResult<Vec<(String, String, String)>> {
    let path = stage.join("schema-reference.db");
    if path.exists() {
        plain(&path, false)?;
        io(fs::remove_file(&path))?;
    }
    let db = Database::new(path.clone());
    db.migrate().map_err(err)?;
    let signature = schema(&db.connect().map_err(err)?)?;
    // The last connection has closed; this file is an empty generated schema, never user data.
    for suffix in ["", "-wal", "-shm"] {
        let p = PathBuf::from(format!("{}{suffix}", path.display()));
        if p.exists() {
            plain(&p, false)?;
            io(fs::remove_file(p))?;
        }
    }
    Ok(signature)
}
fn check_db(path: &Path, expected: &[(String, String, String)]) -> AppResult<Vec<PathBuf>> {
    if io(fs::metadata(path))?.len() > MAX_DB {
        return Err("BACKUP_LIMIT".into());
    }
    let c = readonly(path)?;
    let versions: Vec<i64> = {
        let mut s =
            sql(c.prepare("SELECT version FROM mediashelf_schema_migrations ORDER BY version"))?;
        let result = sql(sql(s.query_map([], |r| r.get(0)))?.collect())?;
        result
    };
    let version = versions.last().copied().unwrap_or(0);
    let compatible = expected
        .iter()
        .filter(|(_, name, _)| {
            (version >= 35 || name != "library_all_resources_visibility")
                && (version >= 33 || (name != "private_notes" && name != "book_manual_covers"))
                && (version >= 34
                    || (name != "episode_watch_states"
                        && name != "episode_watch_scope"
                        && name != "episode_watch_source"))
        })
        .map(|entry| {
            let mut entry = entry.clone();
            if version < 36
                && entry.0 == "trigger"
                && matches!(
                    entry.1.as_str(),
                    "tmdb_movie_scope" | "tmdb_movie_update_scope"
                )
            {
                entry.2 = normalize_schema_sql(legacy_tmdb_guard(&entry.1)?);
            }
            Ok(entry)
        })
        .collect::<AppResult<Vec<_>>>()?;
    if !(32..=SCHEMA).contains(&version)
        || schema(&c)? != compatible
        || versions != (1..=version).collect::<Vec<_>>()
    {
        return Err("BACKUP_SCHEMA".into());
    }
    if sql(c.query_row("PRAGMA integrity_check(1)", [], |r| r.get::<_, String>(0)))? != "ok" {
        return Err("BACKUP_DATABASE".into());
    }
    let mut s = sql(c.prepare("PRAGMA foreign_key_check"))?;
    if sql(s.exists([]))? {
        return Err("BACKUP_DATABASE".into());
    }
    let mut s = sql(c.prepare("SELECT path FROM library_roots"))?;
    let roots: Vec<String> = sql(sql(s.query_map([], |r| r.get(0)))?.collect())?;
    if roots.iter().any(|r| !Path::new(r).is_absolute()) {
        return Err("BACKUP_INVALID".into());
    }
    Ok(roots.into_iter().map(PathBuf::from).collect())
}
fn snapshot(source: &Path, destination: &Path) -> AppResult<()> {
    if destination.exists() {
        return Err("BACKUP_IO".into());
    }
    if io(fs::metadata(source))?.len() > MAX_DB {
        return Err("BACKUP_LIMIT".into());
    }
    let source = readonly(source)?;
    sql(source.execute_batch("BEGIN; SELECT count(*) FROM sqlite_master;"))?;
    let mut dest = sql(Connection::open(destination))?;
    {
        let backup = sql(rusqlite::backup::Backup::new(&source, &mut dest))?;
        sql(backup.run_to_completion(128, Duration::from_millis(2), None))?;
    }
    sql(dest.execute_batch("PRAGMA journal_mode=DELETE;"))?;
    drop(dest);
    io(io(FileOptions::new().write(true).open(destination))?.sync_all())
}
fn allowed_setting(key: &str) -> bool {
    SAFE_SETTINGS.contains(&key)
        || key
            .strip_prefix("library_scan_warnings_ignored:")
            .is_some_and(|s| s.parse::<i64>().is_ok_and(|id| id > 0))
}
fn sanitize(path: &Path) -> AppResult<()> {
    let c = sql(Connection::open(path))?;
    let keys: Vec<String> = {
        let mut s = sql(c.prepare("SELECT key FROM settings"))?;
        let result = sql(sql(s.query_map([], |r| r.get(0)))?.collect())?;
        result
    };
    for key in keys {
        if !allowed_setting(&key) {
            sql(c.execute("DELETE FROM settings WHERE key=?1", [key]))?;
        }
    }
    sql(c.execute_batch("UPDATE nodes SET cover_cache_path=NULL WHERE cover_source<>'MANUAL'; UPDATE tmdb_movie_bindings SET cover_cache_path=NULL,cover_error=NULL; UPDATE metadata_bindings SET cover_download_error=NULL; DELETE FROM poster_cache_failures; DELETE FROM scan_runs; UPDATE library_scan_health SET detail=NULL; VACUUM;"))?;
    Ok(())
}
fn cover_name(id: i64) -> String {
    format!("manual/{id}.image")
}
fn valid_entry(e: &Entry) -> bool {
    e.sha256.len() == 64
        && e.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        && match (e.node_id, e.book_id) {
            (None, None) => e.name == "collection.db" && e.bytes <= MAX_DB,
            (Some(id), None) => {
                id > 0 && e.name == cover_name(id) && e.bytes > 0 && e.bytes <= MAX_COVER
            }
            (None, Some(id)) => {
                id > 0
                    && e.name == format!("manual/book-{id}.image")
                    && e.bytes > 0
                    && e.bytes <= MAX_COVER
            }
            _ => false,
        }
}
fn archive_entry(writer: &mut ZipWriter<File>, path: &Path, entry: &Entry) -> AppResult<()> {
    writer
        .start_file(
            &entry.name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .map_err(err)?;
    let mut f = io(File::open(path))?;
    io(std::io::copy(&mut f, writer))?;
    Ok(())
}
pub fn export(db: &Database, app: &Path, destination: &Path) -> AppResult<()> {
    if destination.extension().and_then(|s| s.to_str()) != Some("m2backup") {
        return Err("BACKUP_INVALID".into());
    }
    let roots = db.library_paths().map_err(err)?;
    outside(destination, &roots)?;
    outside(app, &roots)?;
    let parent = destination.parent().ok_or("BACKUP_UNSAFE_PATH")?;
    plain(parent, true)?;
    if cache::is_equal_or_within_checked(destination, app)? {
        return Err("BACKUP_UNSAFE_PATH".into());
    }
    if destination.exists() {
        plain(destination, false)?;
    }
    space(parent, MAX_ARCHIVE)?;
    let (_, dir) = new_stage(app)?;
    let database = dir.join("collection.db");
    let temp = parent.join(format!(".m2shelf-backup-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let _guard = cache::begin_cover_cache_operation();
        snapshot(db.path(), &database)?;
        let expected = reference(&dir)?;
        check_db(&database, &expected)?;
        let c = sql(Connection::open(&database))?;
        let covers: Vec<(i64, String, bool)> = {
            let mut q=sql(c.prepare("SELECT id,cover_cache_path,0 FROM nodes WHERE cover_source='MANUAL' UNION ALL SELECT book_id,cover_cache_path,1 FROM book_manual_covers"))?;
            let result =
                sql(sql(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))))?.collect())?;
            result
        };
        if covers.len() > MAX_COUNT {
            return Err("BACKUP_LIMIT".into());
        }
        io(fs::create_dir(dir.join("manual")))?;
        let mut total = 0;
        let mut entries = Vec::new();
        for (id, source, is_book) in covers {
            let source = PathBuf::from(source);
            plain(&source, false)?;
            outside(&source, &roots)?;
            let manual = source.parent().ok_or("BACKUP_UNSAFE_PATH")?;
            let cache_root = manual.parent().ok_or("BACKUP_UNSAFE_PATH")?;
            let expected_stem = format!("{}-{id}", if is_book { "book" } else { "node" });
            let stem = source
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("BACKUP_UNSAFE_PATH")?;
            if manual.file_name().and_then(|s| s.to_str()) != Some("manual")
                || !cache::is_manual_stem(stem)
                || !(stem == expected_stem || stem.starts_with(&(expected_stem + "-")))
            {
                return Err("BACKUP_UNSAFE_PATH".into());
            }
            if bounded(&cache_root.join(".m2shelf-cover-cache"), 128)?
                != b"M2Shelf cover cache v1\n"
            {
                return Err("BACKUP_UNSAFE_PATH".into());
            }
            let bytes = cache::read_cover_payload(&source).map_err(|_| "BACKUP_COVER")?;
            total += bytes.len() as u64;
            if total > MAX_COVERS {
                return Err("BACKUP_LIMIT".into());
            }
            let name = if is_book {
                format!("manual/book-{id}.image")
            } else {
                cover_name(id)
            };
            write_new(&dir.join(&name), &bytes)?;
            if is_book {
                sql(c.execute(
                    "UPDATE book_manual_covers SET cover_cache_path=?2 WHERE book_id=?1",
                    params![id, format!("m2backup:book:{id}")],
                ))?;
            } else {
                sql(c.execute(
                    "UPDATE nodes SET cover_cache_path=?2 WHERE id=?1",
                    params![id, format!("m2backup:{id}")],
                ))?;
            }
            entries.push(Entry {
                name,
                bytes: bytes.len() as u64,
                sha256: digest(&bytes),
                node_id: if is_book { None } else { Some(id) },
                book_id: if is_book { Some(id) } else { None },
            });
        }
        drop(c);
        sanitize(&database)?;
        check_db(&database, &expected)?;
        let (bytes, sha256) = file_hash(&database, MAX_DB)?;
        entries.insert(
            0,
            Entry {
                name: "collection.db".into(),
                bytes,
                sha256,
                node_id: None,
                book_id: None,
            },
        );
        let manifest = Manifest {
            format_version: 1,
            schema_version: SCHEMA,
            app_version: env!("CARGO_PKG_VERSION").into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            entries,
        };
        let file = io(FileOptions::new().write(true).create_new(true).open(&temp))?;
        let mut zip = ZipWriter::new(file);
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .map_err(err)?;
        io(zip.write_all(&serde_json::to_vec(&manifest).map_err(err)?))?;
        for entry in &manifest.entries {
            archive_entry(&mut zip, &dir.join(&entry.name), entry)?;
        }
        io(zip.finish().map_err(err)?.sync_all())?;
        replace(&temp, destination)
    })();
    if temp.exists() {
        let _ = plain(&temp, false).and_then(|()| io(fs::remove_file(&temp)));
    }
    cleanup_stage(&dir)?;
    result
}
fn cleanup_stage(dir: &Path) -> AppResult<()> {
    plain(dir, true)?;
    // Enumerate first, reject reparse/unknown types; never use recursive deletion on untrusted paths.
    for e in io(fs::read_dir(dir))? {
        let p = io(e)?.path();
        let m = io(fs::symlink_metadata(&p))?;
        if m.is_dir() {
            plain(&p, true)?;
            for f in io(fs::read_dir(&p))? {
                let f = io(f)?.path();
                plain(&f, false)?;
                io(fs::remove_file(f))?;
            }
            io(fs::remove_dir(p))?;
        } else {
            plain(&p, false)?;
            io(fs::remove_file(p))?;
        }
    }
    io(fs::remove_dir(dir))
}
fn extract(source: &Path, dir: &Path) -> AppResult<Manifest> {
    plain(source, false)?;
    if io(fs::metadata(source))?.len() > MAX_ARCHIVE {
        return Err("BACKUP_LIMIT".into());
    }
    let mut archive = ZipArchive::new(io(File::open(source))?).map_err(err)?;
    if archive.len() < 2 || archive.len() > MAX_COUNT + 2 {
        return Err("BACKUP_LIMIT".into());
    }
    let mut names = BTreeSet::new();
    let mut total = 0;
    for i in 0..archive.len() {
        let f = archive.by_index(i).map_err(err)?;
        let name = f.name().to_string();
        total += f.size();
        if f.is_dir()
            || f.encrypted()
            || !names.insert(name.clone())
            || name.contains('\\')
            || name.contains("..")
            || name.starts_with('/')
            || name.contains(':')
            || f.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
            || f.size() > MAX_DB
            || total > MAX_ARCHIVE
        {
            return Err("BACKUP_INVALID".into());
        }
    }
    let raw = {
        let f = archive.by_name("manifest.json").map_err(err)?;
        if f.size() > 1024 * 1024 {
            return Err("BACKUP_LIMIT".into());
        }
        let mut bytes = Vec::new();
        io(f.take(1024 * 1024 + 1).read_to_end(&mut bytes))?;
        bytes
    };
    let manifest: Manifest = serde_json::from_slice(&raw).map_err(err)?;
    if manifest.format_version != 1 || !(32..=SCHEMA).contains(&manifest.schema_version) {
        return Err("BACKUP_SCHEMA".into());
    }
    if manifest.entries.len() + 1 != names.len()
        || manifest.entries.iter().filter(|e| !e.is_cover()).count() != 1
        || manifest.entries.len() > MAX_COUNT + 1
        || chrono::DateTime::parse_from_rfc3339(&manifest.created_at).is_err()
    {
        return Err("BACKUP_INVALID".into());
    }
    let mut listed = BTreeSet::new();
    let mut cover_total = 0;
    io(fs::create_dir(dir.join("manual")))?;
    for e in &manifest.entries {
        if !valid_entry(e) || !names.contains(&e.name) || !listed.insert(&e.name) {
            return Err("BACKUP_INVALID".into());
        }
        if e.is_cover() {
            cover_total += e.bytes;
            if cover_total > MAX_COVERS {
                return Err("BACKUP_LIMIT".into());
            }
        }
        let mut f = archive.by_name(&e.name).map_err(err)?;
        if f.size() != e.bytes {
            return Err("BACKUP_INVALID".into());
        }
        let path = dir.join(&e.name);
        let mut dest = io(FileOptions::new().write(true).create_new(true).open(&path))?;
        let mut hash = Sha256::new();
        let mut count = 0;
        let mut buffer = [0; 65536];
        loop {
            let n = io(f.read(&mut buffer))?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > e.bytes {
                return Err("BACKUP_LIMIT".into());
            }
            hash.update(&buffer[..n]);
            io(dest.write_all(&buffer[..n]))?;
        }
        io(dest.sync_all())?;
        if count != e.bytes || format!("{:x}", hash.finalize()) != e.sha256 {
            return Err("BACKUP_INVALID".into());
        }
        if e.is_cover() {
            cache::read_cover_payload(&path).map_err(|_| "BACKUP_COVER")?;
        }
    }
    write_new(&dir.join("manifest.json"), &raw)?;
    Ok(manifest)
}
fn validate_stage(dir: &Path) -> AppResult<(Manifest, Vec<PathBuf>)> {
    let manifest: Manifest =
        serde_json::from_slice(&bounded(&dir.join("manifest.json"), 1024 * 1024)?).map_err(err)?;
    if manifest.format_version != 1 || !(32..=SCHEMA).contains(&manifest.schema_version) {
        return Err("BACKUP_SCHEMA".into());
    }
    for e in &manifest.entries {
        if !valid_entry(e)
            || file_hash(
                &dir.join(&e.name),
                if e.is_cover() { MAX_COVER } else { MAX_DB },
            )? != (e.bytes, e.sha256.clone())
        {
            return Err("BACKUP_INVALID".into());
        }
    }
    let roots = check_db(&dir.join("collection.db"), &reference(dir)?)?;
    outside(dir, &roots)?;
    let c = readonly(&dir.join("collection.db"))?;
    let manuals: Vec<(i64, String, bool)> = {
        let query = if manifest.schema_version >= 33 {
            "SELECT id,cover_cache_path,0 FROM nodes WHERE cover_source='MANUAL' UNION ALL SELECT book_id,cover_cache_path,1 FROM book_manual_covers"
        } else {
            "SELECT id,cover_cache_path,0 FROM nodes WHERE cover_source='MANUAL'"
        };
        let mut q = sql(c.prepare(query))?;
        let result =
            sql(sql(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))))?.collect())?;
        result
    };
    if manuals.len() != manifest.entries.iter().filter(|e| e.is_cover()).count() {
        return Err("BACKUP_INVALID".into());
    }
    for (id, path, is_book) in manuals {
        let expected = if is_book {
            format!("m2backup:book:{id}")
        } else {
            format!("m2backup:{id}")
        };
        if path != expected
            || !manifest.entries.iter().any(|e| {
                if is_book {
                    e.book_id == Some(id)
                } else {
                    e.node_id == Some(id)
                }
            })
        {
            return Err("BACKUP_INVALID".into());
        }
    }
    let mut s = sql(c.prepare("SELECT key FROM settings"))?;
    for key in sql(sql(s.query_map([], |r| r.get::<_, String>(0)))?.collect::<Result<Vec<_>, _>>())?
    {
        if !allowed_setting(&key) {
            return Err("BACKUP_INVALID".into());
        }
    }
    Ok((manifest, roots))
}
pub fn prepare(app: &Path, source: &Path) -> AppResult<Preview> {
    plain(app, true)?;
    let (token, dir) = new_stage(app)?;
    let result = (|| {
        space(app, MAX_ARCHIVE * 2)?;
        let manifest = extract(source, &dir)?;
        let (_, roots) = validate_stage(&dir)?;
        Ok(Preview {
            token,
            created_at: manifest.created_at,
            offline_roots: roots.iter().filter(|p| !p.is_dir()).count(),
            roots: roots.len(),
            manual_covers: manifest.entries.iter().filter(|e| e.is_cover()).count(),
        })
    })();
    if result.is_err() {
        cleanup_stage(&dir)?;
    }
    result
}
pub fn discard(app: &Path, token: &str) -> AppResult<()> {
    if owned_root(app)?.join("restore.json").exists() {
        return Err("BACKUP_BUSY".into());
    }
    cleanup_stage(&stage(app, token)?)
}
pub fn arm(app: &Path, token: &str, current: &Database) -> AppResult<()> {
    let dir = stage(app, token)?;
    let (_, roots) = validate_stage(&dir)?;
    outside(app, &roots)?;
    outside(app, &current.library_paths().map_err(err)?)?;
    space(app, MAX_ARCHIVE * 2)?;
    let (incoming_bytes, incoming_hash) = file_hash(&dir.join("collection.db"), MAX_DB)?;
    let marker = owned_root(app)?.join("restore.json");
    if marker.exists() {
        return Err("BACKUP_BUSY".into());
    }
    atomic_json(
        &marker,
        &Journal {
            token: token.into(),
            phase: "Armed".into(),
            rollback_hash: None,
            rollback_bytes: None,
            incoming_hash,
            incoming_bytes,
        },
    )
}
pub fn disarm(app: &Path, token: &str) -> AppResult<()> {
    let marker = owned_root(app)?.join("restore.json");
    let journal: Journal = serde_json::from_slice(&bounded(&marker, 8192)?).map_err(err)?;
    if journal.phase != "Armed" || journal.token != token {
        return Err("BACKUP_BUSY".into());
    }
    io(fs::remove_file(marker))
}
fn clear_sidecars(database: &Path) -> AppResult<()> {
    for suffix in ["-wal", "-shm"] {
        let p = PathBuf::from(format!("{}{suffix}", database.display()));
        if p.exists() {
            plain(&p, false)?;
            io(fs::remove_file(p))?;
        }
    }
    Ok(())
}
fn preserve_local(current: &Path, incoming: &Path) -> AppResult<()> {
    let old = readonly(current)?;
    let source: Option<String> = sql(old
        .query_row(
            "SELECT value FROM settings WHERE key='tmdb_credential_source'",
            [],
            |r| r.get(0),
        )
        .optional())?;
    let c = sql(Connection::open(incoming))?;
    if let Some(source) = source {
        sql(c.execute(
            "INSERT OR REPLACE INTO settings(key,value) VALUES('tmdb_credential_source',?1)",
            [source],
        ))?;
    }
    // This machine's validated cache location remains local; backup paths are not portable.
    let cache: Option<String> = sql(old
        .query_row(
            "SELECT value FROM settings WHERE key='cover_cache_directory'",
            [],
            |r| r.get(0),
        )
        .optional())?;
    if let Some(cache) = cache {
        let mut s = sql(c.prepare("SELECT path FROM library_roots"))?;
        let roots: Vec<PathBuf> =
            sql(sql(s.query_map([], |r| Ok(PathBuf::from(r.get::<_, String>(0)?))))?.collect())?;
        crate::cache::validate_cache_location(Path::new(&cache), roots)
            .map_err(|_| "BACKUP_UNSAFE_PATH")?;
        sql(c.execute(
            "INSERT OR REPLACE INTO settings(key,value) VALUES('cover_cache_directory',?1)",
            [cache],
        ))?;
    }
    Ok(())
}
/// Called only under the normal process mutex and before Database::new/migrate/background jobs.
/// A Committing journal always rolls back, including after an unclean process interruption.
pub fn startup(app: &Path) -> AppResult<Option<&'static str>> {
    let root = owned_root(app)?;
    let marker = root.join("restore.json");
    if !marker.exists() {
        return Ok(None);
    }
    let mut journal: Journal = serde_json::from_slice(&bounded(&marker, 8192)?).map_err(err)?;
    let database = app.join("morimediashelf.db");
    if journal.phase == "Completed" || journal.phase == "RolledBack" {
        if Uuid::parse_str(&journal.token).map_err(err)?.to_string() != journal.token
            || file_hash(&database, MAX_DB)?
                != (journal.incoming_bytes, journal.incoming_hash.clone())
        {
            return Err("BACKUP_ROLLBACK_FAILED".into());
        }
        let dir = root.join(&journal.token);
        if dir.exists() {
            cleanup_stage(&dir)?;
        }
        io(fs::remove_file(marker))?;
        return Ok(Some(if journal.phase == "Completed" {
            "BACKUP_RESTORED"
        } else {
            "BACKUP_ROLLED_BACK"
        }));
    }
    let dir = stage(app, &journal.token)?;
    let rollback = dir.join("rollback.db");
    if journal.phase == "Committing" {
        let bytes = journal.rollback_bytes.ok_or("BACKUP_INVALID")?;
        let hash = journal.rollback_hash.as_ref().ok_or("BACKUP_INVALID")?;
        if file_hash(&rollback, MAX_DB) != (Ok((bytes, hash.clone()))) {
            return Err("BACKUP_ROLLBACK_FAILED".into());
        }
        check_db(&rollback, &reference(&dir)?)?;
        let restore = dir.join("rollback-apply.db");
        if restore.exists() {
            plain(&restore, false)?;
            io(fs::remove_file(&restore))?;
        }
        snapshot(&rollback, &restore)?;
        clear_sidecars(&database)?;
        replace(&restore, &database)?;
        journal.phase = "RolledBack".into();
        (journal.incoming_bytes, journal.incoming_hash) = file_hash(&database, MAX_DB)?;
        atomic_json(&marker, &journal)?;
        cleanup_stage(&dir)?;
        io(fs::remove_file(marker))?;
        return Ok(Some("BACKUP_ROLLED_BACK"));
    }
    if journal.phase != "Armed" {
        return Err("BACKUP_INVALID".into());
    }
    let apply: AppResult<()> = (|| {
        let (manifest, roots) = validate_stage(&dir)?;
        outside(app, &roots)?;
        if file_hash(&dir.join("collection.db"), MAX_DB)
            != (Ok((journal.incoming_bytes, journal.incoming_hash.clone())))
        {
            return Err("BACKUP_INVALID".into());
        }
        plain(&database, false)?;
        let old_roots = check_db(&database, &reference(&dir)?)?;
        outside(app, &old_roots)?;
        space(app, MAX_ARCHIVE * 2)?;
        snapshot(&database, &rollback)?;
        let (bytes, hash) = file_hash(&rollback, MAX_DB)?;
        check_db(&rollback, &reference(&dir)?)?;
        let incoming = dir.join("apply.db");
        snapshot(&dir.join("collection.db"), &incoming)?;
        Database::new(incoming.clone()).migrate().map_err(err)?;
        preserve_local(&database, &incoming)?;
        let cache_dir = app.join("cache");
        if !cache_dir.exists() {
            io(fs::create_dir(&cache_dir))?;
        }
        plain(&cache_dir, true)?;
        let covers = cache_dir.join("restored-covers");
        outside(&covers, &roots)?;
        if !covers.exists() {
            io(fs::create_dir(&covers))?;
        }
        plain(&covers, true)?;
        let cover_root = covers.join(&journal.token);
        io(fs::create_dir(&cover_root))?;
        cache::ensure_directories(&cover_root).map_err(err)?;
        {
            let c = sql(Connection::open(&incoming))?;
            for e in &manifest.entries {
                if let Some(id) = e.node_id.or(e.book_id) {
                    let bytes = bounded(&dir.join(&e.name), MAX_COVER)?;
                    let format = image::guess_format(&bytes).map_err(err)?;
                    let ext = match format {
                        image::ImageFormat::Png => "png",
                        image::ImageFormat::Jpeg => "jpg",
                        image::ImageFormat::WebP => "webp",
                        _ => return Err("BACKUP_COVER".into()),
                    };
                    let target = cover_root.join("manual").join(format!(
                        "{}-{id}.{ext}",
                        if e.book_id.is_some() { "book" } else { "node" }
                    ));
                    write_new(&target, &bytes)?;
                    sql(c.execute(
                        if e.book_id.is_some() {
                            "UPDATE book_manual_covers SET cover_cache_path=?2 WHERE book_id=?1"
                        } else {
                            "UPDATE nodes SET cover_cache_path=?2 WHERE id=?1"
                        },
                        params![id, target.to_string_lossy()],
                    ))?;
                }
            }
        }
        check_db(&incoming, &reference(&dir)?)?;
        journal.phase = "Committing".into();
        journal.rollback_bytes = Some(bytes);
        journal.rollback_hash = Some(hash);
        atomic_json(&marker, &journal)?;
        clear_sidecars(&database)?;
        replace(&incoming, &database)?;
        #[cfg(test)]
        if FAIL_AFTER_REPLACE.with(|flag| flag.replace(false)) {
            return Err("BACKUP_IO".into());
        }
        check_db(&database, &reference(&dir)?)?;
        journal.phase = "Completed".into();
        (journal.incoming_bytes, journal.incoming_hash) = file_hash(&database, MAX_DB)?;
        atomic_json(&marker, &journal)?;
        Ok(())
    })();
    if let Err(error) = apply {
        if journal.phase == "Committing" {
            startup(app)?;
            return Ok(Some("BACKUP_ROLLED_BACK"));
        }
        cleanup_stage(&dir)?;
        io(fs::remove_file(marker))?;
        let _ = error;
        return Ok(Some("BACKUP_RESTORE_FAILED"));
    }
    cleanup_stage(&dir)?;
    io(fs::remove_file(marker))?;
    Ok(Some("BACKUP_RESTORED"))
}

#[cfg(test)]
thread_local! {static FAIL_AFTER_REPLACE:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{self, Status, Target};
    fn fixture() -> (tempfile::TempDir, PathBuf, Database, PathBuf) {
        let t = crate::db::test_temp_dir();
        let app = t.path().join("application");
        fs::create_dir(&app).unwrap();
        let root = t.path().join("Media");
        fs::create_dir(&root).unwrap();
        let db = Database::new(app.join("morimediashelf.db"));
        db.migrate().unwrap();
        let c = db.connect().unwrap();
        c.execute("INSERT INTO library_roots(id,path,display_name,media_kind,video_subject_scope) VALUES(1,?1,'Movies','VIDEO','LIVE_ACTION')",[root.to_string_lossy()]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(1,1,?1,'Media','Media','CONTAINER')",[root.to_string_lossy()]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,direct_video_count,total_video_count,manual_display_name) VALUES(2,1,1,?1,'Movie','My movie','WORK',1,1,1)",[root.join("Movie").to_string_lossy()]).unwrap();
        c.execute("INSERT INTO media_files(node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(2,?1,'movie.mkv','mkv',1,'2026-10-22')",[root.join("Movie/movie.mkv").to_string_lossy()]).unwrap();
        c.execute("INSERT INTO comic_books(id,node_id,source_path,source_kind,display_name,modified_at,page_count) VALUES(2,2,?1,'ZIP_ARCHIVE','PDF book','2026-10-22',3)",[root.join("Book.pdf").to_string_lossy()]).unwrap();
        c.execute_batch("INSERT INTO comic_reading_progress(comic_book_id,last_page_index) VALUES(2,1);INSERT INTO comic_bookmarks(comic_book_id,page_index) VALUES(2,1);INSERT INTO tags(id,name,normalized_name) VALUES(1,'My tag','my tag');INSERT INTO node_tags(node_id,tag_id) VALUES(2,1);INSERT INTO favorite_folders(id,name,normalized_name) VALUES(1,'Favorites','favorites');INSERT INTO node_favorite_folders(node_id,folder_id) VALUES(2,1);INSERT INTO tmdb_movie_bindings(node_id,movie_id,payload_json) VALUES(2,550,'{\"id\":550,\"title\":\"Movie\",\"originalTitle\":\"Movie\",\"overview\":\"Fixture\",\"releaseDate\":null,\"posterPath\":null}');INSERT INTO settings VALUES('personal_api_secret','SECRET_MUST_NEVER_EXPORT');INSERT INTO settings VALUES('tmdb_credential_source','PERSONAL');INSERT INTO settings VALUES('theme','dark');").unwrap();
        let cache = app.join("cache/covers");
        cache::ensure_directories(&cache).unwrap();
        let cover = cache.join("manual/node-2.png");
        image::RgbImage::from_pixel(20, 30, image::Rgb([31, 88, 109]))
            .save(&cover)
            .unwrap();
        c.execute(
            "UPDATE nodes SET cover_source='MANUAL',cover_cache_path=?1 WHERE id=2",
            [cover.to_string_lossy()],
        )
        .unwrap();
        collection::set_status(&db, Target::Node(2), Some(Status::Want)).unwrap();
        collection::set_status(&db, Target::Book(2), Some(Status::Completed)).unwrap();
        db.record_node_watched(2).unwrap();
        collection::record_recent(&c, Target::Book(2)).unwrap();
        c.execute("INSERT INTO episode_watch_states(library_root_id,identity_key,scope_key,identity_kind,source_node_id,source_file_id,source_name,season,episode,watched,revision) VALUES(1,?1,?2,'EPISODE',2,1,'Synthetic S01E01.mkv',1,1,1,3)", rusqlite::params!["a".repeat(64),"b".repeat(64)]).unwrap();
        db.set_root_hidden_from_all(1, true).unwrap();
        let archive = t.path().join("collection.m2backup");
        (t, app, db, archive)
    }
    fn business(c: &Connection) -> Vec<String> {
        let queries = [
            "SELECT display_name||':'||manual_display_name FROM nodes WHERE id=2",
            "SELECT COUNT(*) FROM media_files",
            "SELECT hidden FROM library_all_resources_visibility WHERE library_root_id=1",
            "SELECT last_page_index FROM comic_reading_progress WHERE comic_book_id=2",
            "SELECT COUNT(*) FROM comic_bookmarks",
            "SELECT COUNT(*) FROM node_tags",
            "SELECT COUNT(*) FROM node_favorite_folders",
            "SELECT movie_id FROM tmdb_movie_bindings WHERE node_id=2",
            "SELECT COUNT(*) FROM collection_states",
            "SELECT COUNT(*) FROM recent_activity",
            "SELECT watch_count FROM watch_history WHERE node_id=2",
            "SELECT identity_key||':'||scope_key||':'||source_node_id||':'||source_file_id||':'||source_name||':'||season||':'||episode||':'||watched||':'||revision FROM episode_watch_states LIMIT 1",
        ];
        queries
            .into_iter()
            .map(|q| {
                c.query_row(&format!("SELECT CAST(({q}) AS TEXT)"), [], |r| r.get(0))
                    .unwrap()
            })
            .collect()
    }
    #[test]
    fn backup_roundtrip_preserves_typed_business_manual_cover_and_local_credentials_offline() {
        let (_t, app, db, archive) = fixture();
        db.connect().unwrap().execute_batch("ALTER TABLE mediashelf_schema_migrations RENAME TO historical_versions;CREATE TABLE mediashelf_schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);INSERT INTO mediashelf_schema_migrations SELECT * FROM historical_versions;DROP TABLE historical_versions;").unwrap();
        let expected = business(&db.connect().unwrap());
        let cover: String = db
            .connect()
            .unwrap()
            .query_row("SELECT cover_cache_path FROM nodes WHERE id=2", [], |r| {
                r.get(0)
            })
            .unwrap();
        let cover_bytes = fs::read(&cover).unwrap();
        export(&db, &app, &archive).unwrap();
        let preview = prepare(&app, &archive).unwrap();
        assert_eq!(preview.manual_covers, 1);
        let staged = stage(&app, &preview.token).unwrap();
        let raw = fs::read(staged.join("collection.db")).unwrap();
        assert!(!raw
            .windows(b"SECRET_MUST_NEVER_EXPORT".len())
            .any(|w| w == b"SECRET_MUST_NEVER_EXPORT"));
        assert!(!raw
            .windows(b"tmdb_credential_source".len())
            .any(|w| w == b"tmdb_credential_source"));
        let root = db.library_paths().unwrap()[0].clone();
        fs::rename(&root, root.with_file_name("OfflineMedia")).unwrap();
        db.connect().unwrap().execute_batch("DELETE FROM recent_activity;DELETE FROM collection_states;DELETE FROM comic_bookmarks;DELETE FROM episode_watch_states;UPDATE settings SET value='DEFAULT' WHERE key='tmdb_credential_source';").unwrap();
        discard(&app, &preview.token).unwrap();
        let preview = prepare(&app, &archive).unwrap();
        assert_eq!(preview.offline_roots, 1);
        arm(&app, &preview.token, &db).unwrap();
        assert_eq!(startup(&app).unwrap(), Some("BACKUP_RESTORED"));
        assert_eq!(business(&db.connect().unwrap()), expected);
        let restored: String = db
            .connect()
            .unwrap()
            .query_row("SELECT cover_cache_path FROM nodes WHERE id=2", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(fs::read(restored).unwrap(), cover_bytes);
        assert_eq!(fs::read(cover).unwrap(), cover_bytes);
        assert_eq!(
            db.connect()
                .unwrap()
                .query_row(
                    "SELECT value FROM settings WHERE key='tmdb_credential_source'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "DEFAULT"
        );
        assert_eq!(db.library_paths().unwrap(), vec![root]);
        assert_eq!(startup(&app).unwrap(), None);
        // A restored original remains eligible for the next backup without another poster pipeline.
        export(&db, &app, &archive).unwrap();
    }
    #[test]
    fn backup_write_failure_rolls_back_database_and_preserves_original_covers() {
        let (_t, app, db, archive) = fixture();
        export(&db, &app, &archive).unwrap();
        db.connect()
            .unwrap()
            .execute("UPDATE settings SET value='light' WHERE key='theme'", [])
            .unwrap();
        let before = business(&db.connect().unwrap());
        let preview = prepare(&app, &archive).unwrap();
        arm(&app, &preview.token, &db).unwrap();
        FAIL_AFTER_REPLACE.with(|flag| flag.set(true));
        assert_eq!(startup(&app).unwrap(), Some("BACKUP_ROLLED_BACK"));
        assert_eq!(business(&db.connect().unwrap()), before);
        assert_eq!(
            db.connect()
                .unwrap()
                .query_row("SELECT value FROM settings WHERE key='theme'", [], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap(),
            "light"
        );
    }
    #[test]
    fn backup_interrupted_commit_and_completed_cleanup_are_recoverable() {
        let (_t, app, db, archive) = fixture();
        export(&db, &app, &archive).unwrap();
        let preview = prepare(&app, &archive).unwrap();
        arm(&app, &preview.token, &db).unwrap();
        let dir = stage(&app, &preview.token).unwrap();
        let rollback = dir.join("rollback.db");
        snapshot(db.path(), &rollback).unwrap();
        let (bytes, hash) = file_hash(&rollback, MAX_DB).unwrap();
        let marker = owned_root(&app).unwrap().join("restore.json");
        let mut journal: Journal = serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
        journal.phase = "Committing".into();
        journal.rollback_bytes = Some(bytes);
        journal.rollback_hash = Some(hash);
        atomic_json(&marker, &journal).unwrap();
        db.connect()
            .unwrap()
            .execute("DELETE FROM recent_activity", [])
            .unwrap();
        assert_eq!(startup(&app).unwrap(), Some("BACKUP_ROLLED_BACK"));
        assert_eq!(db.list_recently_watched().unwrap().len(), 2);
        let (token, dir) = new_stage(&app).unwrap();
        journal.token = token;
        journal.phase = "Completed".into();
        (journal.incoming_bytes, journal.incoming_hash) = file_hash(db.path(), MAX_DB).unwrap();
        atomic_json(&marker, &journal).unwrap();
        assert_eq!(startup(&app).unwrap(), Some("BACKUP_RESTORED"));
        assert!(!dir.exists());
        assert!(!marker.exists());
        let (token, dir) = new_stage(&app).unwrap();
        journal.token = token;
        atomic_json(&marker, &journal).unwrap();
        cleanup_stage(&dir).unwrap();
        assert_eq!(startup(&app).unwrap(), Some("BACKUP_RESTORED"));
    }
    fn hostile(path: &Path, names: &[&str]) {
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        for name in names {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"{}").unwrap();
        }
        zip.finish().unwrap();
    }
    #[test]
    fn backup_rejects_bad_archives_paths_schema_and_preserves_active_database() {
        let (t, app, db, archive) = fixture();
        export(&db, &app, &archive).unwrap();
        let before = business(&db.connect().unwrap());
        for names in [
            vec!["manifest.json", "../collection.db"],
            vec!["manifest.json", "C:/collection.db"],
            vec!["manifest.json", "manual\\2.image"],
            vec!["manifest.json", "unknown"],
            vec!["manifest.json", "collection.db"],
        ] {
            let bad = t.path().join("bad.zip");
            hostile(&bad, &names);
            assert!(prepare(&app, &bad).is_err());
        }
        let p = prepare(&app, &archive).unwrap();
        let dir = stage(&app, &p.token).unwrap();
        fs::write(dir.join("collection.db"), b"broken").unwrap();
        assert!(arm(&app, &p.token, &db).is_err());
        discard(&app, &p.token).unwrap();
        let p = prepare(&app, &archive).unwrap();
        let dir = stage(&app, &p.token).unwrap();
        let mut m: Manifest =
            serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
        m.schema_version = SCHEMA + 1;
        fs::write(dir.join("manifest.json"), serde_json::to_vec(&m).unwrap()).unwrap();
        assert!(arm(&app, &p.token, &db).is_err());
        discard(&app, &p.token).unwrap();
        assert_eq!(business(&db.connect().unwrap()), before);
        assert!(export(
            &db,
            &app,
            &db.library_paths().unwrap()[0].join("bad.m2backup")
        )
        .is_err());
        assert!(export(&db, &app, &t.path().join("missing/file.m2backup")).is_err());
    }
    #[test]
    fn backup_rejects_symlinks_entry_overflow_forged_schema_and_hashes() {
        let (t, app, db, archive) = fixture();
        export(&db, &app, &archive).unwrap();
        let expected = business(&db.connect().unwrap());
        let bad = t.path().join("hostile.zip");
        {
            let mut zip = ZipWriter::new(File::create(&bad).unwrap());
            zip.start_file("manifest.json", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"{}").unwrap();
            zip.add_symlink("collection.db", "../outside", SimpleFileOptions::default())
                .unwrap();
            zip.finish().unwrap();
        }
        assert!(prepare(&app, &bad).is_err());
        {
            let mut zip = ZipWriter::new(File::create(&bad).unwrap());
            for i in 0..MAX_COUNT + 3 {
                zip.start_file(format!("{i}"), SimpleFileOptions::default())
                    .unwrap();
            }
            zip.finish().unwrap();
        }
        assert!(prepare(&app, &bad).is_err());
        for forged in [false, true] {
            let p = prepare(&app, &archive).unwrap();
            let dir = stage(&app, &p.token).unwrap();
            let mut manifest: Manifest =
                serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
            if forged {
                let c = Connection::open(dir.join("collection.db")).unwrap();
                c.execute_batch("CREATE TABLE injected_credentials(value TEXT);")
                    .unwrap();
                drop(c);
                let (bytes, hash) = file_hash(&dir.join("collection.db"), MAX_DB).unwrap();
                manifest.entries[0].bytes = bytes;
                manifest.entries[0].sha256 = hash;
            } else {
                manifest.entries[0].sha256 = "0".repeat(64);
            }
            let (mut out, mut source) = (
                ZipWriter::new(File::create(&bad).unwrap()),
                ZipArchive::new(File::open(&archive).unwrap()).unwrap(),
            );
            out.start_file("manifest.json", SimpleFileOptions::default())
                .unwrap();
            out.write_all(&serde_json::to_vec(&manifest).unwrap())
                .unwrap();
            for entry in &manifest.entries {
                out.start_file(&entry.name, SimpleFileOptions::default())
                    .unwrap();
                if entry.name == "collection.db" {
                    out.write_all(&fs::read(dir.join("collection.db")).unwrap())
                        .unwrap();
                } else {
                    std::io::copy(&mut source.by_name(&entry.name).unwrap(), &mut out).unwrap();
                }
            }
            out.finish().unwrap();
            assert!(prepare(&app, &bad).is_err());
            discard(&app, &p.token).unwrap();
        }
        assert!(space(&app, u64::MAX).is_err());
        assert_eq!(business(&db.connect().unwrap()), expected);
    }
    #[test]
    fn collection_mutations_wait_for_background_writes_without_snapshot_upgrade_failures() {
        let (_t, _app, db, _archive) = fixture();
        let writer = db.clone();
        let thread = std::thread::spawn(move || {
            let c = writer.connect().unwrap();
            for i in 0..80 {
                c.execute(
                    "INSERT OR REPLACE INTO settings(key,value) VALUES('background-test',?1)",
                    [i.to_string()],
                )
                .unwrap();
            }
        });
        for _ in 0..80 {
            collection::set_status(&db, Target::Book(2), Some(Status::Paused)).unwrap();
            collection::remove_recent(&db, &[Target::Node(2)]).unwrap();
        }
        thread.join().unwrap();
        assert_eq!(
            db.connect()
                .unwrap()
                .query_row(
                    "SELECT last_page_index FROM comic_reading_progress WHERE comic_book_id=2",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn schema32_to_35_backups_validate_before_staged_upgrade() {
        for old_schema in [32, 33, 34, 35] {
            let (t, app, db, archive) = fixture();
            export(&db, &app, &archive).unwrap();
            let p = prepare(&app, &archive).unwrap();
            let dir = stage(&app, &p.token).unwrap();
            let old = t.path().join("legacy.db");
            snapshot(&dir.join("collection.db"), &old).unwrap();
            let c = Connection::open(&old).unwrap();
            c.execute_batch(&format!("DROP TRIGGER tmdb_movie_scope;DROP TRIGGER tmdb_movie_update_scope;{};{};DELETE FROM mediashelf_schema_migrations WHERE version>=36;",legacy_tmdb_guard("tmdb_movie_scope").unwrap(),legacy_tmdb_guard("tmdb_movie_update_scope").unwrap())).unwrap();
            if old_schema < 35 {
                c.execute_batch("DROP TABLE library_all_resources_visibility;DELETE FROM mediashelf_schema_migrations WHERE version>=35;").unwrap();
            }
            if old_schema < 34 {
                c.execute_batch("DROP TABLE episode_watch_states;DELETE FROM mediashelf_schema_migrations WHERE version>=34;").unwrap();
            }
            if old_schema == 32 {
                c.execute_batch("DROP TABLE private_notes;DROP TABLE book_manual_covers;DELETE FROM mediashelf_schema_migrations WHERE version>=33;").unwrap();
            }
            c.execute_batch("VACUUM;").unwrap();
            drop(c);
            let mut manifest: Manifest =
                serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
            manifest.schema_version = old_schema;
            let entry = manifest.entries.iter_mut().find(|e| !e.is_cover()).unwrap();
            (entry.bytes, entry.sha256) = file_hash(&old, MAX_DB).unwrap();
            let legacy = t.path().join("legacy.m2backup");
            let mut z = ZipWriter::new(File::create(&legacy).unwrap());
            z.start_file("manifest.json", SimpleFileOptions::default())
                .unwrap();
            z.write_all(&serde_json::to_vec(&manifest).unwrap())
                .unwrap();
            for e in &manifest.entries {
                let entry_path = if e.is_cover() {
                    dir.join(&e.name)
                } else {
                    old.clone()
                };
                archive_entry(&mut z, &entry_path, e).unwrap();
            }
            z.finish().unwrap();
            discard(&app, &p.token).unwrap();
            let p = prepare(&app, &legacy).unwrap();
            arm(&app, &p.token, &db).unwrap();
            assert_eq!(startup(&app).unwrap(), Some("BACKUP_RESTORED"));
            assert_eq!(
                db.connect()
                    .unwrap()
                    .query_row(
                        "SELECT max(version) FROM mediashelf_schema_migrations",
                        [],
                        |r| r.get::<_, i64>(0)
                    )
                    .unwrap(),
                36
            );
            assert_eq!(
                db.connect()
                    .unwrap()
                    .query_row("SELECT COUNT(*) FROM episode_watch_states", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                if old_schema >= 34 { 1 } else { 0 }
            );
            assert_eq!(db.get_root(1).unwrap().hidden_from_all, old_schema >= 35);
        }
    }
}

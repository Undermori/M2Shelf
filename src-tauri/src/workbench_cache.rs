//! On-demand, application-owned thumbnails. No source enumeration or startup work.
//! Cache failures are optional: callers keep their checked source-read fallback.
use crate::db::{AppResult, Database};
use image::ImageEncoder;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

const MAX_FILES: usize = 4000;
const MAX_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ITEM: u64 = 4 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"M2WB001\n";
const HEADER: usize = 72;

#[derive(Default)]
struct Index {
    files: HashMap<String, (u64, u64)>,
    bytes: u64,
    clock: u64,
}
static INDEXES: OnceLock<Mutex<HashMap<PathBuf, Index>>> = OnceLock::new();
#[cfg(test)]
thread_local! { static ENCODE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
pub(crate) fn encode_count() -> usize {
    ENCODE_COUNT.get()
}

pub(crate) struct Cache {
    db: Database,
    root: PathBuf,
    // On Windows these deny directory deletion/rename for the whole IO operation.
    _directories: Vec<File>,
    max_files: usize,
    max_bytes: u64,
}

fn validate_scope(c: &rusqlite::Connection, root: &Path) -> AppResult<()> {
    let mut statement = c.prepare("SELECT path FROM library_roots UNION ALL SELECT path FROM screenshot_folders UNION ALL SELECT path FROM workspace_folders").map_err(|_| "WORKBENCH_CACHE_PATH")?;
    let paths = statement
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|_| "WORKBENCH_CACHE_PATH")?;
    for path in paths {
        let path = path.map_err(|_| "WORKBENCH_CACHE_PATH")?;
        if crate::cache::paths_overlap_checked(root, Path::new(&path))? {
            return Err("WORKBENCH_CACHE_PATH".into());
        }
    }
    Ok(())
}

fn plain(path: &Path, directory: bool) -> AppResult<fs::Metadata> {
    let m = fs::symlink_metadata(path).map_err(|_| "WORKBENCH_CACHE_PATH")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("WORKBENCH_CACHE_PATH".into());
        }
    }
    if m.file_type().is_symlink() || if directory { !m.is_dir() } else { !m.is_file() } {
        return Err("WORKBENCH_CACHE_PATH".into());
    }
    Ok(m)
}

fn pin_directory(path: &Path) -> AppResult<File> {
    plain(path, true)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };
        options
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }
    let file = options.open(path).map_err(|_| "WORKBENCH_CACHE_PATH")?;
    let metadata = file.metadata().map_err(|_| "WORKBENCH_CACHE_PATH")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("WORKBENCH_CACHE_PATH".into());
        }
    }
    if !metadata.is_dir() {
        return Err("WORKBENCH_CACHE_PATH".into());
    }
    plain(path, true)?;
    Ok(file)
}

fn recognized(name: &str) -> bool {
    name.len() == 74
        && name.starts_with("wb1-")
        && name.ends_with(".thumb")
        && name[4..68]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

pub(crate) fn key(kind: &str, identity: &str, revision: &str) -> String {
    let mut digest = Sha256::new();
    for value in ["triangle-768-j90-pngfast-v1", kind, identity, revision] {
        digest.update((value.len() as u64).to_le_bytes());
        digest.update(value.as_bytes());
    }
    format!("wb1-{:x}.thumb", digest.finalize())
}

impl Cache {
    pub(crate) fn open(db: &Database) -> AppResult<Self> {
        let app = db.path().parent().ok_or("WORKBENCH_CACHE_PATH")?;
        if !app.is_absolute()
            || app
                .components()
                .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
        {
            return Err("WORKBENCH_CACHE_PATH".into());
        }
        // Pin each ancestor before touching its descendants. Never follow a junction.
        let mut directories = Vec::new();
        for path in app.ancestors().collect::<Vec<_>>().into_iter().rev() {
            directories.push(pin_directory(path)?);
        }
        let app = fs::canonicalize(app).map_err(|_| "WORKBENCH_CACHE_PATH")?;
        let root = app.join("cache").join("workbench");
        db.read_snapshot(|c| validate_scope(c, &root))?;
        for path in [app.join("cache"), root.clone()] {
            match fs::symlink_metadata(&path) {
                Ok(_) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // A concurrent source registration cannot turn a pending directory write
                    // into a write inside a newly registered source.
                    let mut connection = db.connect()?;
                    let tx = connection
                        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                        .map_err(|_| "WORKBENCH_CACHE_IO")?;
                    validate_scope(&tx, &root)?;
                    fs::create_dir(&path).map_err(|_| "WORKBENCH_CACHE_PATH")?;
                }
                Err(_) => return Err("WORKBENCH_CACHE_PATH".into()),
            }
            directories.push(pin_directory(&path)?);
            if !fs::canonicalize(&path)
                .map_err(|_| "WORKBENCH_CACHE_PATH")?
                .starts_with(&app)
            {
                return Err("WORKBENCH_CACHE_PATH".into());
            }
        }
        Ok(Self {
            db: db.clone(),
            root,
            _directories: directories,
            max_files: MAX_FILES,
            max_bytes: MAX_BYTES,
        })
    }

    fn index(&self) -> AppResult<std::sync::MutexGuard<'static, HashMap<PathBuf, Index>>> {
        let mut all = INDEXES
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .map_err(|_| "WORKBENCH_CACHE_IO")?;
        if !all.contains_key(&self.root) {
            // Test/portable database switches must not retain unbounded directory indexes.
            if all.len() >= 8 {
                all.clear();
            }
            let mut index = Index::default();
            let mut entries = Vec::new();
            for (count, entry) in fs::read_dir(&self.root)
                .map_err(|_| "WORKBENCH_CACHE_IO")?
                .enumerate()
            {
                if count >= 8000 {
                    return Err("WORKBENCH_CACHE_LIMIT".into());
                }
                let entry = entry.map_err(|_| "WORKBENCH_CACHE_IO")?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if !recognized(&name) {
                    continue;
                }
                if let Ok(m) = plain(&entry.path(), false) {
                    entries.push((
                        m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        name,
                        m.len(),
                    ));
                }
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            for (_, name, size) in entries {
                index.clock += 1;
                index.bytes += size;
                index.files.insert(name, (size, index.clock));
            }
            all.insert(self.root.clone(), index);
        }
        Ok(all)
    }

    fn checked_bytes(&self, name: &str, digest: bool) -> AppResult<Vec<u8>> {
        if !recognized(name) {
            return Err("WORKBENCH_CACHE_PATH".into());
        }
        plain(&self.root, true)?;
        let path = self.root.join(name);
        let m = plain(&path, false)?;
        if m.len() < HEADER as u64 || m.len() > MAX_ITEM {
            return Err("WORKBENCH_CACHE_LIMIT".into());
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            };
            options
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .share_mode(FILE_SHARE_READ);
        }
        let mut f = options.open(&path).map_err(|_| "WORKBENCH_CACHE_IO")?;
        let stamp = crate::comics::modified(&m);
        crate::comic_reader::validate_file_handle(&f, &path, &self.root, m.len(), &stamp)?;
        let mut bytes = Vec::with_capacity(m.len() as usize);
        Read::by_ref(&mut f)
            .take(MAX_ITEM + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "WORKBENCH_CACHE_IO")?;
        crate::comic_reader::validate_file_handle(&f, &path, &self.root, m.len(), &stamp)?;
        if bytes.len() as u64 != m.len()
            || &bytes[..8] != MAGIC
            || Sha256::digest(name.as_bytes()).as_slice() != &bytes[8..40]
            || (digest && Sha256::digest(&bytes[HEADER..]).as_slice() != &bytes[40..72])
        {
            return Err("WORKBENCH_CACHE_INVALID".into());
        }
        Ok(bytes)
    }

    pub(crate) fn get(&self, name: &str) -> Option<Vec<u8>> {
        let bytes = self.checked_bytes(name, true).ok()?;
        valid_image(&bytes[HEADER..]).ok()?;
        let mut all = self.index().ok()?;
        let index = all.get_mut(&self.root)?;
        index.clock += 1;
        if let Some(entry) = index.files.get_mut(name) {
            entry.1 = index.clock;
        }
        Some(bytes[HEADER..].to_vec())
    }

    fn trim(&self, index: &mut Index, incoming: u64) -> AppResult<()> {
        while index.files.len() >= self.max_files
            || index.bytes.saturating_add(incoming) > self.max_bytes
        {
            let name = index
                .files
                .iter()
                .min_by_key(|(_, value)| value.1)
                .map(|(name, _)| name.clone())
                .ok_or("WORKBENCH_CACHE_LIMIT")?;
            // A cache-looking filename alone never authorizes deletion of unrelated data.
            self.checked_bytes(&name, false)?;
            fs::remove_file(self.root.join(&name)).map_err(|_| "WORKBENCH_CACHE_IO")?;
            if let Some((size, _)) = index.files.remove(&name) {
                index.bytes = index.bytes.saturating_sub(size);
            }
        }
        Ok(())
    }

    pub(crate) fn put(&self, name: &str, payload: &[u8]) -> AppResult<()> {
        if !recognized(name) {
            return Err("WORKBENCH_CACHE_PATH".into());
        }
        valid_image(payload)?;
        let size = payload.len() as u64 + HEADER as u64;
        if size > MAX_ITEM || size > self.max_bytes {
            return Err("WORKBENCH_CACHE_LIMIT".into());
        }
        // Only short cache writes/evictions reserve the writer; decoding and cache hits don't.
        let mut connection = self.db.connect()?;
        let tx = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| "WORKBENCH_CACHE_IO")?;
        validate_scope(&tx, &self.root)?;
        let mut all = self.index()?;
        let index = all.get_mut(&self.root).ok_or("WORKBENCH_CACHE_IO")?;
        let destination = self.root.join(name);
        if fs::symlink_metadata(&destination).is_ok() {
            // Only a matching application header permits repair; arbitrary files are preserved.
            self.checked_bytes(name, false)?;
            if self.checked_bytes(name, true).is_ok() {
                return Ok(());
            }
            fs::remove_file(&destination).map_err(|_| "WORKBENCH_CACHE_IO")?;
            if let Some((old_size, _)) = index.files.remove(name) {
                index.bytes = index.bytes.saturating_sub(old_size);
            }
        }
        self.trim(index, size)?;
        plain(&self.root, true)?;
        let (mut pending, mut file) = crate::cache::create_pending_cache_file(&destination, true)
            .map_err(|_| "WORKBENCH_CACHE_IO")?;
        file.write_all(MAGIC)
            .and_then(|()| file.write_all(&Sha256::digest(name.as_bytes())))
            .and_then(|()| file.write_all(&Sha256::digest(payload)))
            .and_then(|()| file.write_all(payload))
            .map_err(|_| "WORKBENCH_CACHE_IO")?;
        file.sync_all().map_err(|_| "WORKBENCH_CACHE_IO")?;
        let m = file.metadata().map_err(|_| "WORKBENCH_CACHE_IO")?;
        crate::comic_reader::validate_file_handle(
            &file,
            pending.path(),
            &self.root,
            size,
            &crate::comics::modified(&m),
        )?;
        drop(file);
        // Ancestors stay pinned through atomic commit; no source path is used as a target.
        pending
            .commit_to(&destination)
            .map_err(|_| "WORKBENCH_CACHE_IO")?;
        index.clock += 1;
        index.bytes += size;
        index.files.insert(name.into(), (size, index.clock));
        Ok(())
    }
}

fn valid_image(bytes: &[u8]) -> AppResult<()> {
    let (mime, w, h) = crate::screenshot_gallery::dimensions(bytes)?;
    if !matches!(mime, "image/jpeg" | "image/png") || w > 768 || h > 768 {
        return Err("WORKBENCH_CACHE_INVALID".into());
    }
    Ok(())
}

pub(crate) fn encode(decoded: image::DynamicImage) -> AppResult<Vec<u8>> {
    #[cfg(test)]
    ENCODE_COUNT.set(ENCODE_COUNT.get() + 1);
    let image = if decoded.width() > 768 || decoded.height() > 768 {
        decoded.resize(768, 768, image::imageops::FilterType::Triangle)
    } else {
        decoded
    };
    let rgba = image.to_rgba8();
    let mut bytes = Vec::new();
    if rgba.pixels().any(|p| p.0[3] != 255) {
        image::codecs::png::PngEncoder::new_with_quality(
            &mut bytes,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::Adaptive,
        )
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|_| "WORKBENCH_CACHE_FORMAT")?;
    } else {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 90)
            .encode_image(&image::DynamicImage::ImageRgb8(image.to_rgb8()))
            .map_err(|_| "WORKBENCH_CACHE_FORMAT")?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Database) {
        let parent = std::env::current_dir()
            .unwrap()
            .join("target/workbench-cache-tests");
        fs::create_dir_all(&parent).unwrap();
        let temp = tempfile::Builder::new()
            .prefix("synthetic-")
            .tempdir_in(parent)
            .unwrap();
        let app = temp.path().join("app");
        fs::create_dir(&app).unwrap();
        let db = Database::new(app.join("collection.db"));
        db.migrate().unwrap();
        (temp, db)
    }
    fn jpeg() -> Vec<u8> {
        encode(image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            24,
            16,
            image::Rgb([30, 80, 120]),
        )))
        .unwrap()
    }

    #[test]
    fn round11_cache_is_lazy_hits_and_revision_keys_do_not_collide() {
        let (_temp, db) = fixture();
        let root = db.path().parent().unwrap().join("cache/workbench");
        assert!(!root.exists());
        let cache = Cache::open(&db).unwrap();
        let first = key("screenshot", "1:synthetic", "revision-a");
        let changed = key("screenshot", "1:synthetic", "revision-b");
        let other = key("workspace", "1:synthetic", "revision-a");
        let payload = jpeg();
        assert_eq!(cache.get(&first), None);
        cache.put(&first, &payload).unwrap();
        assert_eq!(cache.get(&first), Some(payload.clone()));
        assert_eq!(cache.get(&changed), None);
        assert_eq!(cache.get(&other), None);
        drop(cache);
        assert_eq!(Cache::open(&db).unwrap().get(&first), Some(payload));
    }

    #[test]
    fn round11_cache_count_bytes_lru_and_unrelated_files_are_bounded() {
        let (_temp, db) = fixture();
        let mut cache = Cache::open(&db).unwrap();
        let payload = jpeg();
        cache.max_files = 2;
        cache.max_bytes = 2 * (payload.len() + HEADER) as u64;
        let keys = (0..4)
            .map(|n| key("workspace", &n.to_string(), "r1"))
            .collect::<Vec<_>>();
        fs::write(cache.root.join("keep.txt"), b"unrelated").unwrap();
        cache.put(&keys[0], &payload).unwrap();
        cache.put(&keys[1], &payload).unwrap();
        assert!(cache.get(&keys[0]).is_some());
        cache.put(&keys[2], &payload).unwrap();
        assert!(cache.get(&keys[1]).is_none());
        assert!(cache.get(&keys[0]).is_some());
        cache.max_files = 100;
        cache.put(&keys[3], &payload).unwrap();
        let records = fs::read_dir(&cache.root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| recognized(&e.file_name().to_string_lossy()))
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 2);
        assert!(
            records
                .iter()
                .map(|e| e.metadata().unwrap().len())
                .sum::<u64>()
                <= cache.max_bytes
        );
        assert_eq!(fs::read(cache.root.join("keep.txt")).unwrap(), b"unrelated");
        assert!(cache.get("../outside.thumb").is_none());
        assert!(cache.put("../outside.thumb", &payload).is_err());
    }

    #[test]
    fn round11_cache_rejects_all_source_root_kinds_before_creating_directories() {
        for table in ["library_roots", "screenshot_folders", "workspace_folders"] {
            let (_temp, db) = fixture();
            let app = db.path().parent().unwrap();
            let sql = if table == "library_roots" {
                "INSERT INTO library_roots(path,display_name) VALUES(?1,'synthetic')".into()
            } else {
                format!("INSERT INTO {table}(path,alias) VALUES(?1,'synthetic')")
            };
            db.connect()
                .unwrap()
                .execute(&sql, [app.to_string_lossy().as_ref()])
                .unwrap();
            assert!(Cache::open(&db).is_err(), "{table}");
            assert!(!app.join("cache").exists(), "{table}");
        }
    }

    #[test]
    fn round11_cache_corruption_falls_back_and_only_owned_derivatives_can_be_repaired() {
        let (_temp, db) = fixture();
        let cache = Cache::open(&db).unwrap();
        let name = key("workspace", "image", "r1");
        let payload = jpeg();
        cache.put(&name, &payload).unwrap();
        let path = cache.root.join(&name);
        let mut stored = fs::read(&path).unwrap();
        stored[HEADER + 30] ^= 1;
        fs::write(&path, &stored).unwrap();
        assert!(cache.get(&name).is_none());
        cache.put(&name, &payload).unwrap();
        assert_eq!(cache.get(&name), Some(payload.clone()));
        fs::write(&path, b"unrelated document with a cache-looking name").unwrap();
        assert!(cache.put(&name, &payload).is_err());
        assert_eq!(
            fs::read(path).unwrap(),
            b"unrelated document with a cache-looking name"
        );
    }

    #[test]
    fn round11_triangle_encodes_opaque_as_jpeg_and_retains_alpha_in_fast_png() {
        let rgb = image::RgbImage::from_pixel(1600, 900, image::Rgb([30, 80, 120]));
        let opaque = encode(image::DynamicImage::ImageRgb8(rgb)).unwrap();
        assert_eq!(
            crate::screenshot_gallery::dimensions(&opaque).unwrap(),
            ("image/jpeg", 768, 432)
        );
        let rgba = image::RgbaImage::from_pixel(24, 16, image::Rgba([30, 80, 120, 17]));
        let alpha = encode(image::DynamicImage::ImageRgba8(rgba)).unwrap();
        assert_eq!(
            crate::screenshot_gallery::dimensions(&alpha).unwrap(),
            ("image/png", 24, 16)
        );
        let decoded = image::load_from_memory(&alpha).unwrap().to_rgba8();
        assert_eq!(decoded.get_pixel(0, 0).0, [30, 80, 120, 17]);
    }

    #[cfg(windows)]
    #[test]
    fn round11_junction_cache_and_ancestor_are_rejected_without_outside_writes() {
        let (temp, db) = fixture();
        let app = db.path().parent().unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let link = app.join("cache");
        let status = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(crate::db::display_path(&link).replace('/', "\\"))
            .arg(crate::db::display_path(&outside).replace('/', "\\"))
            .output()
            .unwrap();
        assert!(status.status.success(), "{status:?}");
        assert!(Cache::open(&db).is_err());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        fs::remove_dir(&link).unwrap();
        // A cache root nested below a reparse ancestor is equally forbidden.
        let app_link = temp.path().join("app-link");
        let status = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(crate::db::display_path(&app_link).replace('/', "\\"))
            .arg(crate::db::display_path(app).replace('/', "\\"))
            .output()
            .unwrap();
        assert!(status.status.success(), "{status:?}");
        assert!(Cache::open(&Database::new(app_link.join("collection.db"))).is_err());
        fs::remove_dir(&app_link).unwrap();
    }
}

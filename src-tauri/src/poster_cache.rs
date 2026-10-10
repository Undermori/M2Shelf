//! Persistent, replaceable poster derivatives. Originals and library media stay untouched.
use std::{
    collections::VecDeque,
    fs::{self, File, FileTimes, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    cache, comic_reader,
    db::{AppResult, Database},
    models::{PosterCachePhase, PosterCacheStatus},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{
    codecs::{jpeg::JpegEncoder, webp::WebPEncoder},
    imageops::FilterType,
    ImageDecoder, ImageEncoder, ImageReader,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const WIDTHS: [u32; 4] = [256, 384, 512, 768];
const VERSION: &str = "v2";
const MAX_THUMB_BYTES: u64 = 4 * 1024 * 1024;
const MAX_DISK_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DISK_FILES: usize = 4096;
// One native decoder/resizer, independent of the four lightweight cover transports.
static GENERATION: Mutex<()> = Mutex::new(());
static DISK_MUTATIONS: AtomicU64 = AtomicU64::new(0);

enum Source {
    Cached(PathBuf),
    Page(i64, String),
}

fn source(database: &Database, node_id: i64) -> AppResult<Option<(Source, String)>> {
    let (path, roots) = database.cover_read_context(node_id)?;
    if let Some(path) = path {
        for root in roots {
            if cache::is_equal_or_within_checked(&path, &root)? {
                return Err("拒绝从媒体资源库读取缓存封面。".into());
            }
        }
        let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() > 15 * 1024 * 1024 {
            return Err("封面缓存文件无效。".into());
        }
        let canonical = path.canonicalize().map_err(|e| e.to_string())?;
        let stamp = metadata
            .modified()
            .map_err(|e| e.to_string())?
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let key = format!(
            "{VERSION}:{}:{}:{stamp}",
            canonical.to_string_lossy(),
            metadata.len()
        );
        return Ok(Some((Source::Cached(path), digest(key.as_bytes()))));
    }
    Ok(
        comic_reader::first_image_cover_source(database, node_id)?.map(|(id, revision, name)| {
            let key = digest(
                format!(
                    "{VERSION}:{}:{id}:{revision}:{name}",
                    database.path().to_string_lossy()
                )
                .as_bytes(),
            );
            (Source::Page(id, revision), key)
        }),
    )
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl Source {
    fn read(&self, database: &Database) -> AppResult<Option<Vec<u8>>> {
        match self {
            Self::Cached(path) => cache::read_cover_payload(path).map(Some),
            Self::Page(id, revision) => comic_reader::read_book_cover(database, *id, revision),
        }
    }
    fn data_url(&self, bytes: &[u8]) -> String {
        let mime = image::guess_format(bytes)
            .map(|format| format.to_mime_type())
            .unwrap_or("image/jpeg");
        format!("data:{mime};base64,{}", STANDARD.encode(bytes))
    }
}

fn plain_directory(path: &Path) -> AppResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("缩略图目录不能是重解析路径。".into());
        }
    }
    if !metadata.file_type().is_dir() {
        return Err("缩略图目录不是普通目录。".into());
    }
    Ok(())
}

fn directory(database: &Database, cache_root: &Path) -> AppResult<PathBuf> {
    cache::validate_cache_location(cache_root, database.library_paths()?)?;
    plain_directory(cache_root)?;
    cache::ensure_existing_custom_cache(cache_root)?;
    let directory = cache_root.join("posters");
    plain_directory(&directory)?;
    if !cache::is_equal_or_within_checked(&directory, cache_root)? {
        return Err("缩略图目录越界。".into());
    }
    Ok(directory)
}

pub(crate) fn is_owned_name(name: &str) -> bool {
    if let Some(stem) = name.strip_prefix('.').and_then(|n| n.strip_suffix(".tmp")) {
        return stem.rsplit_once('-').is_some_and(|(stem, id)| {
            id.len() == 32
                && id.bytes().all(|b| b.is_ascii_hexdigit())
                && is_owned_name(&format!("{stem}.m2thumb"))
        });
    }
    let Some(stem) = name
        .strip_prefix("v1-")
        .or_else(|| name.strip_prefix("v2-"))
        .and_then(|n| n.strip_suffix(".m2thumb"))
    else {
        return false;
    };
    let Some((hash, width)) = stem.rsplit_once('-') else {
        return false;
    };
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && width
            .parse::<u32>()
            .ok()
            .is_some_and(|w| WIDTHS.contains(&w))
}

fn path_for(directory: &Path, key: &str, width: u32) -> PathBuf {
    directory.join(format!("{VERSION}-{key}-{width}.m2thumb"))
}

fn read_thumbnail(path: &Path, width: u32) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return None;
        }
    }
    if !metadata.file_type().is_file() || metadata.len() > MAX_THUMB_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(MAX_THUMB_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_THUMB_BYTES || bytes.len() < 40 || &bytes[..8] != b"M2THUMB1" {
        return None;
    }
    if Sha256::digest(&bytes[40..]).as_slice() != &bytes[8..40] {
        return None;
    }
    bytes.drain(..40);
    let (w, h) = cache::cover_payload_dimensions(&bytes).ok()?;
    if w > width || h > width * 4 {
        return None;
    }
    Some(bytes)
}

fn thumbnail_present(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    metadata.file_type().is_file() && metadata.len() >= 40 && metadata.len() <= MAX_THUMB_BYTES
}

fn touch_recent(path: &Path) {
    // Frontend LRU hits do not cross IPC. Touch only these disk reads, at most once per minute.
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata
        .modified()
        .ok()
        .and_then(|time| time.elapsed().ok())
        .is_some_and(|age| age < Duration::from_secs(60))
    {
        return;
    }
    if let Ok(file) = OpenOptions::new().write(true).open(path) {
        let _ = file.set_times(FileTimes::new().set_modified(SystemTime::now()));
    }
}

fn generate(
    operation: &cache::CoverCacheOperationGuard,
    directory: &Path,
    key: &str,
    bytes: &[u8],
    widths: &[u32],
) -> AppResult<()> {
    generate_limited(operation, directory, key, bytes, widths, None).map(|_| ())
}

struct DiskBudget {
    bytes: u64,
    files: usize,
    revision: u64,
}
impl DiskBudget {
    fn read(directory: &Path) -> AppResult<Self> {
        let files = owned_files(directory)?;
        Ok(Self {
            bytes: files.iter().map(|file| file.1).sum(),
            files: files.len(),
            revision: DISK_MUTATIONS.load(Ordering::Relaxed),
        })
    }
}

// Upgrade only our obsolete derivatives; never enumerate or remove original-cover directories.
fn retire_obsolete(operation: &cache::CoverCacheOperationGuard, directory: &Path) -> AppResult<()> {
    let root = directory.parent().ok_or("缩略图缓存目录无效。")?;
    for (path, _, _) in owned_files(directory)? {
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("v1-"))
        {
            cache::remove_cached_file(operation, &path, root)?;
            DISK_MUTATIONS.fetch_add(1, Ordering::Relaxed);
        }
    }
    Ok(())
}

fn encode_thumbnail(
    resized: &image::DynamicImage,
    profile: Option<&Vec<u8>>,
) -> AppResult<Vec<u8>> {
    let rgba = resized.to_rgba8();
    let mut encoded = Vec::new();
    if rgba.pixels().any(|pixel| pixel[3] != 255) {
        let mut encoder = WebPEncoder::new_lossless(&mut encoded);
        if let Some(profile) = profile {
            encoder
                .set_icc_profile(profile.clone())
                .map_err(|e| e.to_string())?;
        }
        encoder
            .encode(
                &rgba,
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| e.to_string())?;
    } else {
        // OpenComic's mature thumbnail tradeoff: quality 95 JPEG, with no source recompression.
        let mut encoder = JpegEncoder::new_with_quality(&mut encoded, 95);
        if let Some(profile) = profile {
            encoder
                .set_icc_profile(profile.clone())
                .map_err(|e| e.to_string())?;
        }
        encoder
            .encode_image(&resized.to_rgb8())
            .map_err(|e| e.to_string())?;
    }
    Ok(encoded)
}

// Background preparation never evicts useful thumbnails just to fill another tier. Foreground
// requests retain the normal bounded LRU policy and can make room for the size actually needed.
fn generate_limited(
    operation: &cache::CoverCacheOperationGuard,
    directory: &Path,
    key: &str,
    bytes: &[u8],
    widths: &[u32],
    mut budget: Option<&mut DiskBudget>,
) -> AppResult<bool> {
    cache::validate_cover_payload(bytes)?;
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(192 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let profile = decoder.icc_profile().map_err(|e| e.to_string())?;
    let mut original = image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    original.apply_orientation(orientation);
    for &width in widths {
        let destination = path_for(directory, key, width);
        if read_thumbnail(&destination, width).is_some() {
            continue;
        }
        // Preserve aspect/alpha and never enlarge a small original. The UI displays this prepared
        // image directly; there is no foreground Canvas resampling or preview replacement.
        let resized = if original.width() <= width && original.height() <= width * 4 {
            original.clone()
        } else {
            original.resize(width, width * 4, FilterType::Lanczos3)
        };
        let encoded = encode_thumbnail(&resized, profile.as_ref())?;
        if encoded.len() as u64 + 40 > MAX_THUMB_BYTES {
            return Err("缩略图超过安全限额。".into());
        }
        if let Some(budget) = budget.as_ref() {
            if budget.files >= MAX_DISK_FILES
                || budget.bytes.saturating_add(encoded.len() as u64 + 40) > MAX_DISK_BYTES
            {
                return Ok(false);
            }
        }
        plain_directory(directory)?;
        let (mut pending, mut file) =
            cache::create_pending_cache_file(&destination, false).map_err(|e| e.to_string())?;
        file.write_all(b"M2THUMB1")
            .and_then(|_| file.write_all(&Sha256::digest(&encoded)))
            .and_then(|_| file.write_all(&encoded))
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        pending.commit_to(&destination).map_err(|e| e.to_string())?;
        let revision = DISK_MUTATIONS.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(budget) = budget.as_mut() {
            budget.bytes += encoded.len() as u64 + 40;
            budget.files += 1;
            budget.revision = revision;
        }
    }
    // Bound disk derivatives after each atomic batch; originals are never pruning candidates.
    if budget.is_none() {
        prune(operation, directory, MAX_DISK_BYTES, MAX_DISK_FILES)?;
    }
    Ok(true)
}

fn prune(
    operation: &cache::CoverCacheOperationGuard,
    directory: &Path,
    max_bytes: u64,
    max_files: usize,
) -> AppResult<()> {
    let mut files = owned_files(directory)?;
    let mut bytes: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.2);
    let mut remaining = files.len();
    for (path, size, _) in files {
        if bytes <= max_bytes && remaining <= max_files {
            break;
        }
        let cache_root = directory.parent().ok_or("缩略图缓存目录无效。")?;
        cache::remove_cached_file(operation, &path, cache_root)?;
        DISK_MUTATIONS.fetch_add(1, Ordering::Relaxed);
        bytes -= size;
        remaining -= 1;
    }
    Ok(())
}

fn owned_files(directory: &Path) -> AppResult<Vec<(PathBuf, u64, SystemTime)>> {
    plain_directory(directory)?;
    let mut files = Vec::new();
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !is_owned_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                continue;
            }
        }
        if metadata.file_type().is_file() {
            files.push((
                entry.path(),
                metadata.len(),
                metadata.modified().unwrap_or(UNIX_EPOCH),
            ));
        }
    }
    Ok(files)
}

/// Fast hits read only a small disk derivative. A missing/corrupt derivative rebuilds once under
/// the shared generation gate; failures retain the validated original preview.
pub(crate) fn cover_data_url(
    database: &Database,
    cache_root: &Path,
    node_id: i64,
    width: u32,
) -> AppResult<Option<String>> {
    thumbnail_data_url(database, cache_root, width, || source(database, node_id))
}

/// A logical card without an exclusive Node still uses the original indexed book identity,
/// the same persistent derivatives and reader validation. Never manufacture a Node or path.
pub(crate) fn book_cover_data_url(
    database: &Database,
    cache_root: &Path,
    book_id: i64,
    revision: &str,
    width: u32,
) -> AppResult<Option<String>> {
    thumbnail_data_url(database, cache_root, width, || {
        let opened = comic_reader::cover_identity(database, book_id, revision)?;
        Ok(opened.map(|name| {
            (
                Source::Page(book_id, revision.into()),
                digest(
                    format!(
                        "{VERSION}:{}:{book_id}:{revision}:{name}",
                        database.path().to_string_lossy()
                    )
                    .as_bytes(),
                ),
            )
        }))
    })
}

fn thumbnail_data_url(
    database: &Database,
    cache_root: &Path,
    width: u32,
    resolve: impl Fn() -> AppResult<Option<(Source, String)>>,
) -> AppResult<Option<String>> {
    if !WIDTHS.contains(&width) {
        return Err("缩略图尺寸无效。".into());
    }
    let _operation = cache::begin_cover_cache_operation();
    let Some((source, key)) = resolve()? else {
        return Ok(None);
    };
    let directory = directory(database, cache_root)?;
    let destination = path_for(&directory, &key, width);
    let mut thumbnail = read_thumbnail(&destination, width);
    if thumbnail.is_none() {
        let _generation = GENERATION.lock().unwrap_or_else(|p| p.into_inner());
        thumbnail = read_thumbnail(&destination, width);
        if thumbnail.is_none() {
            let Some(bytes) = source.read(database)? else {
                return Ok(None);
            };
            if resolve()?.map(|s| s.1) != Some(key.clone()) {
                return Err("封面已变化，请重新加载。".into());
            }
            if generate(&_operation, &directory, &key, &bytes, &[width]).is_err() {
                return Ok(Some(source.data_url(&bytes)));
            }
            thumbnail = read_thumbnail(&destination, width);
        }
    }
    if thumbnail.is_some() {
        touch_recent(&destination);
    }
    Ok(thumbnail.map(|bytes| source.data_url(&bytes)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Checkpoint {
    #[serde(default)]
    version: String,
    cache_root: PathBuf,
    cursor: i64,
    failed_only: bool,
    status: PosterCacheStatus,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WarmupKind {
    Changed,
    Resume,
    Retry,
}

#[derive(Clone)]
struct Warmup {
    database: Database,
    cache_root: PathBuf,
    cancelled: Arc<AtomicBool>,
    kind: WarmupKind,
    progress: Arc<Mutex<Checkpoint>>,
}

impl Warmup {
    fn new(database: &Database, cache_root: &Path, kind: WarmupKind) -> Self {
        Self {
            database: database.clone(),
            cache_root: cache_root.into(),
            cancelled: Arc::new(AtomicBool::new(false)),
            kind,
            progress: Arc::new(Mutex::new(Checkpoint {
                version: VERSION.into(),
                cache_root: cache_root.into(),
                cursor: 0,
                failed_only: false,
                status: PosterCacheStatus {
                    phase: PosterCachePhase::Queued,
                    ..Default::default()
                },
            })),
        }
    }
    fn checkpoint(&self) -> Checkpoint {
        self.progress
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
    fn persist(
        &self,
        next: &Checkpoint,
        outcome: Option<(i64, Option<(&str, &str)>)>,
        reset: bool,
    ) -> AppResult<()> {
        self.database.save_poster_checkpoint(
            &serde_json::to_string(next).map_err(|e| e.to_string())?,
            outcome,
            reset,
        )?;
        *self.progress.lock().unwrap_or_else(|p| p.into_inner()) = next.clone();
        Ok(())
    }
}

struct Warmups {
    running: bool,
    active: Option<Warmup>,
    queue: VecDeque<Warmup>,
    last: Option<Warmup>,
}
static WARMUPS: Mutex<Warmups> = Mutex::new(Warmups {
    running: false,
    active: None,
    queue: VecDeque::new(),
    last: None,
});

pub(crate) fn status(database: &Database) -> PosterCacheStatus {
    let state = WARMUPS.lock().unwrap_or_else(|p| p.into_inner());
    state
        .active
        .iter()
        .filter(|job| !job.cancelled.load(Ordering::Relaxed))
        .chain(state.queue.iter())
        .chain(state.last.iter())
        .find(|job| job.database.path() == database.path())
        .map(|job| job.checkpoint().status)
        .unwrap_or_default()
}

pub(crate) fn cancel_warmup() {
    let mut state = WARMUPS.lock().unwrap_or_else(|p| p.into_inner());
    for job in state.active.iter().chain(state.queue.iter()) {
        job.cancelled.store(true, Ordering::Relaxed);
        job.progress
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .status
            .phase = PosterCachePhase::Cancelled;
    }
    state.last = state.active.as_ref().or(state.queue.front()).cloned();
    state.queue.clear();
}

pub(crate) fn forget_status(database: &Database) {
    let mut state = WARMUPS.lock().unwrap_or_else(|p| p.into_inner());
    if state
        .last
        .as_ref()
        .is_some_and(|job| job.database.path() == database.path())
    {
        state.last = None;
    }
}

pub(crate) fn schedule_warmup(database: &Database, cache_root: &Path) {
    enqueue(database, cache_root, WarmupKind::Changed);
}
pub(crate) fn resume_warmup(database: &Database, cache_root: &Path) {
    enqueue(database, cache_root, WarmupKind::Resume);
}
pub(crate) fn retry_warmup(database: &Database, cache_root: &Path) {
    enqueue(database, cache_root, WarmupKind::Retry);
}

fn enqueue(database: &Database, cache_root: &Path, kind: WarmupKind) {
    let mut state = WARMUPS.lock().unwrap_or_else(|p| p.into_inner());
    if kind == WarmupKind::Retry
        && state.active.iter().chain(state.queue.iter()).any(|job| {
            !job.cancelled.load(Ordering::Relaxed)
                && job.database.path() == database.path()
                && job.cache_root == cache_root
        })
    {
        return;
    }
    if let Some(job) = state
        .queue
        .iter_mut()
        .find(|job| job.database.path() == database.path() && job.cache_root == cache_root)
    {
        if kind == WarmupKind::Changed {
            job.kind = kind;
        }
    } else {
        if state.queue.len() >= 8 {
            return;
        }
        state
            .queue
            .push_back(Warmup::new(database, cache_root, kind));
    }
    if state.running {
        return;
    }
    state.running = true;
    if std::thread::Builder::new()
        .name("m2shelf-poster-cache".into())
        .spawn(|| loop {
            let job = {
                let mut state = WARMUPS.lock().unwrap_or_else(|p| p.into_inner());
                match state.queue.pop_front() {
                    Some(job) => {
                        state.active = Some(job.clone());
                        job
                    }
                    None => {
                        state.running = false;
                        return;
                    }
                }
            };
            let outcome = std::panic::catch_unwind(|| warmup(&job));
            if !job.cancelled.load(Ordering::Relaxed) {
                if let Some(error) = match outcome {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error),
                    Err(_) => Some("封面后台任务意外结束。".into()),
                } {
                    let mut next = job.checkpoint();
                    next.status.phase = PosterCachePhase::Failed;
                    next.status.error = Some(bounded_detail(&error));
                    // Preserve an in-memory error even when the database itself cannot be written.
                    let _operation = cache::begin_cover_cache_operation();
                    if !job.cancelled.load(Ordering::Relaxed) {
                        let _ = job.persist(&next, None, false);
                        *job.progress.lock().unwrap_or_else(|p| p.into_inner()) = next;
                    }
                }
            }
            let mut state = WARMUPS.lock().unwrap_or_else(|p| p.into_inner());
            state.active = None;
            if !job.cancelled.load(Ordering::Relaxed) {
                state.last = Some(job);
            }
        })
        .is_err()
    {
        state.running = false;
        while let Some(job) = state.queue.pop_front() {
            job.progress
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .status
                .phase = PosterCachePhase::Failed;
            state.last = Some(job);
        }
    }
}

fn warmup(job: &Warmup) -> AppResult<()> {
    if job.cancelled.load(Ordering::Relaxed) {
        return Ok(());
    }
    let saved = job
        .database
        .poster_checkpoint()?
        .and_then(|json| serde_json::from_str::<Checkpoint>(&json).ok())
        .filter(|saved| saved.cache_root == job.cache_root && saved.version == VERSION);
    let mut next = job.checkpoint();
    let mut reset = true;
    if let Some(mut saved) = saved {
        if job.kind == WarmupKind::Resume
            && matches!(
                saved.status.phase,
                PosterCachePhase::Completed | PosterCachePhase::Failed | PosterCachePhase::Idle
            )
        {
            let _operation = cache::begin_cover_cache_operation();
            if job.cancelled.load(Ordering::Relaxed) {
                return Ok(());
            }
            // Volumes collapsed into detail rows can leave historical failures behind. Keep
            // the completed pass, but report only failures still eligible for cover work.
            let previous_failed = saved.status.failed;
            if previous_failed > 0 {
                saved.status.failed = job.database.poster_failure_count()?;
            }
            if saved.status.failed != previous_failed {
                job.persist(&saved, None, false)?;
            } else {
                *job.progress.lock().unwrap_or_else(|p| p.into_inner()) = saved;
            }
            return Ok(());
        }
        if job.kind == WarmupKind::Resume
            || (job.kind == WarmupKind::Retry && saved.status.phase == PosterCachePhase::Failed)
        {
            next = saved;
            reset = false;
        } else if job.kind == WarmupKind::Retry {
            next.failed_only = true;
            reset = false;
        }
    } else if job.kind == WarmupKind::Retry {
        next.failed_only = job.database.poster_failure_count()? > 0;
        reset = !next.failed_only;
    }
    if next.cursor == 0 {
        next.status = PosterCacheStatus {
            phase: PosterCachePhase::Running,
            total: if next.failed_only {
                job.database.poster_failure_count()?
            } else {
                job.database.poster_node_count()?
            },
            failed: if next.failed_only {
                job.database.poster_failure_count()?
            } else {
                0
            },
            ..Default::default()
        };
    } else {
        next.status.phase = PosterCachePhase::Running;
        next.status.error = None;
    }
    // Cache clearing holds the cache writer barrier; checkpoint writes share the read side so a
    // cancelled worker cannot resurrect old state after the clear transaction.
    {
        let _operation = cache::begin_cover_cache_operation();
        if job.cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        job.persist(&next, None, reset)?;
    }
    let directory;
    let mut budget;
    {
        let operation = cache::begin_cover_cache_operation();
        if job.cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        let _generation = GENERATION.lock().unwrap_or_else(|p| p.into_inner());
        directory = self::directory(&job.database, &job.cache_root)?;
        retire_obsolete(&operation, &directory)?;
        budget = DiskBudget::read(&directory)?;
    }
    let mut budget_full = false;
    loop {
        let nodes = if next.failed_only {
            job.database.poster_failed_nodes_after(next.cursor)?
        } else {
            job.database.poster_nodes_after(next.cursor)?
        };
        if nodes.is_empty() {
            break;
        }
        for node_id in nodes {
            let operation = cache::begin_cover_cache_operation();
            if job.cancelled.load(Ordering::Relaxed) {
                return Ok(());
            }
            let result = prepare_node(
                job,
                node_id,
                &operation,
                &directory,
                &mut budget,
                &mut budget_full,
            );
            let old_failed = next.status.failed;
            next.cursor = node_id;
            next.status.processed += 1;
            next.status.total = next.status.total.max(next.status.processed);
            let failure = result.as_ref().err();
            if failure.is_some() && !next.failed_only {
                next.status.failed += 1;
            }
            if matches!(result, Ok(Preparation::Ready)) && next.failed_only {
                next.status.failed = old_failed.saturating_sub(1);
            }
            if matches!(result, Ok(Preparation::Deferred)) {
                next.status.deferred += 1;
            }
            let outcome = if matches!(result, Ok(Preparation::Deferred)) {
                None
            } else {
                Some((
                    node_id,
                    failure.map(|failure| (failure.reason, failure.detail.as_str())),
                ))
            };
            job.persist(&next, outcome, false)?;
            drop(operation);
            std::thread::sleep(Duration::from_millis(12));
        }
    }
    let _operation = cache::begin_cover_cache_operation();
    if job.cancelled.load(Ordering::Relaxed) {
        return Ok(());
    }
    next.status.phase = PosterCachePhase::Completed;
    next.status.total = next.status.processed;
    next.status.failed = job.database.poster_failure_count()?;
    job.persist(&next, None, false)
}

enum Preparation {
    Ready,
    Deferred,
}
struct PreparationFailure {
    reason: &'static str,
    detail: String,
}
fn bounded_detail(detail: &str) -> String {
    detail
        .chars()
        .filter(|ch| !ch.is_control())
        .take(512)
        .collect()
}
fn preparation_failure(reason: &'static str, detail: String) -> PreparationFailure {
    PreparationFailure {
        reason,
        detail: bounded_detail(&detail),
    }
}

fn prepare_node(
    job: &Warmup,
    node_id: i64,
    operation: &cache::CoverCacheOperationGuard,
    directory: &Path,
    budget: &mut DiskBudget,
    budget_full: &mut bool,
) -> Result<Preparation, PreparationFailure> {
    let Some((source, key)) = source(&job.database, node_id)
        .map_err(|error| preparation_failure("SOURCE_READ", error))?
    else {
        return Ok(Preparation::Ready);
    };
    if WIDTHS
        .iter()
        .all(|&width| thumbnail_present(&path_for(directory, &key, width)))
    {
        return Ok(Preparation::Ready);
    }
    if *budget_full {
        return Ok(Preparation::Deferred);
    }
    let _generation = GENERATION.lock().unwrap_or_else(|p| p.into_inner());
    let bytes = source
        .read(&job.database)
        .map_err(|error| preparation_failure("SOURCE_READ", error))?;
    let Some(bytes) = bytes else {
        return Ok(Preparation::Ready);
    };
    if self::source(&job.database, node_id)
        .map_err(|error| preparation_failure("SOURCE_READ", error))?
        .map(|source| source.1)
        != Some(key.clone())
    {
        return Err(preparation_failure(
            "SOURCE_CHANGED",
            "封面在读取期间发生变化。".into(),
        ));
    }
    // Foreground generation shares this gate but may have changed disk occupancy since the last Node.
    if budget.revision != DISK_MUTATIONS.load(Ordering::Relaxed) {
        *budget = DiskBudget::read(directory)
            .map_err(|error| preparation_failure("PROCESSING", error))?;
    }
    let complete = generate_limited(operation, directory, &key, &bytes, &WIDTHS, Some(budget))
        .map_err(|error| preparation_failure("PROCESSING", error))?;
    if complete {
        Ok(Preparation::Ready)
    } else {
        *budget_full = true;
        Ok(Preparation::Deferred)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CoverSource;

    fn fixture(width: u32, height: u32) -> (tempfile::TempDir, Database, PathBuf, i64, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("library");
        let cache_root = temp.path().join("cache");
        fs::create_dir(&library).unwrap();
        cache::ensure_directories(&cache_root).unwrap();
        let database = Database::new(temp.path().join("index.db"));
        database.migrate().unwrap();
        let root = database.add_root(&library, None).unwrap();
        let connection = database.connect().unwrap();
        connection.execute("INSERT INTO nodes(library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(?1,?2,'Root','Root','CONTAINER')",rusqlite::params![root.id,library.to_string_lossy()]).unwrap();
        let parent = connection.last_insert_rowid();
        connection.execute("INSERT INTO nodes(library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,total_video_count) VALUES(?1,?2,?3,'Work','Work','WORK',1)",rusqlite::params![root.id,parent,library.join("Work").to_string_lossy()]).unwrap();
        let node = connection.last_insert_rowid();
        let original = cache_root.join("manual").join(format!("node-{node}.png"));
        let pixels = image::RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([
                (x % 256) as u8,
                (y % 256) as u8,
                128,
                if x < width / 2 { 0 } else { 255 },
            ])
        });
        pixels.save(&original).unwrap();
        database
            .set_node_cover(node, CoverSource::Manual, Some(&original))
            .unwrap();
        (temp, database, cache_root, node, original)
    }

    #[test]
    fn completed_checkpoint_survives_reopen_without_source_read_or_rewrites() {
        let (_temp, database, root, node, original) = fixture(32, 48);
        let job = Warmup::new(&database, &root, WarmupKind::Changed);
        warmup(&job).unwrap();
        assert_eq!(job.checkpoint().status.processed, 1);
        let (_, key) = source(&database, node).unwrap().unwrap();
        let stamps: Vec<_> = WIDTHS
            .iter()
            .map(|width| {
                fs::metadata(path_for(&root.join("posters"), &key, *width))
                    .unwrap()
                    .modified()
                    .unwrap()
            })
            .collect();
        // Proves startup does not even reopen the source or walk all cover Nodes.
        fs::remove_file(&original).unwrap();
        let reopened = Database::new(database.path().into());
        let resumed = Warmup::new(&reopened, &root, WarmupKind::Resume);
        warmup(&resumed).unwrap();
        assert_eq!(
            resumed.checkpoint().status.phase,
            PosterCachePhase::Completed
        );
        assert_eq!(resumed.checkpoint().status.failed, 0);
        assert_eq!(
            stamps,
            WIDTHS
                .iter()
                .map(
                    |width| fs::metadata(path_for(&root.join("posters"), &key, *width))
                        .unwrap()
                        .modified()
                        .unwrap()
                )
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn failures_are_durable_and_retry_only_failed_nodes() {
        let (_temp, database, root, node, original) = fixture(32, 48);
        let before = fs::read(&original).unwrap();
        let connection = database.connect().unwrap();
        connection.execute("INSERT INTO nodes(library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,total_video_count) SELECT library_root_id,parent_node_id,absolute_path || '-second','Second','Second','WORK',1 FROM nodes WHERE id=?1",[node]).unwrap();
        let second = connection.last_insert_rowid();
        let second_original = root.join("manual").join(format!("node-{second}.png"));
        fs::write(&second_original, &before).unwrap();
        database
            .set_node_cover(second, CoverSource::Manual, Some(&second_original))
            .unwrap();
        fs::write(&original, b"broken image").unwrap();
        warmup(&Warmup::new(&database, &root, WarmupKind::Changed)).unwrap();
        let (_, key) = source(&database, second).unwrap().unwrap();
        let second_stamps: Vec<_> = WIDTHS
            .iter()
            .map(|width| {
                fs::metadata(path_for(&root.join("posters"), &key, *width))
                    .unwrap()
                    .modified()
                    .unwrap()
            })
            .collect();
        let reopened = Database::new(database.path().into());
        assert_eq!(reopened.poster_failure_count().unwrap(), 1);
        let failure = reopened.poster_failures().unwrap().remove(0);
        assert_eq!(failure.node_id, node);
        assert_eq!(failure.reason, "SOURCE_READ");
        assert!(!failure.detail.is_empty());
        fs::write(&original, before).unwrap();
        let retry = Warmup::new(&reopened, &root, WarmupKind::Retry);
        warmup(&retry).unwrap();
        assert!(retry.checkpoint().failed_only);
        assert_eq!(retry.checkpoint().status.processed, 1);
        assert_eq!(
            second_stamps,
            WIDTHS
                .iter()
                .map(
                    |width| fs::metadata(path_for(&root.join("posters"), &key, *width))
                        .unwrap()
                        .modified()
                        .unwrap()
                )
                .collect::<Vec<_>>()
        );
        assert_eq!(reopened.poster_failure_count().unwrap(), 0);
        let retry = Warmup::new(&reopened, &root, WarmupKind::Retry);
        warmup(&retry).unwrap();
        assert_eq!(retry.checkpoint().status.processed, 0);
        reopened.clear_poster_checkpoint().unwrap();
        assert!(reopened.poster_checkpoint().unwrap().is_none());
    }

    #[test]
    fn interrupted_checkpoint_continues_after_recorded_cursor() {
        let (_temp, database, root, node, original) = fixture(32, 48);
        let connection = database.connect().unwrap();
        connection.execute("INSERT INTO nodes(library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,cover_source,cover_cache_path,total_video_count) SELECT library_root_id,parent_node_id,absolute_path || '-second','Second','Second','WORK',cover_source,cover_cache_path,1 FROM nodes WHERE id=?1",[node]).unwrap();
        let second = connection.last_insert_rowid();
        let job = Warmup::new(&database, &root, WarmupKind::Changed);
        let mut checkpoint = job.checkpoint();
        checkpoint.cursor = node;
        checkpoint.status.phase = PosterCachePhase::Running;
        checkpoint.status.processed = 1;
        checkpoint.status.total = 2;
        job.persist(&checkpoint, None, false).unwrap();
        // If the first Node were retried it would now report a source-read failure.
        connection
            .execute(
                "UPDATE nodes SET cover_cache_path=?1 WHERE id=?2",
                rusqlite::params![original.with_extension("gone").to_string_lossy(), node],
            )
            .unwrap();
        let resumed = Warmup::new(
            &Database::new(database.path().into()),
            &root,
            WarmupKind::Resume,
        );
        warmup(&resumed).unwrap();
        assert_eq!(resumed.checkpoint().cursor, second);
        assert_eq!(
            (
                resumed.checkpoint().status.processed,
                resumed.checkpoint().status.failed
            ),
            (2, 0)
        );
    }

    #[test]
    fn background_quota_defers_without_evicting_or_retrying_on_restart() {
        let (_temp, database, root, _node, _original) = fixture(32, 48);
        let directory = directory(&database, &root).unwrap();
        let existing = path_for(&directory, &"a".repeat(64), 256);
        File::create(&existing)
            .unwrap()
            .set_len(MAX_DISK_BYTES)
            .unwrap();
        let stamp = fs::metadata(&existing).unwrap().modified().unwrap();
        let job = Warmup::new(&database, &root, WarmupKind::Changed);
        warmup(&job).unwrap();
        assert_eq!(
            (
                job.checkpoint().status.deferred,
                job.checkpoint().status.failed
            ),
            (1, 0)
        );
        warmup(&Warmup::new(&database, &root, WarmupKind::Resume)).unwrap();
        assert_eq!(fs::metadata(&existing).unwrap().modified().unwrap(), stamp);
        assert_eq!(fs::read_dir(directory).unwrap().count(), 1);
    }

    #[test]
    fn opaque_thumbnails_use_compact_jpeg_and_upgrade_only_owned_v1() {
        let (_temp, database, root, node, original) = fixture(600, 900);
        image::RgbImage::from_fn(600, 900, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        })
        .save(&original)
        .unwrap();
        let directory = directory(&database, &root).unwrap();
        let obsolete = directory.join(format!("v1-{}-256.m2thumb", "a".repeat(64)));
        fs::write(&obsolete, b"old").unwrap();
        let unrelated = directory.join("family.jpg");
        fs::write(&unrelated, b"keep").unwrap();
        warmup(&Warmup::new(&database, &root, WarmupKind::Changed)).unwrap();
        assert!(!obsolete.exists());
        assert!(unrelated.exists());
        let (_, key) = source(&database, node).unwrap().unwrap();
        for width in WIDTHS {
            let bytes = read_thumbnail(&path_for(&directory, &key, width), width).unwrap();
            assert_eq!(
                image::guess_format(&bytes).unwrap(),
                image::ImageFormat::Jpeg
            );
            assert!(cover_data_url(&database, &root, node, width)
                .unwrap()
                .unwrap()
                .starts_with("data:image/jpeg;"));
        }
    }

    /// Opt-in only: use a disposable index copy and a new application-owned benchmark cache.
    /// Never point this at the live database/cache. Sources are read through normal Root guards.
    #[test]
    #[ignore = "requires a disposable index copy and benchmark cache"]
    fn disposable_library_warmup_benchmark() {
        let database = Database::new(
            std::env::var_os("M2SHELF_BENCH_INDEX")
                .expect("disposable index required")
                .into(),
        );
        let root = PathBuf::from(
            std::env::var_os("M2SHELF_BENCH_CACHE").expect("new benchmark cache required"),
        );
        assert!(!root.exists(), "benchmark must use a new cache");
        cache::ensure_directories(&root).unwrap();
        database.migrate().unwrap();
        let start = std::time::Instant::now();
        let first = Warmup::new(&database, &root, WarmupKind::Changed);
        warmup(&first).unwrap();
        let first_ms = start.elapsed().as_millis();
        let before = owned_files(&root.join("posters")).unwrap();
        let reopened = Database::new(database.path().into());
        let start = std::time::Instant::now();
        let second = Warmup::new(&reopened, &root, WarmupKind::Resume);
        warmup(&second).unwrap();
        let resume_us = start.elapsed().as_micros();
        let after = owned_files(&root.join("posters")).unwrap();
        assert_eq!(before, after);
        let bytes: u64 = after.iter().map(|file| file.1).sum();
        eprintln!("poster benchmark: first={first_ms}ms resume={resume_us}us files={} bytes={bytes} processed={} failed={} deferred={} rewritten=0",after.len(),second.checkpoint().status.processed,second.checkpoint().status.failed,second.checkpoint().status.deferred);
        // Report aggregate only. Diagnostics remain in the private disposable database.
    }

    #[test]
    fn derivatives_preserve_aspect_alpha_and_original_and_survive_reopen() {
        let (_temp, database, root, node, original) = fixture(600, 900);
        let before = fs::read(&original).unwrap();
        let (_, key) = source(&database, node).unwrap().unwrap();
        let directory = directory(&database, &root).unwrap();
        generate(
            &cache::begin_cover_cache_operation(),
            &directory,
            &key,
            &before,
            &WIDTHS,
        )
        .unwrap();
        for width in WIDTHS {
            let bytes = read_thumbnail(&path_for(&directory, &key, width), width).unwrap();
            let decoded = image::load_from_memory(&bytes).unwrap();
            assert_eq!(
                (decoded.width(), decoded.height()),
                (width.min(600), width.min(600) * 3 / 2)
            );
            assert_eq!(decoded.to_rgba8().get_pixel(0, 0)[3], 0);
        }
        let first = cover_data_url(&database, &root, node, 384).unwrap();
        let stamps: Vec<_> = fs::read_dir(&directory)
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().modified().unwrap())
            .collect();
        let reopened = Database::new(database.path().into());
        assert_eq!(cover_data_url(&reopened, &root, node, 384).unwrap(), first);
        assert_eq!(
            stamps,
            fs::read_dir(&directory)
                .unwrap()
                .map(|e| e.unwrap().metadata().unwrap().modified().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(fs::read(original).unwrap(), before);
    }

    #[test]
    fn changed_original_and_corrupt_derivative_are_rebuilt() {
        let (_temp, database, root, node, original) = fixture(32, 48);
        let first = cover_data_url(&database, &root, node, 256).unwrap();
        let (_, key) = source(&database, node).unwrap().unwrap();
        let path = path_for(&root.join("posters"), &key, 256);
        let mut corrupt = fs::read(&path).unwrap();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        fs::write(&path, corrupt).unwrap();
        assert_eq!(cover_data_url(&database, &root, node, 256).unwrap(), first);
        image::RgbaImage::from_pixel(40, 60, image::Rgba([200, 10, 20, 255]))
            .save(&original)
            .unwrap();
        assert_ne!(source(&database, node).unwrap().unwrap().1, key);
        assert_ne!(cover_data_url(&database, &root, node, 256).unwrap(), first);
        assert!(cover_data_url(&database, &root, node, 300).is_err());
    }

    #[test]
    fn cleanup_and_quota_only_touch_owned_plain_derivatives() {
        let (_temp, database, root, node, original) = fixture(32, 48);
        cover_data_url(&database, &root, node, 256).unwrap();
        let posters = root.join("posters");
        let unrelated = posters.join("family.webp");
        fs::write(&unrelated, b"keep").unwrap();
        prune(&cache::begin_cover_cache_operation(), &posters, 0, 0).unwrap();
        assert!(original.exists());
        assert!(unrelated.exists());
        cover_data_url(&database, &root, node, 256).unwrap();
        cache::clear_cover_cache(&cache::begin_cover_cache_clear(), &root).unwrap();
        assert!(unrelated.exists());
        assert!(!fs::read_dir(posters)
            .unwrap()
            .any(|e| is_owned_name(&e.unwrap().file_name().to_string_lossy())));
    }

    #[test]
    fn cache_inside_a_library_and_linked_directories_are_rejected() {
        let (temp, database, root, node, _) = fixture(32, 48);
        let media_root = temp.path().join("library");
        assert!(cover_data_url(&database, &media_root, node, 256).is_err());
        let posters = root.join("posters");
        fs::remove_dir(&posters).unwrap();
        let outside = temp.path().join("unrelated");
        fs::create_dir(&outside).unwrap();
        #[cfg(windows)]
        {
            // Junction creation needs no developer-mode symlink privilege.
            let status = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(&posters)
                .arg(&outside)
                .output()
                .unwrap();
            assert!(status.status.success());
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &posters).unwrap();
        assert!(cover_data_url(&database, &root, node, 256).is_err());
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }

    #[test]
    fn oriented_jpeg_is_prepared_once_and_warm_disk_reads_reuse_it() {
        let (_temp, database, root, node, original) = fixture(60, 90);
        let pixels = image::RgbImage::from_pixel(600, 900, image::Rgb([120, 150, 180]));
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode_image(&pixels)
            .unwrap();
        // Minimal little-endian EXIF: orientation 6 (90 degrees clockwise).
        let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut oriented = jpeg[..2].to_vec();
        oriented.extend_from_slice(&[0xff, 0xe1]);
        oriented.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        oriented.extend_from_slice(exif);
        oriented.extend_from_slice(&jpeg[2..]);
        fs::write(&original, &oriented).unwrap();
        let cold = std::time::Instant::now();
        let mut urls = serde_json::Map::new();
        for width in WIDTHS {
            let url = cover_data_url(&database, &root, node, width)
                .unwrap()
                .unwrap();
            let bytes = STANDARD.decode(url.split_once(',').unwrap().1).unwrap();
            let decoded = image::load_from_memory(&bytes).unwrap();
            assert!(decoded.width() > decoded.height());
            urls.insert(width.to_string(), url.into());
        }
        let cold_ms = cold.elapsed().as_secs_f64() * 1000.0;
        let warm = std::time::Instant::now();
        for _ in 0..20 {
            assert_eq!(
                cover_data_url(&Database::new(database.path().into()), &root, node, 384)
                    .unwrap()
                    .unwrap(),
                urls["384"].as_str().unwrap()
            );
        }
        let warm_ms = warm.elapsed().as_secs_f64() * 1000.0 / 20.0;
        eprintln!("poster fixture cold four tiers: {cold_ms:.2}ms; warm disk mean: {warm_ms:.2}ms");
        assert_eq!(fs::read(original).unwrap(), oriented);
        if let Ok(output) = std::env::var("M2SHELF_POSTER_FIXTURE") {
            fs::write(output,serde_json::to_vec(&serde_json::json!({"tiers":urls,"original":format!("data:image/jpeg;base64,{}",STANDARD.encode(oriented)),"coldMs":cold_ms,"warmMs":warm_ms})).unwrap()).unwrap();
        }
    }
}

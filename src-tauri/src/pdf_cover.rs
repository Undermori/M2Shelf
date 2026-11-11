//! One-shot authorization for a bounded PDF.js first-page raster. No source/path write API.
use crate::{
    comic_reader,
    db::{AppResult, Database},
    poster_cache,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        LazyLock, Mutex,
    },
    time::{Duration, Instant},
};

static EPOCH: AtomicU64 = AtomicU64::new(0);
pub(crate) fn epoch() -> u64 {
    EPOCH.load(Ordering::Acquire)
}
struct Pending {
    epoch: u64,
    database: PathBuf,
    cache: PathBuf,
    book: i64,
    revision: String,
    node: Option<i64>,
    key: String,
    started: Instant,
}
type MissingSources = HashMap<(PathBuf, i64, String), Instant>;
static MISSING: LazyLock<Mutex<MissingSources>> = LazyLock::new(|| Mutex::new(HashMap::new()));
pub(crate) fn record_missing(db: &Database, book: i64, revision: &str) {
    let mut missing = MISSING.lock().unwrap_or_else(|p| p.into_inner());
    missing.retain(|_, at| at.elapsed() < Duration::from_secs(120));
    if missing.len() >= 512 {
        if let Some(oldest) = missing
            .iter()
            .min_by_key(|(_, at)| **at)
            .map(|(key, _)| key.clone())
        {
            missing.remove(&oldest);
        }
    }
    missing.insert((db.path().into(), book, revision.into()), Instant::now());
}
static PENDING: LazyLock<Mutex<HashMap<String, Pending>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub ticket: Option<String>,
    pub ready: bool,
    pub book_id: i64,
    pub revision: String,
    pub source_size: u64,
}

pub(crate) fn cancel_all() {
    EPOCH.fetch_add(1, Ordering::AcqRel);
    PENDING.lock().unwrap_or_else(|p| p.into_inner()).clear();
}
pub(crate) fn cancel(ticket: &str) {
    PENDING
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(ticket);
}

pub(crate) fn begin(
    db: &Database,
    cache: &std::path::Path,
    node: Option<i64>,
    book: Option<i64>,
    revision: Option<&str>,
) -> AppResult<Option<Request>> {
    if node.is_some() == book.is_some() {
        return Err("PDF_COVER_TARGET_INVALID".into());
    }
    let candidate = if let Some(node) = node {
        if db.cover_read_context(node)?.0.is_some() {
            return Ok(None);
        }
        comic_reader::first_image_cover_source(db, node)?
    } else {
        let id = book.unwrap();
        let rev = revision.ok_or("PDF_COVER_REVISION_REQUIRED")?;
        comic_reader::cover_identity(db, id, rev)?.map(|name| (id, rev.into(), name))
    };
    let Some((book, revision, _)) = candidate else {
        return Ok(None);
    };
    let size = match comic_reader::pdf_cover_context(db, book, &revision)? {
        Some(size) => size,
        None => return Ok(None),
    };
    let key = poster_cache::pdf_source_key(db, book, &revision)?;
    let _ = comic_reader::read_pdf_range(db, book, 0, size.min(64), &revision)?;
    if poster_cache::pdf_tiers_ready(db, cache, &key)? {
        return Ok(Some(Request {
            ticket: None,
            ready: true,
            book_id: book,
            revision,
            source_size: size,
        }));
    }
    // Native embedded JPEG remains first. This is only the final placeholder branch.
    if MISSING
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&(db.path().into(), book, revision.clone()))
        .is_none_or(|at| at.elapsed() >= Duration::from_secs(120))
    {
        return Ok(None);
    }
    let mut pending = PENDING.lock().unwrap_or_else(|p| p.into_inner());
    pending.retain(|_, p| p.started.elapsed() < Duration::from_secs(25));
    if pending.len() >= 8 {
        return Ok(None);
    }
    let ticket = uuid::Uuid::new_v4().simple().to_string();
    pending.insert(
        ticket.clone(),
        Pending {
            epoch: epoch(),
            database: db.path().into(),
            cache: cache.into(),
            book,
            revision: revision.clone(),
            node,
            key,
            started: Instant::now(),
        },
    );
    Ok(Some(Request {
        ticket: Some(ticket),
        ready: false,
        book_id: book,
        revision,
        source_size: size,
    }))
}

pub(crate) fn commit(
    db: &Database,
    cache: &std::path::Path,
    ticket: &str,
    base64: &str,
) -> AppResult<bool> {
    use base64::Engine;
    if base64.len() > 2 * 1024 * 1024 {
        return Err("PDF_COVER_LIMIT".into());
    }
    let pending = PENDING
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(ticket)
        .ok_or("PDF_COVER_EXPIRED")?;
    if pending.epoch != epoch()
        || pending.started.elapsed() > Duration::from_secs(25)
        || pending.database != db.path()
        || pending.cache != cache
    {
        return Err("PDF_COVER_EXPIRED".into());
    }
    if let Some(node) = pending.node {
        if db.cover_read_context(node)?.0.is_some() {
            return Ok(false);
        }
    }
    let Some(size) = comic_reader::pdf_cover_context(db, pending.book, &pending.revision)? else {
        return Ok(false);
    };
    let _ = comic_reader::read_pdf_range(db, pending.book, 0, size.min(64), &pending.revision)?;
    if poster_cache::pdf_source_key(db, pending.book, &pending.revision)? != pending.key {
        return Err("COMIC_PAGE_CHANGED".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64)
        .map_err(|_| "PDF_COVER_INVALID")?;
    if bytes.len() > 1024 * 1024
        || image::guess_format(&bytes).ok() != Some(image::ImageFormat::Png)
    {
        return Err("PDF_COVER_INVALID".into());
    }
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| "PDF_COVER_INVALID")?
        .into_dimensions()
        .map_err(|_| "PDF_COVER_INVALID")?;
    if width == 0
        || height == 0
        || width > 768
        || height > 3072
        || u64::from(width) * u64::from(height) > 2_400_000
    {
        return Err("PDF_COVER_LIMIT".into());
    }
    poster_cache::accept_pdf_raster(db, cache, &pending.key, &bytes, pending.epoch)?;
    Ok(true)
}

//! Raster-only local resources. No caller-supplied filesystem paths and no image bytes in JS.
use crate::{
    cache,
    db::{AppResult, Database},
    poster_cache::{self, PosterPayload},
    AppState,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
};
use tauri::{
    http::{Request, Response, StatusCode},
    Manager,
};

#[derive(Debug, PartialEq, Eq)]
enum Target {
    Node(i64),
    Book(i64, String),
}
#[derive(Debug, PartialEq, Eq)]
struct Resource {
    target: Target,
    width: u32,
}

fn parse(request: &Request<Vec<u8>>) -> Option<Resource> {
    if !matches!(request.method().as_str(), "GET" | "HEAD")
        || !request.body().is_empty()
        || request.uri().to_string().len() > 1024
    {
        return None;
    }
    let url = reqwest::Url::parse(&request.uri().to_string()).ok()?;
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return None;
    }
    if !(matches!(url.scheme(), "http" | "https") && url.host_str() == Some("m2poster.localhost")
        || url.scheme() == "m2poster" && url.host_str() == Some("localhost"))
    {
        return None;
    }
    let parts = url.path().split('/').collect::<Vec<_>>();
    if parts.len() != 4 || !matches!(parts[1], "node" | "book") {
        return None;
    }
    let id = parts[2].parse::<i64>().ok()?;
    let width = parts[3].parse::<u32>().ok()?;
    if id <= 0
        || id.to_string() != parts[2]
        || width.to_string() != parts[3]
        || !poster_cache::WIDTHS.contains(&width)
    {
        return None;
    }
    let mut version = None;
    let mut revision = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "v" if version.is_none()
                && (16..=96).contains(&value.len())
                && value.bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                version = Some(value.into_owned())
            }
            "revision"
                if revision.is_none()
                    && !value.is_empty()
                    && value.len() <= 256
                    && !value.chars().any(char::is_control) =>
            {
                revision = Some(value.into_owned())
            }
            _ => return None,
        }
    }
    version?;
    let target = match parts[1] {
        "node" if revision.is_none() => Target::Node(id),
        "book" => Target::Book(id, revision?),
        _ => return None,
    };
    Some(Resource { target, width })
}

fn original(database: &Database, target: &Target) -> AppResult<Option<PosterPayload>> {
    let bytes = match target {
        Target::Node(id) => {
            let (path, roots) = database.cover_read_context(*id)?;
            if let Some(path) = path {
                for root in roots {
                    if cache::is_equal_or_within_checked(&path, &root)? {
                        return Err("POSTER_SOURCE_SCOPE".into());
                    }
                }
                Some(cache::read_cover_payload(&path)?)
            } else if let Some((book, revision, _)) =
                crate::comic_reader::first_image_cover_source(database, *id)?
            {
                crate::comic_reader::read_book_cover(database, book, &revision)?
                    .filter(|b| b.len() <= 8 * 1024 * 1024)
            } else {
                None
            }
        }
        Target::Book(id, revision) => {
            database.read_snapshot(|c| {
                let (node, current): (i64, String) = c
                    .query_row(
                        "SELECT node_id,revision FROM comic_books WHERE id=?1",
                        [id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .map_err(|e| e.to_string())?;
                crate::db::ensure_node_visible_conn(c, node)?;
                if &current != revision {
                    return Err("COMIC_PAGE_CHANGED".into());
                }
                Ok(())
            })?;
            if let Some(path) = crate::cover_selection::book_manual(database, *id)? {
                for root in database.library_paths()? {
                    if cache::is_equal_or_within_checked(&path, &root)? {
                        return Err("POSTER_SOURCE_SCOPE".into());
                    }
                }
                Some(cache::read_cover_payload(&path)?)
            } else {
                crate::comic_reader::read_book_cover(database, *id, revision)?
            }
        }
    };
    bytes.map(PosterPayload::new).transpose()
}

fn read(
    database: &Database,
    default_cache: &Path,
    resource: &Resource,
) -> AppResult<Option<PosterPayload>> {
    database.read_snapshot(|c| {
        let node = match &resource.target {
            Target::Node(id) => *id,
            Target::Book(id, expected) => {
                let (node, revision): (i64, String) = c
                    .query_row(
                        "SELECT node_id,revision FROM comic_books WHERE id=?1",
                        [id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .map_err(|e| e.to_string())?;
                if &revision != expected {
                    return Err("COMIC_PAGE_CHANGED".into());
                }
                node
            }
        };
        crate::db::ensure_node_visible_conn(c, node)
    })?;
    let root = database.poster_cache_root(default_cache)?;
    let result = match &resource.target {
        Target::Node(id) => poster_cache::cover_payload(database, &root, *id, resource.width),
        Target::Book(id, revision) => {
            poster_cache::book_cover_payload(database, &root, *id, revision, resource.width)
        }
    };
    // Retain the established validated-original fallback for an unavailable custom cache.
    result.or_else(|_| {
        let _operation = cache::begin_cover_cache_operation();
        original(database, &resource.target)
    })
}

fn unavailable(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(Vec::new())
        .expect("static response")
}
const MAX_ENCODED_BYTES: usize = 32 * 1024 * 1024;
const MAX_RESOURCE_ENTRIES: usize = 4096;
struct Raster {
    payload: PosterPayload,
    etag: String,
}
struct MemoryEntry {
    raster: Arc<Raster>,
    tick: u64,
}
#[derive(Default)]
struct ResourceCache {
    images: HashMap<String, MemoryEntry>,
    order: BTreeMap<u64, String>,
    files: HashMap<String, (crate::poster_cache::PreparedPosterFile, u64)>,
    file_order: BTreeMap<u64, String>,
    bytes: usize,
    tick: u64,
}
static RESOURCE_EPOCH: AtomicU64 = AtomicU64::new(0);
static RESOURCES: OnceLock<Mutex<ResourceCache>> = OnceLock::new();
fn resources() -> &'static Mutex<ResourceCache> {
    RESOURCES.get_or_init(|| Mutex::new(ResourceCache::default()))
}
fn memory_hit(key: &str) -> Option<Arc<Raster>> {
    let mut cache = resources().lock().unwrap_or_else(|p| p.into_inner());
    let old = cache.images.get(key)?.tick;
    cache.order.remove(&old);
    cache.tick += 1;
    let tick = cache.tick;
    let entry = cache.images.get_mut(key)?;
    entry.tick = tick;
    let raster = entry.raster.clone();
    cache.order.insert(tick, key.into());
    Some(raster)
}
fn file_hit(key: &str) -> Option<crate::poster_cache::PreparedPosterFile> {
    let mut cache = resources().lock().unwrap_or_else(|p| p.into_inner());
    let old = cache.files.get(key)?.1;
    cache.file_order.remove(&old);
    cache.tick += 1;
    let tick = cache.tick;
    let entry = cache.files.get_mut(key)?;
    entry.1 = tick;
    let file = entry.0.clone();
    cache.file_order.insert(tick, key.into());
    Some(file)
}
fn remember(key: String, payload: PosterPayload, epoch: u64) -> Arc<Raster> {
    let raster = Arc::new(Raster {
        etag: format!("\"{:x}\"", Sha256::digest(&payload.bytes)),
        payload,
    });
    let mut cache = resources().lock().unwrap_or_else(|p| p.into_inner());
    if RESOURCE_EPOCH.load(Ordering::Acquire) != epoch {
        return raster;
    }
    cache.tick += 1;
    let tick = cache.tick;
    if let Some(file) = raster.payload.prepared.clone() {
        if let Some((_, old)) = cache.files.remove(&key) {
            cache.file_order.remove(&old);
        }
        cache.files.insert(key.clone(), (file, tick));
        cache.file_order.insert(tick, key.clone());
        while cache.files.len() > MAX_RESOURCE_ENTRIES {
            let Some((_, key)) = cache.file_order.pop_first() else {
                break;
            };
            cache.files.remove(&key);
        }
    }
    if raster.payload.bytes.len() <= MAX_ENCODED_BYTES {
        if let Some(old) = cache.images.remove(&key) {
            cache.order.remove(&old.tick);
            cache.bytes -= old.raster.payload.bytes.len();
        }
        cache.bytes += raster.payload.bytes.len();
        cache.images.insert(
            key.clone(),
            MemoryEntry {
                raster: raster.clone(),
                tick,
            },
        );
        cache.order.insert(tick, key);
        while cache.bytes > MAX_ENCODED_BYTES || cache.images.len() > MAX_RESOURCE_ENTRIES {
            let Some((_, key)) = cache.order.pop_first() else {
                break;
            };
            if let Some(entry) = cache.images.remove(&key) {
                cache.bytes -= entry.raster.payload.bytes.len();
            }
        }
    }
    raster
}
pub(crate) fn clear_memory() {
    RESOURCE_EPOCH.fetch_add(1, Ordering::AcqRel);
    *resources().lock().unwrap_or_else(|p| p.into_inner()) = ResourceCache::default();
}
fn raster_response(request: &Request<Vec<u8>>, raster: &Raster) -> Response<Vec<u8>> {
    let cached = request
        .headers()
        .get("If-None-Match")
        .is_some_and(|v| v.as_bytes() == raster.etag.as_bytes());
    Response::builder()
        .status(if cached {
            StatusCode::NOT_MODIFIED
        } else {
            StatusCode::OK
        })
        .header("Content-Type", raster.payload.mime)
        .header("Cache-Control", "private, max-age=31536000, immutable")
        .header("ETag", &raster.etag)
        .header("X-Content-Type-Options", "nosniff")
        .body(if cached || request.method() == "HEAD" {
            Vec::new()
        } else {
            raster.payload.bytes.clone()
        })
        .expect("raster response")
}
fn resource_key(database: &Database, request: &Request<Vec<u8>>) -> String {
    format!(
        "{:x}|{}",
        Sha256::digest(database.path().to_string_lossy().as_bytes()),
        request.uri()
    )
}
#[cfg(test)]
fn serve(
    database: &Database,
    default_cache: &Path,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    serve_with_reader(database, default_cache, request, None)
}
fn serve_with_reader(
    database: &Database,
    default_cache: &Path,
    request: Request<Vec<u8>>,
    reader: Option<&mut poster_cache::PreparedPosterReader>,
) -> Response<Vec<u8>> {
    let Some(resource) = parse(&request) else {
        return unavailable(StatusCode::BAD_REQUEST);
    };
    let epoch = RESOURCE_EPOCH.load(Ordering::Acquire);
    let key = resource_key(database, &request);
    if let Some(raster) = memory_hit(&key) {
        return raster_response(&request, &raster);
    }
    // Windows WebView does not cache intercepted protocol responses like ordinary HTTP files.
    // A RAM miss reuses the exact checked derivative lease, with no SQL/source reopening.
    let payload = {
        let _operation = cache::begin_cover_cache_operation();
        file_hit(&key).and_then(|file| file.read()).or_else(|| {
            reader.and_then(|reader| match &resource.target {
                Target::Node(id) => reader.read(Some(*id), None, resource.width).ok().flatten(),
                Target::Book(id, revision) => reader
                    .read(None, Some((*id, revision)), resource.width)
                    .ok()
                    .flatten(),
            })
        })
    };
    let result = if payload.is_some() {
        Ok(payload)
    } else {
        read(database, default_cache, &resource)
    };
    match result {
        Ok(Some(payload)) => raster_response(&request, &remember(key, payload, epoch)),
        _ => unavailable(StatusCode::NOT_FOUND),
    }
}

pub(crate) fn register(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    type ReaderState = Option<(std::path::PathBuf, u64, poster_cache::PreparedPosterReader)>;
    struct Slot {
        gate: tauri::async_runtime::Mutex<()>,
        reader: Mutex<ReaderState>,
    }
    static SLOTS: OnceLock<Vec<Slot>> = OnceLock::new();
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    builder.register_asynchronous_uri_scheme_protocol("m2poster", |context, request, responder| {
        if context.webview_label() != "main" {
            responder.respond(unavailable(StatusCode::FORBIDDEN));
            return;
        }
        if parse(&request).is_none() {
            responder.respond(unavailable(StatusCode::BAD_REQUEST));
            return;
        }
        let Some(state) = context.app_handle().try_state::<AppState>() else {
            responder.respond(unavailable(StatusCode::SERVICE_UNAVAILABLE));
            return;
        };
        if let Some(raster) = memory_hit(&resource_key(&state.database, &request)) {
            responder.respond(raster_response(&request, &raster));
            return;
        }
        let database = state.database.clone();
        let root = state.default_cover_cache_dir.clone();
        tauri::async_runtime::spawn(async move {
            // Four bounded cold/disk transports; RAM hits bypass this work, generation stays single.
            // Waiting does not occupy an event-thread or blocking-worker slot.
            let slots = SLOTS.get_or_init(|| {
                (0..4)
                    .map(|_| Slot {
                        gate: tauri::async_runtime::Mutex::new(()),
                        reader: Mutex::new(None),
                    })
                    .collect()
            });
            let slot = &slots[NEXT.fetch_add(1, Ordering::Relaxed) % 4];
            let _permit = slot.gate.lock().await;
            let response = tauri::async_runtime::spawn_blocking(move || {
                let mut state = slot.reader.lock().unwrap_or_else(|p| p.into_inner());
                let epoch = RESOURCE_EPOCH.load(Ordering::Acquire);
                if state
                    .as_ref()
                    .is_none_or(|(path, previous, _)| path != database.path() || *previous != epoch)
                {
                    *state = poster_cache::PreparedPosterReader::new(&database, &root)
                        .ok()
                        .map(|reader| (database.path().into(), epoch, reader));
                }
                serve_with_reader(
                    &database,
                    &root,
                    request,
                    state.as_mut().map(|(_, _, reader)| reader),
                )
            })
            .await
            .unwrap_or_else(|_| unavailable(StatusCode::INTERNAL_SERVER_ERROR));
            responder.respond(response);
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    static TEST_CACHE: Mutex<()> = Mutex::new(());
    fn fixture() -> (
        tempfile::TempDir,
        Database,
        std::path::PathBuf,
        i64,
        std::path::PathBuf,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("library");
        let root = temp.path().join("cache");
        std::fs::create_dir(&library).unwrap();
        cache::ensure_directories(&root).unwrap();
        let database = Database::new(temp.path().join("index.db"));
        database.migrate().unwrap();
        let library = database.add_root(&library, None).unwrap();
        let c = database.connect().unwrap();
        c.execute("INSERT INTO nodes(library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(?1,?2,'Root','Root','CONTAINER')",rusqlite::params![library.id,library.path]).unwrap();
        let parent = c.last_insert_rowid();
        c.execute("INSERT INTO nodes(library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type) VALUES(?1,?2,?3,'Work','Work','WORK')",rusqlite::params![library.id,parent,std::path::Path::new(&library.path).join("Work").to_string_lossy()]).unwrap();
        let node = c.last_insert_rowid();
        let original = root.join("manual").join("prepared.png");
        image::RgbImage::from_pixel(300, 450, image::Rgb([50, 80, 120]))
            .save(&original)
            .unwrap();
        database
            .set_node_cover(node, crate::models::CoverSource::Manual, Some(&original))
            .unwrap();
        (temp, database, root, node, original)
    }
    #[test]
    fn serves_validated_binary_tiers_and_http_cache_receipts_without_regeneration() {
        let _test = TEST_CACHE.lock().unwrap_or_else(|p| p.into_inner());
        let (_temp, db, root, node, original) = fixture();
        let original_bytes = std::fs::read(&original).unwrap();
        let path = format!("/node/{node}/256?v=0123456789abcdef");
        let first = serve(&db, &root, request(&path));
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(first.headers()["Content-Type"], "image/jpeg");
        assert_eq!(first.headers()["X-Content-Type-Options"], "nosniff");
        assert!(first.headers()["Cache-Control"]
            .to_str()
            .unwrap()
            .contains("immutable"));
        assert_eq!(&first.body()[..2], &[255, 216]);
        let pixels = image::load_from_memory(first.body()).unwrap();
        assert!(pixels.width() <= 256);
        let etag = first.headers()["ETag"].clone();
        let mut conditional = request(&path);
        conditional
            .headers_mut()
            .insert("If-None-Match", etag.clone());
        let second = serve(&db, &root, conditional);
        assert_eq!(second.status(), StatusCode::NOT_MODIFIED);
        assert!(second.body().is_empty());
        let mut head = request(&path);
        *head.method_mut() = tauri::http::Method::HEAD;
        let head = serve(&db, &root, head);
        assert_eq!(head.status(), StatusCode::OK);
        assert!(head.body().is_empty());
        assert_eq!(head.headers()["ETag"], etag);
        assert_eq!(serve(&db, &root, request(&path)).body(), first.body());
        assert_eq!(std::fs::read(&original).unwrap(), original_bytes);
        assert_eq!(std::fs::read_dir(root.join("posters")).unwrap().count(), 1);
    }
    #[test]
    fn never_serves_media_root_paths_and_does_not_cache_failures() {
        let _test = TEST_CACHE.lock().unwrap_or_else(|p| p.into_inner());
        let (temp, db, root, node, _original) = fixture();
        let media = temp.path().join("library").join("private.png");
        image::RgbImage::new(10, 10).save(&media).unwrap();
        db.connect()
            .unwrap()
            .execute(
                "UPDATE nodes SET cover_cache_path=?1 WHERE id=?2",
                rusqlite::params![media.to_string_lossy(), node],
            )
            .unwrap();
        for path in [
            format!("/node/{node}/256?v=0123456789abcdef"),
            "/node/999999/256?v=0123456789abcdef".into(),
        ] {
            let response = serve(&db, &root, request(&path));
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(response.headers()["Cache-Control"], "no-store");
            assert!(response.body().is_empty());
        }
        let invalid = serve(&db, &root, request("/node/../256?v=0123456789abcdef"));
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        assert_eq!(invalid.headers()["Cache-Control"], "no-store");
    }
    #[test]
    fn encoded_budget_eviction_keeps_a_checked_file_fast_path_and_clear_fences_old_reads() {
        let _test = TEST_CACHE.lock().unwrap_or_else(|p| p.into_inner());
        clear_memory();
        let (_temp, db, root, node, _original) = fixture();
        let path = format!("/node/{node}/256?v=abcdef0123456789");
        let first = serve(&db, &root, request(&path));
        assert_eq!(first.status(), StatusCode::OK);
        let key = resource_key(&db, &request(&path));
        let epoch = RESOURCE_EPOCH.load(Ordering::Acquire);
        // Test-only encoded pressure; production inserts only validated raster payloads.
        for i in 0..18 {
            remember(
                format!("pressure-{i}"),
                PosterPayload {
                    bytes: vec![0; 2 * 1024 * 1024],
                    mime: "image/jpeg",
                    prepared: None,
                },
                epoch,
            );
        }
        assert!(memory_hit(&key).is_none());
        assert!(file_hit(&key).is_some());
        {
            let cache = resources().lock().unwrap();
            assert!(cache.bytes <= MAX_ENCODED_BYTES);
            assert!(cache.images.len() <= MAX_RESOURCE_ENTRIES);
        }
        // The exact previous asset lease remains immutable; this does not reopen its source.
        db.connect()
            .unwrap()
            .execute(
                "UPDATE nodes SET cover_cache_path='missing-source' WHERE id=?1",
                [node],
            )
            .unwrap();
        let returned = serve(&db, &root, request(&path));
        assert_eq!(returned.status(), StatusCode::OK);
        assert_eq!(returned.body(), first.body());
        clear_memory();
        let raster = PosterPayload::new(first.body().clone()).unwrap();
        remember(key.clone(), raster, epoch);
        assert!(memory_hit(&key).is_none());
        assert!(file_hit(&key).is_none());
    }
    fn request(path: &str) -> Request<Vec<u8>> {
        Request::builder()
            .uri(format!("http://m2poster.localhost{path}"))
            .body(Vec::new())
            .unwrap()
    }
    #[test]
    fn accepts_only_canonical_indexed_raster_targets() {
        assert_eq!(
            parse(&request("/node/3/256?v=0123456789abcdef")),
            Some(Resource {
                target: Target::Node(3),
                width: 256
            })
        );
        assert_eq!(
            parse(&request(
                "/book/4/384?v=0123456789abcdef&revision=revision-a"
            )),
            Some(Resource {
                target: Target::Book(4, "revision-a".into()),
                width: 384
            })
        );
        for path in [
            "/node/0/256?v=0123456789abcdef",
            "/node/03/256?v=0123456789abcdef",
            "/node/3/300?v=0123456789abcdef",
            "/node/3/256",
            "/book/3/256?v=0123456789abcdef",
            "/node/3/256?v=0123456789abcdef&path=C:/Windows",
            "/node/3/256?v=0123456789abcdef&v=0123456789abcdef",
            "/node/../256?v=0123456789abcdef",
            "/node/%2e%2e/256?v=0123456789abcdef",
        ] {
            assert!(parse(&request(path)).is_none(), "{path}");
        }
        assert!(parse(
            &Request::builder()
                .method("POST")
                .uri("http://m2poster.localhost/node/3/256?v=0123456789abcdef")
                .body(Vec::new())
                .unwrap()
        )
        .is_none());
        assert!(parse(
            &Request::builder()
                .uri("http://evil.test/node/3/256?v=0123456789abcdef")
                .body(Vec::new())
                .unwrap()
        )
        .is_none());
    }
}

//! Movie-only provider. Official hosts, bounded responses and separate provider identity.
use crate::{
    cache,
    db::{AppResult, Database},
    models::{LibraryMediaKind, MediaNode, NodeType},
};
use reqwest::{
    blocking::Client,
    header::{CONTENT_LENGTH, RETRY_AFTER},
    StatusCode,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const BASE: &str = "https://api.themoviedb.org/3";
static MUTATION_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn cancel_mutation() {
    MUTATION_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}
const JSON_LIMIT: u64 = 2 * 1024 * 1024;
const POSTER_POLICY_VERSION: u8 = 1;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Movie {
    pub id: i64,
    pub title: String,
    pub original_title: String,
    pub release_date: Option<String>,
    pub overview: String,
    pub poster_path: Option<String>,
    /// Search and detail genre evidence is retained for conservative automatic movie selection.
    /// Old cached/manual bindings remain readable when this field did not exist.
    #[serde(default)]
    pub genre_ids: Option<Vec<i64>>,
    #[serde(default)]
    pub original_language: Option<String>,
    /// Compatibility flag; provider covers now share the original-language policy, including
    /// manual movie selection. Node-level MANUAL images always retain priority.
    #[serde(default)]
    pub automatic_poster: bool,
    #[serde(default)]
    pub poster_policy_version: u8,
    #[serde(default)]
    pub alternative_titles: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub movie: Movie,
    pub active: bool,
    pub cover_cache_path: Option<String>,
    pub cover_error: Option<String>,
}
#[derive(Deserialize)]
struct ApiMovie {
    id: i64,
    title: String,
    original_title: String,
    release_date: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    #[serde(default)]
    genre_ids: Option<Vec<i64>>,
    #[serde(default)]
    genres: Option<Vec<ApiGenre>>,
    #[serde(default)]
    original_language: Option<String>,
    #[serde(default)]
    alternative_titles: Option<ApiAlternativeTitles>,
}
#[derive(Deserialize)]
struct ApiAlternativeTitles {
    #[serde(default)]
    titles: Vec<ApiAlternativeTitle>,
}
#[derive(Deserialize)]
struct ApiAlternativeTitle {
    title: String,
}
#[derive(Deserialize)]
struct ApiGenre {
    id: i64,
}
#[derive(Deserialize)]
struct SearchResponse {
    results: Vec<ApiMovie>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Search {
    pub movies: Vec<Movie>,
    pub snapshot: String,
}
type Cached = (String, Instant, Vec<Movie>);
static CACHE: OnceLock<Mutex<VecDeque<Cached>>> = OnceLock::new();
static CLIENT: OnceLock<Mutex<Option<Client>>> = OnceLock::new();
fn build_client() -> AppResult<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(7))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("M2Shelf movie metadata")
        .build()
        .map_err(|_| "TMDB_CLIENT_FAILED".into())
}
fn client() -> AppResult<Client> {
    let cell = CLIENT.get_or_init(|| Mutex::new(None));
    let mut value = cell.lock().map_err(|_| "TMDB_CLIENT_FAILED")?;
    if value.is_none() {
        *value = Some(build_client()?);
    }
    value
        .as_ref()
        .cloned()
        .ok_or_else(|| "TMDB_CLIENT_FAILED".into())
}
fn refresh() -> AppResult<Client> {
    let value = build_client()?;
    if let Some(cell) = CLIENT.get() {
        *cell.lock().map_err(|_| "TMDB_CLIENT_FAILED")? = Some(value.clone());
    }
    Ok(value)
}
fn locale(value: &str) -> AppResult<&str> {
    match value {
        "zh-CN" | "en-US" | "ja-JP" | "ko-KR" => Ok(value),
        _ => Err("TMDB_LANGUAGE_INVALID".into()),
    }
}
fn clean(value: String, max: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(max)
        .collect()
}
fn movie(m: ApiMovie) -> AppResult<Movie> {
    if m.id <= 0 || m.title.is_empty() {
        return Err("TMDB_INVALID_RESPONSE".into());
    }
    let poster_path = m.poster_path.filter(|s| valid_poster(s));
    Ok(Movie {
        id: m.id,
        title: clean(m.title, 500),
        original_title: clean(m.original_title, 500),
        release_date: m.release_date.map(|s| clean(s, 16)),
        overview: clean(m.overview.unwrap_or_default(), 16_000),
        poster_path,
        genre_ids: m
            .genre_ids
            .or_else(|| m.genres.map(|g| g.into_iter().map(|g| g.id).collect())),
        original_language: m.original_language.filter(|s| valid_image_language(s)),
        automatic_poster: false,
        poster_policy_version: 0,
        alternative_titles: m
            .alternative_titles
            .into_iter()
            .flat_map(|a| a.titles)
            .take(128)
            .map(|a| clean(a.title, 300))
            .filter(|a| !a.is_empty())
            .collect(),
    })
}
fn valid_poster(value: &str) -> bool {
    value.len() < 240
        && value.starts_with('/')
        && !value[1..].contains('/')
        && value[1..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && value.ends_with(".jpg")
}
fn request_json<T: serde::de::DeserializeOwned>(
    path: &str,
    query: &[(&str, String)],
) -> AppResult<T> {
    let token = crate::tmdb_credentials::read()?;
    request_json_with_token(path, query, &token)
}
fn request_json_with_token<T: serde::de::DeserializeOwned>(
    path: &str,
    query: &[(&str, String)],
    token: &str,
) -> AppResult<T> {
    request_at(&format!("{BASE}{path}"), query, token, client()?)
}
fn request_at<T: serde::de::DeserializeOwned>(
    endpoint: &str,
    query: &[(&str, String)],
    token: &str,
    mut active: Client,
) -> AppResult<T> {
    for attempt in 0..2 {
        let mut request = active.get(endpoint).query(query);
        if token.len() == 32 && token.bytes().all(|b| b.is_ascii_hexdigit()) {
            request = request.query(&[("api_key", token)]);
        } else {
            request = request.bearer_auth(token);
        }
        let response = request.send();
        let mut response = match response {
            Ok(value) => value,
            Err(e) => {
                if attempt == 0 && (e.is_connect() || e.is_timeout()) {
                    active = refresh()?;
                    continue;
                }
                return Err("TMDB_NETWORK_FAILED".into());
            }
        };
        let status = response.status();
        if !status.is_success() {
            if attempt == 0 && (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
            {
                let seconds = response
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(1)
                    .min(2);
                std::thread::sleep(Duration::from_secs(seconds));
                continue;
            }
            return Err(format!("TMDB_HTTP_{}", status.as_u16()));
        }
        if response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .is_some_and(|size| size > JSON_LIMIT)
        {
            return Err("TMDB_RESPONSE_LIMIT".into());
        }
        let mut bytes = Vec::new();
        response
            .by_ref()
            .take(JSON_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "TMDB_READ_FAILED")?;
        if bytes.len() as u64 > JSON_LIMIT {
            return Err("TMDB_RESPONSE_LIMIT".into());
        }
        return serde_json::from_slice(&bytes).map_err(|_| "TMDB_INVALID_RESPONSE".into());
    }
    Err("TMDB_NETWORK_FAILED".into())
}
fn scope(node: &MediaNode) -> AppResult<()> {
    if !matches!(
        node.media_kind,
        LibraryMediaKind::LiveAction | LibraryMediaKind::Video
    ) || !matches!(node.node_type, NodeType::Work | NodeType::AutoWork)
        || node.total_video_count <= 0
        || node
            .binding
            .as_ref()
            .is_some_and(|b| b.provider_subject_type == 2)
    {
        return Err("TMDB_REQUIRES_MOVIE_WORK".into());
    }
    Ok(())
}
pub fn binding(c: &Connection, node: i64) -> AppResult<Option<Binding>> {
    let row:Option<(String,bool,Option<String>,Option<String>)>=c.query_row("SELECT payload_json,active,cover_cache_path,cover_error FROM tmdb_movie_bindings WHERE node_id=?1",[node],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|e.to_string())?;
    row.map(|(json, active, cover_cache_path, cover_error)| {
        Ok(Binding {
            movie: serde_json::from_str(&json).map_err(|_| "TMDB_INVALID_CACHE")?,
            active,
            cover_cache_path,
            cover_error,
        })
    })
    .transpose()
}
pub fn hydrate(c: &Connection, nodes: &mut [MediaNode]) -> AppResult<()> {
    if nodes.is_empty()
        || !c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='tmdb_movie_bindings')",
                [],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| e.to_string())?
    {
        return Ok(());
    }
    let positions = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, i))
        .collect::<std::collections::HashMap<_, _>>();
    let ids = nodes.iter().map(|n| n.id).collect::<Vec<_>>();
    for chunk in ids.chunks(500) {
        let mut stmt=c.prepare(&format!("SELECT node_id,payload_json,active,cover_cache_path,cover_error FROM tmdb_movie_bindings WHERE node_id IN ({})",vec!["?";chunk.len()].join(","))).map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(chunk), |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, json, active, cover_cache_path, cover_error) =
                row.map_err(|e| e.to_string())?;
            let binding = Binding {
                movie: serde_json::from_str(&json).map_err(|_| "TMDB_INVALID_CACHE")?,
                active,
                cover_cache_path,
                cover_error,
            };
            let node = &mut nodes[positions[&id]];
            if active && node.cover_source != crate::models::CoverSource::Manual {
                if let Some(path) = &binding.cover_cache_path {
                    node.cover_cache_path = Some(path.clone());
                }
            }
            node.tmdb_binding = Some(binding);
        }
    }
    Ok(())
}
fn fingerprint(c: &Connection, node: i64) -> AppResult<String> {
    let value:(String,String,String,Option<i64>,Option<i64>,Option<i64>,i64)=c.query_row("SELECT n.absolute_path,n.updated_at,n.node_type,b.provider_subject_id,t.movie_id,t.active,o.revision FROM nodes n LEFT JOIN metadata_bindings b ON b.node_id=n.id AND b.provider='BANGUMI' LEFT JOIN tmdb_movie_bindings t ON t.node_id=n.id JOIN book_organization_state o ON o.root_id=n.library_root_id WHERE n.id=?1",[node],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(|_|"TMDB_TARGET_STALE")?;
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(node, value)).map_err(|_| "TMDB_TARGET_STALE")?)
    ))
}
pub fn search(
    db: &Database,
    node: i64,
    query: &str,
    year: Option<u16>,
    language: &str,
) -> AppResult<Search> {
    scope(&db.get_node(node)?)?;
    let language = locale(language)?;
    let query = query.trim();
    if query.is_empty()
        || query.chars().count() > 200
        || year.is_some_and(|y| !(1800..=2200).contains(&y))
    {
        return Err("TMDB_QUERY_INVALID".into());
    }
    let snapshot = fingerprint(&db.connect()?, node)?;
    let key = format!("search:{language}:{year:?}:{query}");
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    if let Some(movies) = cache
        .lock()
        .map_err(|_| "TMDB_CACHE_FAILED")?
        .iter()
        .find(|(k, time, _)| k == &key && time.elapsed() < Duration::from_secs(900))
        .map(|x| x.2.clone())
    {
        return Ok(Search { movies, snapshot });
    }
    let args = movie_search_arguments(query, year, language);
    let response: SearchResponse = request_json("/search/movie", &args)?;
    let movies = response
        .results
        .into_iter()
        .take(20)
        .map(movie)
        .collect::<AppResult<Vec<_>>>()?;
    let mut entries = cache.lock().map_err(|_| "TMDB_CACHE_FAILED")?;
    entries.push_back((key, Instant::now(), movies.clone()));
    while entries.len() > 64 {
        entries.pop_front();
    }
    Ok(Search { movies, snapshot })
}

fn movie_search_arguments(
    query: &str,
    year: Option<u16>,
    language: &str,
) -> Vec<(&'static str, String)> {
    let mut args = vec![
        ("query", query.to_string()),
        ("language", language.into()),
        ("include_adult", "false".into()),
        ("page", "1".into()),
    ];
    if let Some(year) = year {
        args.push(("primary_release_year", year.to_string()));
    }
    args
}
pub fn detail(id: i64, language: &str) -> AppResult<Movie> {
    if id <= 0 {
        return Err("TMDB_INVALID_ID".into());
    }
    let language = locale(language)?;
    let key = format!("detail:{language}:{id}");
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    if let Some(movie) = cache
        .lock()
        .map_err(|_| "TMDB_CACHE_FAILED")?
        .iter()
        .find(|(k, t, _)| k == &key && t.elapsed() < Duration::from_secs(900))
        .and_then(|x| x.2.first().cloned())
    {
        return Ok(movie);
    }
    let result = movie(request_json(
        &format!("/movie/{id}"),
        &[
            ("language", language.into()),
            ("append_to_response", "alternative_titles".into()),
        ],
    )?)?;
    let mut entries = cache.lock().map_err(|_| "TMDB_CACHE_FAILED")?;
    entries.push_back((key, Instant::now(), vec![result.clone()]));
    while entries.len() > 64 {
        entries.pop_front();
    }
    Ok(result)
}
#[derive(Deserialize)]
struct MovieImages {
    #[serde(default)]
    posters: Vec<MoviePoster>,
}
#[derive(Deserialize)]
struct MoviePoster {
    file_path: String,
    iso_639_1: Option<String>,
    width: u32,
    height: u32,
    #[serde(default)]
    vote_average: f64,
    #[serde(default)]
    vote_count: u32,
}
fn valid_image_language(value: &str) -> bool {
    value.len() == 2 && value.bytes().all(|c| c.is_ascii_lowercase())
}
fn preferred_poster(images: &MovieImages, original: Option<&str>) -> Option<String> {
    images
        .posters
        .iter()
        .take(1000)
        .filter(|p| {
            valid_poster(&p.file_path)
                && (100..=30000).contains(&p.width)
                && (100..=30000).contains(&p.height)
                && p.height > p.width
                && p.iso_639_1.as_deref().is_none_or(valid_image_language)
        })
        .max_by(|a, b| {
            let rank = |p: &MoviePoster| {
                (
                    if original.is_some() && p.iso_639_1.as_deref() == original {
                        5
                    } else if matches!(original, Some("zh" | "cn"))
                        && matches!(p.iso_639_1.as_deref(), Some("zh" | "cn"))
                    {
                        4
                    } else if p.iso_639_1.is_none() {
                        3
                    } else if p.iso_639_1.as_deref() == Some("en") {
                        2
                    } else if p.iso_639_1.as_deref() != Some("zh") {
                        1
                    } else {
                        0
                    },
                    p.width >= 500,
                    p.vote_count.min(10000),
                )
            };
            rank(a)
                .cmp(&rank(b))
                .then_with(|| {
                    a.vote_average
                        .clamp(0.0, 10.0)
                        .total_cmp(&b.vote_average.clamp(0.0, 10.0))
                })
                .then_with(|| a.width.min(2000).cmp(&b.width.min(2000)))
                .then_with(|| b.file_path.cmp(&a.file_path))
        })
        .map(|p| p.file_path.clone())
}
fn automatic_poster_path(movie: &Movie) -> AppResult<Option<String>> {
    automatic_poster_path_using(movie, &mut |path, args| request_json(path, args))
}
fn automatic_poster_path_using<F>(movie: &Movie, request: &mut F) -> AppResult<Option<String>>
where
    F: FnMut(&str, &[(&'static str, String)]) -> AppResult<serde_json::Value>,
{
    let mut original = movie.original_language.clone();
    if original.is_none() {
        let metadata: ApiMovie =
            serde_json::from_value(request(&format!("/movie/{}", movie.id), &[])?)
                .map_err(|_| "TMDB_INVALID_RESPONSE")?;
        if metadata.id != movie.id {
            return Err("TMDB_INVALID_RESPONSE".into());
        }
        original = metadata
            .original_language
            .filter(|s| valid_image_language(s));
    }
    // Images expose language, not proven release-country ownership. No UI locale filter.
    let images = request(&format!("/movie/{}/images", movie.id), &[]).and_then(|value| {
        serde_json::from_value::<MovieImages>(value)
            .map_err(|_| "TMDB_INVALID_RESPONSE".to_string())
    });
    if let Ok(images) = &images {
        if let Some(path) = preferred_poster(images, original.as_deref()) {
            return Ok(Some(path));
        }
    }
    // Fall back to the provider's original-language default (or locale-neutral request when
    // original language is absent), never the poster selected for the application's locale.
    let args = original
        .as_ref()
        .filter(|s| valid_image_language(s))
        .map(|s| vec![("language", s.clone())])
        .unwrap_or_default();
    let fallback: ApiMovie =
        serde_json::from_value(request(&format!("/movie/{}", movie.id), &args)?)
            .map_err(|_| "TMDB_INVALID_RESPONSE")?;
    if fallback.id != movie.id {
        return Err("TMDB_INVALID_RESPONSE".into());
    }
    Ok(fallback.poster_path.filter(|s| valid_poster(s)))
}
fn download_cover(root: &Path, movie: &Movie) -> AppResult<Option<PathBuf>> {
    // Selecting a movie chooses its metadata, not a language-specific image. File-picked
    // MANUAL covers remain authoritative; every provider cover uses the original language.
    let selected = automatic_poster_path(movie)?;
    let Some(path) = selected.as_ref() else {
        return Ok(None);
    };
    if !valid_poster(path) {
        return Err("TMDB_INVALID_POSTER".into());
    }
    let url = format!("https://image.tmdb.org/t/p/w500{path}");
    let mut response = client()?
        .get(&url)
        .send()
        .map_err(|_| "TMDB_COVER_NETWORK")?;
    if !response.status().is_success() {
        return Err("TMDB_COVER_HTTP".into());
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(15 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "TMDB_COVER_READ")?;
    if bytes.len() > 15 * 1024 * 1024 {
        return Err("TMDB_COVER_LIMIT".into());
    }
    store_downloaded_cover(root, movie, &url, &bytes).map(Some)
}

fn store_downloaded_cover(
    root: &Path,
    movie: &Movie,
    url: &str,
    bytes: &[u8],
) -> AppResult<PathBuf> {
    cache::validate_cover_payload(bytes)?;
    cache::ensure_existing_custom_cache(root)?;
    // Namespace-specific filename in the existing owned directory; no provider ID collision.
    let hash = format!("{:x}", Sha256::digest(url.as_bytes()));
    let destination = root
        .join("bangumi")
        .join(format!("tmdb-{}-{}.jpg", movie.id, &hash[..16]));
    let (mut pending, mut file) =
        cache::create_pending_cache_file(&destination, false).map_err(|_| "TMDB_CACHE_WRITE")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "TMDB_CACHE_WRITE")?;
    drop(file);
    pending
        .commit_to(&destination)
        .map_err(|_| "TMDB_CACHE_WRITE")?;
    Ok(destination)
}
pub fn bind(
    db: &Database,
    node: i64,
    id: i64,
    language: &str,
    expected: &str,
    root: &Path,
) -> AppResult<Binding> {
    let generation = MUTATION_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    scope(&db.get_node(node)?)?;
    if fingerprint(&db.connect()?, node)? != expected {
        return Err("TMDB_TARGET_STALE".into());
    }
    let mut movie = detail(id, language)?;
    movie.automatic_poster = true;
    let operation = cache::begin_cover_cache_operation();
    let (path, error) =
        match if db.get_node(node)?.cover_source == crate::models::CoverSource::Manual {
            Ok(None)
        } else {
            download_cover(root, &movie)
        } {
            Ok(path) => (path, None),
            Err(e) => (None, Some(e)),
        };
    if error.is_none() {
        movie.poster_policy_version = POSTER_POLICY_VERSION;
    }
    let saved = (|| {
        let mut c = db.connect()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        scope(&crate::db::get_node_conn(&tx, node)?)?;
        if fingerprint(&tx, node)? != expected {
            return Err("TMDB_TARGET_STALE".into());
        }
        if MUTATION_GENERATION.load(std::sync::atomic::Ordering::SeqCst) != generation {
            return Err("TMDB_CANCELLED".into());
        }
        // Existing Bangumi rows and manual Node covers are deliberately retained.
        tx.execute("INSERT INTO tmdb_movie_bindings(node_id,movie_id,payload_json,cover_cache_path,cover_error) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(node_id) DO UPDATE SET movie_id=excluded.movie_id,payload_json=excluded.payload_json,cover_cache_path=COALESCE(excluded.cover_cache_path,tmdb_movie_bindings.cover_cache_path),cover_error=excluded.cover_error,active=1,bound_at=CURRENT_TIMESTAMP",params![node,id,serde_json::to_string(&movie).map_err(|_|"TMDB_INVALID_RESPONSE")?,path.as_ref().map(|p|p.to_string_lossy().into_owned()),error]).map_err(|e|e.to_string())?;
        let result = binding(&tx, node)?.ok_or("TMDB_SAVE_FAILED")?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(result)
    })();
    if saved.is_err() {
        if let Some(path) = &path {
            if db.cover_path_reference_count(path).ok() == Some(0) {
                let _ = cache::remove_cached_file(&operation, path, root);
            }
        }
    }
    saved
}
pub fn retry_cover(db: &Database, node: i64, root: &Path) -> AppResult<Binding> {
    retry_cover_using(
        db,
        node,
        root,
        &mut NativeAutomaticProvider,
        &|| false,
        false,
    )
}
fn retry_cover_using<P: AutomaticProvider, C: Fn() -> bool>(
    db: &Database,
    node: i64,
    root: &Path,
    provider: &mut P,
    cancelled: &C,
    automatic: bool,
) -> AppResult<Binding> {
    let generation = MUTATION_GENERATION.load(std::sync::atomic::Ordering::SeqCst);
    let target = db.get_node(node)?;
    scope(&target)?;
    let expected = fingerprint(&db.connect()?, node)?;
    let previous = binding(&db.connect()?, node)?
        .filter(|b| b.active)
        .ok_or("TMDB_NO_ACTIVE_BINDING")?;
    if target.cover_source == crate::models::CoverSource::Manual {
        return Ok(previous);
    }
    if cancelled() {
        return Err("TMDB_CANCELLED".into());
    }
    let mut movie = previous.movie.clone();
    let operation = cache::begin_cover_cache_operation();
    let download = (|| {
        if movie.original_language.is_none() {
            let current = provider.detail(movie.id, "en-US")?;
            if current.id != movie.id {
                return Err("TMDB_INVALID_RESPONSE".into());
            }
            movie.original_language = current.original_language;
        }
        movie.automatic_poster = true;
        if cancelled() {
            return Err("TMDB_CANCELLED".into());
        }
        provider.cover(root, &movie)
    })();
    let mut c = db.connect()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    if cancelled()
        || automatic && !cover_refresh_allowed(&tx, node)?
        || fingerprint(&tx, node)? != expected
        || crate::db::get_node_conn(&tx, node)?.cover_source == crate::models::CoverSource::Manual
        || binding(&tx, node)?.as_ref() != Some(&previous)
        || MUTATION_GENERATION.load(std::sync::atomic::Ordering::SeqCst) != generation
    {
        if let Ok(Some(path)) = download {
            if db.cover_path_reference_count(&path).ok() == Some(0) {
                let _ = cache::remove_cached_file(&operation, &path, root);
            }
        }
        return Err("TMDB_TARGET_STALE".into());
    }
    let (path, error) = match download {
        Ok(path) => (path.map(|p| p.to_string_lossy().into_owned()), None),
        Err(e) => (None, Some(e)),
    };
    if error.is_none() {
        movie.poster_policy_version = POSTER_POLICY_VERSION;
    }
    tx.execute("UPDATE tmdb_movie_bindings SET cover_cache_path=COALESCE(?2,cover_cache_path),cover_error=?3,payload_json=?4 WHERE node_id=?1",params![node,path,error,serde_json::to_string(&movie).map_err(|_|"TMDB_INVALID_RESPONSE")?]).map_err(|e|e.to_string())?;
    let result = binding(&tx, node)?.ok_or("TMDB_SAVE_FAILED")?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}
pub(crate) fn needs_cover_refresh(node: &MediaNode, bound: &Binding) -> bool {
    bound.active
        && node.cover_source != crate::models::CoverSource::Manual
        && (bound.movie.poster_policy_version < POSTER_POLICY_VERSION
            || bound
                .cover_cache_path
                .as_deref()
                .is_some_and(|p| !cache::cached_cover_is_valid(Path::new(p)))
            || bound.cover_error.is_some())
}
fn cover_refresh_allowed(c: &Connection, node: i64) -> AppResult<bool> {
    c.query_row("SELECT r.auto_bangumi AND COALESCE((SELECT value<>'false' FROM settings WHERE key='bangumi_search_enabled'),1) AND NOT EXISTS (WITH RECURSIVE a(id,parent_node_id,node_type) AS (SELECT id,parent_node_id,node_type FROM nodes WHERE id=?1 UNION ALL SELECT n.id,n.parent_node_id,n.node_type FROM nodes n JOIN a ON n.id=a.parent_node_id) SELECT 1 FROM a WHERE node_type='IGNORED') FROM nodes n JOIN library_roots r ON r.id=n.library_root_id WHERE n.id=?1",[node],|r|r.get(0)).map_err(|e|e.to_string())
}
pub fn deactivate(db: &Database, node: i64) -> AppResult<()> {
    cancel_mutation();
    db.connect()?
        .execute(
            "UPDATE tmdb_movie_bindings SET active=0 WHERE node_id=?1",
            [node],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}
pub fn invalidate_search_cache() {
    if let Some(cache) = CACHE.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.clear();
        }
    }
}

const MAX_AUTOMATIC_SEARCHES: usize = 128;
static AUTOMATIC_GATE: Mutex<()> = Mutex::new(());
#[cfg(test)]
pub(crate) static MOVIE_TEST_GATE: Mutex<()> = Mutex::new(());

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDiagnostic {
    pub node_id: i64,
    pub query: String,
    pub year: Option<i32>,
    pub outcome: String,
    pub updated_at: String,
}
pub fn diagnostics(db: &Database) -> AppResult<Vec<MatchDiagnostic>> {
    let c = db.connect()?;
    let raw: Option<String> = c
        .query_row(
            "SELECT value FROM settings WHERE key='tmdb_auto_diagnostics' AND length(value)<=65536",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(raw
        .and_then(|v| serde_json::from_str::<Vec<MatchDiagnostic>>(&v).ok())
        .unwrap_or_default()
        .into_iter()
        .take(128)
        .collect())
}
pub(crate) fn diagnostic(
    db: &Database,
    node: &MediaNode,
    evidence: &crate::title_extractor::MatchEvidence,
    outcome: &str,
) {
    // Bounded application data, separate from complete AppSettings snapshots. Never store paths,
    // raw transport errors or credentials. A new attempt replaces this Node's previous reason.
    let _ = (|| -> AppResult<()> {
        let mut c = db.connect()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let raw:Option<String>=tx.query_row("SELECT value FROM settings WHERE key='tmdb_auto_diagnostics' AND length(value)<=65536",[],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
        let mut rows = raw
            .and_then(|v| serde_json::from_str::<Vec<MatchDiagnostic>>(&v).ok())
            .unwrap_or_default();
        rows.retain(|r| r.node_id != node.id);
        rows.truncate(127);
        rows.insert(
            0,
            MatchDiagnostic {
                node_id: node.id,
                query: evidence
                    .primary_title
                    .chars()
                    .filter(|c| !c.is_control() && !matches!(c, '/' | '\\'))
                    .take(200)
                    .collect(),
                year: evidence.year.filter(|_| evidence.year_is_strong),
                outcome: outcome.into(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        );
        let value = serde_json::to_string(&rows).map_err(|e| e.to_string())?;
        if value.len() <= 65536 {
            tx.execute("INSERT INTO settings(key,value) VALUES('tmdb_auto_diagnostics',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[value]).map_err(|e|e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    })();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutomaticOutcome {
    Matched,
    MatchedWithCoverError,
    Unmatched,
    Cancelled,
    Stale,
}

/// One coordinator per matching run. A provider-wide failure disables only this optional
/// provider for the rest of the run; it never aborts scanning or prompts for credentials.
#[derive(Default)]
pub(crate) struct AutomaticRun {
    searches: usize,
    details: usize,
    cover_refreshes: usize,
    disabled: bool,
    credential_checked: bool,
    disabled_reason: Option<&'static str>,
}

pub(crate) trait AutomaticProvider {
    fn available(&mut self) -> bool;
    fn search(
        &mut self,
        db: &Database,
        node: i64,
        query: &str,
        year: Option<u16>,
        language: &str,
    ) -> AppResult<Vec<Movie>>;
    fn detail(&mut self, id: i64, language: &str) -> AppResult<Movie>;
    fn cover(&mut self, root: &Path, movie: &Movie) -> AppResult<Option<PathBuf>>;
}

pub(crate) struct NativeAutomaticProvider;
#[cfg(test)]
pub(crate) fn fixture_exhausted_details() -> AutomaticRun {
    AutomaticRun {
        details: 256,
        ..Default::default()
    }
}
impl AutomaticProvider for NativeAutomaticProvider {
    fn available(&mut self) -> bool {
        crate::tmdb_credentials::read().is_ok()
    }
    fn search(
        &mut self,
        db: &Database,
        node: i64,
        query: &str,
        year: Option<u16>,
        language: &str,
    ) -> AppResult<Vec<Movie>> {
        search(db, node, query, year, language).map(|s| s.movies)
    }
    fn detail(&mut self, id: i64, language: &str) -> AppResult<Movie> {
        let mut movie = detail(id, language)?;
        movie.automatic_poster = true;
        Ok(movie)
    }
    fn cover(&mut self, root: &Path, movie: &Movie) -> AppResult<Option<PathBuf>> {
        download_cover(root, movie)
    }
}

pub(crate) fn automatic_movie_source(
    node: &MediaNode,
    evidence: &crate::title_extractor::MatchEvidence,
    files: &[String],
) -> bool {
    use crate::title_extractor::EditionKind;
    if node.media_kind != LibraryMediaKind::LiveAction
        || !node.node_type.is_work()
        || node.total_video_count != 1
        || files.len() != 1
        || node.parent_node_id.is_none()
        || evidence.season_number.is_some()
        || !matches!(
            evidence.edition_kind,
            EditionKind::Unknown | EditionKind::Movie
        )
    {
        return false;
    }
    static EPISODE: OnceLock<regex::Regex> = OnceLock::new();
    let episode = EPISODE.get_or_init(|| regex::Regex::new(
        r"(?i)(?:^|[^a-z0-9])(?:s\d{1,2}(?:e\d{1,3})?|e(?:p(?:isode)?)?[ ._-]*\d{1,3}|season[ ._-]*\d{1,2})(?:$|[^a-z0-9])|第[0-9一二三四五六七八九十百]+[集話话季]|纪录片|紀錄片|documentary"
    ).expect("static episode evidence pattern"));
    !std::iter::once(node.folder_name.as_str())
        .chain(std::iter::once(node.display_name.as_str()))
        .chain(files.iter().map(String::as_str))
        .any(|raw| {
            let signals = crate::title_extractor::extract_title_signals(raw);
            signals.season_number.is_some()
                || !matches!(
                    signals.edition_kind,
                    EditionKind::Unknown | EditionKind::Movie
                )
                || episode.is_match(raw)
        })
}

pub(crate) fn automatic_evidence(
    node: &MediaNode,
    evidence: &crate::title_extractor::MatchEvidence,
    files: &[String],
) -> bool {
    automatic_movie_source(node, evidence, files)
        && crate::title_extractor::is_safe_movie_query(&evidence.primary_title)
}

fn automatic_title(value: &str) -> Option<String> {
    // Evidence was already parsed at the source boundary. Parsing again would turn the
    // meaningful 2049 in "Blade Runner 2049" into a release year and truncate the query.
    let title = value.trim().to_string();
    (title.chars().count() <= 200 && crate::title_extractor::is_safe_movie_query(&title))
        .then_some(title)
}

fn movie_year_and_kind(movie: &Movie, evidence: &crate::title_extractor::MatchEvidence) -> bool {
    if !evidence.year_is_strong {
        return false;
    }
    let Some(year) = evidence.year else {
        return false;
    };
    let date_matches = movie.release_date.as_deref().is_some_and(|date| {
        date.len() == 10
            && date.as_bytes()[4] == b'-'
            && date.as_bytes()[7] == b'-'
            && date.get(..4).and_then(|y| y.parse::<i32>().ok()) == Some(year)
    });
    let live_movie = movie.genre_ids.as_ref().is_some_and(|genres| {
        !genres.is_empty()
            && genres.len() <= 32
            && !genres.iter().any(|id| matches!(id, 16 | 99 | 10770))
    });
    movie.id > 0 && date_matches && live_movie
}
fn reliable_movie(movie: &Movie, evidence: &crate::title_extractor::MatchEvidence) -> bool {
    // Only the Node's own title and official alternate names establish an exact match.
    // A generic parent directory, provider ranking or fuzzy similarity is never sufficient.
    let names = [&evidence.primary_title]
        .into_iter()
        .chain(evidence.folder_title.iter())
        .chain(evidence.frequent_file_title.iter())
        .chain(evidence.alternate_titles.iter());
    let exact = names
        .filter_map(|title| automatic_title(title))
        .any(|title| {
            let own = crate::title_extractor::normalize_title_for_match(&title);
            [movie.title.as_str(), movie.original_title.as_str()]
                .into_iter()
                .chain(movie.alternative_titles.iter().map(String::as_str))
                .any(|candidate| {
                    crate::title_extractor::normalize_title_for_match(candidate) == own
                })
        });
    movie_year_and_kind(movie, evidence) && exact
}

fn unique_movie(
    movies: &[Movie],
    evidence: &crate::title_extractor::MatchEvidence,
) -> Option<Movie> {
    // A full first page may hide another equally named result; retain manual choice instead.
    if movies.len() >= 20 {
        return None;
    }
    let mut reliable = movies.iter().filter(|m| reliable_movie(m, evidence));
    let first = reliable.next()?;
    reliable
        .all(|other| other.id == first.id)
        .then(|| first.clone())
}

pub(crate) fn automatic_snapshot(c: &Connection, node: i64) -> AppResult<Option<String>> {
    let target = crate::db::get_node_conn(c, node)?;
    if target.media_kind != LibraryMediaKind::LiveAction || !target.node_type.is_work()
        || target.total_video_count != 1 || target.binding.is_some()
        // A deactivated provider row also represents an explicit prior choice; do not undo it.
        || target.tmdb_binding.is_some()
    {
        return Ok(None);
    }
    let policy: (bool, bool) = c.query_row(
        "SELECT r.auto_bangumi, COALESCE((SELECT value<>'false' FROM settings WHERE key='bangumi_search_enabled'),1) FROM library_roots r WHERE r.id=?1",
        [target.library_root_id], |r| Ok((r.get(0)?, r.get(1)?))
    ).map_err(|e| e.to_string())?;
    if !policy.0 || !policy.1 {
        return Ok(None);
    }
    let ignored: bool = c.query_row(
        "WITH RECURSIVE a(id,parent_node_id,node_type) AS (SELECT id,parent_node_id,node_type FROM nodes WHERE id=?1 UNION ALL SELECT n.id,n.parent_node_id,n.node_type FROM nodes n JOIN a ON n.id=a.parent_node_id) SELECT EXISTS(SELECT 1 FROM a WHERE node_type='IGNORED')",
        [node], |r| r.get(0)
    ).map_err(|e| e.to_string())?;
    if ignored {
        return Ok(None);
    }
    // Existing provider fingerprint includes the Root revision, which advances for Node,
    // media-file and binding changes. Include policy and all cover/evidence fields explicitly.
    let value = (
        fingerprint(c, node)?,
        policy,
        target.folder_name,
        target.display_name,
        target.cover_source.as_db(),
        target.cover_cache_path,
        target.parent_node_id,
    );
    Ok(Some(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value).map_err(|_| "TMDB_TARGET_STALE")?)
    )))
}

impl AutomaticRun {
    pub(crate) fn refresh_bound<P: AutomaticProvider, C: Fn() -> bool>(
        &mut self,
        db: &Database,
        node: &MediaNode,
        bound: &Binding,
        root: Result<&Path, &str>,
        provider: &mut P,
        cancelled: &C,
    ) -> AppResult<AutomaticOutcome> {
        if !needs_cover_refresh(node, bound)
            || self.disabled
            || self.cover_refreshes >= MAX_AUTOMATIC_SEARCHES
        {
            return Ok(AutomaticOutcome::Stale);
        }
        if !cover_refresh_allowed(&db.connect()?, node.id)? {
            return Ok(AutomaticOutcome::Stale);
        }
        if cancelled() {
            return Ok(AutomaticOutcome::Cancelled);
        }
        if !self.credential_checked {
            self.credential_checked = true;
            if !provider.available() {
                self.record_failure("TMDB_CREDENTIALS_REQUIRED");
                return Ok(AutomaticOutcome::Stale);
            }
        }
        let _gate = AUTOMATIC_GATE.lock().unwrap_or_else(|p| p.into_inner());
        self.cover_refreshes += 1;
        let result = retry_cover_using(
            db,
            node.id,
            root.map_err(str::to_string)?,
            provider,
            cancelled,
            true,
        );
        let evidence = crate::title_extractor::build_movie_match_evidence(
            &node.folder_name,
            &node.display_name,
            &[],
            crate::models::LibraryRecognitionMode::Folder,
        );
        match result {
            Ok(binding) if binding.cover_error.is_some() => {
                self.record_failure(binding.cover_error.as_deref().unwrap());
                diagnostic(db, node, &evidence, "matched-cover-failed");
                Ok(AutomaticOutcome::MatchedWithCoverError)
            }
            Ok(_) => {
                diagnostic(db, node, &evidence, "matched");
                Ok(AutomaticOutcome::Matched)
            }
            Err(error) if cancelled() || error == "TMDB_CANCELLED" => {
                Ok(AutomaticOutcome::Cancelled)
            }
            Err(error) if error == "TMDB_TARGET_STALE" => Ok(AutomaticOutcome::Stale),
            Err(error) => {
                self.record_failure(&error);
                Err(error)
            }
        }
    }

    pub(crate) fn fallback<P: AutomaticProvider, C: Fn() -> bool>(
        &mut self,
        request: AutomaticRequest<'_>,
        provider: &mut P,
        cancelled: &C,
    ) -> AppResult<AutomaticOutcome> {
        let AutomaticRequest {
            db,
            node,
            evidence,
            files,
            root,
            expected,
        } = request;
        if self.disabled || self.searches >= MAX_AUTOMATIC_SEARCHES {
            diagnostic(
                db,
                node,
                evidence,
                self.disabled_reason.unwrap_or("budget-deferred"),
            );
            return Ok(AutomaticOutcome::Unmatched);
        }
        if !automatic_evidence(node, evidence, files) {
            diagnostic(db, node, evidence, "ineligible");
            return Ok(AutomaticOutcome::Unmatched);
        }
        if cancelled() {
            return Ok(AutomaticOutcome::Cancelled);
        }
        if automatic_snapshot(&db.connect()?, node.id)?.as_deref() != Some(expected) {
            return Ok(AutomaticOutcome::Stale);
        }
        let generation = MUTATION_GENERATION.load(std::sync::atomic::Ordering::SeqCst);
        if !self.credential_checked {
            self.credential_checked = true;
            if !provider.available() {
                self.disabled = true;
                self.disabled_reason = Some("credentials-unavailable");
                diagnostic(db, node, evidence, "credentials-unavailable");
                eprintln!("auto-match provider=TMDB outcome=credentials-unavailable");
                return Ok(AutomaticOutcome::Unmatched);
            }
        }
        let _gate = AUTOMATIC_GATE.lock().unwrap_or_else(|p| p.into_inner());
        if cancelled() {
            return Ok(AutomaticOutcome::Cancelled);
        }
        self.searches += 1;
        let language = db
            .connect()?
            .query_row("SELECT value FROM settings WHERE key='language'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "zh-CN".into());
        let language = locale(&language)?;
        let Some(query) = automatic_title(&evidence.primary_title) else {
            diagnostic(db, node, evidence, "title-uncertain");
            return Ok(AutomaticOutcome::Unmatched);
        };
        let mut request_evidence = evidence.clone();
        let mut queries = vec![query];
        for alternative in crate::auto_match::match_queries(evidence)
            .into_iter()
            .map(|v| {
                if evidence
                    .year
                    .is_some_and(|year| v == format!("{} {year}", evidence.primary_title))
                {
                    evidence.primary_title.clone()
                } else {
                    v
                }
            })
            .filter_map(|v| automatic_title(&v))
        {
            if !queries.contains(&alternative) {
                queries.push(alternative);
            }
            if queries.len() == 3 {
                break;
            }
        }
        let mut movies = Vec::new();
        let mut enriched = Vec::<Movie>::new();
        let mut selected = None;
        for (index, query) in queries.into_iter().enumerate() {
            if cancelled() {
                return Ok(AutomaticOutcome::Cancelled);
            }
            if index > 0 {
                if self.searches >= MAX_AUTOMATIC_SEARCHES {
                    diagnostic(db, node, &request_evidence, "budget-deferred");
                    return Ok(AutomaticOutcome::Unmatched);
                }
                self.searches += 1;
            }
            request_evidence.primary_title = query.clone();
            diagnostic(db, node, &request_evidence, "searching");
            movies = match provider.search(
                db,
                node.id,
                &query,
                evidence
                    .year
                    .filter(|y| evidence.year_is_strong && (1900..=2200).contains(y))
                    .map(|y| y as u16),
                language,
            ) {
                Ok(movies) => movies,
                Err(error) => {
                    self.record_failure(&error);
                    diagnostic(
                        db,
                        node,
                        &request_evidence,
                        self.disabled_reason.unwrap_or("request-failed"),
                    );
                    return Ok(AutomaticOutcome::Unmatched);
                }
            };
            if movies.len() >= 20
                || movies
                    .iter()
                    .filter(|m| reliable_movie(m, evidence))
                    .count()
                    > 1
            {
                break; // An incomplete or genuinely ambiguous pool must not be cherry-picked.
            }
            let mut eligible = Vec::new();
            for candidate in movies.iter().filter(|m| movie_year_and_kind(m, evidence)) {
                if !eligible.iter().any(|m: &&Movie| m.id == candidate.id) {
                    eligible.push(candidate);
                }
            }
            let missing = eligible
                .iter()
                .filter(|m| !enriched.iter().any(|d| d.id == m.id))
                .count();
            if enriched.len() + missing > 5 || self.details + missing > 256 {
                diagnostic(db, node, &request_evidence, "budget-deferred");
                return Ok(AutomaticOutcome::Unmatched);
            }
            for candidate in eligible {
                if cancelled() {
                    return Ok(AutomaticOutcome::Cancelled);
                }
                if enriched.iter().any(|d| d.id == candidate.id) {
                    continue;
                }
                self.details += 1;
                match provider.detail(candidate.id, language) {
                    Ok(movie) if movie.id == candidate.id => {
                        enriched.push(movie);
                    }
                    Ok(_) => {
                        diagnostic(db, node, &request_evidence, "detail-conflict");
                        return Ok(AutomaticOutcome::Unmatched);
                    }
                    Err(error) => {
                        self.record_failure(&error);
                        diagnostic(
                            db,
                            node,
                            &request_evidence,
                            self.disabled_reason.unwrap_or("request-failed"),
                        );
                        return Ok(AutomaticOutcome::Unmatched);
                    }
                }
            }
            let reliable = enriched
                .iter()
                .filter(|m| reliable_movie(m, evidence))
                .count();
            if reliable > 1 {
                break;
            }
            selected = unique_movie(&enriched, evidence);
            if selected.is_some() {
                break;
            }
            // TMDb searches its aliases but doesn't return them in search rows. Bounded detail
            // enrichment above proves an exact official alias; unrelated rows may use the next
            // local bilingual name, whereas a proven ambiguity never does.
            if !evidence.year_is_strong && !movies.is_empty() {
                break;
            }
        }
        if cancelled() {
            return Ok(AutomaticOutcome::Cancelled);
        }
        let Some(mut movie) = selected else {
            diagnostic(
                db,
                node,
                &request_evidence,
                if movies.is_empty() {
                    "no-results"
                } else if !evidence.year_is_strong {
                    "year-uncertain"
                } else {
                    "no-unique-title-year"
                },
            );
            eprintln!(
                "auto-match provider=TMDB node={} outcome=no-unique-title-year",
                node.id
            );
            return Ok(AutomaticOutcome::Unmatched);
        };
        movie.automatic_poster = true;
        if cancelled() {
            return Ok(AutomaticOutcome::Cancelled);
        }
        let operation = cache::begin_cover_cache_operation();
        // A manual cover is never downloaded over, while metadata may still bind safely.
        let cover = if node.cover_source == crate::models::CoverSource::Manual {
            Ok(None)
        } else {
            root.map_err(str::to_string)
                .and_then(|root| provider.cover(root, &movie))
        };
        let (path, error) = match cover {
            Ok(path) => (path, None),
            Err(error) => (None, Some(error)),
        };
        if error.is_none() {
            movie.poster_policy_version = POSTER_POLICY_VERSION;
        }
        let saved = (|| {
            let mut c = db.connect()?;
            let tx = c
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            if cancelled()
                || MUTATION_GENERATION.load(std::sync::atomic::Ordering::SeqCst) != generation
            {
                return Ok(AutomaticOutcome::Cancelled);
            }
            if automatic_snapshot(&tx, node.id)?.as_deref() != Some(expected) {
                return Ok(AutomaticOutcome::Stale);
            }
            tx.execute("INSERT INTO tmdb_movie_bindings(node_id,movie_id,payload_json,cover_cache_path,cover_error) VALUES(?1,?2,?3,?4,?5)",
                params![node.id, movie.id, serde_json::to_string(&movie).map_err(|_| "TMDB_INVALID_RESPONSE")?,
                    path.as_ref().map(|p| p.to_string_lossy().into_owned()), error]).map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
            diagnostic(
                db,
                node,
                &request_evidence,
                if error.is_some() {
                    "matched-cover-failed"
                } else {
                    "matched"
                },
            );
            eprintln!(
                "auto-match provider=TMDB node={} root={} outcome=matched",
                node.id, node.library_root_id
            );
            Ok(if error.is_some() {
                AutomaticOutcome::MatchedWithCoverError
            } else {
                AutomaticOutcome::Matched
            })
        })();
        if !matches!(
            saved,
            Ok(AutomaticOutcome::Matched | AutomaticOutcome::MatchedWithCoverError)
        ) {
            if let (Some(path), Ok(root)) = (path.as_ref(), root) {
                if db.cover_path_reference_count(path).ok() == Some(0) {
                    let _ = cache::remove_cached_file(&operation, path, root);
                }
            }
        }
        saved
    }

    fn record_failure(&mut self, error: &str) {
        // Emit only our fixed code class. Never print URLs, queries, transport errors or tokens.
        let code = match error {
            "TMDB_HTTP_401" | "TMDB_HTTP_403" => "unauthorized",
            "TMDB_HTTP_429" => "rate-limited",
            "TMDB_CREDENTIALS_REQUIRED" | "TMDB_CREDENTIALS_INVALID" => "credentials-unavailable",
            _ => "request-failed",
        };
        self.disabled = true;
        self.disabled_reason = Some(code);
        eprintln!("auto-match provider=TMDB outcome={code} disabled-for-run=true");
    }
}

pub(crate) struct AutomaticRequest<'a> {
    pub db: &'a Database,
    pub node: &'a MediaNode,
    pub evidence: &'a crate::title_extractor::MatchEvidence,
    pub files: &'a [String],
    pub root: Result<&'a Path, &'a str>,
    pub expected: &'a str,
}

#[cfg(test)]
pub(crate) fn fixture_movie_http(
    query: &str,
    year: Option<u16>,
) -> (Vec<Movie>, Vec<(String, String)>) {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = server.local_addr().unwrap();
    let body=serde_json::json!({"results":[{"id":42,"title":query,"original_title":query,"release_date":format!("{}-01-01",year.unwrap_or(2000)),"genre_ids":[28]}]}).to_string();
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = server.accept().unwrap();
        let mut input = vec![0; 8192];
        let count = socket.read(&mut input).unwrap();
        let header = String::from_utf8_lossy(&input[..count]);
        let target = header
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        let url = reqwest::Url::parse(&format!("http://{address}{target}")).unwrap();
        let arguments = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<Vec<_>>();
        assert_eq!(url.path(), "/search/movie");
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        arguments
    });
    let response: SearchResponse = request_at(
        &format!("http://{address}/search/movie"),
        &movie_search_arguments(query, year, "zh-CN"),
        "synthetic-not-a-real-token",
        Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap(),
    )
    .unwrap();
    (
        response
            .results
            .into_iter()
            .map(movie)
            .collect::<AppResult<Vec<_>>>()
            .unwrap(),
        worker.join().unwrap(),
    )
}
#[cfg(test)]
pub(crate) fn fixture_movie_cover(root: &Path, movie: &Movie) -> AppResult<PathBuf> {
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(24, 36)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    store_downloaded_cover(
        root,
        movie,
        "https://image.tmdb.org/t/p/w500/synthetic.png",
        encoded.get_ref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn poster_requests_use_original_language_for_manual_and_legacy_bindings_too() {
        let old = serde_json::json!({"id":7,"title":"本地译名","originalTitle":"Original","releaseDate":"2020-01-01","overview":"","posterPath":"/zh.jpg"});
        let movie: Movie = serde_json::from_value(old).unwrap();
        assert!(!movie.automatic_poster);
        let mut requests = Vec::new();
        let path=automatic_poster_path_using(&movie,&mut |path,args|{
            requests.push((path.to_string(),args.to_vec()));
            Ok(if path.ends_with("/images") {serde_json::json!({"posters":[
                {"file_path":"/zh.jpg","iso_639_1":"zh","width":2000,"height":3000,"vote_count":999},
                {"file_path":"/ja.jpg","iso_639_1":"ja","width":1000,"height":1500,"vote_count":1}
            ]})} else {serde_json::json!({"id":7,"title":"Original","original_title":"Original","original_language":"ja","poster_path":"/zh.jpg"})})
        }).unwrap();
        assert_eq!(path.as_deref(), Some("/ja.jpg"));
        assert_eq!(
            requests,
            vec![
                ("/movie/7".into(), vec![]),
                ("/movie/7/images".into(), vec![])
            ]
        );
        let mut known = movie.clone();
        known.original_language = Some("fr".into());
        requests.clear();
        let path=automatic_poster_path_using(&known,&mut |path,args|{
            requests.push((path.to_string(),args.to_vec()));
            Ok(if path.ends_with("/images") {serde_json::json!({"posters":[]})} else {serde_json::json!({"id":7,"title":"Original","original_title":"Original","poster_path":"/fr.jpg"})})
        }).unwrap();
        assert_eq!(path.as_deref(), Some("/fr.jpg"));
        assert_eq!(
            requests,
            vec![
                ("/movie/7/images".into(), vec![]),
                ("/movie/7".into(), vec![("language", "fr".into())])
            ]
        );
    }

    #[test]
    fn absent_original_posters_prefer_neutral_or_english_over_foreign_chinese_images() {
        let images: MovieImages = serde_json::from_value(serde_json::json!({"posters":[
            {"file_path":"/zh.jpg","iso_639_1":"zh","width":2000,"height":3000,"vote_count":999},
            {"file_path":"/en.jpg","iso_639_1":"en","width":1000,"height":1500,"vote_count":1}
        ]}))
        .unwrap();
        assert_eq!(
            preferred_poster(&images, Some("ja")).as_deref(),
            Some("/en.jpg")
        );
        assert_eq!(
            preferred_poster(&images, Some("zh")).as_deref(),
            Some("/zh.jpg")
        );
        assert_eq!(
            preferred_poster(&images, Some("cn")).as_deref(),
            Some("/zh.jpg")
        );
    }

    #[test]
    fn movie_details_parse_official_aliases_without_relaxing_title_or_year_checks() {
        let data = serde_json::json!({"id":7,"title":"本地译名","original_title":"異国の作品","release_date":"2014-01-01","genres":[{"id":18}],"alternative_titles":{"titles":[{"title":"Snow on the Blades","iso_3166_1":"US"}]}});
        let m = movie(serde_json::from_value(data).unwrap()).unwrap();
        let e = crate::title_extractor::build_movie_match_evidence(
            "Snow.on.the.Blades.2014.JAPANESE.1080p.BluRay.H264.AAC-GROUP",
            "",
            &[],
            crate::models::LibraryRecognitionMode::Folder,
        );
        assert!(reliable_movie(&m, &e));
        let mut wrong = m.clone();
        wrong.release_date = Some("2015-01-01".into());
        assert!(!reliable_movie(&wrong, &e));
        wrong = m.clone();
        wrong.alternative_titles = vec!["Snow on the Blades II".into()];
        assert!(!reliable_movie(&wrong, &e));
        let oversized = serde_json::json!({"id":7,"title":"Movie","original_title":"Movie","alternative_titles":{"titles":(0..300).map(|_|serde_json::json!({"title":"x".repeat(600)})).collect::<Vec<_>>()}});
        let m = movie(serde_json::from_value(oversized).unwrap()).unwrap();
        assert_eq!(m.alternative_titles.len(), 128);
        assert!(m.alternative_titles.iter().all(|s| s.len() == 300));
    }
    #[test]
    fn automatic_posters_prioritize_real_original_language_over_localized_popularity() {
        let images: MovieImages = serde_json::from_value(serde_json::json!({"posters":[
            {"file_path":"/zh.jpg","iso_639_1":"zh","width":2000,"height":3000,"vote_count":999,"vote_average":9},
            {"file_path":"/en.jpg","iso_639_1":"en","width":1000,"height":1500,"vote_count":100},
            {"file_path":"/fr.jpg","iso_639_1":"fr","width":500,"height":750,"vote_count":1},
            {"file_path":"/neutral.jpg","iso_639_1":null,"width":800,"height":1200}
        ]})).unwrap();
        assert_eq!(
            preferred_poster(&images, Some("fr")).as_deref(),
            Some("/fr.jpg")
        );
        assert_eq!(
            preferred_poster(&images, Some("en")).as_deref(),
            Some("/en.jpg")
        );
        assert_eq!(
            preferred_poster(&images, Some("ja")).as_deref(),
            Some("/neutral.jpg")
        );
        assert_eq!(
            preferred_poster(&images, None).as_deref(),
            Some("/neutral.jpg")
        );
    }
    #[test]
    fn automatic_posters_reject_invalid_paths_languages_dimensions_and_use_official_fallback() {
        let images: MovieImages = serde_json::from_value(serde_json::json!({"posters":[
            {"file_path":"https://evil/fr.jpg","iso_639_1":"fr","width":500,"height":750},
            {"file_path":"/bad.jpg","iso_639_1":"fr-FR","width":500,"height":750},
            {"file_path":"/tiny.jpg","iso_639_1":"fr","width":1,"height":2},
            {"file_path":"/wide.jpg","iso_639_1":"fr","width":1500,"height":500},
            {"file_path":"/en.jpg","iso_639_1":"en","width":500,"height":750}
        ]}))
        .unwrap();
        assert_eq!(
            preferred_poster(&images, Some("fr")).as_deref(),
            Some("/en.jpg")
        );
        assert_eq!(
            preferred_poster(&MovieImages { posters: vec![] }, Some("fr")),
            None
        );
    }
    #[test]
    fn legacy_manual_movie_payloads_keep_the_selected_poster_and_automatic_retry_preference_roundtrips(
    ) {
        let old = serde_json::json!({"id":7,"title":"Legacy","originalTitle":"Legacy","releaseDate":"2020-01-01","overview":"","posterPath":"/manual.jpg"});
        let mut movie: Movie = serde_json::from_value(old).unwrap();
        assert!(!movie.automatic_poster);
        assert_eq!(movie.original_language, None);
        assert_eq!(movie.poster_path.as_deref(), Some("/manual.jpg"));
        movie.automatic_poster = true;
        movie.original_language = Some("fr".into());
        let reopened: Movie =
            serde_json::from_str(&serde_json::to_string(&movie).unwrap()).unwrap();
        assert!(reopened.automatic_poster);
        assert_eq!(reopened.original_language.as_deref(), Some("fr"));
    }
    #[test]
    fn poster_urls_are_fixed_and_paths_bounded() {
        for bad in [
            "https://x/a.jpg",
            "//x/a.jpg",
            "/../a.jpg",
            "/x.jpg?token=x",
            "/a.svg",
            "/a.jpg#x",
        ] {
            assert!(!valid_poster(bad));
        }
        assert!(valid_poster("/abc_123-X.jpg"));
    }
    #[test]
    fn malformed_and_oversize_fields_are_bounded() {
        assert!(movie(ApiMovie {
            id: 0,
            title: "x".into(),
            original_title: "x".into(),
            overview: None,
            release_date: None,
            poster_path: None,
            genre_ids: None,
            genres: None,
            original_language: None,
            alternative_titles: None,
        })
        .is_err());
        let m = movie(ApiMovie {
            id: 1,
            title: "x".repeat(600),
            original_title: "x".into(),
            overview: Some("y".repeat(17000)),
            release_date: None,
            poster_path: Some("https://evil".into()),
            genre_ids: None,
            genres: None,
            original_language: None,
            alternative_titles: None,
        })
        .unwrap();
        assert_eq!(m.title.len(), 500);
        assert_eq!(m.overview.len(), 16000);
        assert!(m.poster_path.is_none());
    }
    #[test]
    fn native_http_handles_statuses_invalid_json_and_bounded_retries() {
        use std::net::TcpListener;
        fn call(replies: Vec<(&'static str, &'static str)>) -> AppResult<serde_json::Value> {
            let server = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = server.local_addr().unwrap();
            let worker = std::thread::spawn(move || {
                for (status, body) in replies {
                    let (mut socket, _) = server.accept().unwrap();
                    let mut request = [0u8; 4096];
                    let _ = socket.read(&mut request);
                    write!(socket,"HTTP/1.1 {status}\r\nContent-Length: {}\r\nRetry-After: 0\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                }
            });
            let result = request_at(
                &format!("http://{address}/synthetic"),
                &[],
                "synthetic-not-a-real-token",
                Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(1))
                    .build()
                    .unwrap(),
            );
            worker.join().unwrap();
            result
        }
        for status in ["401 Unauthorized", "403 Forbidden", "404 Not Found"] {
            assert_eq!(
                call(vec![(status, "{}")]).unwrap_err(),
                format!("TMDB_HTTP_{}", &status[..3])
            );
        }
        assert_eq!(
            call(vec![
                ("429 Too Many Requests", "{}"),
                ("429 Too Many Requests", "{}")
            ])
            .unwrap_err(),
            "TMDB_HTTP_429"
        );
        assert_eq!(
            call(vec![
                ("503 Service Unavailable", "{}"),
                ("200 OK", r#"{"ok":true}"#)
            ])
            .unwrap()["ok"],
            true
        );
        assert_eq!(
            call(vec![("200 OK", "{")]).unwrap_err(),
            "TMDB_INVALID_RESPONSE"
        );
    }
    #[test]
    fn native_http_rejects_oversized_header_before_body_and_sanitizes_transport_errors() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = server.accept().unwrap();
            let mut request = [0u8; 4096];
            let _ = socket.read(&mut request);
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                JSON_LIMIT + 1
            )
            .unwrap();
        });
        let result: AppResult<serde_json::Value> = request_at(
            &format!("http://{address}/synthetic"),
            &[],
            "synthetic",
            Client::builder().no_proxy().build().unwrap(),
        );
        assert_eq!(result.unwrap_err(), "TMDB_RESPONSE_LIMIT");
        worker.join().unwrap();
    }
    #[test]
    fn schema_twenty_five_upgrade_retains_provider_and_reader_state() {
        use m2shelf_smart_mixed_lab::model::LibraryKind;
        use m2shelf_smart_mixed_shadow::fixture::Factory;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("upgrade.db");
        let mut f = Factory::create(&path, LibraryKind::LiveAction, "FOLDER").unwrap();
        f.connection
            .execute_batch(include_str!("../migrations/0025_smart_mixed.sql"))
            .unwrap();
        f.connection
            .execute(
                "INSERT INTO mediashelf_schema_migrations(version) VALUES(25)",
                [],
            )
            .unwrap();
        let movie = f.node("Movie").unwrap();
        f.connection
            .execute(
                "UPDATE nodes SET node_type='WORK',total_video_count=1 WHERE id=?1",
                [movie],
            )
            .unwrap();
        let resource = f.resource("Extras/Novel.epub").unwrap();
        let book = f.file_book("Extras/Novel.epub").unwrap();
        f.connection.execute("UPDATE comic_books SET source_resource_id=?1,source_resource_stamp='2026-01-01' WHERE id=?2", params![resource,book]).unwrap();
        f.connection.execute("INSERT INTO comic_reading_progress(comic_book_id,last_page_index,last_read_at) VALUES(?1,0,'saved-time')", [book]).unwrap();
        f.connection
            .execute(
                "INSERT INTO comic_bookmarks(comic_book_id,page_index) VALUES(?1,0)",
                [book],
            )
            .unwrap();
        f.connection.execute("INSERT INTO tmdb_movie_bindings(node_id,movie_id,payload_json) VALUES(?1,123,'{\"title\":\"retained\"}')", [movie]).unwrap();
        f.connection.execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(?1,'BANGUMI',456,6,'retained')", [movie]).unwrap();
        f.connection
            .execute(
                "INSERT INTO settings(key,value) VALUES('synthetic-retained-setting','unchanged')",
                [],
            )
            .unwrap();
        let db = Database::new(path);
        db.migrate().unwrap();
        db.migrate().unwrap();
        let c = db.connect().unwrap();
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM mediashelf_schema_migrations",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            26
        );
        assert_eq!(
            c.query_row(
                "SELECT last_read_at FROM comic_reading_progress WHERE comic_book_id=?1",
                [book],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "saved-time"
        );
        assert!(c
            .query_row(
                "SELECT text_block_index FROM comic_reading_progress WHERE comic_book_id=?1",
                [book],
                |r| r.get::<_, Option<i64>>(0)
            )
            .unwrap()
            .is_none());
        assert_eq!(
            c.query_row(
                "SELECT text_character_offset FROM comic_bookmarks WHERE comic_book_id=?1",
                [book],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            c.query_row(
                "SELECT movie_id FROM tmdb_movie_bindings WHERE node_id=?1",
                [movie],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            123
        );
        assert_eq!(
            c.query_row(
                "SELECT provider_subject_id FROM metadata_bindings WHERE node_id=?1",
                [movie],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            456
        );
        assert_eq!(
            c.query_row(
                "SELECT value FROM settings WHERE key='synthetic-retained-setting'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "unchanged"
        );
        assert_eq!(
            c.query_row(
                "SELECT source_resource_id FROM comic_books WHERE id=?1",
                [book],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            resource
        );
        assert!(c
            .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
            .optional()
            .unwrap()
            .is_none());
    }
    #[test]
    fn movie_binding_eligibility_is_identical_in_both_recognition_modes() {
        use m2shelf_smart_mixed_lab::model::LibraryKind;
        use m2shelf_smart_mixed_shadow::fixture::Factory;
        for mode in ["FOLDER", "VIDEO_FILE"] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("movies.db");
            let mut fixture = Factory::create(&path, LibraryKind::LiveAction, mode).unwrap();
            let id = fixture
                .node(if mode == "FOLDER" {
                    "Movie"
                } else {
                    "Movie.mkv"
                })
                .unwrap();
            fixture
                .connection
                .execute(
                    "UPDATE nodes SET node_type='WORK',total_video_count=1 WHERE id=?1",
                    [id],
                )
                .unwrap();
            let db = Database::new(path);
            db.migrate().unwrap();
            let node = db.get_node(id).unwrap();
            assert!(scope(&node).is_ok(), "{mode}");
            assert_eq!(node.id, id);
            assert_eq!(
                db.get_root(node.library_root_id)
                    .unwrap()
                    .recognition_mode
                    .as_db(),
                mode
            );
        }
    }
    #[test]
    fn provider_namespace_scope_manual_cover_and_stale_edits() {
        let _serial = crate::tmdb::MOVIE_TEST_GATE
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        use m2shelf_smart_mixed_lab::model::LibraryKind;
        use m2shelf_smart_mixed_shadow::{adapter, fixture::Factory};
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("movies.db");
        let mut f = Factory::create(&path, LibraryKind::LiveAction, "FOLDER").unwrap();
        let id = f.node("Movie").unwrap();
        let db = Database::new(path);
        db.migrate().unwrap();
        f.connection.execute("UPDATE nodes SET node_type='WORK',total_video_count=1,cover_source='MANUAL',cover_cache_path='synthetic-manual' WHERE id=?1",[id]).unwrap();
        f.connection.execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(?1,'BANGUMI',99,6,'Original')",[id]).unwrap();
        let movie = Movie {
            id: 99,
            title: "Different provider".into(),
            original_title: "Fixture".into(),
            release_date: Some("2020-01-01".into()),
            overview: String::new(),
            poster_path: None,
            genre_ids: None,
            original_language: None,
            automatic_poster: false,
            poster_policy_version: 0,
            alternative_titles: Vec::new(),
        };
        f.connection.execute("INSERT INTO tmdb_movie_bindings(node_id,movie_id,payload_json,cover_cache_path,cover_error) VALUES(?1,99,?2,'synthetic-tmdb','TMDB_COVER_NETWORK')",params![id,serde_json::to_string(&movie).unwrap()]).unwrap();
        let node = db.get_node(id).unwrap();
        assert!(scope(&node).is_ok());
        assert_eq!(node.binding.as_ref().unwrap().provider_subject_id, 99);
        assert_eq!(
            node.tmdb_binding.as_ref().unwrap().movie.title,
            "Different provider"
        );
        assert_eq!(node.cover_cache_path.as_deref(), Some("synthetic-manual"));
        assert_eq!(
            db.cover_read_context(id).unwrap().0,
            Some(PathBuf::from("synthetic-manual"))
        );
        f.connection.execute("UPDATE nodes SET cover_source='BANGUMI',cover_cache_path='synthetic-bangumi' WHERE id=?1",[id]).unwrap();
        assert_eq!(
            db.cover_read_context(id).unwrap().0,
            Some(PathBuf::from("synthetic-tmdb"))
        );
        let before = fingerprint(&f.connection, id).unwrap();
        f.connection
            .execute("UPDATE nodes SET display_name='Renamed' WHERE id=?1", [id])
            .unwrap();
        assert_ne!(before, fingerprint(&f.connection, id).unwrap());
        deactivate(&db, id).unwrap();
        assert_eq!(
            db.cover_read_context(id).unwrap().0,
            Some(PathBuf::from("synthetic-bangumi"))
        );
        assert!(!db.get_node(id).unwrap().tmdb_binding.unwrap().active);
        assert_eq!(db.get_binding(id).unwrap().unwrap().provider_subject_id, 99);
        f.connection
            .execute(
                "UPDATE metadata_bindings SET provider_subject_type=2 WHERE node_id=?1",
                [id],
            )
            .unwrap();
        assert!(scope(&db.get_node(id).unwrap()).is_err());
        assert!(adapter::ReadIndex::open(db.path()).is_ok());
    }
}

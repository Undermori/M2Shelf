//! Watch confirmations and verified mpv file transitions over the existing indexed ownership snapshot.
use crate::{
    db::{AppResult, Database},
    media_scraper,
    models::{MediaFile, MediaNode, NodeDetail},
    title_extractor::{extract_title_signals, EditionKind},
};
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

const MAX_FILES: usize = 20_000;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeWatchRow {
    pub file_id: i64,
    pub kind: &'static str,
    pub season: Option<u16>,
    pub episodes: Vec<u16>,
    pub watched: bool,
    pub partially_watched: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeWatchSnapshot {
    pub node_id: i64,
    pub work_view: bool,
    pub snapshot: String,
    pub rows: Vec<EpisodeWatchRow>,
    pub watched_episodes: usize,
    pub total_episodes: usize,
    pub watched_files: usize,
    pub total_files: usize,
    pub unresolved_records: usize,
    pub latest_watched_file_id: Option<i64>,
    pub latest_watched_file_name: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
struct Identity {
    root: i64,
    key: String,
    scope: String,
    kind: &'static str,
    owner: i64,
    file: i64,
    name: String,
    season: Option<u16>,
    episode: Option<u16>,
}
#[derive(Clone)]
struct Row {
    view: EpisodeWatchRow,
    identities: Vec<Identity>,
    signature: serde_json::Value,
}
#[derive(Default)]
struct IdentityCache {
    entries: HashMap<String, (Arc<Vec<Row>>, usize, u64)>,
    order: BTreeMap<u64, String>,
    bytes: usize,
    clock: u64,
}
static IDENTITY_CACHE: OnceLock<Mutex<IdentityCache>> = OnceLock::new();
fn cached_identity_rows(key: &str) -> Option<Arc<Vec<Row>>> {
    let mut cache = IDENTITY_CACHE.get_or_init(Default::default).lock().ok()?;
    cache.clock += 1;
    let tick = cache.clock;
    let (rows, _, old) = cache.entries.get_mut(key)?;
    let rows = Arc::clone(rows);
    let previous = *old;
    *old = tick;
    cache.order.remove(&previous);
    cache.order.insert(tick, key.to_string());
    Some(rows)
}
fn remember_identity_rows(key: String, rows: Arc<Vec<Row>>) {
    // Account for owned allocations directly: serializing a large Series merely
    // to estimate its cache size adds a second full-file pass and a large buffer.
    // Aliases share Arcs but are charged separately, conservatively.
    let mut bytes = 512usize.saturating_add(rows.capacity().saturating_mul(size_of::<Row>()));
    for row in rows.iter() {
        if !row.signature.is_null() || row.view.watched || row.view.partially_watched {
            return;
        }
        bytes = bytes
            .saturating_add(
                row.view
                    .episodes
                    .capacity()
                    .saturating_mul(size_of::<u16>()),
            )
            .saturating_add(
                row.identities
                    .capacity()
                    .saturating_mul(size_of::<Identity>()),
            );
        for identity in &row.identities {
            bytes = bytes
                .saturating_add(identity.key.capacity())
                .saturating_add(identity.scope.capacity())
                .saturating_add(identity.name.capacity());
        }
    }
    if bytes > 8 * 1024 * 1024 {
        return;
    }
    if let Ok(mut cache) = IDENTITY_CACHE.get_or_init(Default::default).lock() {
        if cache.entries.contains_key(&key) {
            return;
        }
        while cache.entries.len() >= 4096 || cache.bytes + bytes > 32 * 1024 * 1024 {
            let Some((_, old)) = cache.order.pop_first() else {
                break;
            };
            if let Some((_, size, _)) = cache.entries.remove(&old) {
                cache.bytes -= size;
            }
        }
        cache.clock += 1;
        let tick = cache.clock;
        cache.order.insert(tick, key.clone());
        cache.entries.insert(key, (rows, bytes, tick));
        cache.bytes += bytes;
    }
}

/// Snapshot-local Root index generation, with raw-input fallback for missing
/// legacy generation rows. It contains no watch state and never opens media.
pub(crate) fn library_projection_stamp(c: &Connection, root: i64) -> AppResult<String> {
    use rusqlite::types::ValueRef;
    let mut hash = Sha256::new();
    // Schema 25 already revisions node, file and Bangumi writes for EVERY Root.
    // Reuse that transaction-local generation rather than hashing every filename
    // on every navigation. Counts additionally fence moves OUT of this Root:
    // the legacy UPDATE triggers increment only the destination generation.
    let revision: Option<i64> = sql(c
        .query_row(
            "SELECT revision FROM book_organization_state WHERE root_id=?1",
            [root],
            |r| r.get(0),
        )
        .optional())?;
    let database: String = sql(c.query_row(
        "SELECT file FROM pragma_database_list WHERE name='main'",
        [],
        |r| r.get(0),
    ))?;
    let queries = if let Some(revision) = revision.filter(|_| !database.is_empty()) {
        hash.update(digest(&(database, revision)).as_bytes());
        vec![
            "SELECT * FROM library_roots WHERE id=?1 ORDER BY id",
            "SELECT (SELECT COUNT(*) FROM nodes WHERE library_root_id=?1),(SELECT COUNT(*) FROM media_files f JOIN nodes n ON n.id=f.node_id WHERE n.library_root_id=?1),(SELECT COUNT(*) FROM metadata_bindings b JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=?1)",
            // TMDb writes have no matching generation trigger in the old schema.
            "SELECT b.* FROM tmdb_movie_bindings b JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=?1 ORDER BY b.node_id",
        ]
    } else {
        vec![
        "SELECT * FROM library_roots WHERE id=?1 ORDER BY id",
        "SELECT * FROM nodes WHERE library_root_id=?1 ORDER BY id",
        "SELECT f.* FROM media_files f JOIN nodes n ON n.id=f.node_id WHERE n.library_root_id=?1 ORDER BY f.id",
        "SELECT b.* FROM metadata_bindings b JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=?1 ORDER BY b.id",
        "SELECT b.* FROM tmdb_movie_bindings b JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=?1 ORDER BY b.node_id",
    ]
    };
    for query in queries {
        hash.update(query.as_bytes());
        let mut statement = sql(c.prepare(query))?;
        let columns = statement.column_count();
        let mut rows = sql(statement.query([root]))?;
        while let Some(row) = sql(rows.next())? {
            hash.update([0xff]);
            for i in 0..columns {
                match sql(row.get_ref(i))? {
                    ValueRef::Null => hash.update([0]),
                    ValueRef::Integer(v) => {
                        hash.update([1]);
                        hash.update(v.to_le_bytes());
                    }
                    ValueRef::Real(v) => {
                        hash.update([2]);
                        hash.update(v.to_bits().to_le_bytes());
                    }
                    ValueRef::Text(v) => {
                        hash.update([3]);
                        hash.update((v.len() as u64).to_le_bytes());
                        hash.update(v);
                    }
                    ValueRef::Blob(v) => {
                        hash.update([4]);
                        hash.update((v.len() as u64).to_le_bytes());
                        hash.update(v);
                    }
                }
            }
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn library_row_key(stamp: &str, node: i64) -> String {
    digest(&("library-identity-v1", stamp, node, false))
}
fn digest(value: &impl Serialize) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("serializable episode identity"))
    )
}
fn sql<T>(result: rusqlite::Result<T>) -> AppResult<T> {
    result.map_err(|e| e.to_string())
}

pub(crate) fn supplemental(value: &str) -> bool {
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    crate::scanner::is_supplementary_directory_name(value) || TOKEN.get_or_init(|| Regex::new(r"(?i)(?:^|[\s._\-\[\]()])(?:NCOP|NCED|OP|ED|PV|CM|SP|specials?|extras?|bonus|trailer|preview|creditless)(?:[\s._-]*\d{0,3})(?:$|[\s._\-\[\]()])|特典|番外|花絮|预告|預告|映像特典|ノンクレジット|予告").unwrap()).is_match(value)
}
fn movie(owner: &MediaNode) -> bool {
    owner
        .tmdb_binding
        .as_ref()
        .is_some_and(|b| b.active && b.movie.media_type == crate::tmdb::MediaType::Movie)
        || extract_title_signals(&owner.folder_name).edition_kind == EditionKind::Movie
}
fn provider(owner: &MediaNode) -> Option<String> {
    if let Some(b) = owner.tmdb_binding.as_ref().filter(|b| b.active) {
        return Some(format!("tmdb:{:?}:{}", b.movie.media_type, b.movie.id));
    }
    owner.binding.as_ref().map(|b| {
        format!(
            "{}:{}:{}",
            b.provider, b.provider_subject_type, b.provider_subject_id
        )
    })
}
fn parsed_episode(
    file: &MediaFile,
    owner: &MediaNode,
    relative: &str,
    owner_evidence: &media_scraper::Parsed,
    owner_season: Option<u16>,
) -> media_scraper::Parsed {
    let mut parsed = m2shelf_media_parser::watch_episodes(
        &file.file_name,
        media_scraper::kind(owner.media_kind),
    );
    parsed
        .edition
        .extend(owner_evidence.edition.iter().cloned());
    parsed.edition.sort();
    parsed.edition.dedup();
    if parsed.season.is_none() {
        // Indexed relative path components only; this never opens a source directory.
        parsed.season = relative
            .split(['/', '\\'])
            .rev()
            .filter(|part| !part.is_empty())
            .find_map(|part| {
                let p = m2shelf_media_parser::parse(part, media_scraper::kind(owner.media_kind));
                p.season.or(extract_title_signals(part).season_number)
            })
            .or(owner_season);
    }
    parsed
}
struct OwnerEvidence {
    source: crate::video_presentation::SourceEvidence,
    season: Option<u16>,
    movie: bool,
}
/// A read-snapshot-local memo. No retained database state and no media access.
#[derive(Default)]
pub(crate) struct PresentationReader {
    roots: Option<HashMap<i64, (String, String)>>,
    library_stamp: Option<String>,
    owners: HashMap<i64, OwnerEvidence>,
    parsed: HashMap<(i64, i64, String), media_scraper::Parsed>,
    scopes: HashSet<(i64, String)>,
    watched: HashSet<(i64, String)>,
}
fn build_rows(c: &Connection, detail: &NodeDetail, work_view: bool) -> AppResult<Vec<Row>> {
    PresentationReader::default().mutation_rows(c, detail, work_view)
}
#[cfg(test)]
fn build_rows_with(
    c: &Connection,
    detail: &NodeDetail,
    work_view: bool,
    reader: &mut PresentationReader,
    mutation: bool,
) -> AppResult<Vec<Row>> {
    build_rows_with_context(c, detail, work_view, reader, mutation, None)
}
fn build_rows_with_context(
    c: &Connection,
    detail: &NodeDetail,
    work_view: bool,
    reader: &mut PresentationReader,
    mutation: bool,
    context_file_count: Option<usize>,
) -> AppResult<Vec<Row>> {
    if detail.node.media_kind.is_book() || (!work_view && detail.node.parent_node_id.is_none()) {
        return Ok(Vec::new());
    }
    let sources = detail
        .work_sources
        .as_deref()
        .unwrap_or(std::slice::from_ref(&detail.node));
    let owners: HashMap<i64, &MediaNode> = sources.iter().map(|n| (n.id, n)).collect();
    for owner in sources {
        reader
            .owners
            .entry(owner.id)
            .or_insert_with(|| OwnerEvidence {
                source: crate::video_presentation::SourceEvidence::new(owner),
                season: media_scraper::parse(owner, &[])
                    .season
                    .or_else(|| extract_title_signals(&owner.folder_name).season_number),
                movie: movie(owner),
            });
    }
    if reader.roots.is_none() {
        let mut query = sql(c.prepare("SELECT id,path,recognition_mode FROM library_roots"))?;
        let rows = sql(query.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?)))))?;
        reader.roots = Some(sql(rows.collect())?);
    }
    let roots = reader.roots.as_ref().expect("loaded");
    let mut files: Vec<(&MediaFile, i64, &str)> = detail
        .media_files
        .iter()
        .map(|f| (f, f.node_id, ""))
        .collect();
    files.extend(
        detail
            .nested_media_files
            .iter()
            .map(|e| (&e.file, e.source_node_id, e.relative_directory.as_str())),
    );
    if files.len() > MAX_FILES {
        return Err("EPISODE_WATCH_LIMIT".into());
    }
    let mut parsed_files = HashMap::new();
    let mut known_seasons: HashMap<i64, BTreeSet<u16>> = HashMap::new();
    for (file, source, relative) in &files {
        if let Some(owner) = owners.get(source) {
            let evidence = &reader.owners[source];
            let context = &evidence.source;
            let key = (file.id, *source, (*relative).to_string());
            let parsed = reader.parsed.get(&key).cloned().unwrap_or_else(|| {
                parsed_episode(
                    file,
                    owner,
                    relative,
                    &context.parsed_owner,
                    evidence.season,
                )
            });
            if reader.parsed.len() < MAX_FILES {
                reader.parsed.insert(key, parsed.clone());
            }
            if !context.supplemental(&file.file_name)
                && !relative.split(['/', '\\']).any(|s| context.supplemental(s))
            {
                if let Some(season) = parsed.season {
                    known_seasons.entry(*source).or_default().insert(season);
                }
            }
            parsed_files.insert(file.id, parsed);
        }
    }
    let mut result = Vec::new();
    for (file, source, relative) in &files {
        let Some(owner) = owners.get(source).copied() else {
            continue;
        };
        let evidence = &reader.owners[source];
        let context = &evidence.source;
        let mut parsed = parsed_files
            .remove(&file.id)
            .expect("indexed file parsed with its owner");
        let is_supplement = context.supplemental(&file.file_name)
            || relative.split(['/', '\\']).any(|s| context.supplemental(s))
            || parsed.season == Some(0);
        if evidence.movie && !is_supplement {
            continue;
        }
        let Some((root_path, mode)) = roots.get(&owner.library_root_id) else {
            continue;
        };
        let seasons = known_seasons.get(source);
        if parsed.season.is_none() && seasons.is_some_and(|s| s.len() == 1) {
            parsed.season = seasons.and_then(|s| s.first().copied());
        }
        let ambiguous_season = parsed.season.is_none() && seasons.is_some_and(|s| s.len() > 1);
        let is_episode = !is_supplement
            && !ambiguous_season
            && parsed.season != Some(0)
            && parsed.episodes.iter().all(|e| *e > 0)
            && !parsed.episodes.is_empty()
            && parsed.edition.is_empty()
            && !parsed.warnings.iter().any(|w| w.contains("CONFLICT"));
        if !is_episode && !is_supplement && context_file_count.unwrap_or(files.len()) == 1 {
            continue;
        } // Ordinary films never gain a 1/1 episode checkbox.
        let episode_ui = is_episode && (mode != "VIDEO_FILE" || work_view);
        let mut cut = parsed.edition.clone();
        cut.sort();
        cut.dedup();
        let binding = provider(owner);
        let local_title = m2shelf_media_parser::title_key(&parsed.display_title);
        let physical_parent = Path::new(&file.absolute_path)
            .parent()
            .and_then(|p| p.strip_prefix(root_path).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/").to_lowercase());
        let scope = if let Some(binding) = binding {
            digest(&(owner.library_root_id, "bound", binding, &cut))
        } else if mode == "VIDEO_FILE"
            && is_episode
            && local_title.chars().count() >= 3
            && !local_title.chars().all(|c| c.is_numeric())
        {
            digest(&(
                owner.library_root_id,
                "file-series",
                physical_parent,
                local_title,
                parsed.year,
                &cut,
            ))
        } else {
            digest(&(owner.library_root_id, "node", owner.id, &cut))
        };
        let mut identities = Vec::new();
        if is_episode {
            for episode in &parsed.episodes {
                identities.push(Identity {
                    root: owner.library_root_id,
                    key: digest(&(&scope, "episode", parsed.season, episode)),
                    scope: scope.clone(),
                    kind: "EPISODE",
                    owner: owner.id,
                    file: file.id,
                    name: if mutation {
                        file.file_name.clone()
                    } else {
                        String::new()
                    },
                    season: parsed.season,
                    episode: Some(*episode),
                });
            }
        } else {
            identities.push(Identity {
                root: owner.library_root_id,
                key: digest(&(
                    &scope,
                    "file",
                    file.id,
                    &file.file_name,
                    file.file_size,
                    &file.modified_at,
                )),
                scope,
                kind: "FILE",
                owner: owner.id,
                file: file.id,
                name: if mutation {
                    file.file_name.clone()
                } else {
                    String::new()
                },
                season: None,
                episode: None,
            });
        }
        result.push(Row {
            view: EpisodeWatchRow {
                file_id: file.id,
                kind: if episode_ui { "EPISODE" } else { "FILE" },
                season: parsed.season,
                episodes: if episode_ui {
                    parsed.episodes
                } else {
                    Vec::new()
                },
                watched: false,
                partially_watched: false,
            },
            signature: if mutation {
                serde_json::json!([
                    file.id,
                    file.node_id,
                    file.absolute_path,
                    file.file_name,
                    file.file_size,
                    file.modified_at,
                    owner.id,
                    owner.updated_at,
                    relative
                ])
            } else {
                serde_json::Value::Null
            },
            identities,
        });
    }
    Ok(result)
}
#[derive(Serialize)]
struct WatchState {
    watched: bool,
    revision: i64,
    scope: String,
    owner: Option<i64>,
    file: Option<i64>,
    updated: String,
}
type StateMap = HashMap<(i64, String), WatchState>;
fn collect_states(
    c: &Connection,
    root: i64,
    column: &str,
    values: Vec<rusqlite::types::Value>,
    states: &mut StateMap,
) -> AppResult<()> {
    for chunk in values.chunks(256) {
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let mut query=sql(c.prepare(&format!("SELECT library_root_id,identity_key,watched,revision,scope_key,source_node_id,source_file_id,updated_at FROM episode_watch_states WHERE library_root_id=? AND {column} IN ({placeholders})")))?;
        let arguments =
            std::iter::once(rusqlite::types::Value::Integer(root)).chain(chunk.iter().cloned());
        let records = sql(query.query_map(rusqlite::params_from_iter(arguments), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, String>(7)?,
            ))
        }))?;
        for row in records {
            let (root, key, watched, revision, scope, owner, file, updated) = sql(row)?;
            states.insert(
                (root, key),
                WatchState {
                    watched,
                    revision,
                    scope,
                    owner,
                    file,
                    updated,
                },
            );
            if states.len() > 100_000 {
                return Err("EPISODE_WATCH_LIMIT".into());
            }
        }
    }
    Ok(())
}
fn load_states(c: &Connection, rows: &[Row]) -> AppResult<StateMap> {
    let mut scopes: BTreeMap<i64, BTreeSet<String>> = BTreeMap::new();
    let mut owners: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    for identity in rows.iter().flat_map(|r| &r.identities) {
        scopes
            .entry(identity.root)
            .or_default()
            .insert(identity.scope.clone());
        owners
            .entry(identity.root)
            .or_default()
            .insert(identity.owner);
    }
    let mut states = HashMap::new();
    // Both lookups have dedicated indexes; unrelated Root history is never materialized.
    for (root, values) in scopes {
        collect_states(
            c,
            root,
            "scope_key",
            values
                .into_iter()
                .map(rusqlite::types::Value::Text)
                .collect(),
            &mut states,
        )?;
    }
    for (root, values) in owners {
        collect_states(
            c,
            root,
            "source_node_id",
            values
                .into_iter()
                .map(rusqlite::types::Value::Integer)
                .collect(),
            &mut states,
        )?;
    }
    Ok(states)
}
fn snapshot_conn(
    c: &Connection,
    node_id: i64,
    work_view: bool,
) -> AppResult<(EpisodeWatchSnapshot, Vec<Row>)> {
    crate::db::ensure_node_visible_conn(c, node_id)?;
    let detail = if work_view {
        crate::works::work_detail_conn(c, node_id)?
    } else {
        let mut index = crate::logical_works::LogicalWorkIndex::load_for_detail(c)?;
        crate::works::progress_detail_conn(c, &mut index, node_id, false)?
    };
    snapshot_detail_conn(c, &detail, node_id, work_view)
}
fn snapshot_detail_conn(
    c: &Connection,
    detail: &NodeDetail,
    node_id: i64,
    work_view: bool,
) -> AppResult<(EpisodeWatchSnapshot, Vec<Row>)> {
    let rows = build_rows(c, detail, work_view)?;
    snapshot_rows_conn(c, detail, node_id, work_view, rows)
}

fn snapshot_rows_conn(
    c: &Connection,
    detail: &NodeDetail,
    node_id: i64,
    work_view: bool,
    mut rows: Vec<Row>,
) -> AppResult<(EpisodeWatchSnapshot, Vec<Row>)> {
    let states = load_states(c, &rows)?;
    let mut episode_states = BTreeMap::new();
    let mut watched_files = 0;
    let mut total_files = 0;
    let mut used = HashSet::new();
    for row in &mut rows {
        let count = row
            .identities
            .iter()
            .filter(|i| {
                states
                    .get(&(i.root, i.key.clone()))
                    .is_some_and(|s| s.watched)
            })
            .count();
        row.view.watched = count == row.identities.len();
        row.view.partially_watched = count > 0 && count < row.identities.len();
        for identity in &row.identities {
            used.insert((identity.root, identity.key.clone()));
        }
        if row.view.kind == "EPISODE" {
            // A validated aggregate represents one logical work; versions count once per season/episode.
            for identity in &row.identities {
                let key = episode_key(identity, detail, work_view);
                let watched = states
                    .get(&(identity.root, identity.key.clone()))
                    .is_some_and(|s| s.watched);
                episode_states
                    .entry(key)
                    .and_modify(|value| *value |= watched)
                    .or_insert(watched);
            }
        } else {
            total_files += 1;
            if row.view.watched {
                watched_files += 1;
            }
        }
    }
    // Share the visible aggregate's confirmed state with duplicate encodes even if only one retained origin remains.
    for row in &mut rows {
        if row.view.kind == "EPISODE" {
            let n = row
                .identities
                .iter()
                .filter(|i| {
                    episode_states
                        .get(&episode_key(i, detail, work_view))
                        .copied()
                        .unwrap_or(false)
                })
                .count();
            row.view.watched = n == row.identities.len();
            row.view.partially_watched = n > 0 && n < row.identities.len();
        }
    }
    let mut relevant_states = used
        .iter()
        .map(|key| (key, states.get(key)))
        .collect::<Vec<_>>();
    relevant_states.sort_by(|a, b| a.0.cmp(b.0));
    let signature = digest(&(
        node_id,
        work_view,
        &detail.work_target,
        rows.iter()
            .map(|r| (&r.signature, &r.identities))
            .collect::<Vec<_>>(),
        relevant_states,
    ));
    let scopes = rows
        .iter()
        .flat_map(|r| r.identities.iter().map(|i| i.scope.as_str()))
        .collect::<HashSet<_>>();
    let owners = rows
        .iter()
        .flat_map(|r| r.identities.iter().map(|i| i.owner))
        .collect::<HashSet<_>>();
    let unresolved_records = states
        .iter()
        .filter(|(key, state)| {
            state.watched
                && !used.contains(*key)
                && (scopes.contains(state.scope.as_str())
                    || state.owner.is_some_and(|id| owners.contains(&id)))
        })
        .count();
    // Latest explicit checked record, with the clicked encode preferred over a duplicate.
    // A removed file may fall back only to a currently validated identity, never a stale path.
    let latest = rows
        .iter()
        .flat_map(|row| {
            let states = &states;
            row.identities.iter().filter_map(move |identity| {
                let state = states.get(&(identity.root, identity.key.clone()))?;
                state.watched.then(|| {
                    (
                        (
                            state.updated.replace(' ', "T"),
                            state.file == Some(row.view.file_id),
                            row.view.file_id,
                        ),
                        row,
                    )
                })
            })
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, row)| row);
    let latest_watched_file_id = latest.map(|row| row.view.file_id);
    let latest_watched_file_name = latest
        .and_then(|row| row.identities.first())
        .map(|identity| identity.name.clone());
    let result = EpisodeWatchSnapshot {
        node_id,
        work_view,
        snapshot: signature,
        rows: rows.iter().map(|r| r.view.clone()).collect(),
        watched_episodes: episode_states.values().filter(|v| **v).count(),
        total_episodes: episode_states.len(),
        watched_files,
        total_files,
        unresolved_records,
        latest_watched_file_id,
        latest_watched_file_name,
    };
    Ok((result, rows))
}
#[cfg(test)]
pub(crate) fn presentation_counts(
    c: &Connection,
    detail: &NodeDetail,
    work_view: bool,
) -> AppResult<(usize, usize)> {
    PresentationReader::default().counts(c, detail, work_view)
}
impl PresentationReader {
    fn mutation_rows(
        &mut self,
        c: &Connection,
        detail: &NodeDetail,
        work_view: bool,
    ) -> AppResult<Vec<Row>> {
        let mut rows = self.cached_rows(c, detail, work_view)?.as_ref().clone();
        let sources = detail
            .work_sources
            .as_deref()
            .unwrap_or(std::slice::from_ref(&detail.node));
        let owners = sources.iter().map(|n| (n.id, n)).collect::<HashMap<_, _>>();
        let files = detail
            .media_files
            .iter()
            .map(|f| (f.id, (f, f.node_id, "")))
            .chain(detail.nested_media_files.iter().map(|e| {
                (
                    e.file.id,
                    (&e.file, e.source_node_id, e.relative_directory.as_str()),
                )
            }))
            .collect::<HashMap<_, _>>();
        for row in &mut rows {
            let (file, source, relative) =
                files.get(&row.view.file_id).ok_or("EPISODE_WATCH_STALE")?;
            let owner = owners.get(source).ok_or("EPISODE_WATCH_STALE")?;
            for identity in &mut row.identities {
                identity.name = file.file_name.clone();
            }
            row.signature = serde_json::json!([
                file.id,
                file.node_id,
                file.absolute_path,
                file.file_name,
                file.file_size,
                file.modified_at,
                owner.id,
                owner.updated_at,
                relative
            ]);
        }
        Ok(rows)
    }
    pub(crate) fn snapshot(
        &mut self,
        c: &Connection,
        detail: &NodeDetail,
        work_view: bool,
    ) -> AppResult<EpisodeWatchSnapshot> {
        let rows = self.mutation_rows(c, detail, work_view)?;
        snapshot_rows_conn(c, detail, detail.node.id, work_view, rows).map(|r| r.0)
    }
    fn cached_rows(
        &mut self,
        c: &Connection,
        detail: &NodeDetail,
        work_view: bool,
    ) -> AppResult<std::sync::Arc<Vec<Row>>> {
        use std::sync::Arc;
        let count = detail.media_files.len() + detail.nested_media_files.len();
        if count > MAX_FILES {
            return Err("EPISODE_WATCH_LIMIT".into());
        }
        if detail.node.media_kind.is_book() || (!work_view && detail.node.parent_node_id.is_none())
        {
            return Ok(Arc::new(Vec::new()));
        }
        let sources = detail
            .work_sources
            .as_deref()
            .unwrap_or(std::slice::from_ref(&detail.node));
        if sources.len() <= 1 {
            return self.cached_source_rows(c, detail, work_view, count);
        }
        // Episode/season evidence is owner-scoped. Reuse the same immutable rows
        // when a Work also appears in a Series; watched state remains snapshot-local.
        // Keep the enclosing file count: a lone unlabelled film differs from a
        // file inside a multi-work series even though each owner has one file.
        let mut direct: HashMap<i64, Vec<MediaFile>> = HashMap::new();
        let mut nested: HashMap<i64, Vec<crate::models::NestedMediaFile>> = HashMap::new();
        for file in &detail.media_files {
            direct.entry(file.node_id).or_default().push(file.clone());
        }
        for entry in &detail.nested_media_files {
            nested
                .entry(entry.source_node_id)
                .or_default()
                .push(entry.clone());
        }
        let order = detail
            .media_files
            .iter()
            .map(|f| f.id)
            .chain(detail.nested_media_files.iter().map(|e| e.file.id))
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect::<HashMap<_, _>>();
        let mut rows = Vec::new();
        for owner in sources {
            let projection = NodeDetail {
                node: owner.clone(),
                work_sources: None,
                work_target: None,
                media_files: direct.remove(&owner.id).unwrap_or_default(),
                nested_media_files: nested.remove(&owner.id).unwrap_or_default(),
                children: Vec::new(),
                resource_files: Vec::new(),
                breadcrumbs: Vec::new(),
                binding: None,
                comic_books: None,
                video_sections: Vec::new(),
                expanded_folder_ids: Vec::new(),
                recognition_warnings: Vec::new(),
            };
            rows.extend(
                self.cached_source_rows(c, &projection, work_view, count)?
                    .iter()
                    .cloned(),
            );
        }
        rows.sort_by_key(|row| order.get(&row.view.file_id).copied());
        Ok(Arc::new(rows))
    }
    fn cached_source_rows(
        &mut self,
        c: &Connection,
        detail: &NodeDetail,
        work_view: bool,
        context_file_count: usize,
    ) -> AppResult<std::sync::Arc<Vec<Row>>> {
        // Current indexed owner/file evidence and Root settings are the cache key.
        // Activity and card projections do not participate in episode identity.
        // This cache contains only immutable identity rows, never any watched state.
        if self.roots.is_none() {
            let mut q = sql(c.prepare("SELECT id,path,recognition_mode FROM library_roots"))?;
            self.roots =
                Some(sql(sql(q.query_map([], |r| {
                    Ok((r.get(0)?, (r.get(1)?, r.get(2)?)))
                }))?
                .collect())?);
        }
        let mut roots = self
            .roots
            .as_ref()
            .expect("loaded")
            .iter()
            .collect::<Vec<_>>();
        roots.sort_by_key(|(id, _)| **id);
        let sources = detail
            .work_sources
            .as_deref()
            .unwrap_or(std::slice::from_ref(&detail.node));
        let evidence = sources
            .iter()
            .map(|n| {
                let mut n = n.clone();
                n.card_progress = None;
                n.last_watched_at = None;
                n.latest_file_modified_at = None;
                n.direct_video_count = 0;
                n.total_video_count = 0;
                n.child_media_branch_count = 0;
                n
            })
            .collect::<Vec<_>>();
        let key = digest(&(
            evidence,
            &detail.media_files,
            &detail.nested_media_files,
            work_view,
            context_file_count == 1,
            roots,
        ));
        if let Some(rows) = cached_identity_rows(&key) {
            return Ok(rows);
        }
        let rows = Arc::new(build_rows_with_context(
            c,
            detail,
            work_view,
            self,
            false,
            Some(context_file_count),
        )?);
        remember_identity_rows(key, Arc::clone(&rows));
        Ok(rows)
    }
    pub(crate) fn counts(
        &mut self,
        c: &Connection,
        detail: &NodeDetail,
        work_view: bool,
    ) -> AppResult<(usize, usize)> {
        Ok(self
            .counts_many(c, std::slice::from_ref(detail), work_view)?
            .remove(0))
    }
    pub(crate) fn counts_many(
        &mut self,
        c: &Connection,
        details: &[NodeDetail],
        work_view: bool,
    ) -> AppResult<Vec<(usize, usize)>> {
        let mut batches = Vec::with_capacity(details.len());
        for detail in details {
            let rows = self.cached_rows(c, detail, work_view)?;
            if let Some(stamp) = &self.library_stamp {
                remember_identity_rows(library_row_key(stamp, detail.node.id), Arc::clone(&rows));
            }
            batches.push(rows);
        }
        let contexts = details
            .iter()
            .map(|d| !work_view && d.node.node_type == crate::models::NodeType::Container)
            .collect::<Vec<_>>();
        self.counts_batches(c, batches, &contexts)
    }
    pub(crate) fn library_counts(
        &mut self,
        c: &Connection,
        root: i64,
        nodes: &[MediaNode],
    ) -> AppResult<Vec<Option<(usize, usize)>>> {
        let stamp = library_projection_stamp(c, root)?;
        let mut positions = Vec::new();
        let mut batches = Vec::new();
        let mut contexts = Vec::new();
        for (i, node) in nodes.iter().enumerate() {
            if !node.media_kind.is_book() {
                if let Some(rows) = cached_identity_rows(&library_row_key(&stamp, node.id)) {
                    positions.push(i);
                    batches.push(rows);
                    contexts.push(node.node_type == crate::models::NodeType::Container);
                }
            }
        }
        self.library_stamp = Some(stamp);
        let counts = self.counts_batches(c, batches, &contexts)?;
        let mut result = vec![None; nodes.len()];
        for (i, count) in positions.into_iter().zip(counts) {
            result[i] = Some(count);
        }
        Ok(result)
    }
    fn counts_batches(
        &mut self,
        c: &Connection,
        batches: Vec<Arc<Vec<Row>>>,
        contexts: &[bool],
    ) -> AppResult<Vec<(usize, usize)>> {
        let mut missing: BTreeMap<i64, BTreeSet<String>> = BTreeMap::new();
        for identity in batches
            .iter()
            .flat_map(|rows| rows.iter())
            .flat_map(|row| &row.identities)
        {
            if !self
                .scopes
                .contains(&(identity.root, identity.scope.clone()))
            {
                missing
                    .entry(identity.root)
                    .or_default()
                    .insert(identity.scope.clone());
            }
        }
        // Counts require only current identities. Do not construct mutation signatures,
        // unresolved-record diagnostics or latest-file DTOs for every poster card.
        for (root, scopes) in missing {
            let scopes = scopes.into_iter().collect::<Vec<_>>();
            for chunk in scopes.chunks(256) {
                let placeholders = vec!["?"; chunk.len()].join(",");
                let mut query = sql(c.prepare(&format!("SELECT identity_key FROM episode_watch_states WHERE library_root_id=? AND watched=1 AND scope_key IN ({placeholders})")))?;
                let args = std::iter::once(rusqlite::types::Value::Integer(root))
                    .chain(chunk.iter().cloned().map(rusqlite::types::Value::Text));
                for key in
                    sql(query
                        .query_map(rusqlite::params_from_iter(args), |r| r.get::<_, String>(0)))?
                {
                    self.watched.insert((root, sql(key)?));
                    if self.watched.len() > 100_000 {
                        return Err("EPISODE_WATCH_LIMIT".into());
                    }
                }
                self.scopes
                    .extend(chunk.iter().cloned().map(|scope| (root, scope)));
            }
        }
        Ok(contexts
            .iter()
            .zip(batches)
            .map(|(separate, rows)| self.count_rows(&rows, *separate))
            .collect())
    }
    fn count_rows(&self, rows: &[Row], separate_scopes: bool) -> (usize, usize) {
        let mut episodes = BTreeMap::new();
        let (mut watched_files, mut files) = (0, 0);
        for row in rows {
            if row.view.kind == "EPISODE" {
                for identity in &row.identities {
                    let key = episode_scope_key(identity, separate_scopes);
                    let watched = self
                        .watched
                        .contains(&(identity.root, identity.key.clone()));
                    episodes
                        .entry(key)
                        .and_modify(|value| *value |= watched)
                        .or_insert(watched);
                }
            } else {
                files += 1;
                watched_files += usize::from(
                    row.identities
                        .iter()
                        .all(|i| self.watched.contains(&(i.root, i.key.clone()))),
                );
            }
        }
        (
            episodes.values().filter(|v| **v).count() + watched_files,
            episodes.len() + files,
        )
    }
}
fn episode_key(
    identity: &Identity,
    detail: &NodeDetail,
    work_view: bool,
) -> (Option<String>, Option<u16>, Option<u16>) {
    episode_scope_key(
        identity,
        !work_view && detail.node.node_type == crate::models::NodeType::Container,
    )
}
fn episode_scope_key(
    identity: &Identity,
    separate: bool,
) -> (Option<String>, Option<u16>, Option<u16>) {
    (
        separate.then(|| identity.scope.clone()),
        identity.season,
        identity.episode,
    )
}
fn write_rows(
    c: &Connection,
    rows: &[Row],
    file_id: i64,
    watched: bool,
    merge_work_sources: bool,
) -> AppResult<()> {
    let selected = rows
        .iter()
        .find(|r| r.view.file_id == file_id)
        .ok_or("EPISODE_WATCH_INVALID")?;
    let episodes = selected
        .identities
        .iter()
        .filter_map(|i| i.episode.map(|episode| (i.season, episode)))
        .collect::<BTreeSet<_>>();
    let mut identities = BTreeMap::new();
    let scopes = selected
        .identities
        .iter()
        .map(|i| i.scope.as_str())
        .collect::<HashSet<_>>();
    for row in rows {
        for identity in &row.identities {
            let matches = if selected.view.kind == "EPISODE" {
                identity.episode.is_some_and(|e| {
                    episodes.contains(&(identity.season, e))
                        && (merge_work_sources || scopes.contains(identity.scope.as_str()))
                })
            } else {
                row.view.file_id == file_id
            };
            if matches {
                identities
                    .entry((identity.root, identity.key.clone()))
                    .or_insert(identity);
            }
        }
    }
    // Keep the clicked file as provenance when several encodes share the same identity.
    for identity in &selected.identities {
        identities.insert((identity.root, identity.key.clone()), identity);
    }
    for identity in identities.into_values() {
        sql(c.execute("INSERT INTO episode_watch_states(library_root_id,identity_key,scope_key,identity_kind,source_node_id,source_file_id,source_name,season,episode,watched,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(library_root_id,identity_key) DO UPDATE SET watched=excluded.watched,revision=episode_watch_states.revision+1,source_node_id=excluded.source_node_id,source_file_id=excluded.source_file_id,source_name=excluded.source_name,updated_at=excluded.updated_at",params![identity.root,identity.key,identity.scope,identity.kind,identity.owner,identity.file,identity.name,identity.season,identity.episode,watched,chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos,true)]))?;
    }

    Ok(())
}

impl Database {
    pub(crate) fn record_mpv_file(
        &self,
        root_id: i64,
        root_path: &str,
        paths: &[String],
        initial_file: Option<i64>,
    ) -> AppResult<Option<(i64, i64)>> {
        if paths.is_empty() || paths.len() > 8 {
            return Ok(None);
        }
        let mut c = self.connect()?;
        let tx = sql(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
        let current: Option<String> = sql(tx
            .query_row(
                "SELECT path FROM library_roots WHERE id=?1",
                [root_id],
                |r| r.get(0),
            )
            .optional())?;
        if current.as_deref() != Some(root_path) {
            return Ok(None);
        }
        let mut selected = None;
        for path in paths {
            let found:Option<(i64,i64)>=sql(tx.query_row("SELECT f.id,f.node_id FROM media_files f JOIN nodes n ON n.id=f.node_id WHERE n.library_root_id=?1 AND f.absolute_path=?2 COLLATE NOCASE",params![root_id,path],|r|Ok((r.get(0)?,r.get(1)?))).optional())?;
            if let Some(found) = found {
                if selected.is_some_and(|previous| previous != found) {
                    return Ok(None);
                }
                selected = Some(found);
            }
        }
        let Some((file_id, node_id)) = selected else {
            return Ok(None);
        };
        if crate::db::ensure_node_visible_conn(&tx, node_id).is_err() {
            return Ok(None);
        }
        let index = crate::logical_works::LogicalWorkIndex::load_for_detail(&tx)?;
        let owner = index.owners.get(&node_id).copied().unwrap_or(node_id);
        let (_, rows) = snapshot_conn(&tx, owner, false)?;
        if rows.iter().any(|row| row.view.file_id == file_id) {
            write_rows(&tx, &rows, file_id, true, false)?;
        }
        if initial_file != Some(file_id) {
            sql(tx.execute("INSERT INTO watch_history(node_id,last_watched_at,watch_count) VALUES(?1,strftime('%Y-%m-%dT%H:%M:%fZ','now'),1) ON CONFLICT(node_id) DO UPDATE SET last_watched_at=excluded.last_watched_at,watch_count=watch_history.watch_count+1",[node_id]))?;
            crate::collection::record_recent(&tx, crate::collection::Target::Node(node_id))?;
        }
        sql(tx.commit())?;
        Ok(Some((file_id, owner)))
    }
    pub fn episode_watch_snapshot(
        &self,
        node_id: i64,
        work_view: bool,
    ) -> AppResult<EpisodeWatchSnapshot> {
        self.read_snapshot(|c| snapshot_conn(c, node_id, work_view).map(|r| r.0))
    }
    pub fn set_episode_watched(
        &self,
        node_id: i64,
        work_view: bool,
        file_id: i64,
        watched: bool,
        expected: &str,
    ) -> AppResult<EpisodeWatchSnapshot> {
        if node_id <= 0 || file_id <= 0 || expected.len() != 64 {
            return Err("EPISODE_WATCH_INVALID".into());
        }
        let mut c = self.connect()?;
        let tx = sql(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
        let (before, rows) = snapshot_conn(&tx, node_id, work_view)?;
        if before.snapshot != expected {
            return Err("EPISODE_WATCH_STALE".into());
        }
        write_rows(&tx, &rows, file_id, watched, work_view)?;
        let after = snapshot_conn(&tx, node_id, work_view)?.0;
        sql(tx.commit())?;
        Ok(after)
    }

    pub fn clear_episode_watch(
        &self,
        node_id: i64,
        work_view: bool,
        expected: &str,
    ) -> AppResult<EpisodeWatchSnapshot> {
        if node_id <= 0 || expected.len() != 64 {
            return Err("EPISODE_WATCH_INVALID".into());
        }
        let mut c = self.connect()?;
        let tx = sql(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
        let (before, rows) = snapshot_conn(&tx, node_id, work_view)?;
        if before.snapshot != expected {
            return Err("EPISODE_WATCH_STALE".into());
        }
        let identities = rows
            .iter()
            .flat_map(|row| &row.identities)
            .map(|i| (i.root, i.key.as_str()))
            .collect::<BTreeSet<_>>();
        for (root, key) in identities {
            sql(tx.execute("UPDATE episode_watch_states SET watched=0,revision=revision+1,updated_at=?3 WHERE library_root_id=?1 AND identity_key=?2 AND watched=1",params![root,key,chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos,true)]))?;
        }
        let after = snapshot_conn(&tx, node_id, work_view)?.0;
        sql(tx.commit())?;
        Ok(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn card_count_projection_matches_full_watch_snapshot_across_modes_and_mutations() {
        for mode in ["FOLDER", "VIDEO_FILE"] {
            for names in [
                vec![
                    "Show S01E01.mkv",
                    "Show S01E01 2160p.mkv",
                    "Show S01E01-E02.mkv",
                    "Show S02E01.mkv",
                    "Show NCOP01.mkv",
                    "Show SP01.mkv",
                ],
                vec!["Movie.mkv"],
                vec![
                    "Show EP01.mkv",
                    "Show S02E02.mkv",
                    "Show Trailer.mkv",
                    "Show S01E01 Extended Cut.mkv",
                ],
            ] {
                let (_temp, db) = fixture(&names, mode);
                let verify = || {
                    db.read_snapshot(|c| {
                        let mut reader = PresentationReader::default();
                        let mut graph = crate::logical_works::LogicalWorkIndex::load_for_detail(c)?;
                        for work_view in [false, true, false] {
                            let detail =
                                crate::works::progress_detail_conn(c, &mut graph, 2, work_view)?;
                            let (full, _) = snapshot_detail_conn(c, &detail, 2, work_view)?;
                            let original_rows = build_rows_with(
                                c,
                                &detail,
                                work_view,
                                &mut PresentationReader::default(),
                                true,
                            )?;
                            let original =
                                snapshot_rows_conn(c, &detail, 2, work_view, original_rows)?.0;
                            assert_eq!(
                                serde_json::to_value(&full).unwrap(),
                                serde_json::to_value(original).unwrap(),
                                "cached mutation signatures must match the uncached reference"
                            );
                            let expected = (
                                full.watched_episodes + full.watched_files,
                                full.total_episodes + full.total_files,
                            );
                            assert_eq!(
                                reader.counts(c, &detail, work_view)?,
                                expected,
                                "{mode} {names:?}"
                            );
                            assert_eq!(presentation_counts(c, &detail, work_view)?, expected);
                        }
                        Ok(())
                    })
                    .unwrap();
                    for work_view in [false, true] {
                        let response =
                            crate::works::presentation_detail(&db, 2, work_view).unwrap();
                        let expected = db.episode_watch_snapshot(2, work_view).unwrap();
                        assert_eq!(
                            serde_json::to_value(response.episode_watch.unwrap()).unwrap(),
                            serde_json::to_value(expected).unwrap(),
                            "inline detail state must match the standalone transactional snapshot"
                        );
                        let expected_detail =
                            crate::works::reference_detail(&db, 2, work_view).unwrap();
                        assert_eq!(
                            serde_json::to_value(response.detail).unwrap(),
                            serde_json::to_value(expected_detail).unwrap()
                        );
                    }
                };
                verify();
                let before = db.episode_watch_snapshot(2, false).unwrap();
                if let Some(row) = before.rows.first() {
                    let after = db
                        .set_episode_watched(2, false, row.file_id, true, &before.snapshot)
                        .unwrap();
                    verify();
                    db.clear_episode_watch(2, false, &after.snapshot).unwrap();
                    verify();
                }
                let c = db.connect().unwrap();
                c.execute(
                    "UPDATE media_files SET file_name='Show S09E21-E22.mkv' WHERE id=1",
                    [],
                )
                .unwrap();
                verify();
                let rejected = c
                    .execute(
                        "UPDATE library_roots SET recognition_mode=?1 WHERE id=1",
                        [if mode == "FOLDER" {
                            "VIDEO_FILE"
                        } else {
                            "FOLDER"
                        }],
                    )
                    .unwrap_err();
                assert!(rejected
                    .to_string()
                    .contains("LIBRARY_RECOGNITION_MODE_IMMUTABLE"));
                verify();
                c.execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(2,'BANGUMI',999,2,'Changed identity')",[]).unwrap();
                verify();
            }
        }
    }
    #[test]
    fn titles_are_not_supplements_and_real_sps_keep_independent_file_progress() {
        for title in ["Special A", "Bonus Family", "番外家族"] {
            let names = [
                format!("{title} S01E01.mkv"),
                format!("{title} NCOP.mkv"),
                format!("{title} S00E01.mkv"),
            ];
            let (_temp, db) = fixture(
                &names.iter().map(String::as_str).collect::<Vec<_>>(),
                "FOLDER",
            );
            let c = db.connect().unwrap();
            c.execute(
                "UPDATE nodes SET folder_name=?1,display_name=?1 WHERE id=2",
                [title],
            )
            .unwrap();
            let detail = crate::works::node_detail(&db, 2).unwrap();
            use crate::video_presentation::VideoSection;
            assert_eq!(
                detail
                    .video_sections
                    .iter()
                    .find(|r| r.file_id == 1)
                    .map(|r| r.section)
                    .unwrap_or(VideoSection::Main),
                VideoSection::Main,
                "{title}"
            );
            assert!(detail
                .video_sections
                .iter()
                .filter(|r| r.file_id != 1)
                .all(|r| r.section == VideoSection::Sps));
            let before = db.episode_watch_snapshot(2, false).unwrap();
            assert_eq!((before.total_episodes, before.total_files), (1, 2));
            let marked = db
                .set_episode_watched(2, false, 2, true, &before.snapshot)
                .unwrap();
            assert_eq!((marked.watched_episodes, marked.watched_files), (0, 1));
            let marked = db
                .set_episode_watched(2, false, 1, true, &marked.snapshot)
                .unwrap();
            assert_eq!((marked.watched_episodes, marked.watched_files), (1, 1));
            let cleared = db.clear_episode_watch(2, false, &marked.snapshot).unwrap();
            assert_eq!((cleared.watched_episodes, cleared.watched_files), (0, 0));
        }
    }
    #[test]
    fn series_sums_independent_children_without_collapsing_equal_episode_numbers() {
        let (_temp, db) = fixture(&[], "FOLDER");
        let c = db.connect().unwrap();
        c.execute("UPDATE nodes SET node_type='CONTAINER',total_video_count=2,child_media_branch_count=2 WHERE id=2",[]).unwrap();
        let base: String = c
            .query_row("SELECT absolute_path FROM nodes WHERE id=2", [], |r| {
                r.get(0)
            })
            .unwrap();
        for (id, name) in [(3, "First Story"), (4, "Second Story")] {
            let path = Path::new(&base).join(name);
            c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,manual_type_override,direct_video_count,total_video_count) VALUES(?1,1,2,?2,?3,?3,'WORK',1,1,1)",params![id,path.to_string_lossy(),name]).unwrap();
            let file = format!("{name} S01E01.mkv");
            c.execute("INSERT INTO media_files(id,node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(?1,?2,?3,?4,'mkv',100,'2026-10-26')",params![id-2,id,path.join(&file).to_string_lossy(),file]).unwrap();
        }
        let before = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(before.total_episodes, 2);
        let first_only = db
            .set_episode_watched(2, false, 1, true, &before.snapshot)
            .unwrap();
        assert_eq!(first_only.watched_episodes, 1);
        assert_eq!(
            db.episode_watch_snapshot(4, false)
                .unwrap()
                .watched_episodes,
            0
        );
        db.clear_episode_watch(2, false, &first_only.snapshot)
            .unwrap();
        let child = db.episode_watch_snapshot(3, false).unwrap();
        db.set_episode_watched(3, false, 1, true, &child.snapshot)
            .unwrap();
        let series = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!((series.watched_episodes, series.total_episodes), (1, 2));
        let series = db
            .set_episode_watched(2, false, 2, true, &series.snapshot)
            .unwrap();
        assert_eq!(series.watched_episodes, 2);
        assert_eq!(
            db.episode_watch_snapshot(4, false)
                .unwrap()
                .watched_episodes,
            1
        );
        db.clear_episode_watch(2, false, &series.snapshot).unwrap();
        assert_eq!(
            db.episode_watch_snapshot(3, false)
                .unwrap()
                .watched_episodes,
            0
        );
        assert_eq!(
            db.episode_watch_snapshot(4, false)
                .unwrap()
                .watched_episodes,
            0
        );
        assert!(db.clear_episode_watch(2, false, &before.snapshot).is_err());
        c.execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(3,'BANGUMI',900001,2,'Same Story'),(4,'BANGUMI',900001,2,'Same Story')",[]).unwrap();
        let same_work = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(same_work.total_episodes, 1); // Duplicate editions of one validated identity count once.
        assert_eq!(same_work.rows.len(), 2); // Original file actions remain independent destinations.
        let all = db.list_all_resources().unwrap();
        let group = all
            .works
            .iter()
            .find(|group| group.sources.len() == 2)
            .expect("two editions stay in the same validated group");
        let reference = db.episode_watch_snapshot(group.node.id, true).unwrap();
        let progress = group
            .node
            .card_progress
            .as_ref()
            .expect("inline grouped progress");
        assert_eq!(
            (progress.watched, progress.total),
            (
                reference.watched_episodes + reference.watched_files,
                reference.total_episodes + reference.total_files
            )
        );
    }
    fn fixture(names: &[&str], mode: &str) -> (tempfile::TempDir, Database) {
        let temp = crate::db::test_temp_dir();
        let db = Database::new(temp.path().join("episodes.db"));
        db.migrate().unwrap();
        let c = db.connect().unwrap();
        let root = temp.path().join("Synthetic");
        c.execute("INSERT INTO library_roots(id,path,display_name,media_kind,video_subject_scope,recognition_mode) VALUES(1,?1,'Synthetic','VIDEO','ANIMATION',?2)",params![root.to_string_lossy(),mode]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(1,1,?1,'Synthetic','Synthetic','CONTAINER')",[root.to_string_lossy()]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,manual_type_override,direct_video_count,total_video_count) VALUES(2,1,1,?1,'Example Show','Example Show','WORK',1,?2,?2)",params![root.join("Example Show").to_string_lossy(),names.len() as i64]).unwrap();
        for (index, name) in names.iter().enumerate() {
            c.execute("INSERT INTO media_files(id,node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(?1,2,?2,?3,'mkv',1234,'2026-10-24')",params![index as i64+1,root.join("Example Show").join(name).to_string_lossy(),name]).unwrap();
        }
        (temp, db)
    }
    #[test]
    fn library_projection_reuses_inputs_and_reads_watch_values_fresh() {
        for mode in ["FOLDER", "VIDEO_FILE"] {
            let (_temp, db) = fixture(
                &[
                    "Example Show S01E01.mkv",
                    "Example Show S01E02.mkv",
                    "Example Show NCOP.mkv",
                ],
                mode,
            );
            let verify = || {
                db.read_snapshot(|c| {
                    let mut nodes = crate::db::list_children_conn(c, 1)?;
                    crate::card_progress::hydrate_library_nodes_conn(c, 1, &mut nodes)?;
                    let actual = nodes[0].card_progress.as_ref().unwrap();
                    let expected = snapshot_conn(c, 2, false)?.0;
                    assert_eq!(
                        (actual.watched, actual.total),
                        (
                            expected.watched_episodes + expected.watched_files,
                            expected.total_episodes + expected.total_files
                        )
                    );
                    let stamp = library_projection_stamp(c, 1)?;
                    let pure = cached_identity_rows(&library_row_key(&stamp, 2))
                        .expect("root alias cached");
                    assert!(pure.iter().all(|r| !r.view.watched
                        && !r.view.partially_watched
                        && r.signature.is_null()));
                    let hot = PresentationReader::default().library_counts(c, 1, &nodes)?;
                    assert_eq!(hot[0], Some((actual.watched, actual.total)));
                    Ok(stamp)
                })
                .unwrap()
            };
            let stamp = verify();
            let start = db.episode_watch_snapshot(2, false).unwrap();
            let marked = db
                .set_episode_watched(2, false, 1, true, &start.snapshot)
                .unwrap();
            assert_eq!(stamp, verify());
            db.clear_episode_watch(2, false, &marked.snapshot).unwrap();
            assert_eq!(stamp, verify());
            let c = db.connect().unwrap();
            for change in [
                "UPDATE media_files SET file_name='Example Show S03E11.mkv' WHERE id=1",
                "UPDATE media_files SET file_size=9999 WHERE id=2",
                "UPDATE nodes SET display_name='Manual season name',manual_display_name=1 WHERE id=2",
                "INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(2,'BANGUMI',998,2,'New provider title')",
                "UPDATE metadata_bindings SET provider_subject_id=999 WHERE node_id=2",
                "DELETE FROM media_files WHERE id=2",
            ] {let before=verify();c.execute(change,[]).unwrap();assert_ne!(before,verify(),"{change}");}
        }
    }
    #[test]
    fn library_projection_stays_in_the_read_snapshot_during_a_write() {
        let (_temp, db) = fixture(
            &["Example Show S01E01.mkv", "Example Show S01E02.mkv"],
            "FOLDER",
        );
        let old = db
            .read_snapshot(|c| {
                let mut nodes = crate::db::list_children_conn(c, 1)?;
                crate::card_progress::hydrate_library_nodes_conn(c, 1, &mut nodes)?;
                let stamp = library_projection_stamp(c, 1)?;
                db.connect()?
                    .execute(
                        "UPDATE media_files SET file_name='Example Show NCOP.mkv' WHERE id=1",
                        [],
                    )
                    .unwrap();
                assert_eq!(stamp, library_projection_stamp(c, 1)?);
                assert_eq!(
                    PresentationReader::default().library_counts(c, 1, &nodes)?[0],
                    Some((0, 2))
                );
                Ok(stamp)
            })
            .unwrap();
        db.read_snapshot(|c| {
            assert_ne!(old, library_projection_stamp(c, 1)?);
            let mut nodes = crate::db::list_children_conn(c, 1)?;
            crate::card_progress::hydrate_library_nodes_conn(c, 1, &mut nodes)?;
            let expected = snapshot_conn(c, 2, false)?.0;
            assert_eq!(
                nodes[0].card_progress.as_ref().unwrap().total,
                expected.total_episodes + expected.total_files
            );
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn library_projection_fences_outgoing_moves_and_separate_database_copies() {
        let (temp, db) = fixture(&["Example Show S01E01.mkv"], "FOLDER");
        let c = db.connect().unwrap();
        let before = library_projection_stamp(&c, 1).unwrap();
        let copy = Database::new(temp.path().join("copy.db"));
        c.execute("VACUUM INTO ?1", [copy.path().to_string_lossy()])
            .unwrap();
        assert_ne!(
            before,
            library_projection_stamp(&copy.connect().unwrap(), 1).unwrap()
        );
        c.execute("INSERT INTO library_roots(id,path,display_name,media_kind,video_subject_scope) VALUES(2,?1,'Other','VIDEO','ANIMATION')",[temp.path().join("Other").to_string_lossy()]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(3,2,?1,'Other','Other','WORK')",[temp.path().join("Other").to_string_lossy()]).unwrap();
        let revision = || {
            c.query_row(
                "SELECT revision FROM book_organization_state WHERE root_id=1",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
        };
        let generation = revision();
        c.execute("UPDATE media_files SET node_id=3 WHERE id=1", [])
            .unwrap();
        assert_eq!(generation, revision()); // Old trigger updates only the destination.
        let moved_file = library_projection_stamp(&c, 1).unwrap();
        assert_ne!(before, moved_file);
        c.execute(
            "UPDATE nodes SET library_root_id=2,parent_node_id=3 WHERE id=2",
            [],
        )
        .unwrap();
        assert_eq!(generation, revision());
        assert_ne!(moved_file, library_projection_stamp(&c, 1).unwrap());
        c.execute("DELETE FROM book_organization_state WHERE root_id=1", [])
            .unwrap();
        let fallback = library_projection_stamp(&c, 1).unwrap();
        c.execute("UPDATE nodes SET display_name='Changed' WHERE id=1", [])
            .unwrap();
        assert_ne!(fallback, library_projection_stamp(&c, 1).unwrap());
    }
    #[test]
    fn lazy_card_summary_matches_watch_identity_and_invalidates_after_clear() {
        let (_temp, db) = fixture(
            &[
                "Example Show S01E01.mkv",
                "Example Show S01E02.mkv",
                "Example Show NCOP.mkv",
            ],
            "FOLDER",
        );
        let requests = [crate::card_progress::Request {
            node_id: 2,
            work_view: false,
        }];
        let read = || {
            crate::card_progress::read(&db, &requests)
                .unwrap()
                .remove(0)
        };
        let start = read();
        assert_eq!((start.watched, start.total, start.started), (0, 3, false));
        let snapshot = db.episode_watch_snapshot(2, false).unwrap();
        db.set_episode_watched(2, false, 2, true, &snapshot.snapshot)
            .unwrap();
        let progress = read();
        assert_eq!(
            (progress.watched, progress.total, progress.started),
            (1, 3, true)
        );
        let snapshot = db.episode_watch_snapshot(2, false).unwrap();
        db.clear_episode_watch(2, false, &snapshot.snapshot)
            .unwrap();
        assert_eq!(read().watched, 0);
        let c = db.connect().unwrap();
        c.execute(
            "UPDATE media_files SET file_name='Example Show NCED.mkv' WHERE id=2",
            [],
        )
        .unwrap();
        assert_eq!(read().total, 3);
        assert!(crate::card_progress::read(&db, &vec![requests[0].clone(); 17]).is_err());
        assert!(crate::card_progress::read(
            &db,
            &[crate::card_progress::Request {
                node_id: 0,
                work_view: false
            }]
        )
        .is_err());
    }
    #[test]
    fn sections_use_existing_sps_directory_and_filename_evidence_without_losing_files() {
        let (_temp, db) = fixture(
            &[
                "Example Show S01E01.mkv",
                "Example Show NCOP.mkv",
                "Example Show S00E01.mkv",
            ],
            "FOLDER",
        );
        let detail = crate::works::node_detail(&db, 2).unwrap();
        use crate::video_presentation::{classify, VideoSection};
        assert_eq!(detail.video_sections.len(), 2);
        assert_eq!(
            detail
                .video_sections
                .iter()
                .find(|e| e.file_id == 1)
                .map(|r| r.section)
                .unwrap_or(VideoSection::Main),
            VideoSection::Main
        );
        assert_eq!(
            classify(
                detail.media_files.iter().find(|f| f.id == 1).unwrap(),
                &detail.node,
                "SPs"
            ),
            VideoSection::Sps
        );
        assert_eq!(
            classify(
                detail.media_files.iter().find(|f| f.id == 1).unwrap(),
                &detail.node,
                "Disc 1"
            ),
            VideoSection::Main
        );
        let mut unknown = detail.media_files[0].clone();
        unknown.file_name = "制作记录.mkv".into();
        assert_eq!(
            classify(&unknown, &detail.node, "unclassified"),
            VideoSection::Other
        );
        assert!(
            detail
                .video_sections
                .iter()
                .filter(|e| e.section == VideoSection::Sps)
                .count()
                >= 2
        );
    }

    #[test]
    fn direct_and_disc_section_fast_path_preserves_season_zero_and_supplements() {
        let (_temp, db) = fixture(&["Example Show [01] [1080p].mkv"], "FOLDER");
        let detail = crate::works::node_detail(&db, 2).unwrap();
        use crate::video_presentation::{classify, VideoSection};
        let mut file = detail.media_files[0].clone();
        for (name, expected) in [
            ("Example Show [01] [1080p].mkv", VideoSection::Main),
            ("Example Show EP01.mkv", VideoSection::Main),
            ("Example Show S01E01.mkv", VideoSection::Main),
            ("Example Show S00E01.mkv", VideoSection::Sps),
            ("Example Show 0x01.mkv", VideoSection::Sps),
            ("Example Show ０ｘ０１.mkv", VideoSection::Sps),
            ("Example Show NCOP.mkv", VideoSection::Sps),
        ] {
            file.file_name = name.into();
            for directory in ["", "Disc 1"] {
                assert_eq!(
                    classify(&file, &detail.node, directory),
                    expected,
                    "{name}/{directory}"
                );
            }
        }
    }

    #[test]
    fn mpv_updates_only_current_indexed_root_and_preserves_movie_supplement_and_hidden_boundaries()
    {
        let (_temp, db) = fixture(
            &[
                "Example Show S01E01.mkv",
                "Example Show S01E02.mkv",
                "Example Show NCOP.mkv",
            ],
            "FOLDER",
        );
        let c = db.connect().unwrap();
        let root: String = c
            .query_row("SELECT path FROM library_roots WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        let file = db.get_media_file(2).unwrap();
        assert!(db
            .record_mpv_file(
                1,
                "stale root",
                std::slice::from_ref(&file.absolute_path),
                None
            )
            .unwrap()
            .is_none());
        assert!(db
            .record_mpv_file(99, &root, std::slice::from_ref(&file.absolute_path), None)
            .unwrap()
            .is_none());
        assert!(db
            .record_mpv_file(1, &root, &["X:/Unindexed/file.mkv".into()], None)
            .unwrap()
            .is_none());
        assert_eq!(
            db.record_mpv_file(1, &root, std::slice::from_ref(&file.absolute_path), Some(2))
                .unwrap(),
            Some((2, 2))
        );
        assert_eq!(
            db.episode_watch_snapshot(2, false)
                .unwrap()
                .latest_watched_file_id,
            Some(2)
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM watch_history", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        ); // launch already owns initial history
        let supplement = db.get_media_file(3).unwrap();
        db.record_mpv_file(
            1,
            &root,
            std::slice::from_ref(&supplement.absolute_path),
            None,
        )
        .unwrap();
        assert_eq!(
            db.episode_watch_snapshot(2, false)
                .unwrap()
                .latest_watched_file_id,
            Some(3)
        );
        c.execute("UPDATE nodes SET node_type='IGNORED' WHERE id=2", [])
            .unwrap();
        assert!(db
            .record_mpv_file(1, &root, std::slice::from_ref(&file.absolute_path), None)
            .unwrap()
            .is_none());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM collection_states", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn series_keeps_direct_controls_and_summarizes_independent_children_with_original_ids() {
        let (_temp, db) = fixture(
            &["Example Show S01E01.mkv", "Example Show S01E02.mkv"],
            "FOLDER",
        );
        let c = db.connect().unwrap();
        c.execute("UPDATE nodes SET node_type='CONTAINER' WHERE id=2", [])
            .unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,manual_type_override,direct_video_count,total_video_count) VALUES(3,1,2,'X:/Synthetic/Other','Other Show','Other Show','WORK',1,1,1)",[]).unwrap();
        c.execute("INSERT INTO media_files(id,node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(20,3,'X:/Synthetic/Other/20.mkv','Other Show S01E01.mkv','mkv',1,'stamp')",[]).unwrap();
        let series = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(series.rows.len(), 3);
        assert!(series.rows.iter().any(|row| row.file_id == 20));
        let marked = db
            .set_episode_watched(2, false, 2, true, &series.snapshot)
            .unwrap();
        assert_eq!(marked.latest_watched_file_id, Some(2));
        assert_eq!(
            db.episode_watch_snapshot(3, false)
                .unwrap()
                .watched_episodes,
            0
        );
        assert!(db.episode_watch_snapshot(1, false).unwrap().rows.is_empty());
        db.read_snapshot(|c| {
            let mut index = crate::logical_works::LogicalWorkIndex::load_for_detail(c)?;
            let detail = crate::works::progress_detail_conn(c, &mut index, 2, false)?;
            let rows =
                build_rows_with(c, &detail, false, &mut PresentationReader::default(), true)?;
            let reference = snapshot_rows_conn(c, &detail, 2, false, rows)?.0;
            assert_eq!(
                serde_json::to_value(reference).unwrap(),
                serde_json::to_value(db.episode_watch_snapshot(2, false).unwrap()).unwrap()
            );
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn owner_cache_retains_enclosing_single_file_context() {
        let (_temp, db) = fixture(&["Unlabelled.mkv"], "FOLDER");
        assert!(db.episode_watch_snapshot(2, false).unwrap().rows.is_empty());
        let c = db.connect().unwrap();
        c.execute("UPDATE nodes SET node_type='CONTAINER' WHERE id=2", [])
            .unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,manual_type_override,direct_video_count,total_video_count) VALUES(3,1,2,'X:/Synthetic/Child','Child','Child','WORK',1,1,1)",[]).unwrap();
        c.execute("INSERT INTO media_files(id,node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(20,3,'X:/Synthetic/Child/Unlabelled.mkv','Unlabelled.mkv','mkv',1,'stamp')",[]).unwrap();
        assert!(db.episode_watch_snapshot(3, false).unwrap().rows.is_empty());
        let series = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(series.rows.len(), 2);
        assert_eq!(series.total_files, 2);
        let inline = crate::works::presentation_detail(&db, 2, false)
            .unwrap()
            .episode_watch
            .unwrap();
        assert_eq!(
            serde_json::to_value(&inline).unwrap(),
            serde_json::to_value(&series).unwrap()
        );
        let marked = db
            .set_episode_watched(2, false, 20, true, &inline.snapshot)
            .unwrap();
        assert_eq!(marked.watched_files, 1);
        assert!(db.episode_watch_snapshot(3, false).unwrap().rows.is_empty());
    }
    #[test]
    fn latest_check_is_persistent_file_provenance_and_clear_is_atomic_scoped_and_stale_safe() {
        let (_temp, db) = fixture(
            &[
                "Example Show S01E01.mkv",
                "Example Show S01E02.mkv",
                "Example Show NCOP01.mkv",
            ],
            "FOLDER",
        );
        let start = db.episode_watch_snapshot(2, false).unwrap();
        let second = db
            .set_episode_watched(2, false, 2, true, &start.snapshot)
            .unwrap();
        let first = db
            .set_episode_watched(2, false, 1, true, &second.snapshot)
            .unwrap();
        assert_eq!(first.latest_watched_file_id, Some(1));
        assert_eq!(
            first.latest_watched_file_name.as_deref(),
            Some("Example Show S01E01.mkv")
        );
        assert!(db
            .clear_episode_watch(2, false, &start.snapshot)
            .unwrap_err()
            .contains("STALE"));
        let reopened = Database::new(db.path().to_path_buf());
        assert_eq!(
            reopened
                .episode_watch_snapshot(2, false)
                .unwrap()
                .latest_watched_file_id,
            Some(1)
        );
        let undone = db
            .set_episode_watched(2, false, 1, false, &first.snapshot)
            .unwrap();
        assert_eq!(undone.latest_watched_file_id, Some(2));
        let c = db.connect().unwrap();
        c.execute("INSERT INTO episode_watch_states(library_root_id,identity_key,scope_key,identity_kind,source_node_id,source_file_id,source_name,season,episode,watched) VALUES(1,?1,?2,'EPISODE',2,NULL,'unresolved',1,50,1)",params!["c".repeat(64),"d".repeat(64)]).unwrap();
        let fresh = db.episode_watch_snapshot(2, false).unwrap();
        let cleared = db.clear_episode_watch(2, false, &fresh.snapshot).unwrap();
        assert_eq!(cleared.watched_episodes, 0);
        assert_eq!(cleared.latest_watched_file_id, None);
        assert_eq!(cleared.unresolved_records, 1);
        assert!(cleared
            .rows
            .iter()
            .all(|row| !row.watched && !row.partially_watched));
        assert_eq!(
            c.query_row(
                "SELECT watched FROM episode_watch_states WHERE source_name='unresolved'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM collection_states", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        c.execute("UPDATE nodes SET node_type='IGNORED' WHERE id=2", [])
            .unwrap();
        assert!(db.clear_episode_watch(2, false, &cleared.snapshot).is_err());
    }
    #[test]
    fn unique_episode_versions_ranges_and_seasons_are_independent_of_playback_history() {
        let (_temp, db) = fixture(
            &[
                "Example Show S01E01 1080p.mkv",
                "Example Show S01E01 2160p.mkv",
                "Example Show S01E01-E02.mkv",
                "Example Show S02E01.mkv",
                "Example Show NCOP01.mkv",
                "Example Show SP01.mkv",
            ],
            "FOLDER",
        );
        let before = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(before.total_episodes, 3);
        assert_eq!(before.rows.len(), 6);
        assert_eq!(before.total_files, 2);
        let after = db
            .set_episode_watched(2, false, 3, true, &before.snapshot)
            .unwrap();
        assert_eq!(after.watched_episodes, 2);
        assert!(after
            .rows
            .iter()
            .filter(|r| r.season == Some(1))
            .all(|r| r.watched));
        assert!(
            !after
                .rows
                .iter()
                .find(|r| r.season == Some(2))
                .unwrap()
                .watched
        );
        assert!(db
            .set_episode_watched(2, false, 4, true, &before.snapshot)
            .unwrap_err()
            .contains("STALE"));
        db.record_node_watched(2).unwrap();
        let played = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(played.watched_episodes, 2);
        let undone = db
            .set_episode_watched(2, false, 1, false, &played.snapshot)
            .unwrap();
        assert_eq!(undone.watched_episodes, 1);
        assert!(
            undone
                .rows
                .iter()
                .find(|r| r.file_id == 3)
                .unwrap()
                .partially_watched
        );
        let reopened = Database::new(db.path().to_path_buf());
        assert_eq!(
            reopened
                .episode_watch_snapshot(2, false)
                .unwrap()
                .watched_episodes,
            1
        );
        let c = db.connect().unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM collection_states", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn stable_episode_survives_id_replacement_offline_and_relocation_but_changed_identity_does_not()
    {
        let (_temp, db) = fixture(
            &["Example Show S01E01.mkv", "Example Show S01E02.mkv"],
            "FOLDER",
        );
        let before = db.episode_watch_snapshot(2, false).unwrap();
        db.set_episode_watched(2, false, 1, true, &before.snapshot)
            .unwrap();
        let c = db.connect().unwrap();
        c.execute("DELETE FROM media_files WHERE id=1", []).unwrap();
        c.execute("INSERT INTO media_files(id,node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(50,2,'X:/Synthetic/Example Show/S01E01.mkv','Example Show S01E01.mkv','mkv',9876,'2026-10-25')",[]).unwrap();
        let after = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(after.watched_episodes, 1);
        assert!(after.rows.iter().find(|r| r.file_id == 50).unwrap().watched);
        c.execute(
            "UPDATE library_roots SET path='Z:\\Offline\\Synthetic' WHERE id=1",
            [],
        )
        .unwrap();
        c.execute(
            "UPDATE nodes SET absolute_path='Z:\\Offline\\Synthetic\\Show' WHERE id=2",
            [],
        )
        .unwrap();
        c.execute("UPDATE media_files SET absolute_path='Z:\\Offline\\Synthetic\\Show\\S01E01.mkv' WHERE id=50",[]).unwrap();
        assert_eq!(
            db.episode_watch_snapshot(2, false)
                .unwrap()
                .watched_episodes,
            1
        );
        c.execute(
            "UPDATE media_files SET file_name='Example Show S02E01.mkv' WHERE id=50",
            [],
        )
        .unwrap();
        let changed = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(changed.watched_episodes, 0);
        assert_eq!(changed.unresolved_records, 1);
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM episode_watch_states WHERE watched=1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        c.execute("UPDATE nodes SET node_type='IGNORED' WHERE id=2", [])
            .unwrap();
        assert!(db.episode_watch_snapshot(2, false).is_err());
        c.execute("UPDATE nodes SET node_type='WORK' WHERE id=2", [])
            .unwrap();
        c.execute("DELETE FROM library_roots WHERE id=1", [])
            .unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM episode_watch_states", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn no_guessed_episodes_for_movies_bare_numbers_or_file_mode() {
        let (_temp, db) = fixture(&["2024.mkv", "1080.mkv", "notes.mkv"], "FOLDER");
        let data = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(data.total_episodes, 0);
        assert_eq!(data.total_files, 3);
        assert!(data.rows.iter().all(|r| r.kind == "FILE"));
        db.connect()
            .unwrap()
            .execute(
                "UPDATE nodes SET folder_name='Example Movie 劇場版' WHERE id=2",
                [],
            )
            .unwrap();
        assert!(db.episode_watch_snapshot(2, false).unwrap().rows.is_empty());
        let (_temp, db) = fixture(
            &["Example Show S01E01.mkv", "Example Show S01E02.mkv"],
            "VIDEO_FILE",
        );
        let file = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(file.total_episodes, 0);
        assert_eq!(file.total_files, 2);
        let (_temp, single) = fixture(&["Example Film 2026.mkv"], "FOLDER");
        assert!(single
            .episode_watch_snapshot(2, false)
            .unwrap()
            .rows
            .is_empty());
    }
    #[test]
    fn malformed_requests_hidden_owners_and_cross_root_same_names_never_copy_status() {
        let (_temp, db) = fixture(
            &["Example Show S01E01.mkv", "Example Show S01E02.mkv"],
            "FOLDER",
        );
        let a = db.episode_watch_snapshot(2, false).unwrap();
        assert!(db
            .set_episode_watched(2, false, 999, true, &a.snapshot)
            .is_err());
        db.set_episode_watched(2, false, 1, true, &a.snapshot)
            .unwrap();
        let c = db.connect().unwrap();
        c.execute("INSERT INTO library_roots(id,path,display_name,media_kind,video_subject_scope) VALUES(2,'Y:\\Synthetic','Other','VIDEO','ANIMATION')",[]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(3,2,'Y:\\Synthetic','Other','Other','CONTAINER')",[]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,manual_type_override,direct_video_count,total_video_count) VALUES(4,2,3,'Y:\\Synthetic\\Show','Example Show','Example Show','WORK',1,2,2)",[]).unwrap();
        c.execute("INSERT INTO media_files(id,node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(10,4,'Y:\\Synthetic\\Show\\1.mkv','Example Show S01E01.mkv','mkv',1,'x'),(11,4,'Y:\\Synthetic\\Show\\2.mkv','Example Show S01E02.mkv','mkv',1,'x')",[]).unwrap();
        let b = db.episode_watch_snapshot(4, false).unwrap();
        assert_eq!(b.watched_episodes, 0);
        assert_eq!(b.unresolved_records, 0);
        c.execute("UPDATE nodes SET node_type='IGNORED' WHERE id=1", [])
            .unwrap();
        assert!(db
            .set_episode_watched(2, false, 1, false, &a.snapshot)
            .is_err());
    }
    #[test]
    fn twenty_four_episode_manual_flow_does_not_count_supplements_or_touch_collection() {
        let names = (1..=24)
            .map(|n| format!("Example Show EP{n:02} 1080p.mkv"))
            .collect::<Vec<_>>();
        let (_temp, db) = fixture(
            &names.iter().map(String::as_str).collect::<Vec<_>>(),
            "FOLDER",
        );
        let mut state = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(state.total_episodes, 24);
        for file in [1, 2, 8] {
            state = db
                .set_episode_watched(2, false, file, true, &state.snapshot)
                .unwrap();
        }
        assert_eq!(state.watched_episodes, 3);
        db.record_node_watched(2).unwrap();
        assert!(
            !db.episode_watch_snapshot(2, false)
                .unwrap()
                .rows
                .iter()
                .find(|r| r.file_id == 9)
                .unwrap()
                .watched
        );
        let c = db.connect().unwrap();
        c.execute("UPDATE nodes SET updated_at='2030-01-01' WHERE id=2", [])
            .unwrap();
        assert!(db
            .set_episode_watched(2, false, 9, true, &state.snapshot)
            .unwrap_err()
            .contains("STALE"));
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM collection_states", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn seasonless_episode_reuses_only_an_unambiguous_season_and_cuts_never_merge() {
        let (_temp, db) = fixture(
            &[
                "Example Show EP01.mkv",
                "Example Show S01E01.mkv",
                "Example Show S01E02 Extended.mkv",
            ],
            "FOLDER",
        );
        let state = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(state.total_episodes, 1);
        assert_eq!(state.total_files, 1);
        let state = db
            .set_episode_watched(2, false, 1, true, &state.snapshot)
            .unwrap();
        assert!(state.rows.iter().find(|r| r.file_id == 2).unwrap().watched);
        assert!(!state.rows.iter().find(|r| r.file_id == 3).unwrap().watched);
        db.connect()
            .unwrap()
            .execute(
                "UPDATE media_files SET file_name='Example Show S02E01.mkv' WHERE id=3",
                [],
            )
            .unwrap();
        let mixed = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(
            mixed.rows.iter().find(|r| r.file_id == 1).unwrap().kind,
            "FILE"
        );
    }
    #[test]
    fn aggregate_members_share_only_validated_episodes_and_reject_changed_membership() {
        let (_temp, db) = fixture(
            &["Example Show S01E01.mkv", "Example Show S01E02.mkv"],
            "FOLDER",
        );
        let c = db.connect().unwrap();
        c.execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(2,'BANGUMI',123,2,'Example Show')",[]).unwrap();
        let first = db.episode_watch_snapshot(2, true).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,manual_type_override,direct_video_count,total_video_count) VALUES(3,1,1,'X:/Synthetic/Release B','Example Show B','Example Show B','WORK',1,2,2)",[]).unwrap();
        c.execute("INSERT INTO media_files(id,node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(10,3,'X:/Synthetic/Release B/1.mkv','Example Show S01E01 2160p.mkv','mkv',1,'x'),(11,3,'X:/Synthetic/Release B/2.mkv','Example Show S01E02 2160p.mkv','mkv',1,'x')",[]).unwrap();
        c.execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(3,'BANGUMI',123,2,'Example Show')",[]).unwrap();
        assert!(db
            .set_episode_watched(2, true, 1, true, &first.snapshot)
            .unwrap_err()
            .contains("STALE"));
        let combined = db.episode_watch_snapshot(2, true).unwrap();
        assert_eq!(combined.total_episodes, 2);
        assert_eq!(combined.rows.len(), 4);
        let confirmed = db
            .set_episode_watched(2, true, 1, true, &combined.snapshot)
            .unwrap();
        assert_eq!(confirmed.watched_episodes, 1);
        assert!(
            confirmed
                .rows
                .iter()
                .find(|r| r.file_id == 10)
                .unwrap()
                .watched
        );
        assert!(
            db.episode_watch_snapshot(3, false)
                .unwrap()
                .rows
                .iter()
                .find(|r| r.file_id == 10)
                .unwrap()
                .watched
        );
        c.execute(
            "UPDATE metadata_bindings SET provider_subject_id=456 WHERE node_id=3",
            [],
        )
        .unwrap();
        let separated = db.episode_watch_snapshot(3, false).unwrap();
        assert_eq!(separated.watched_episodes, 0);
        assert_eq!(separated.unresolved_records, 0);
        c.execute(
            "UPDATE nodes SET folder_name='Example Show Extended Cut' WHERE id=2",
            [],
        )
        .unwrap();
        let cut = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(cut.total_episodes, 0);
        assert_eq!(cut.watched_files, 0);
        assert_eq!(cut.unresolved_records, 1);
    }
}

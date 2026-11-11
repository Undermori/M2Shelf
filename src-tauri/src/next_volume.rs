//! Conservative continuation over the existing book index. No source reads or writes.
use crate::{
    comics::{self, ComicBook},
    db::{self, AppResult, Database},
    scraper_metadata::Metadata,
};
use m2shelf_smart_mixed_lab::signals;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const MAX_BOOKS: usize = 4096;
const MAX_NODES: usize = 16384;
const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextVolumeResult {
    pub status: String,
    pub current: ComicBook,
    pub next: Option<ComicBook>,
    pub current_volume: Option<u32>,
    pub next_volume: Option<u32>,
    /// Excludes reading progress, bookmarks, notes and collection status.
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize)]
struct Node {
    id: i64,
    parent: Option<i64>,
    name: String,
    path: String,
    kind: String,
    manual: bool,
    binding: Option<(String, i64, i64)>,
}

#[derive(Debug, Clone, Serialize)]
struct Override {
    path: String,
    json: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize)]
struct Embedded {
    series: Option<String>,
    authors: Vec<String>,
    volume: Option<u32>,
    conflict: bool,
}

#[derive(Debug, Clone)]
struct Evidence {
    book: ComicBook,
    relative: String,
    title: String,
    volume: Option<u32>,
    number_source: &'static str,
    manual_series: Option<String>,
    embedded: Embedded,
    binding: Option<(String, i64, i64)>,
    extra: bool,
    independent: bool,
    continuation: bool,
    editions: Vec<String>,
}

fn error(e: rusqlite::Error) -> String {
    e.to_string()
}

fn path_key(path: &str) -> String {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn title_key(title: &str) -> String {
    title.trim().to_lowercase()
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn source_name(book: &ComicBook) -> String {
    let path = book.source_path.replace('\\', "/");
    basename(&path).to_owned()
}

fn relative_path(source: &str, root: &str) -> Option<String> {
    let source = source.replace('\\', "/");
    let root = root.replace('\\', "/");
    let root = root.trim_end_matches('/');
    if source.len() <= root.len()
        || !source.get(..root.len())?.eq_ignore_ascii_case(root)
        || source.as_bytes().get(root.len()) != Some(&b'/')
    {
        return None;
    }
    let relative = &source[root.len() + 1..];
    signals::normalize_path(relative).ok()
}

fn supported(book: &ComicBook) -> bool {
    book.source_kind == "IMAGE_FOLDER"
        || (book.source_kind == "ZIP_ARCHIVE"
            && (book.source_path.to_ascii_lowercase().ends_with(".cbz")
                || matches!(
                    book.document_format.as_deref(),
                    Some("PDF" | "EPUB" | "TXT" | "MOBI" | "AZW3")
                )))
}

fn ancestors(nodes: &BTreeMap<i64, Node>, id: i64) -> Vec<&Node> {
    let mut cursor = Some(id);
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    while let Some(id) = cursor {
        if result.len() >= 64 || !seen.insert(id) {
            break;
        }
        let Some(node) = nodes.get(&id) else { break };
        cursor = node.parent;
        result.push(node);
    }
    result
}

fn matching_overrides<'a>(relative: &str, overrides: &'a [Override]) -> Vec<&'a Override> {
    let key = path_key(relative);
    let mut matched = overrides
        .iter()
        .filter(|o| {
            let path = path_key(&o.path);
            key == path || (!path.is_empty() && key.starts_with(&format!("{path}/")))
        })
        .collect::<Vec<_>>();
    matched.sort_by_key(|o| std::cmp::Reverse(o.path.len()));
    matched
}

fn embedded(c: &Connection, root: i64, books: &[ComicBook]) -> AppResult<BTreeMap<i64, Embedded>> {
    let map = books.iter().map(|b| (b.id, b)).collect::<BTreeMap<_, _>>();
    let mut statement = c.prepare(
        "SELECT m.book_id,m.source_key,m.revision,m.payload_json FROM indexed_media_metadata m \
         JOIN comic_books b ON b.id=m.book_id JOIN nodes n ON n.id=b.node_id \
         WHERE n.library_root_id=?1 AND b.source_resource_id IS NULL ORDER BY m.book_id,m.source_key LIMIT 8193",
    ).map_err(error)?;
    let mut result = BTreeMap::<i64, Embedded>::new();
    let mut total = 0usize;
    for (count, row) in statement
        .query_map([root], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(error)?
        .enumerate()
    {
        let (id, source, revision, json) = row.map_err(error)?;
        total = total.saturating_add(json.len());
        if count >= 8192 || total > MAX_METADATA_BYTES || json.len() > 65536 {
            return Err("NEXT_VOLUME_LIMIT".into());
        }
        let Some(book) = map.get(&id) else { continue };
        // Only the existing, current file's own imported evidence establishes a series.
        if path_key(&source) != path_key(&book.source_path)
            || revision != format!("1:{}:{}", book.source_size, book.modified_at)
        {
            continue;
        }
        let Ok(metadata) = serde_json::from_str::<Metadata>(&json) else {
            continue;
        };
        if !metadata.warnings.is_empty() {
            continue;
        }
        let value = result.entry(id).or_default();
        let series = metadata
            .series
            .map(|s| title_key(&s))
            .filter(|s| !s.is_empty());
        let volume = metadata.volume.filter(|n| (1..=9999).contains(n));
        let mut authors = metadata
            .authors
            .iter()
            .map(|s| title_key(s))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        authors.sort();
        authors.dedup();
        value.conflict |= (value.series.is_some() && series.is_some() && value.series != series)
            || (value.volume.is_some() && volume.is_some() && value.volume != volume)
            || (!value.authors.is_empty() && !authors.is_empty() && value.authors != authors);
        value.series = value.series.clone().or(series);
        value.volume = value.volume.or(volume);
        if value.authors.is_empty() {
            value.authors = authors;
        }
    }
    Ok(result)
}

fn evidence(
    book: ComicBook,
    root_path: &str,
    nodes: &BTreeMap<i64, Node>,
    imported: Embedded,
    overrides: &[Override],
    artbook: bool,
) -> Option<Evidence> {
    let relative = relative_path(&book.source_path, root_path)?;
    let name = source_name(&book);
    let parents = ancestors(nodes, book.node_id);
    let relevant = matching_overrides(&relative, overrides);
    let manual_volume = relevant.iter().find_map(|o| {
        o.json
            .get("volume")
            .and_then(|v| v.as_u64())
            .filter(|n| (1..=9999).contains(n))
            .map(|n| n as u32)
    });
    let chapter = relevant
        .iter()
        .any(|o| o.json.get("chapter").is_some_and(|v| !v.is_null()));
    let manual_series = relevant.iter().find_map(|o| {
        ["logical_target_group", "series_directory"]
            .into_iter()
            .find_map(|key| o.json.get(key).and_then(|v| v.as_str()))
            .filter(|s| !s.trim().is_empty())
            .map(path_key)
    });
    let independent = relevant.iter().any(|o| {
        o.json.get("no_merge").and_then(|v| v.as_bool()) == Some(true)
            && ["logical_target_group", "series_directory"]
                .into_iter()
                .all(|key| {
                    o.json
                        .get(key)
                        .and_then(|v| v.as_str())
                        .is_none_or(|s| s.trim().is_empty())
                })
    });
    let parsed_volume = comics::book_volume(&name)
        .or_else(|| comics::book_volume(&book.display_name))
        .or_else(|| {
            parents
                .iter()
                .take_while(|n| n.parent.is_some())
                .find_map(|n| comics::book_volume(&n.name))
        })
        .map(u32::from)
        .filter(|n| (1..=9999).contains(n));
    let (volume, number_source) = if let Some(n) = manual_volume {
        (Some(n), "MANUAL")
    } else if let Some(n) = imported.volume {
        (Some(n), "EMBEDDED")
    } else {
        (parsed_volume, "STRUCTURED")
    };
    let extra_name = |s: &str| {
        signals::extra(s)
            || signals::collection(s)
            || signals::assets(s)
            || (!artbook
                && ["画集", "畫集", "artbook"]
                    .iter()
                    .any(|needle| s.to_lowercase().contains(needle)))
    };
    let extra = chapter
        || extra_name(&name)
        || parents
            .iter()
            .take_while(|n| n.parent.is_some())
            .any(|n| extra_name(&n.name))
        || relevant
            .iter()
            .any(|o| o.json.get("role").and_then(|v| v.as_str()) == Some("EXTRAS"));
    let binding = parents.iter().find_map(|n| n.binding.clone());
    let parsed = m2shelf_media_parser::parse_for_grouping(&name, m2shelf_media_parser::Kind::Comic);
    let mut editions = parsed.edition;
    for term in [
        "新装版",
        "新裝版",
        "完全版",
        "修订版",
        "修訂版",
        "豪華版",
        "豪华版",
        "カラー版",
        "全彩版",
        "文庫版",
        "文库版",
        "特装版",
        "特裝版",
    ] {
        if relative.contains(term) {
            editions.push(term.into());
        }
    }
    let continuation = relative.split('/').any(comics::book_is_re);
    Some(Evidence {
        title: title_key(&comics::clean_title(&name)),
        continuation,
        book,
        relative,
        volume,
        number_source,
        manual_series,
        embedded: imported,
        binding,
        extra,
        independent,
        editions,
    })
}

fn usable_title(title: &str) -> bool {
    !title.is_empty()
        && !crate::title_extractor::is_generic_title(title)
        && !signals::intermediate(title)
        && !title.chars().all(|c| c.is_ascii_digit())
}

fn same_work(current: &Evidence, other: &Evidence, established_group: bool) -> bool {
    if other.extra
        || other.independent
        || other.embedded.conflict
        || current.continuation != other.continuation
        || current.editions != other.editions
        || current.binding != other.binding
    {
        return false;
    }
    // Explicit persisted manual membership precedes imported series/title hints. This still
    // requires the existing scope above and never crosses binding or edition boundaries.
    if let (Some(a), Some(b)) = (&current.manual_series, &other.manual_series) {
        return established_group && a == b;
    }
    if !current.embedded.authors.is_empty()
        && !other.embedded.authors.is_empty()
        && current.embedded.authors != other.embedded.authors
    {
        return false;
    }
    match (&current.embedded.series, &other.embedded.series) {
        (Some(a), Some(b)) => return a == b,
        (Some(_), None) | (None, Some(_)) if !established_group => return false,
        _ => {}
    }
    if usable_title(&current.title) && usable_title(&other.title) {
        return current.title == other.title;
    }
    established_group
}

fn result(
    current: &ComicBook,
    status: &str,
    current_volume: Option<u32>,
    next_volume: Option<u32>,
    snapshot: String,
    next: Option<ComicBook>,
) -> NextVolumeResult {
    NextVolumeResult {
        status: status.into(),
        current: current.clone(),
        next,
        current_volume,
        next_volume,
        snapshot,
    }
}

fn fingerprint(
    root: i64,
    state: &(i64, Option<i64>, String),
    current: i64,
    nodes: &BTreeMap<i64, Node>,
    rows: &[Evidence],
    overrides: &[Override],
    logical: &[serde_json::Value],
) -> String {
    // Progress/last-read timestamps and all personal metadata intentionally stay out of this key.
    let data = serde_json::json!({"v":1,"root":root,"organization":state,"current":current,"nodes":nodes,
        "books":rows.iter().map(|e| serde_json::json!([e.book.id,e.book.node_id,e.book.source_path,e.book.revision,e.book.page_count,e.book.index_error,e.relative,e.volume,e.number_source,e.embedded,e.extra,e.independent,e.binding,e.continuation,e.editions])).collect::<Vec<_>>(),"overrides":overrides,"logical":logical});
    format!("{:x}", Sha256::digest(data.to_string().as_bytes()))
}

pub fn lookup(
    database: &Database,
    book_id: i64,
    expected_revision: Option<&str>,
) -> AppResult<NextVolumeResult> {
    database.read_snapshot(|c| lookup_conn(c, book_id, expected_revision))
}

pub fn confirm(
    database: &Database,
    book_id: i64,
    expected_revision: &str,
    expected_next_id: i64,
    expected_snapshot: &str,
) -> AppResult<NextVolumeResult> {
    let value = lookup(database, book_id, Some(expected_revision))?;
    if value.snapshot != expected_snapshot
        || value.status != "READY"
        || value.next.as_ref().map(|b| b.id) != Some(expected_next_id)
    {
        return Err("NEXT_VOLUME_STALE".into());
    }
    Ok(value)
}

fn lookup_conn(
    c: &Connection,
    book_id: i64,
    expected_revision: Option<&str>,
) -> AppResult<NextVolumeResult> {
    let current = c
        .query_row(
            &format!("{} WHERE b.id=?1", comics::book_select()),
            [book_id],
            comics::book_from_row,
        )
        .optional()
        .map_err(error)?
        .ok_or("COMIC_BOOK_NOT_FOUND")?;
    db::ensure_node_visible_conn(c, current.node_id)?;
    if expected_revision.is_some_and(|r| r != current.revision) {
        return Err("COMIC_PAGE_CHANGED".into());
    }
    let (root, root_path, stored_kind, mode, strategy, artbook, attachment): (i64,String,String,String,String,bool,bool) = c.query_row(
        "SELECT r.id,r.path,r.media_kind,r.recognition_mode,r.book_organization_strategy,r.artbook_library,b.source_resource_id IS NOT NULL FROM comic_books b JOIN nodes n ON n.id=b.node_id JOIN library_roots r ON r.id=n.library_root_id WHERE b.id=?1", [book_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(error)?;
    if stored_kind != "COMIC" || attachment || !supported(&current) {
        return Ok(result(
            &current,
            "UNSUPPORTED",
            None,
            None,
            String::new(),
            None,
        ));
    }
    if current.page_count <= 0 || current.index_error.is_some() {
        return Ok(result(
            &current,
            "UNREADABLE",
            None,
            None,
            String::new(),
            None,
        ));
    }
    let mut statement = c.prepare("SELECT n.id,n.parent_node_id,n.folder_name,n.node_type,n.manual_type_override,b.provider,b.provider_subject_id,b.provider_subject_type,n.absolute_path FROM nodes n LEFT JOIN metadata_bindings b ON b.node_id=n.id WHERE n.library_root_id=?1 ORDER BY n.id LIMIT 16385").map_err(error)?;
    let mut nodes = BTreeMap::new();
    for row in statement
        .query_map([root], |r| {
            Ok(Node {
                id: r.get(0)?,
                parent: r.get(1)?,
                name: r.get(2)?,
                path: r.get(8)?,
                kind: r.get(3)?,
                manual: r.get(4)?,
                binding: r
                    .get::<_, Option<String>>(5)?
                    .map(|provider| Ok::<_, rusqlite::Error>((provider, r.get(6)?, r.get(7)?)))
                    .transpose()?,
            })
        })
        .map_err(error)?
    {
        let row = row.map_err(error)?;
        nodes.insert(row.id, row);
        if nodes.len() > MAX_NODES {
            return Ok(result(&current, "LIMIT", None, None, String::new(), None));
        }
    }
    let hidden = db::hidden_node_ids_conn(c)?;
    let mut statement = c.prepare(&format!("{} JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=?1 AND b.source_resource_id IS NULL ORDER BY b.id LIMIT 4097",comics::book_select())).map_err(error)?;
    let books = statement
        .query_map([root], comics::book_from_row)
        .map_err(error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(error)?;
    if books.len() > MAX_BOOKS {
        return Ok(result(&current, "LIMIT", None, None, String::new(), None));
    }
    let imported = embedded(c, root, &books)?;
    let mut overrides_statement=c.prepare("SELECT relative_path,override_json FROM book_organization_overrides WHERE root_id=?1 ORDER BY relative_path LIMIT 2049").map_err(error)?;
    let mut overrides = Vec::new();
    for row in overrides_statement
        .query_map([root], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(error)?
    {
        let (path, json) = row.map_err(error)?;
        if overrides.len() >= 2048 {
            return Ok(result(&current, "LIMIT", None, None, String::new(), None));
        }
        let json = serde_json::from_str(&json).map_err(|_| "SMART_OVERRIDE_INVALID")?;
        overrides.push(Override { path, json });
    }
    let rows = books
        .into_iter()
        .filter(|b| !hidden.contains(&b.node_id) && supported(b))
        .filter_map(|b| {
            let metadata = imported.get(&b.id).cloned().unwrap_or_default();
            evidence(b, &root_path, &nodes, metadata, &overrides, artbook)
        })
        .collect::<Vec<_>>();
    let current_evidence = rows
        .iter()
        .find(|b| b.book.id == book_id)
        .ok_or("COMIC_BOOK_NOT_FOUND")?;
    let organization: (i64, Option<i64>, String) = c
        .query_row(
            "SELECT revision,applied_revision,status FROM book_organization_state WHERE root_id=?1",
            [root],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(error)?;
    let mut logical_statement=c.prepare("SELECT g.group_id,g.kind,g.title,g.relative_path,g.decision,m.book_id,m.ordinal,m.role FROM book_logical_groups g LEFT JOIN book_logical_members m ON m.root_id=g.root_id AND m.group_id=g.group_id WHERE g.root_id=?1 ORDER BY g.group_id,m.ordinal,m.book_id LIMIT 8193").map_err(error)?;
    let logical = logical_statement
        .query_map([root], |r| {
            Ok(serde_json::json!([
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, Option<String>>(7)?
            ]))
        })
        .map_err(error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(error)?;
    if logical.len() > 8192 {
        return Ok(result(&current, "LIMIT", None, None, String::new(), None));
    }
    let snapshot = fingerprint(
        root,
        &organization,
        book_id,
        &nodes,
        &rows,
        &overrides,
        &logical,
    );
    let reject = |status: &str, next_volume| {
        result(
            &current,
            status,
            current_evidence.volume,
            next_volume,
            snapshot.clone(),
            None,
        )
    };
    if current_evidence.extra || current_evidence.independent || current_evidence.embedded.conflict
    {
        return Ok(reject("NO_SERIES", None));
    }
    let Some(current_volume) = current_evidence.volume else {
        return Ok(reject("UNNUMBERED", None));
    };
    let mut allowed = BTreeSet::new();
    let mut established = false;
    if strategy == "SMART_MIXED" {
        if organization.1 != Some(organization.0) || organization.2 != "READY" {
            return Ok(reject("ORGANIZATION_STALE", None));
        }
        let group:Option<(String,String,String)>=c.query_row("SELECT g.group_id,g.kind,g.decision FROM book_logical_members m JOIN book_logical_groups g ON g.root_id=m.root_id AND g.group_id=m.group_id WHERE m.root_id=?1 AND m.book_id=?2",params![root,book_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(error)?;
        let Some((group, kind, decision)) = group else {
            return Ok(reject("NO_SERIES", None));
        };
        if kind != "SERIES" || !decision.eq_ignore_ascii_case("Apply") {
            return Ok(reject("NO_SERIES", None));
        }
        let mut members=c.prepare("SELECT book_id,role FROM book_logical_members WHERE root_id=?1 AND group_id=?2 ORDER BY ordinal,book_id").map_err(error)?;
        for row in members
            .query_map(params![root, group], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(error)?
        {
            let (id, role) = row.map_err(error)?;
            if role.eq_ignore_ascii_case("VOLUME") || role.eq_ignore_ascii_case("MAIN") {
                allowed.insert(id);
            }
        }
        established = true;
    } else if mode == "FOLDER" {
        for ancestor in ancestors(&nodes, current.node_id) {
            if ancestor.parent.is_none() {
                break;
            }
            if overrides.iter().any(|o| {
                path_key(&o.path)
                    == path_key(&relative_path(&ancestor.path, &root_path).unwrap_or_default())
                    && matches!(
                        o.json.get("role").and_then(|v| v.as_str()),
                        Some("CATEGORY" | "AMBIGUOUS")
                    )
            }) {
                break;
            }
            let owned = comics::owned_books(c, ancestor.id)?
                .into_iter()
                .map(|b| b.id)
                .collect::<BTreeSet<_>>();
            let owner_title = title_key(&comics::clean_title(&ancestor.name));
            let owner_relative = relative_path(&ancestor.path, &root_path).unwrap_or_default();
            let owner_establishes_series = !usable_title(&current_evidence.title)
                || current_evidence.title == owner_title
                || current_evidence.embedded.series.as_ref() == Some(&owner_title)
                || current_evidence.manual_series.as_ref() == Some(&path_key(&owner_relative))
                || ancestor.manual
                || ancestor.binding.is_some();
            if owned.contains(&book_id) && owned.len() > 1 && owner_establishes_series {
                allowed = owned;
                established = true;
                break;
            }
            // A named independent child never acquires its author's sibling books by climbing.
            let parent_binding = ancestor.parent.and_then(|parent| {
                ancestors(&nodes, parent)
                    .iter()
                    .find_map(|n| n.binding.clone())
            });
            if ancestor.manual
                || (ancestor.binding.is_some() && ancestor.binding != parent_binding)
                || (!signals::intermediate(&ancestor.name)
                    && comics::book_volume(&ancestor.name).is_none())
            {
                break;
            }
        }
    }
    if allowed.is_empty() {
        // Trusted embedded series labels are considered only within the exact source directory.
        // This does not regroup loose filename prefixes or author/category descendants.
        let Some(series) = &current_evidence.embedded.series else {
            return Ok(reject("NO_SERIES", None));
        };
        let directory = signals::parent(&current_evidence.relative).unwrap_or("");
        for row in &rows {
            if signals::parent(&row.relative).unwrap_or("") == directory
                && row.embedded.series.as_ref() == Some(series)
            {
                allowed.insert(row.book.id);
            }
        }
    }
    if !allowed.contains(&book_id) {
        return Ok(reject("NO_SERIES", None));
    }
    let candidates = rows
        .iter()
        .filter(|e| allowed.contains(&e.book.id) && same_work(current_evidence, e, established))
        .collect::<Vec<_>>();
    if candidates
        .iter()
        .filter(|e| e.volume == Some(current_volume))
        .count()
        != 1
    {
        return Ok(reject("DUPLICATE_VOLUME", None));
    }
    if candidates.iter().any(|e| e.volume.is_none()) {
        return Ok(reject("UNORDERED", None));
    }
    let next_volume = candidates
        .iter()
        .filter_map(|e| e.volume)
        .filter(|n| *n > current_volume)
        .min();
    let Some(next_volume) = next_volume else {
        return Ok(reject("END_OF_SERIES", None));
    };
    if current_volume.checked_add(1) != Some(next_volume) {
        return Ok(reject("MISSING_VOLUME", Some(next_volume)));
    }
    let next = candidates
        .iter()
        .filter(|e| e.volume == Some(next_volume))
        .collect::<Vec<_>>();
    if next.len() != 1 {
        return Ok(reject("DUPLICATE_VOLUME", Some(next_volume)));
    }
    let next = &next[0].book;
    if next.index_error.is_some() || next.page_count <= 0 {
        return Ok(reject("NEXT_UNREADABLE", Some(next_volume)));
    }
    Ok(result(
        &current,
        "READY",
        Some(current_volume),
        Some(next_volume),
        snapshot,
        Some(next.clone()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use m2shelf_smart_mixed_lab::model::LibraryKind;
    use m2shelf_smart_mixed_shadow::fixture::Factory;

    fn fixture(mode: &str, smart: bool) -> (tempfile::TempDir, Factory, Database) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("next-volume.db");
        let factory = if smart {
            Factory::create_smart(&path, LibraryKind::Comic).unwrap()
        } else {
            Factory::create(&path, LibraryKind::Comic, mode).unwrap()
        };
        let db = Database::new(path);
        db.migrate().unwrap();
        (temp, factory, db)
    }

    fn metadata(f: &Factory, book: i64, series: &str, volume: u32, author: &str) {
        let value = Metadata {
            source: "OPF".into(),
            series: Some(series.into()),
            volume: Some(volume),
            authors: vec![author.into()],
            ..Default::default()
        };
        f.connection.execute("INSERT OR REPLACE INTO indexed_media_metadata(source_key,node_id,book_id,revision,payload_json) SELECT source_path,node_id,id,'1:'||file_size||':'||modified_at,?2 FROM comic_books WHERE id=?1", params![book,serde_json::to_string(&value).unwrap()]).unwrap();
    }

    fn manual(f: &Factory, path: &str, value: serde_json::Value) {
        f.connection.execute("INSERT OR REPLACE INTO book_organization_overrides(root_id,relative_path,override_json) VALUES(1,?1,?2)",params![path,value.to_string()]).unwrap();
    }

    fn group(f: &Factory, books: &[(i64, &str)], decision: &str) {
        f.connection.execute("INSERT INTO book_logical_groups(root_id,group_id,title,kind,relative_path,decision) VALUES(1,'series','Synthetic series','SERIES','Series',?1)",[decision]).unwrap();
        for (ordinal, (book, role)) in books.iter().enumerate() {
            f.connection.execute("INSERT INTO book_logical_members(root_id,group_id,book_id,ordinal,role) VALUES(1,'series',?1,?2,?3)",params![book,ordinal,role]).unwrap();
        }
        f.connection.execute("UPDATE book_organization_state SET applied_revision=revision,status='READY' WHERE root_id=1",[]).unwrap();
    }

    fn ready(db: &Database, current: i64, next: i64, volume: u32) -> NextVolumeResult {
        let result = lookup(db, current, None).unwrap();
        assert_eq!(result.status, "READY", "{result:?}");
        assert_eq!(result.next.as_ref().map(|b| b.id), Some(next));
        assert_eq!(result.next_volume, Some(volume));
        assert_eq!(result.snapshot.len(), 64);
        result
    }

    fn rejected(db: &Database, current: i64, status: &str) -> NextVolumeResult {
        let result = lookup(db, current, None).unwrap();
        assert_eq!(result.status, status, "{result:?}");
        assert!(result.next.is_none());
        result
    }

    #[test]
    fn mixed_formats_follow_explicit_volumes_and_retain_next_progress() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Vol 01.pdf").unwrap();
        let b = f.file_book("Series/Vol 02.cbz").unwrap();
        let c = f
            .image_book("Series/Vol 03", &["001.png", "002.jpg"])
            .unwrap();
        f.connection
            .execute(
                "INSERT INTO comic_reading_progress(comic_book_id,last_page_index) VALUES(?1,1)",
                [c],
            )
            .unwrap();
        ready(&db, a, b, 2);
        let result = ready(&db, b, c, 3);
        assert_eq!(result.next.unwrap().progress.unwrap().last_page_index, 1);
        rejected(&db, c, "END_OF_SERIES");
    }

    #[test]
    fn existing_parser_handles_chinese_japanese_and_fullwidth_volume_labels() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("作品/第1卷.pdf").unwrap();
        let b = f.file_book("作品/第２巻.cbz").unwrap();
        let c = f.image_book("作品/第三卷", &["001.png"]).unwrap();
        ready(&db, a, b, 2);
        ready(&db, b, c, 3);
    }

    #[test]
    fn missing_volume_is_not_silently_skipped_or_renumbered() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Vol 01.pdf").unwrap();
        f.file_book("Series/Vol 03.cbz").unwrap();
        let result = rejected(&db, a, "MISSING_VOLUME");
        assert_eq!(result.next_volume, Some(3));
    }

    #[test]
    fn duplicate_current_volume_formats_are_ambiguous() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Vol 01.pdf").unwrap();
        f.file_book("Series/Vol 01.cbz").unwrap();
        f.file_book("Series/Vol 02.cbz").unwrap();
        rejected(&db, a, "DUPLICATE_VOLUME");
    }

    #[test]
    fn duplicate_next_volume_formats_are_ambiguous() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Vol 01.pdf").unwrap();
        f.file_book("Series/Vol 02.pdf").unwrap();
        f.file_book("Series/Vol 02.cbz").unwrap();
        let result = rejected(&db, a, "DUPLICATE_VOLUME");
        assert_eq!(result.next_volume, Some(2));
    }

    #[test]
    fn supplementary_and_collection_books_do_not_block_real_next_volume() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Vol 01.pdf").unwrap();
        f.file_book("Series/特典 Vol 02.pdf").unwrap();
        f.file_book("Series/画集 Vol 02.pdf").unwrap();
        f.file_book("Series/全集 Vol 02.cbz").unwrap();
        let b = f.file_book("Series/Vol 02.cbz").unwrap();
        ready(&db, a, b, 2);
    }

    #[test]
    fn loose_root_prefixes_and_author_siblings_are_not_series() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let root_book = f.file_book("Same title Vol 01.pdf").unwrap();
        f.file_book("Same title Vol 02.pdf").unwrap();
        rejected(&db, root_book, "NO_SERIES");
        let a = f.file_book("Author/Red Vol 01.pdf").unwrap();
        f.file_book("Author/Blue Vol 02.pdf").unwrap();
        rejected(&db, a, "NO_SERIES");
        let same = f
            .file_book("Another author/Matching title Vol 01.pdf")
            .unwrap();
        f.file_book("Another author/Matching title Vol 02.cbz")
            .unwrap();
        rejected(&db, same, "NO_SERIES");
    }

    #[test]
    fn unnumbered_candidates_never_create_natural_order_guess() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Book A.pdf").unwrap();
        f.file_book("Series/Book B.pdf").unwrap();
        rejected(&db, a, "UNNUMBERED");
        let x = f.file_book("Other/Vol 01.pdf").unwrap();
        f.file_book("Other/unknown.pdf").unwrap();
        f.file_book("Other/Vol 02.cbz").unwrap();
        rejected(&db, x, "UNORDERED");
    }

    #[test]
    fn distinct_bindings_manual_and_ignored_boundaries_are_preserved() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.image_book("Series/Vol 01", &["001.png"]).unwrap();
        let b = f.image_book("Series/Vol 02", &["001.png"]).unwrap();
        f.bind(f.nodes["Series/Vol 01"], 100).unwrap();
        f.bind(f.nodes["Series/Vol 02"], 200).unwrap();
        rejected(&db, a, "NO_SERIES");
        assert_ne!(a, b);
        let c = f.file_book("Separate/Vol 01.pdf").unwrap();
        f.image_book("Separate/Vol 02", &["001.png"]).unwrap();
        f.manual(f.nodes["Separate/Vol 02"], "WORK").unwrap();
        rejected(&db, c, "NO_SERIES");
        f.manual(f.nodes["Separate"], "IGNORED").unwrap();
        assert!(lookup(&db, c, None).is_err());
    }

    #[test]
    fn same_explicit_binding_can_keep_owned_volumes_together() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.image_book("Series/Vol 01", &["001.png"]).unwrap();
        let b = f.image_book("Series/Vol 02", &["001.png"]).unwrap();
        for key in ["Series", "Series/Vol 01", "Series/Vol 02"] {
            f.bind(f.nodes[key], 100).unwrap();
        }
        ready(&db, a, b, 2);
    }

    #[test]
    fn file_mode_requires_trusted_embedded_series_in_same_directory_and_author() {
        let (_temp, mut f, db) = fixture("VIDEO_FILE", false);
        let a = f.file_book("Author/First.epub").unwrap();
        let b = f.file_book("Author/Second.mobi").unwrap();
        let c = f.file_book("Other/Third.azw3").unwrap();
        metadata(&f, a, "Same series", 1, "Author");
        metadata(&f, b, "Same series", 2, "Author");
        metadata(&f, c, "Same series", 3, "Author");
        ready(&db, a, b, 2);
        rejected(&db, b, "END_OF_SERIES");
        metadata(&f, b, "Same series", 2, "Different author");
        rejected(&db, a, "END_OF_SERIES");
    }

    #[test]
    fn stale_imported_metadata_cannot_establish_file_mode_series() {
        let (_temp, mut f, db) = fixture("VIDEO_FILE", false);
        let a = f.file_book("One.epub").unwrap();
        let b = f.file_book("Two.epub").unwrap();
        metadata(&f, a, "Series", 1, "Author");
        metadata(&f, b, "Series", 2, "Author");
        f.connection
            .execute(
                "UPDATE indexed_media_metadata SET revision='old' WHERE book_id=?1",
                [a],
            )
            .unwrap();
        rejected(&db, a, "UNNUMBERED");
    }

    #[test]
    fn metadata_volume_precedes_filename_number_and_manual_volume_precedes_metadata() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Vol 09.pdf").unwrap();
        let b = f.file_book("Series/Vol 08.cbz").unwrap();
        metadata(&f, a, "Series", 1, "Author");
        metadata(&f, b, "Series", 2, "Author");
        ready(&db, a, b, 2);
        manual(&f, "Series/Vol 09.pdf", serde_json::json!({"volume":4}));
        manual(&f, "Series/Vol 08.cbz", serde_json::json!({"volume":5}));
        let result = ready(&db, a, b, 5);
        assert_eq!(result.current_volume, Some(4));
    }

    #[test]
    fn continuation_and_edition_qualifiers_never_cross() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Series Vol 01.pdf").unwrap();
        f.file_book("Series/Series:re Vol 02.cbz").unwrap();
        f.file_book("Series/新装版/Series Vol 02.cbz").unwrap();
        rejected(&db, a, "END_OF_SERIES");
    }

    #[test]
    fn smart_mixed_requires_current_applied_series_and_eligible_role() {
        let (_temp, mut f, db) = fixture("FOLDER", true);
        let a = f.file_book("Series/Vol 01.pdf").unwrap();
        let b = f.file_book("Series/Vol 02.cbz").unwrap();
        let extra = f.file_book("Series/Vol 02.txt").unwrap();
        rejected(&db, a, "ORGANIZATION_STALE");
        group(
            &f,
            &[(a, "Volume"), (b, "Volume"), (extra, "Extra")],
            "Apply",
        );
        ready(&db, a, b, 2);
        f.connection
            .execute("UPDATE book_logical_groups SET decision='Review'", [])
            .unwrap();
        rejected(&db, a, "NO_SERIES");
        f.connection
            .execute("UPDATE book_logical_groups SET decision='Apply'", [])
            .unwrap();
        f.connection
            .execute("UPDATE book_organization_state SET revision=revision+1", [])
            .unwrap();
        rejected(&db, a, "ORGANIZATION_STALE");
    }

    #[test]
    fn smart_mixed_real_rebuild_is_used_without_new_grouping_algorithm() {
        let (_temp, mut f, db) = fixture("FOLDER", true);
        let a = f.file_book("Tokyo Ghoul/Vol 01.pdf").unwrap();
        let b = f.file_book("Tokyo Ghoul/Vol 02.cbz").unwrap();
        f.image_book("Tokyo Ghoul/Vol 03", &["001.png"]).unwrap();
        crate::smart_mixed::rebuild(
            &db,
            1,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .unwrap();
        ready(&db, a, b, 2);
    }

    #[test]
    fn independent_override_blocks_next_and_stale_confirmation() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Vol 01.pdf").unwrap();
        let b = f.file_book("Series/Vol 02.cbz").unwrap();
        let result = ready(&db, a, b, 2);
        manual(
            &f,
            "Series/Vol 02.cbz",
            serde_json::json!({"no_merge":true}),
        );
        rejected(&db, a, "END_OF_SERIES");
        assert_eq!(
            confirm(&db, a, &result.current.revision, b, &result.snapshot).unwrap_err(),
            "NEXT_VOLUME_STALE"
        );
    }

    #[test]
    fn explicit_manual_series_and_order_precede_embedded_titles() {
        let (_temp, mut f, db) = fixture("FOLDER", true);
        let a = f.file_book("Series/First title.pdf").unwrap();
        let b = f.file_book("Series/Second title.cbz").unwrap();
        metadata(&f, a, "Old imported A", 9, "One author");
        metadata(&f, b, "Old imported B", 8, "Other author");
        manual(
            &f,
            "Series/First title.pdf",
            serde_json::json!({"no_merge":true,"logical_target_group":"series","volume":1}),
        );
        manual(
            &f,
            "Series/Second title.cbz",
            serde_json::json!({"no_merge":true,"logical_target_group":"series","volume":2}),
        );
        group(&f, &[(a, "Volume"), (b, "Volume")], "Apply");
        ready(&db, a, b, 2);
        manual(
            &f,
            "Series/Second title.cbz",
            serde_json::json!({"no_merge":true,"logical_target_group":null,"series_directory":null,"volume":2}),
        );
        f.connection.execute("UPDATE book_organization_state SET applied_revision=revision,status='READY' WHERE root_id=1",[]).unwrap();
        rejected(&db, a, "END_OF_SERIES");
    }

    #[test]
    fn failed_next_index_is_not_skipped_to_later_volume() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Vol 01.pdf").unwrap();
        let b = f.file_book("Series/Vol 02.cbz").unwrap();
        f.file_book("Series/Vol 03.pdf").unwrap();
        f.connection
            .execute(
                "UPDATE comic_books SET index_error='SYNTHETIC_UNREADABLE' WHERE id=?1",
                [b],
            )
            .unwrap();
        rejected(&db, a, "NEXT_UNREADABLE");
        rejected(&db, b, "UNREADABLE");
    }

    #[test]
    fn progress_bookmarks_notes_and_status_do_not_invalidate_snapshot_or_get_written() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Vol 01.pdf").unwrap();
        let b = f.file_book("Series/Vol 02.cbz").unwrap();
        let before = ready(&db, a, b, 2);
        f.connection
            .execute(
                "INSERT INTO comic_reading_progress(comic_book_id,last_page_index) VALUES(?1,0)",
                [a],
            )
            .unwrap();
        f.connection
            .execute(
                "INSERT INTO comic_bookmarks(comic_book_id,page_index) VALUES(?1,0)",
                [a],
            )
            .unwrap();
        f.connection
            .execute(
                "INSERT INTO private_notes(book_id,body) VALUES(?1,'Personal note remains')",
                [a],
            )
            .unwrap();
        f.connection
            .execute(
                "INSERT INTO collection_states(kind,book_id,state) VALUES('BOOK',?1,'PAUSED')",
                [a],
            )
            .unwrap();
        let changes = f.connection.total_changes();
        let after = lookup_conn(&f.connection, a, None).unwrap();
        assert_eq!(f.connection.total_changes(), changes);
        assert_eq!(before.snapshot, after.snapshot);
        assert!(after.current.progress.is_some());
        confirm(&db, a, &before.current.revision, b, &before.snapshot).unwrap();
        let body: String = f
            .connection
            .query_row(
                "SELECT body FROM private_notes WHERE book_id=?1",
                [a],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(body, "Personal note remains");
    }

    #[test]
    fn confirmation_rejects_stale_source_revision_and_wrong_next_id() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Vol 01.pdf").unwrap();
        let b = f.file_book("Series/Vol 02.cbz").unwrap();
        let before = ready(&db, a, b, 2);
        assert_eq!(
            confirm(&db, a, &before.current.revision, b + 1, &before.snapshot).unwrap_err(),
            "NEXT_VOLUME_STALE"
        );
        f.connection
            .execute(
                "UPDATE comic_books SET revision='rescanned-next' WHERE id=?1",
                [b],
            )
            .unwrap();
        assert_eq!(
            confirm(&db, a, &before.current.revision, b, &before.snapshot).unwrap_err(),
            "NEXT_VOLUME_STALE"
        );
        f.connection
            .execute(
                "UPDATE comic_books SET revision='rescanned-current' WHERE id=?1",
                [a],
            )
            .unwrap();
        assert_eq!(
            lookup(&db, a, Some(&before.current.revision)).unwrap_err(),
            "COMIC_PAGE_CHANGED"
        );
    }

    #[test]
    fn roots_and_attachments_are_not_promoted_or_joined() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Vol 01.pdf").unwrap();
        let b = f.file_book("Series/Vol 02.cbz").unwrap();
        f.connection.execute("INSERT INTO library_roots(id,path,display_name,media_kind) VALUES(2,'S:\\OtherSynthetic','Other','COMIC')",[]).unwrap();
        let foreign = f.node("Foreign").unwrap();
        f.connection.execute("UPDATE nodes SET library_root_id=2,parent_node_id=NULL,absolute_path='S:\\OtherSynthetic' WHERE id=?1",[foreign]).unwrap();
        f.connection.execute("UPDATE comic_books SET node_id=?1,source_path='S:\\OtherSynthetic\\Series Vol 02.cbz' WHERE id=?2",params![foreign,b]).unwrap();
        rejected(&db, a, "NO_SERIES");
        let attachment = f.resource("Series/Vol 01.pdf").unwrap();
        f.connection
            .execute(
                "UPDATE comic_books SET source_resource_id=?1 WHERE id=?2",
                params![attachment, a],
            )
            .unwrap();
        rejected(&db, a, "UNSUPPORTED");
        assert_eq!(
            f.connection
                .query_row(
                    "SELECT source_resource_id FROM comic_books WHERE id=?1",
                    [a],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            attachment
        );
    }

    #[test]
    fn plain_zip_and_outside_root_sources_are_rejected_without_disk_access() {
        let (_temp, mut f, db) = fixture("FOLDER", false);
        let a = f.file_book("Series/Vol 01.cbz").unwrap();
        f.connection.execute("UPDATE comic_books SET source_path='R:\\SyntheticLibrary\\Series\\Vol 01.zip' WHERE id=?1",[a]).unwrap();
        rejected(&db, a, "UNSUPPORTED");
        f.connection.execute("UPDATE comic_books SET source_path='R:\\SyntheticLibrarySpoof\\Vol 01.cbz' WHERE id=?1",[a]).unwrap();
        assert_eq!(lookup(&db, a, None).unwrap_err(), "COMIC_BOOK_NOT_FOUND");
    }
}

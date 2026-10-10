//! Logical book organization over the existing physical index. No media I/O or identity writes.
use crate::{
    comics::{self, ComicBook},
    db::{self, AppResult, Database},
    models::MediaNode,
};
use m2shelf_smart_mixed_lab::{model::*, recognize_indexed, signals::normalize_path};
use m2shelf_smart_mixed_shadow::adapter::{ReadIndex, ReadOptions};
use rusqlite::{params, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicalGroup {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub relative_path: String,
    pub books: Vec<ComicBook>,
    pub cover_node: Option<MediaNode>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalogue {
    pub status: String,
    pub revision: i64,
    pub groups: Vec<LogicalGroup>,
    pub directories: Vec<DirectoryJudgment>,
    pub fallback_books: Vec<ComicBook>,
    pub directory_nodes: Vec<MediaNode>,
}
const VISIBLE_NODES: &str = "WITH RECURSIVE visible_nodes(id) AS (
 SELECT id FROM nodes WHERE library_root_id=?1 AND parent_node_id IS NULL AND node_type<>'IGNORED'
 UNION SELECT n.id FROM nodes n JOIN visible_nodes p ON n.parent_node_id=p.id WHERE n.library_root_id=?1 AND n.node_type<>'IGNORED')";
fn err(e: rusqlite::Error) -> String {
    e.to_string()
}
fn revision(db: &Database, root: i64) -> AppResult<i64> {
    db.connect()?
        .query_row(
            "SELECT revision FROM book_organization_state WHERE root_id=?1",
            [root],
            |r| r.get(0),
        )
        .map_err(err)
}

/// Called inside the single scan worker after successful physical indexing and health receipt.
/// The immediate write transaction compares a Root-scoped revision before replacing relations.
pub fn rebuild(db: &Database, root: i64, cancel: Arc<AtomicBool>) -> AppResult<()> {
    let library = db.get_root(root)?;
    if !library.media_kind.is_book() || library.book_organization_strategy != "SMART_MIXED" {
        return Ok(());
    }
    let expected = revision(db, root)?;
    let mut options = ReadOptions {
        cancelled: cancel.clone(),
        ..Default::default()
    };
    let mut manual_links = Vec::<(String, String)>::new();
    {
        let c = db.connect()?;
        let mut stmt=c.prepare("SELECT override_json FROM book_organization_overrides WHERE root_id=?1 ORDER BY relative_path").map_err(err)?;
        for value in stmt
            .query_map([root], |r| r.get::<_, String>(0))
            .map_err(err)?
        {
            let json: serde_json::Value =
                serde_json::from_str(&value.map_err(err)?).map_err(|_| "INVALID_BOOK_OVERRIDE")?;
            let override_value: ManualOverride =
                serde_json::from_value(json.clone()).map_err(|_| "INVALID_BOOK_OVERRIDE")?;
            if let Some(target) = json.get("logical_target_group").and_then(|v| v.as_str()) {
                manual_links.push((override_value.path.clone(), target.to_owned()));
            }
            options.overrides.push(override_value);
        }
    }
    let snapshot = ReadIndex::open(db.path())?.read(root, &options)?;
    if !snapshot.scan_health.complete || cancel.load(Ordering::Relaxed) {
        return Err("SMART_INDEX_INCOMPLETE".into());
    }
    let images = snapshot
        .source_id_map
        .values()
        .filter(|s| s.status == "INDEXED_IMAGE_BINARY_VALIDATION_UNKNOWN")
        .map(|s| s.path.clone())
        .collect::<BTreeSet<_>>();
    let mut plan = recognize_indexed(&snapshot.recognition_input, &images)?;
    // Explicit manual membership is an application-owned relation, not a new recognition rule.
    // A missing target leaves the original reading unit visible as an independent work.
    let unit_paths = plan
        .reading_units
        .iter()
        .map(|u| (u.source_ref.clone(), u.path.clone()))
        .collect::<BTreeMap<_, _>>();
    for (path, target) in manual_links {
        if !plan
            .groups
            .iter()
            .any(|g| g.proposal_ref == target && g.kind == GroupKind::Series)
        {
            continue;
        }
        let mut moved = Vec::new();
        for group in &mut plan.groups {
            if group.proposal_ref == target {
                continue;
            }
            let mut retained = Vec::new();
            for member in std::mem::take(&mut group.members) {
                let source = &unit_paths[&member.source_ref];
                if source == &path || source.starts_with(&format!("{path}/")) {
                    moved.push(member);
                } else {
                    retained.push(member);
                }
            }
            group.members = retained;
        }
        if let Some(group) = plan.groups.iter_mut().find(|g| g.proposal_ref == target) {
            group.members.extend(moved);
            group.members.sort_by_key(|m| {
                (
                    m.volume.unwrap_or(u32::MAX),
                    m.chapter.unwrap_or(u32::MAX),
                    m.source_ref.clone(),
                )
            });
        }
    }
    plan.groups.retain(|g| !g.members.is_empty());
    let source_ids = snapshot
        .source_id_map
        .values()
        .filter(|s| s.resource_id.is_none())
        .map(|s| {
            let kind = if s.source_kind == "IMAGE_FOLDER" {
                "DIRECT_PAGES"
            } else {
                "FILE"
            };
            (
                serde_json::to_string(&(
                    format!("root:{root}"),
                    kind,
                    format!("native:book:{}", s.book_id),
                ))
                .unwrap(),
                s.book_id,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    for group in &plan.groups {
        for member in &group.members {
            let id = source_ids
                .get(&member.source_ref)
                .ok_or("SMART_SOURCE_IDENTITY_MISSING")?;
            if !seen.insert(id) {
                return Err("SMART_DUPLICATE_SOURCE".into());
            }
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("SMART_CANCELLED".into());
    }
    let mut c = db.connect()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    let current: i64 = tx
        .query_row(
            "SELECT revision FROM book_organization_state WHERE root_id=?1",
            [root],
            |r| r.get(0),
        )
        .map_err(err)?;
    if current != expected || cancel.load(Ordering::Relaxed) {
        return Err("SMART_INDEX_STALE".into());
    }
    tx.execute("DELETE FROM book_logical_groups WHERE root_id=?1", [root])
        .map_err(err)?;
    for group in &plan.groups {
        let path = group.directory.as_deref().unwrap_or_else(|| {
            plan.reading_units
                .iter()
                .find(|u| group.members.iter().any(|m| m.source_ref == u.source_ref))
                .map_or("", |u| u.path.as_str())
        });
        tx.execute("INSERT INTO book_logical_groups(root_id,group_id,title,kind,relative_path,decision) VALUES(?1,?2,?3,?4,?5,?6)",params![root,group.proposal_ref,group.title,if group.kind==GroupKind::Series{"SERIES"}else{"WORK"},path,format!("{:?}",group.decision)]).map_err(err)?;
        for (ordinal, m) in group.members.iter().enumerate() {
            tx.execute("INSERT INTO book_logical_members(root_id,group_id,book_id,ordinal,role) VALUES(?1,?2,?3,?4,?5)",params![root,group.proposal_ref,source_ids[&m.source_ref],ordinal as i64,format!("{:?}",m.role)]).map_err(err)?;
        }
    }
    tx.execute("UPDATE book_organization_state SET applied_revision=?2,index_version=?3,rules_version=?4,status='READY',directories_json=?5,updated_at=CURRENT_TIMESTAMP WHERE root_id=?1",params![root,expected,snapshot.index_snapshot_version,plan.rules_version,serde_json::to_string(&plan.directories).map_err(|_|"SMART_SERIALIZE")?]).map_err(err)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("SMART_CANCELLED".into());
    }
    tx.commit().map_err(err)
}

pub fn catalogue(db: &Database, root: i64) -> AppResult<Catalogue> {
    let library = db.get_root(root)?;
    db.read_snapshot(|c| catalogue_conn(c, &library))
}

/// Share the caller's read transaction with the cross-library collection.
pub(crate) fn catalogue_conn(
    c: &rusqlite::Connection,
    library: &crate::models::LibraryRoot,
) -> AppResult<Catalogue> {
    let root = library.id;
    if library.book_organization_strategy != "SMART_MIXED" {
        return Err("NOT_SMART_MIXED".into());
    }
    let (status,revision,applied,dirs):(String,i64,Option<i64>,String)=c.query_row("SELECT status,revision,applied_revision,directories_json FROM book_organization_state WHERE root_id=?1",[root],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(err)?;
    let mut books_stmt=c.prepare(&format!("{VISIBLE_NODES} {} JOIN visible_nodes n ON n.id=b.node_id WHERE b.source_resource_id IS NULL",comics::book_select())).map_err(err)?;
    let all_books = books_stmt
        .query_map([root], comics::book_from_row)
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let book_map = all_books
        .iter()
        .map(|b| (b.id, b))
        .collect::<BTreeMap<_, _>>();
    let mut members=c.prepare("SELECT group_id,book_id FROM book_logical_members WHERE root_id=?1 ORDER BY group_id,ordinal").map_err(err)?;
    let mut by_group = BTreeMap::<String, Vec<ComicBook>>::new();
    let mut used = BTreeSet::new();
    for row in members
        .query_map([root], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(err)?
    {
        let (group, id) = row.map_err(err)?;
        if let Some(book) = book_map.get(&id) {
            by_group.entry(group).or_default().push((*book).clone());
            used.insert(id);
        }
    }
    let mut stmt=c.prepare("SELECT group_id,title,kind,relative_path FROM book_logical_groups WHERE root_id=?1 ORDER BY title COLLATE NOCASE,group_id").map_err(err)?;
    let mut groups = Vec::new();
    let mut node_stmt=c.prepare(&format!("{VISIBLE_NODES} SELECT n.id,n.parent_node_id,n.absolute_path FROM nodes n JOIN visible_nodes v ON v.id=n.id")).map_err(err)?;
    let physical = node_stmt
        .query_map([root], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let parents = physical
        .iter()
        .map(|(id, parent, _)| (*id, *parent))
        .collect::<BTreeMap<_, _>>();
    let paths = physical
        .iter()
        .filter(|(_, parent, _)| parent.is_some())
        .map(|(id, _, path)| (path.replace('\\', "/").to_lowercase(), *id))
        .collect::<BTreeMap<_, _>>();
    let mut counts = BTreeMap::<i64, usize>::new();
    for book in &all_books {
        let mut id = Some(book.node_id);
        let mut visited = BTreeSet::new();
        while let Some(current) = id {
            if !visited.insert(current) {
                break;
            }
            *counts.entry(current).or_default() += 1;
            id = parents.get(&current).copied().flatten();
        }
    }

    for row in stmt
        .query_map([root], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(err)?
    {
        let (id, title, kind, relative_path) = row.map_err(err)?;
        let books = by_group.remove(&id).unwrap_or_default();
        if books.is_empty() {
            continue;
        }
        // Reuse real Node cover/metadata only when this group owns that Node's whole book set.
        let physical_path = format!(
            "{}/{}",
            library.path.replace('\\', "/").trim_end_matches('/'),
            relative_path
        )
        .to_lowercase();
        let owner = paths.get(&physical_path).copied().or_else(|| {
            let id = books[0].node_id;
            books.iter().all(|b| b.node_id == id).then_some(id)
        });
        let cover_node = owner
            .filter(|id| {
                parents.get(id).copied().flatten().is_some() && counts.get(id) == Some(&books.len())
            })
            .map(|id| db::get_node_conn(c, id))
            .transpose()?;
        groups.push(LogicalGroup {
            id,
            title,
            kind,
            relative_path,
            books,
            cover_node,
        });
    }
    let directories: Vec<DirectoryJudgment> =
        serde_json::from_str(&dirs).map_err(|_| "SMART_INVALID_CATALOGUE")?;
    let directory_nodes = directories
        .iter()
        .filter_map(|d| {
            paths.get(
                &format!(
                    "{}/{}",
                    library.path.replace('\\', "/").trim_end_matches('/'),
                    d.path
                )
                .to_lowercase(),
            )
        })
        .map(|id| db::get_node_conn(c, *id))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(Catalogue {
        status: if applied == Some(revision) {
            status
        } else {
            "STALE".into()
        },
        revision,
        groups,
        directories,
        directory_nodes,
        fallback_books: all_books
            .into_iter()
            .filter(|b| !used.contains(&b.id))
            .collect(),
    })
}

impl Catalogue {
    pub(crate) fn top_level_count(&self, root: &crate::models::LibraryRoot) -> usize {
        let paths = self
            .groups
            .iter()
            .map(|g| g.relative_path.clone())
            .chain(self.fallback_books.iter().map(|b| {
                b.source_path
                    .replace('\\', "/")
                    .strip_prefix(&format!(
                        "{}/",
                        root.path.replace('\\', "/").trim_end_matches('/')
                    ))
                    .unwrap_or(&b.display_name)
                    .to_owned()
            }))
            .collect::<Vec<_>>();
        let directories = self
            .directories
            .iter()
            .filter(|d| {
                !d.path.is_empty()
                    && !d.path.contains('/')
                    && matches!(d.role, DirectoryRole::Category | DirectoryRole::Ambiguous)
                    && paths.iter().any(|p| p.starts_with(&format!("{}/", d.path)))
            })
            .collect::<Vec<_>>();
        directories.len()
            + paths
                .iter()
                .filter(|p| {
                    !directories
                        .iter()
                        .any(|d| p.starts_with(&format!("{}/", d.path)))
                })
                .count()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Correction {
    Category,
    Series,
    Assign,
    Independent,
    Automatic,
}
/// Application-owned corrections are paths from the current indexed catalogue, never disk paths.
pub fn correct(
    db: &Database,
    root: i64,
    path: &str,
    action: Correction,
    target: Option<&str>,
    expected: i64,
) -> AppResult<()> {
    let catalogue = catalogue(db, root)?;
    let path = normalize_path(path)?;
    if path.is_empty() || catalogue.revision != expected {
        return Err("SMART_INDEX_STALE".into());
    }
    let known_dir = catalogue.directories.iter().any(|d| d.path == path);
    let library = db.get_root(root)?;
    let root_prefix = format!("{}/", library.path.replace('\\', "/").trim_end_matches('/'));
    let wanted = format!("{root_prefix}{path}");
    let known_source = catalogue.groups.iter().any(|g| g.relative_path == path)
        || catalogue
            .groups
            .iter()
            .flat_map(|g| g.books.iter())
            .chain(catalogue.fallback_books.iter())
            .any(|book| {
                book.source_path
                    .replace('\\', "/")
                    .eq_ignore_ascii_case(&wanted)
            });
    if !(known_dir || known_source) {
        return Err("SMART_UNKNOWN_TARGET".into());
    }
    let target = target.map(normalize_path).transpose()?;
    if matches!(action, Correction::Assign)
        && !target.as_ref().is_some_and(|t| {
            t != &path
                && !t.starts_with(&format!("{path}/"))
                && catalogue
                    .groups
                    .iter()
                    .any(|g| g.kind == "SERIES" && &g.relative_path == t)
        })
    {
        return Err("SMART_UNKNOWN_SERIES".into());
    }
    if matches!(action, Correction::Category | Correction::Series) && !known_dir {
        return Err("SMART_DIRECTORY_REQUIRED".into());
    }
    let mut c = db.connect()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    let now: i64 = tx
        .query_row(
            "SELECT revision FROM book_organization_state WHERE root_id=?1",
            [root],
            |r| r.get(0),
        )
        .map_err(err)?;
    if now != expected {
        return Err("SMART_INDEX_STALE".into());
    }
    if matches!(action, Correction::Automatic) {
        tx.execute(
            "DELETE FROM book_organization_overrides WHERE root_id=?1 AND relative_path=?2",
            params![root, path],
        )
        .map_err(err)?;
    } else {
        let value = ManualOverride {
            path: path.clone(),
            role: match action {
                Correction::Category => Some(DirectoryRole::Category),
                Correction::Series => Some(DirectoryRole::Series),
                _ => None,
            },
            series_directory: None,
            volume: None,
            chapter: None,
            no_merge: matches!(action, Correction::Independent | Correction::Assign),
        };
        let mut json = serde_json::to_value(value).map_err(|_| "SMART_OVERRIDE_INVALID")?;
        if matches!(action, Correction::Assign) {
            let group = catalogue
                .groups
                .iter()
                .find(|g| Some(&g.relative_path) == target.as_ref() && g.kind == "SERIES")
                .ok_or("SMART_UNKNOWN_SERIES")?;
            json["logical_target_group"] = serde_json::Value::String(group.id.clone());
        }
        tx.execute("INSERT INTO book_organization_overrides(root_id,relative_path,override_json) VALUES(?1,?2,?3) ON CONFLICT(root_id,relative_path) DO UPDATE SET override_json=excluded.override_json",params![root,path,serde_json::to_string(&json).map_err(|_|"SMART_OVERRIDE_INVALID")?]).map_err(err)?;
    }
    tx.commit().map_err(err)?;
    rebuild(db, root, Arc::new(AtomicBool::new(false)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use m2shelf_smart_mixed_shadow::fixture::Factory;
    fn cancelled() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }
    fn sample(kind: LibraryKind) -> (tempfile::TempDir, Factory, Database) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("synthetic.db");
        let mut f = Factory::create_smart(&path, kind).unwrap();
        f.connection.execute_batch("BEGIN").unwrap();
        f.file_book("Tokyo Ghoul/Vol 01.pdf").unwrap();
        f.file_book("Tokyo Ghoul/Vol 02.cbz").unwrap();
        f.image_book("Tokyo Ghoul/Vol 03", &["001.png", "002.jpg"])
            .unwrap();
        f.file_book("Goodbye Eri.pdf").unwrap();
        f.file_book("Look Back.pdf").unwrap();
        f.file_book("Author/Other A.pdf").unwrap();
        f.file_book("Author/Other B.epub").unwrap();
        f.resource("notes.zip").unwrap();
        f.connection.execute_batch("COMMIT").unwrap();
        let db = Database::new(path);
        (temp, f, db)
    }
    #[test]
    fn all_resources_preserves_smart_groups_categories_fallback_and_ignored_boundaries() {
        for kind in [
            LibraryKind::Comic,
            LibraryKind::Ebook,
            LibraryKind::Doujin,
            LibraryKind::Artbook,
        ] {
            let (_temp, f, db) = sample(kind);
            // Before a successful logical rebuild, every indexed book is still reachable.
            let before = db.list_all_resources().unwrap();
            assert!(before.nodes.is_empty());
            assert!(before.comic_nodes.is_empty());
            assert_eq!(before.book_libraries[0].catalogue.fallback_books.len(), 7);
            rebuild(&db, 1, cancelled()).unwrap();
            let all = db.list_all_resources().unwrap();
            let cat = &all.book_libraries[0].catalogue;
            assert_eq!(cat.groups.iter().flat_map(|g| &g.books).count(), 7);
            assert!(cat
                .groups
                .iter()
                .any(|g| g.kind == "SERIES" && g.books.len() == 3));
            assert_eq!(
                all.total_count,
                cat.top_level_count(&all.book_libraries[0].root) as i64
            );
            assert_eq!(all.total_count, 4); // series, two independent books, Author category
            assert!(all.nodes.is_empty() && all.comic_nodes.is_empty());
            f.manual(f.nodes["Author"], "IGNORED").unwrap();
            let ignored = db.list_all_resources().unwrap();
            let cat = &ignored.book_libraries[0].catalogue;
            assert_eq!(cat.groups.iter().flat_map(|g| &g.books).count(), 5);
            assert_eq!(ignored.total_count, 3);
            assert_eq!(
                f.connection
                    .query_row("SELECT count(*) FROM comic_books", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                7
            );
        }
    }
    #[test]
    fn all_resources_never_uses_hidden_root_for_loose_books_in_any_book_mode() {
        for kind in [
            LibraryKind::Comic,
            LibraryKind::Ebook,
            LibraryKind::Doujin,
            LibraryKind::Artbook,
        ] {
            for mode in ["SMART_MIXED", "FOLDER", "VIDEO_FILE"] {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().join("synthetic.db");
                let mut f = if mode == "SMART_MIXED" {
                    Factory::create_smart(&path, kind).unwrap()
                } else {
                    Factory::create(&path, kind, mode).unwrap()
                };
                let db = Database::new(path);
                db.migrate().unwrap();
                let mut ids = BTreeSet::new();
                for format in ["pdf", "epub", "txt", "mobi", "azw3", "cbz"] {
                    ids.insert(
                        f.file_book(&format!("Independent {format}.{format}"))
                            .unwrap(),
                    );
                }
                f.resource("notes.zip").unwrap();
                f.connection.execute_batch("UPDATE nodes SET node_type='WORK',direct_comic_book_count=(SELECT count(*) FROM comic_books b WHERE b.node_id=nodes.id),total_comic_book_count=(SELECT count(*) FROM comic_books b WHERE b.node_id=nodes.id);").unwrap();
                let mut before = db.list_all_resources().unwrap();
                if mode == "SMART_MIXED" {
                    rebuild(&db, 1, cancelled()).unwrap();
                    before = db.list_all_resources().unwrap();
                }
                assert!(before
                    .nodes
                    .iter()
                    .chain(&before.comic_nodes)
                    .all(|n| n.parent_node_id.is_some()));
                assert_eq!(before.total_count, 6, "{kind:?} {mode}");
                let hits = db.search("Independent", Some(1)).unwrap();
                assert_eq!(
                    hits.len(),
                    6,
                    "book search must not coalesce into a shared parent: {kind:?} {mode}"
                );
                assert_eq!(
                    hits.iter()
                        .filter_map(|h| h.comic_book.as_ref().map(|b| b.id))
                        .collect::<BTreeSet<_>>(),
                    ids
                );
                assert!(db.search("Independent", Some(999)).unwrap().is_empty());
                for id in &ids {
                    f.connection.execute("INSERT INTO comic_reading_progress(comic_book_id,last_page_index,last_read_at) VALUES(?1,0,'2026-10-10T00:00:00Z')",[id]).unwrap();
                }
                let recent = db.list_recently_watched().unwrap();
                assert_eq!(
                    recent.len(),
                    6,
                    "recent books must retain individual identity: {kind:?} {mode}"
                );
                assert_eq!(
                    recent
                        .iter()
                        .filter_map(|r| r.comic_book.as_ref().map(|b| b.id))
                        .collect::<BTreeSet<_>>(),
                    ids
                );
                let displayed = before
                    .book_libraries
                    .iter()
                    .flat_map(|l| {
                        l.catalogue
                            .groups
                            .iter()
                            .flat_map(|g| &g.books)
                            .chain(l.catalogue.fallback_books.iter())
                    })
                    .map(|b| b.id)
                    .collect::<BTreeSet<_>>();
                if mode == "VIDEO_FILE" {
                    assert!(before.book_libraries.is_empty());
                    assert_eq!(before.nodes.len(), 6);
                    assert_eq!(before.comic_nodes.len(), 6);
                } else {
                    assert_eq!(displayed, ids);
                    assert!(before.nodes.is_empty() && before.comic_nodes.is_empty());
                }
                assert_eq!(
                    f.connection
                        .query_row("SELECT count(*) FROM resource_files", [], |r| r
                            .get::<_, i64>(0))
                        .unwrap(),
                    1
                );
            }
        }
    }
    #[test]
    fn mixed_catalogue_uses_original_book_ids_and_indexed_image_identity() {
        for kind in [
            LibraryKind::Comic,
            LibraryKind::Ebook,
            LibraryKind::Doujin,
            LibraryKind::Artbook,
        ] {
            let (_temp, f, db) = sample(kind);
            rebuild(&db, 1, cancelled()).unwrap();
            let c = catalogue(&db, 1).unwrap();
            assert_eq!(c.status, "READY");
            let ids = c
                .groups
                .iter()
                .flat_map(|g| g.books.iter().map(|b| b.id))
                .chain(c.fallback_books.iter().map(|b| b.id))
                .collect::<BTreeSet<_>>();
            assert_eq!(ids.len(), 7);
            assert_eq!(
                f.connection
                    .query_row("SELECT count(*) FROM resource_files", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert!(c
                .groups
                .iter()
                .any(|g| g.kind == "SERIES" && g.books.len() == 3));
            assert!(c.directories.iter().any(|d| d.path == "Author"));
        }
    }
    #[test]
    fn series_and_one_hundred_independent_books_stay_parallel() {
        let (_temp, mut f, db) = sample(LibraryKind::Comic);
        for i in 0..100 {
            f.file_book(&format!("Standalone {i:03}.pdf")).unwrap();
        }
        rebuild(&db, 1, cancelled()).unwrap();
        let c = catalogue(&db, 1).unwrap();
        assert_eq!(c.groups.iter().flat_map(|g| &g.books).count(), 107);
        assert_eq!(
            c.groups
                .iter()
                .filter(|g| g.relative_path.starts_with("Standalone "))
                .count(),
            100
        );
        assert!(c
            .groups
            .iter()
            .any(|g| g.kind == "SERIES" && g.books.len() == 3));
        assert!(c.directories.iter().any(|d| d.path == "Author"));
    }
    #[test]
    fn ignored_subtrees_do_not_reappear_as_physical_fallback() {
        let (_temp, f, db) = sample(LibraryKind::Comic);
        rebuild(&db, 1, cancelled()).unwrap();
        f.manual(f.nodes["Author"], "IGNORED").unwrap();
        let c = catalogue(&db, 1).unwrap();
        assert_eq!(
            c.groups
                .iter()
                .flat_map(|g| &g.books)
                .chain(c.fallback_books.iter())
                .count(),
            5
        );
        assert_eq!(
            f.connection
                .query_row("SELECT count(*) FROM comic_books", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            7
        );
        rebuild(&db, 1, cancelled()).unwrap();
        let c = catalogue(&db, 1).unwrap();
        assert!(!c
            .groups
            .iter()
            .any(|g| g.relative_path.starts_with("Author/")));
        assert!(c.fallback_books.is_empty());
    }
    #[test]
    fn version_twenty_four_upgrade_preserves_source_and_user_state() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("legacy.db");
        let mut f = Factory::create(&path, LibraryKind::Comic, "FOLDER").unwrap();
        let book = f.file_book("Original/Vol 01.pdf").unwrap();
        let node = f.nodes["Original"];
        f.bind(node, 42).unwrap();
        let resource = f.resource("Original/notes.txt").unwrap();
        f.connection.execute("UPDATE nodes SET display_name='Custom',cover_source='MANUAL',cover_cache_path='synthetic-cover.png',manual_type_override=1 WHERE id=?1",[node]).unwrap();
        f.connection
            .execute(
                "INSERT INTO comic_reading_progress(comic_book_id,last_page_index) VALUES(?1,0)",
                [book],
            )
            .unwrap();
        f.connection
            .execute(
                "INSERT INTO comic_bookmarks(comic_book_id,page_index) VALUES(?1,0)",
                [book],
            )
            .unwrap();
        f.connection.execute_batch("INSERT INTO tags(id,name,normalized_name) VALUES(1,'Personal','personal');INSERT INTO favorite_folders(id,name,normalized_name) VALUES(1,'Keep','keep');INSERT INTO settings(key,value) VALUES('synthetic-retained','yes');").unwrap();
        f.connection
            .execute("INSERT INTO node_tags(node_id,tag_id) VALUES(?1,1)", [node])
            .unwrap();
        f.connection
            .execute(
                "INSERT INTO node_favorite_folders(node_id,folder_id) VALUES(?1,1)",
                [node],
            )
            .unwrap();
        let db = Database::new(path);
        db.migrate().unwrap();
        db.migrate().unwrap();
        let c = db.connect().unwrap();
        let values:(String,String,String,i64,String)=c.query_row("SELECT n.display_name,n.cover_source,n.cover_cache_path,n.manual_type_override,r.book_organization_strategy FROM nodes n JOIN library_roots r ON r.id=n.library_root_id WHERE n.id=?1",[node],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
        assert_eq!(
            values,
            (
                "Custom".into(),
                "MANUAL".into(),
                "synthetic-cover.png".into(),
                1,
                "LEGACY".into()
            )
        );
        for (table, column, id) in [
            ("comic_books", "id", book),
            ("comic_pages", "comic_book_id", book),
            ("comic_reading_progress", "comic_book_id", book),
            ("comic_bookmarks", "comic_book_id", book),
            ("resource_files", "id", resource),
            ("metadata_bindings", "node_id", node),
            ("node_tags", "node_id", node),
            ("node_favorite_folders", "node_id", node),
        ] {
            assert_eq!(
                c.query_row(
                    &format!("SELECT count(*) FROM {table} WHERE {column}=?1"),
                    [id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1,
                "{table}"
            );
        }
        assert_eq!(
            c.query_row(
                "SELECT value FROM settings WHERE key='synthetic-retained'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "yes"
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
    }
    #[test]
    fn correction_survives_rebuild_and_stale_correction_is_rejected() {
        let (_temp, _f, db) = sample(LibraryKind::Comic);
        rebuild(&db, 1, cancelled()).unwrap();
        let c = catalogue(&db, 1).unwrap();
        correct(&db, 1, "Author", Correction::Series, None, c.revision).unwrap();
        rebuild(&db, 1, cancelled()).unwrap();
        let next = catalogue(&db, 1).unwrap();
        assert!(next
            .groups
            .iter()
            .any(|g| g.relative_path == "Author" && g.kind == "SERIES"));
        assert!(correct(&db, 1, "Author", Correction::Category, None, c.revision).is_err());
        correct(&db, 1, "Author", Correction::Automatic, None, next.revision).unwrap();
        assert_eq!(
            catalogue(&db, 1)
                .unwrap()
                .groups
                .iter()
                .map(|g| g.books.len())
                .sum::<usize>(),
            7
        );
    }
    #[test]
    fn manual_assignment_independent_and_restore_preserve_every_book() {
        let (_temp, f, db) = sample(LibraryKind::Comic);
        f.connection
            .execute(
                "INSERT INTO comic_reading_progress(comic_book_id,last_page_index) VALUES(1,0)",
                [],
            )
            .unwrap();
        f.connection
            .execute(
                "INSERT INTO comic_bookmarks(comic_book_id,page_index) VALUES(1,0)",
                [],
            )
            .unwrap();
        rebuild(&db, 1, cancelled()).unwrap();
        let current = catalogue(&db, 1).unwrap();
        correct(
            &db,
            1,
            "Goodbye Eri.pdf",
            Correction::Assign,
            Some("Tokyo Ghoul"),
            current.revision,
        )
        .unwrap();
        rebuild(&db, 1, cancelled()).unwrap();
        let next = catalogue(&db, 1).unwrap();
        assert_eq!(
            next.groups
                .iter()
                .find(|g| g.relative_path == "Tokyo Ghoul")
                .unwrap()
                .books
                .len(),
            4
        );
        correct(
            &db,
            1,
            "Goodbye Eri.pdf",
            Correction::Independent,
            None,
            next.revision,
        )
        .unwrap();
        let next = catalogue(&db, 1).unwrap();
        assert!(next
            .groups
            .iter()
            .any(|g| g.relative_path == "Goodbye Eri.pdf" && g.books.len() == 1));
        assert_eq!(next.groups.iter().map(|g| g.books.len()).sum::<usize>(), 7);
        correct(
            &db,
            1,
            "Goodbye Eri.pdf",
            Correction::Automatic,
            None,
            next.revision,
        )
        .unwrap();
        assert_eq!(
            f.connection
                .query_row(
                    "SELECT count(*) FROM comic_bookmarks WHERE comic_book_id=1",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            f.connection
                .query_row(
                    "SELECT last_page_index FROM comic_reading_progress WHERE comic_book_id=1",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }
    #[test]
    fn cancellation_and_failed_health_preserve_previous_groups() {
        let (_temp, f, db) = sample(LibraryKind::Comic);
        rebuild(&db, 1, cancelled()).unwrap();
        let before = catalogue(&db, 1).unwrap().groups.len();
        assert!(rebuild(&db, 1, Arc::new(AtomicBool::new(true))).is_err());
        f.connection
            .execute(
                "UPDATE library_scan_health SET outcome='FAILED' WHERE library_root_id=1",
                [],
            )
            .unwrap();
        assert!(rebuild(&db, 1, cancelled()).is_err());
        assert_eq!(catalogue(&db, 1).unwrap().groups.len(), before);
    }
    #[test]
    fn page_revision_invalidates_plan_without_touching_progress() {
        let (_temp, f, db) = sample(LibraryKind::Comic);
        rebuild(&db, 1, cancelled()).unwrap();
        let before = catalogue(&db, 1).unwrap().revision;
        f.connection.execute("UPDATE comic_pages SET file_size=file_size+1 WHERE id=(SELECT min(id) FROM comic_pages)",[]).unwrap();
        assert!(catalogue(&db, 1).unwrap().revision > before);
        assert_eq!(catalogue(&db, 1).unwrap().status, "STALE");
    }
    #[test]
    fn strategy_is_immutable_and_video_cannot_select_smart() {
        let (_temp, f, _db) = sample(LibraryKind::Comic);
        assert!(f
            .connection
            .execute(
                "UPDATE library_roots SET book_organization_strategy='LEGACY' WHERE id=1",
                []
            )
            .is_err());
        let temp = tempfile::tempdir().unwrap();
        assert!(
            Factory::create_smart(&temp.path().join("video.db"), LibraryKind::Animation).is_err()
        );
    }
    #[test]
    #[ignore = "Explicit synthetic performance check, no private library reads"]
    fn normal_scale_production_performance() {
        for count in [100, 1000, 5000, 10000] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("performance.db");
            let mut f = Factory::create_smart(&path, LibraryKind::Comic).unwrap();
            f.connection.execute_batch("BEGIN").unwrap();
            for i in 0..count {
                f.file_book(&format!("Independent {i:05}.pdf")).unwrap();
            }
            f.connection.execute_batch("COMMIT").unwrap();
            let db = Database::new(path);
            let start = std::time::Instant::now();
            rebuild(&db, 1, cancelled()).unwrap();
            let build = start.elapsed();
            let start = std::time::Instant::now();
            let c = catalogue(&db, 1).unwrap();
            assert_eq!(c.groups.len(), count);
            println!(
                "production {count}: group_ms={} catalogue_ms={}",
                build.as_millis(),
                start.elapsed().as_millis()
            );
        }
    }
}

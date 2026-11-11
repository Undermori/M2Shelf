//! Deltas of indexed physical nodes only; no filesystem enumeration or presentation metadata.
use std::collections::{BTreeMap, HashMap};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    db::{AppResult, Database},
    scanner::ScanTarget,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanChanges {
    pub scan_id: String,
    pub scope: Vec<String>,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: Option<usize>,
    pub errors: u64,
    pub complete: bool,
    pub added_node_ids: Vec<i64>,
}

type Snapshot = BTreeMap<String, (i64, Vec<String>)>;

pub(crate) fn capture(database: &Database, targets: &[ScanTarget]) -> AppResult<Snapshot> {
    database.read_snapshot(|c| {
        let mut result = BTreeMap::new();
        for target in targets {
            // A path boundary, not LIKE, so '%' and '_' in library names stay literal.
            let path = target.path.to_string_lossy().to_lowercase();
            let mut s = c.prepare("SELECT id,absolute_path FROM nodes WHERE library_root_id=?1 AND parent_node_id IS NOT NULL").map_err(|e|e.to_string())?;
            let rows = s.query_map([target.root.id], |r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).map_err(|e|e.to_string())?;
            for row in rows {
                let (id, source) = row.map_err(|e|e.to_string())?;
                let key = source.to_lowercase();
                if key == path || key.strip_prefix(&path).is_some_and(|p|p.starts_with(['\\','/'])) {
                    result.entry(key).or_insert((id, Vec::new()));
                }
            }
        }
        let ids = result.iter().map(|(key,(id,_))|(*id,key.clone())).collect::<HashMap<_,_>>();
        // Physical revision fields only. Tags, custom names, binding and cover writes do not count.
        let selected=serde_json::to_string(&ids.keys().collect::<Vec<_>>()).map_err(|e|e.to_string())?;
        let mut s=c.prepare("WITH selected(id) AS (SELECT value FROM json_each(?1)) SELECT node_id,absolute_path,file_size,modified_at FROM media_files WHERE node_id IN (SELECT id FROM selected) UNION ALL SELECT node_id,absolute_path,file_size,modified_at FROM resource_files WHERE node_id IN (SELECT id FROM selected) UNION ALL SELECT node_id,source_path,file_size,modified_at FROM comic_books WHERE source_resource_id IS NULL AND node_id IN (SELECT id FROM selected) UNION ALL SELECT b.node_id,b.source_path || '/' || p.source_locator,p.file_size,p.modified_at FROM comic_pages p JOIN comic_books b ON b.id=p.comic_book_id WHERE b.source_kind='IMAGE_FOLDER' AND b.source_resource_id IS NULL AND b.node_id IN (SELECT id FROM selected)").map_err(|e|e.to_string())?;
        let rows=s.query_map([selected],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,String>(3)?))).map_err(|e|e.to_string())?;
        for row in rows {
            let (id,path,size,stamp)=row.map_err(|e|e.to_string())?;
            if let Some(key)=ids.get(&id) { result.get_mut(key).unwrap().1.push(format!("{path}\0{size}\0{stamp}")); }
        }
        for (_, files) in result.values_mut() { files.sort_unstable(); files.dedup(); }
        Ok(result)
    })
}

pub(crate) fn finish(
    database: &Database,
    targets: &[ScanTarget],
    before: &Snapshot,
    scan_id: &str,
    complete: bool,
    errors: u64,
) -> AppResult<()> {
    let after = capture(database, targets)?;
    let mut change = ScanChanges {
        scan_id: scan_id.into(),
        scope: targets
            .iter()
            .map(|t| t.path.to_string_lossy().into_owned())
            .collect(),
        complete,
        errors,
        removed: complete.then_some(0),
        ..Default::default()
    };
    for (path, (id, files)) in &after {
        match before.get(path) {
            None => {
                change.added += 1;
                change.added_node_ids.push(*id);
            }
            Some((_, old)) if old != files => change.updated += 1,
            Some(_) => change.unchanged += 1,
        }
    }
    if complete {
        change.removed = Some(before.keys().filter(|p| !after.contains_key(*p)).count());
    }
    // One summary per Root, including a scoped partial scan; never overwrite the success baseline.
    if let Some(target) = targets.first() {
        database.connect()?.execute("INSERT INTO library_scan_changes(root_id,summary_json) VALUES(?1,?2) ON CONFLICT(root_id) DO UPDATE SET summary_json=excluded.summary_json",params![target.root.id,serde_json::to_string(&change).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    }
    Ok(())
}

pub(crate) fn read(c: &rusqlite::Connection, root_id: i64) -> AppResult<Option<ScanChanges>> {
    let json: Option<String> = c
        .query_row(
            "SELECT summary_json FROM library_scan_changes WHERE root_id=?1",
            [root_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    json.map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        .transpose()
}

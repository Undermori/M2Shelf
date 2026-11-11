//! Application-owned collection state and activity. No source-media mutation.
use crate::db::{self, AppResult, Database};
use rusqlite::{params, Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Target {
    Node(i64),
    Book(i64),
}

pub(crate) fn validate(c: &Connection, target: Target) -> AppResult<i64> {
    let node = match target {
        Target::Node(id) if id > 0 => id,
        Target::Book(id) if id > 0 => c
            .query_row("SELECT node_id FROM comic_books WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .map_err(|_| "COLLECTION_TARGET_INVALID")?,
        _ => return Err("COLLECTION_TARGET_INVALID".into()),
    };
    db::ensure_node_visible_conn(c, node)?;
    Ok(node)
}

pub(crate) fn record_recent(c: &Connection, target: Target) -> AppResult<()> {
    let (kind, node, book) = match target {
        Target::Node(id) => ("NODE", Some(id), None),
        Target::Book(id) => ("BOOK", None, Some(id)),
    };
    // REPLACE only replaces the tiny activity row; no domain entity references its sequence.
    c.execute("INSERT OR REPLACE INTO recent_activity(kind,node_id,book_id,opened_at) VALUES(?1,?2,?3,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![kind,node,book]).map_err(|e|e.to_string())?;
    c.execute("DELETE FROM recent_activity WHERE entry_seq NOT IN (SELECT entry_seq FROM recent_activity ORDER BY julianday(opened_at) DESC,entry_seq DESC LIMIT 30)",[]).map_err(|e|e.to_string())?;
    Ok(())
}

/// Editable, real work/series metadata; never a hidden Root or a synthetic aggregate.
pub(crate) fn validate_metadata_node(c: &Connection, id: i64) -> AppResult<()> {
    let valid:bool=c.query_row("SELECT parent_node_id IS NOT NULL AND node_type IN ('WORK','AUTO_WORK','CONTAINER') AND (total_video_count>0 OR total_comic_book_count>0) FROM nodes WHERE id=?1",[id],|r|r.get(0)).map_err(|e|e.to_string())?;
    if valid {
        Ok(())
    } else {
        Err("COLLECTION_TARGET_INVALID".into())
    }
}

pub fn remove_recent(db: &Database, targets: &[Target]) -> AppResult<()> {
    if targets.is_empty() || targets.len() > 30 {
        return Err("COLLECTION_SELECTION_INVALID".into());
    }
    let mut c = db.connect()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    for target in targets.iter().copied().collect::<HashSet<_>>() {
        validate(&tx, target)?;
        match target {
            Target::Node(id) => tx.execute(
                "DELETE FROM recent_activity WHERE kind='NODE' AND node_id=?1",
                [id],
            ),
            Target::Book(id) => tx.execute(
                "DELETE FROM recent_activity WHERE kind='BOOK' AND book_id=?1",
                [id],
            ),
        }
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

pub fn clear_recent(db: &Database) -> AppResult<()> {
    db.connect()?
        .execute("DELETE FROM recent_activity", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    Want,
    InProgress,
    Completed,
    Paused,
    Dropped,
}
#[derive(Serialize)]
pub struct StatusEntry {
    pub target: Target,
    pub state: Status,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookMembership {
    pub node_id: i64,
    pub book_ids: Vec<i64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeMembership {
    pub node_id: i64,
    pub node_ids: Vec<i64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSnapshot {
    pub entries: Vec<StatusEntry>,
    pub book_memberships: Vec<BookMembership>,
    pub node_memberships: Vec<NodeMembership>,
}

pub fn statuses(db: &Database) -> AppResult<StatusSnapshot> {
    db.read_snapshot(|c|{
  let mut s=c.prepare("SELECT kind,COALESCE(node_id,book_id),state FROM collection_states").map_err(|e|e.to_string())?;
  let records=s.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?))).map_err(|e|e.to_string())?;
  let mut entries=Vec::new();for row in records {let(kind,id,state)=row.map_err(|e|e.to_string())?;entries.push(StatusEntry{target:if kind=="BOOK" {Target::Book(id)} else {Target::Node(id)},state:serde_json::from_value(serde_json::Value::String(state)).map_err(|e|e.to_string())?});}
  // Filter membership only: a parent's own state is never inferred or written from its books.
  let mut s=c.prepare("WITH RECURSIVE members(node_id,book_id) AS (SELECT node_id,id FROM comic_books UNION SELECT n.parent_node_id,m.book_id FROM nodes n JOIN members m ON m.node_id=n.id WHERE n.parent_node_id IS NOT NULL AND n.node_type<>'IGNORED') SELECT m.node_id,m.book_id FROM members m JOIN nodes n ON n.id=m.node_id WHERE n.node_type<>'IGNORED'").map_err(|e|e.to_string())?;
  let mut groups=std::collections::BTreeMap::<i64,Vec<i64>>::new();for row in s.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?))).map_err(|e|e.to_string())? {let(node,book)=row.map_err(|e|e.to_string())?;groups.entry(node).or_default().push(book);}
  let mut s=c.prepare("WITH RECURSIVE members(node_id,work_id) AS (SELECT id,id FROM nodes WHERE node_type IN ('WORK','AUTO_WORK') AND total_video_count>0 UNION SELECT n.parent_node_id,m.work_id FROM nodes n JOIN members m ON m.node_id=n.id WHERE n.parent_node_id IS NOT NULL AND n.node_type<>'IGNORED') SELECT m.node_id,m.work_id FROM members m JOIN nodes n ON n.id=m.node_id WHERE n.node_type<>'IGNORED'").map_err(|e|e.to_string())?;
  let mut nodes=std::collections::BTreeMap::<i64,Vec<i64>>::new();for row in s.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?))).map_err(|e|e.to_string())? {let(node,work)=row.map_err(|e|e.to_string())?;nodes.entry(node).or_default().push(work);}
  Ok(StatusSnapshot{entries,book_memberships:groups.into_iter().map(|(node_id,book_ids)|BookMembership{node_id,book_ids}).collect(),node_memberships:nodes.into_iter().map(|(node_id,node_ids)|NodeMembership{node_id,node_ids}).collect()})
 })
}

pub fn set_status(db: &Database, target: Target, state: Option<Status>) -> AppResult<()> {
    let mut c = db.connect()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let node = validate(&tx, target)?;
    if matches!(target, Target::Node(_)) {
        validate_metadata_node(&tx, node)?;
    }
    let (kind, node, book) = match target {
        Target::Node(id) => ("NODE", Some(id), None),
        Target::Book(id) => ("BOOK", None, Some(id)),
    };
    match state {
        Some(state) => {
            tx.execute("INSERT OR REPLACE INTO collection_states(kind,node_id,book_id,state) VALUES(?1,?2,?3,?4)",params![kind,node,book,serde_json::to_value(state).map_err(|e|e.to_string())?.as_str().unwrap()]).map_err(|e|e.to_string())?;
        }
        None => {
            tx.execute(
                "DELETE FROM collection_states WHERE kind=?1 AND (node_id=?2 OR book_id=?3)",
                params![kind, node, book],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

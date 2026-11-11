//! Typed, application-owned plain text. No source paths or provider mutations.
use crate::{
    collection::Target,
    db::{AppResult, Database},
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;

#[derive(Serialize)]
pub struct Note {
    pub body: String,
    pub revision: i64,
}
fn ids(target: Target) -> (Option<i64>, Option<i64>) {
    match target {
        Target::Node(id) => (Some(id), None),
        Target::Book(id) => (None, Some(id)),
    }
}
pub fn get(db: &Database, target: Target) -> AppResult<Note> {
    let c = db.connect()?;
    crate::collection::validate(&c, target)?;
    let (n, b) = ids(target);
    c.query_row(
        "SELECT body,revision FROM private_notes WHERE node_id=?1 OR book_id=?2",
        params![n, b],
        |r| {
            Ok(Note {
                body: r.get(0)?,
                revision: r.get(1)?,
            })
        },
    )
    .optional()
    .map(|v| {
        v.unwrap_or(Note {
            body: String::new(),
            revision: 0,
        })
    })
    .map_err(|e| e.to_string())
}
pub fn save(db: &Database, target: Target, body: String, revision: i64) -> AppResult<Note> {
    if body.chars().count() > 10000 || body.len() > 40000 || body.contains('\0') {
        return Err("NOTE_LIMIT".into());
    }
    let mut c = db.connect()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let node = crate::collection::validate(&tx, target)?;
    if matches!(target, Target::Node(_)) {
        crate::collection::validate_metadata_node(&tx, node)?;
    }
    let (n, b) = ids(target);
    let old: Option<(i64, i64)> = tx
        .query_row(
            "SELECT id,revision FROM private_notes WHERE node_id=?1 OR book_id=?2",
            params![n, b],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if old.map_or(0, |(_, r)| r) != revision {
        return Err("NOTE_CONFLICT".into());
    }
    // Keep the revision even for empty notes, so a delayed pre-clear save cannot resurrect text.
    let next = revision.checked_add(1).ok_or("NOTE_CONFLICT")?;
    if let Some((id, _)) = old {
        tx.execute(
            "UPDATE private_notes SET body=?1,revision=?2 WHERE id=?3",
            params![body, next, id],
        )
    } else {
        tx.execute(
            "INSERT INTO private_notes(node_id,book_id,body,revision) VALUES(?1,?2,?3,?4)",
            params![n, b, body, next],
        )
    }
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Note {
        body,
        revision: next,
    })
}

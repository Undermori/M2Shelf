//! Typed, application-owned plain text. No source paths or provider mutations.
use crate::{
    collection::Target,
    db::{AppResult, Database},
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkNoteEntry {
    pub target: Target,
    pub node_id: i64,
    pub title: String,
    pub library_id: i64,
    pub library_name: String,
    pub media_kind: String,
    pub body: String,
    pub revision: i64,
    pub updated_at: Option<String>,
}

#[derive(Serialize)]
pub struct WorkNotesPage {
    pub items: Vec<WorkNoteEntry>,
    pub total: i64,
}

/// One indexed snapshot, with filtering before pagination; never enumerates a media Root.
#[cfg(test)]
pub fn list(
    db: &Database,
    notes_only: bool,
    root_id: Option<i64>,
    media_kind: Option<String>,
    query: String,
    offset: i64,
    limit: i64,
) -> AppResult<WorkNotesPage> {
    list_sorted(
        db, notes_only, root_id, media_kind, query, offset, limit, None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn list_sorted(
    db: &Database,
    notes_only: bool,
    root_id: Option<i64>,
    media_kind: Option<String>,
    query: String,
    offset: i64,
    limit: i64,
    sort: Option<String>,
) -> AppResult<WorkNotesPage> {
    if !matches!(sort.as_deref(), None | Some("recent") | Some("title")) {
        return Err("NOTE_LIMIT".into());
    }
    list_internal(
        db,
        notes_only,
        root_id,
        media_kind,
        query,
        offset,
        limit,
        sort.as_deref() == Some("title"),
        false,
    )
}

pub fn export_entries(db: &Database) -> AppResult<Vec<WorkNoteEntry>> {
    let page = list_internal(db, true, None, None, String::new(), 0, 10000, true, true)?;
    if page.total > 10000
        || page
            .items
            .iter()
            .map(|row| row.body.len() + row.title.len() + row.library_name.len() + 128)
            .sum::<usize>()
            > 16 * 1024 * 1024
    {
        return Err("NOTE_LIMIT".into());
    }
    Ok(page.items)
}

#[allow(clippy::too_many_arguments)]
fn list_internal(
    db: &Database,
    notes_only: bool,
    root_id: Option<i64>,
    media_kind: Option<String>,
    query: String,
    offset: i64,
    limit: i64,
    title_order: bool,
    export: bool,
) -> AppResult<WorkNotesPage> {
    if query.chars().count() > 500
        || offset < 0
        || !(1..=if export { 10000 } else { 100 }).contains(&limit)
    {
        return Err("NOTE_LIMIT".into());
    }
    db.read_snapshot(|c| {
        let language: String = c.query_row("SELECT value FROM settings WHERE key='language'", [], |r| r.get(0)).optional().map_err(|e| e.to_string())?.unwrap_or_else(|| "zh-CN".into());
        let sql = r#"WITH RECURSIVE hidden(id) AS (
            SELECT id FROM nodes WHERE node_type='IGNORED'
            UNION SELECT n.id FROM nodes n JOIN hidden h ON n.parent_node_id=h.id
        ), targets(kind,id,node_id,title,library_id,library_name,media_kind) AS (
            SELECT 'NODE',n.id,n.id,
              CASE WHEN n.manual_display_name=1 THEN n.display_name ELSE
                COALESCE(NULLIF(json_extract(CASE WHEN json_valid(tm.payload_json) THEN tm.payload_json ELSE '{}' END,'$.title'),''),NULLIF(CASE ?1 WHEN 'en-US' THEN m.provider_title_en WHEN 'ja-JP' THEN m.provider_title_ja WHEN 'ko-KR' THEN m.provider_title_ko ELSE m.provider_title_cn END,''),NULLIF(m.provider_title_cn,''),NULLIF(m.provider_title,''),NULLIF(n.display_name,''),n.folder_name) END,
              r.id,r.display_name,
              CASE WHEN r.artbook_library=1 THEN 'ARTBOOK' WHEN r.doujin_library=1 THEN 'DOUJIN' WHEN r.media_kind='COMIC' THEN r.book_library_kind WHEN r.video_subject_scope='MIXED' THEN 'VIDEO' ELSE r.video_subject_scope END
            FROM nodes n JOIN library_roots r ON r.id=n.library_root_id LEFT JOIN metadata_bindings m ON m.id=(SELECT id FROM metadata_bindings WHERE node_id=n.id ORDER BY id DESC LIMIT 1) LEFT JOIN tmdb_movie_bindings tm ON tm.node_id=n.id AND tm.active=1
            WHERE n.parent_node_id IS NOT NULL AND n.node_type IN ('WORK','AUTO_WORK','CONTAINER') AND (n.total_video_count>0 OR n.total_comic_book_count>0) AND n.id NOT IN hidden
            UNION ALL
            SELECT 'BOOK',b.id,n.id,b.display_name,r.id,r.display_name,
              CASE WHEN r.artbook_library=1 THEN 'ARTBOOK' WHEN r.doujin_library=1 THEN 'DOUJIN' WHEN r.media_kind='COMIC' THEN r.book_library_kind WHEN r.video_subject_scope='MIXED' THEN 'VIDEO' ELSE r.video_subject_scope END
            FROM comic_books b JOIN nodes n ON n.id=b.node_id JOIN library_roots r ON r.id=n.library_root_id
            WHERE n.id NOT IN hidden
        ), matches AS (
            SELECT t.*,COALESCE(p.body,'') body,COALESCE(p.revision,0) revision,u.updated_at
            FROM targets t LEFT JOIN private_notes p ON (t.kind='NODE' AND p.node_id=t.id) OR (t.kind='BOOK' AND p.book_id=t.id)
            LEFT JOIN private_note_updates u ON u.note_id=p.id
            WHERE (?2=0 OR length(trim(COALESCE(p.body,'')))>0) AND (?3 IS NULL OR t.library_id=?3)
              AND (?4 IS NULL OR t.media_kind=?4) AND (?5='' OR instr(lower(t.title),lower(?5))>0 OR instr(lower(COALESCE(p.body,'')),lower(?5))>0)
        ) "#;
        let args = params![language, notes_only, root_id, media_kind, query];
        let total = c.query_row(&format!("{sql} SELECT count(*) FROM matches"), args, |r| r.get(0)).map_err(|e| e.to_string())?;
        if export {
            let bytes: i64 = c.query_row(&format!("{sql} SELECT COALESCE(SUM(length(CAST(body AS BLOB))+length(CAST(title AS BLOB))+length(CAST(library_name AS BLOB))+128),0) FROM matches"), args, |r| r.get(0)).map_err(|e| e.to_string())?;
            if total > 10000 || bytes > 16 * 1024 * 1024 {return Err("NOTE_LIMIT".into());}
        }
        let mut s = c.prepare(&format!("{sql} SELECT kind,id,node_id,title,library_id,library_name,media_kind,body,revision,updated_at FROM matches ORDER BY CASE WHEN ?8 THEN title END COLLATE NOCASE, (length(trim(body))>0) DESC,updated_at DESC,revision DESC,title COLLATE NOCASE,kind,id LIMIT ?6 OFFSET ?7")).map_err(|e| e.to_string())?;
        let items = s.query_map(params![language, notes_only, root_id, media_kind, query, limit, offset, title_order], |r| {
            Ok(WorkNoteEntry { target: if r.get::<_,String>(0)? == "BOOK" { Target::Book(r.get(1)?) } else { Target::Node(r.get(1)?) }, node_id:r.get(2)?, title:r.get(3)?, library_id:r.get(4)?, library_name:r.get(5)?, media_kind:r.get(6)?, body:r.get(7)?, revision:r.get(8)?, updated_at:r.get(9)? })
        }).map_err(|e| e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?;
        Ok(WorkNotesPage {items,total})
    })
}

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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn studio_lists_the_same_typed_notes_filters_and_hides_ignored_ancestors() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::new(temp.path().join("notes.db"));
        db.migrate().unwrap();
        db.connect().unwrap().execute_batch("INSERT INTO library_roots(id,path,display_name) VALUES(1,'synthetic-library','My Library');
          INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,total_video_count) VALUES
          (1,1,NULL,'synthetic-library','root','Root','CONTAINER',3),
          (2,1,1,'synthetic-library/work','Work','Work','WORK',1),
          (3,1,1,'synthetic-library/hidden','Hidden','Hidden','IGNORED',1),
          (4,1,3,'synthetic-library/hidden/work','Hidden child','Hidden child','WORK',1),
          (5,1,1,'synthetic-library/second','Second','Second','WORK',1);
          INSERT INTO comic_books(id,node_id,source_path,source_kind,display_name,modified_at) VALUES(2,2,'synthetic-library/book','IMAGE_FOLDER','Book','stamp');").unwrap();
        save(&db, Target::Node(2), "A personal thought".into(), 0).unwrap();
        save(&db, Target::Book(2), "Book memory".into(), 0).unwrap();
        let page = list(&db, true, None, None, "".into(), 0, 100).unwrap();
        assert_eq!(page.total, 2);
        assert!(page.items.iter().all(|i| i.updated_at.is_some()));
        assert!(page.items.iter().any(|i| i.target == Target::Node(2)));
        assert!(page.items.iter().any(|i| i.target == Target::Book(2)));
        let alphabetic = list_sorted(
            &db,
            true,
            None,
            None,
            String::new(),
            0,
            1,
            Some("title".into()),
        )
        .unwrap();
        assert_eq!(alphabetic.items[0].title, "Book");
        let next = list_sorted(
            &db,
            true,
            None,
            None,
            String::new(),
            1,
            1,
            Some("title".into()),
        )
        .unwrap();
        assert_eq!(next.items[0].title, "Work");
        assert_eq!(export_entries(&db).unwrap().len(), 2);
        assert!(list_sorted(
            &db,
            true,
            None,
            None,
            String::new(),
            0,
            10,
            Some("invalid".into())
        )
        .is_err());
        let page = list(&db, false, Some(1), Some("VIDEO".into()), "".into(), 0, 100).unwrap();
        assert_eq!(page.total, 3);
        assert!(page.items.iter().all(|i| i.node_id != 1 && i.node_id != 4));
        assert_eq!(
            list(&db, false, None, None, "thought".into(), 0, 10)
                .unwrap()
                .items[0]
                .target,
            Target::Node(2)
        );
        assert_eq!(
            list(&db, false, None, None, "".into(), 0, 1)
                .unwrap()
                .items
                .len(),
            1
        );
        assert_eq!(
            list(&db, false, Some(99), None, "".into(), 0, 10)
                .unwrap()
                .total,
            0
        );
        save(&db, Target::Node(2), "".into(), 1).unwrap();
        assert_eq!(
            list(&db, true, None, None, "".into(), 0, 10).unwrap().total,
            1
        );
        assert_eq!(
            save(&db, Target::Node(2), "stale".into(), 1)
                .err()
                .as_deref(),
            Some("NOTE_CONFLICT")
        );
        assert_eq!(get(&db, Target::Node(2)).unwrap().revision, 2);
    }
}

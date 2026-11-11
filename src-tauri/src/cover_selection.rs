//! Transient selection tickets. Originals use cache::store_manual_payload; derivatives remain poster_cache.
use crate::{
    cache,
    collection::Target,
    db::{AppResult, Database},
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub url: String,
    pub language: Option<String>,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone)]
struct Ticket {
    db: PathBuf,
    target: Target,
    fingerprint: String,
    at: Instant,
    items: Vec<Candidate>,
}
static TICKETS: LazyLock<Mutex<HashMap<String, Ticket>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
fn fingerprint(c: &Connection, target: Target) -> AppResult<String> {
    let node = crate::collection::validate(c, target)?;
    let row:String=c.query_row("SELECT json_array(n.absolute_path,n.updated_at,n.cover_source,n.cover_cache_path,(SELECT provider_subject_id FROM metadata_bindings WHERE node_id=n.id),(SELECT payload_json FROM tmdb_movie_bindings WHERE node_id=n.id)) FROM nodes n WHERE n.id=?1",[node],|r|r.get(0)).map_err(|e|e.to_string())?;
    let book = if let Target::Book(id) = target {
        c.query_row("SELECT json_array(b.revision,m.cover_cache_path) FROM comic_books b LEFT JOIN book_manual_covers m ON m.book_id=b.id WHERE b.id=?1",[id],|r|r.get::<_,String>(0)).map_err(|e|e.to_string())?
    } else {
        String::new()
    };
    Ok(format!(
        "{:x}",
        Sha256::digest(format!("{row}:{book}").as_bytes())
    ))
}
pub fn begin(db: &Database, target: Target) -> AppResult<String> {
    let fingerprint = fingerprint(&db.connect()?, target)?;
    let mut tickets = TICKETS.lock().unwrap_or_else(|p| p.into_inner());
    tickets.retain(|_, t| t.at.elapsed() < Duration::from_secs(300));
    if tickets.len() >= 8 {
        return Err("COVER_SELECTION_BUSY".into());
    }
    let token = uuid::Uuid::new_v4().simple().to_string();
    tickets.insert(
        token.clone(),
        Ticket {
            db: db.path().into(),
            target,
            fingerprint,
            at: Instant::now(),
            items: vec![],
        },
    );
    Ok(token)
}
fn ticket(db: &Database, token: &str) -> AppResult<Ticket> {
    let t = TICKETS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(token)
        .cloned()
        .ok_or("COVER_SELECTION_EXPIRED")?;
    if t.db != db.path()
        || t.at.elapsed() > Duration::from_secs(300)
        || fingerprint(&db.connect()?, t.target)? != t.fingerprint
    {
        return Err("COVER_SELECTION_EXPIRED".into());
    }
    Ok(t)
}
pub fn discard(token: &str) {
    TICKETS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(token);
}
pub fn online(db: &Database, token: &str) -> AppResult<Vec<Candidate>> {
    let t = ticket(db, token)?;
    let c = db.connect()?;
    let node = crate::collection::validate(&c, t.target)?;
    if matches!(t.target, Target::Book(_)) {
        let count: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM comic_books WHERE node_id=?1",
                [node],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if count != 1 {
            return Ok(vec![]);
        }
    }
    let mut items = if let Some(binding) = crate::tmdb::binding(&c, node)?.filter(|b| b.active) {
        crate::tmdb::cover_candidates(&binding.movie)?
    } else {
        vec![]
    };
    if let Some(url) = db.get_binding(node)?.and_then(|b| b.provider_image_url) {
        if crate::bangumi::valid_candidate_url(&url) {
            items.push(Candidate {
                url,
                language: None,
                width: 0,
                height: 0,
            });
        }
    }
    let current = ticket(db, token)?;
    if current.fingerprint != t.fingerprint {
        return Err("COVER_SELECTION_EXPIRED".into());
    }
    if let Some(current) = TICKETS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get_mut(token)
    {
        current.items = items.clone();
    }
    Ok(items)
}
pub fn embedded_preview(db: &Database, token: &str) -> AppResult<Option<String>> {
    let t = ticket(db, token)?;
    let Target::Book(id) = t.target else {
        return Err("COVER_SELECTION_INVALID".into());
    };
    let opened = crate::comic_reader::open(db, id)?;
    let bytes = crate::comic_reader::read_book_cover(db, id, &opened.book.revision)?;
    use base64::Engine;
    Ok(bytes.map(|b| {
        format!(
            "data:{};base64,{}",
            image::guess_format(&b)
                .map(|f| f.to_mime_type())
                .unwrap_or("image/jpeg"),
            base64::engine::general_purpose::STANDARD.encode(b)
        )
    }))
}
#[allow(clippy::too_many_arguments)]
pub fn apply(
    db: &Database,
    root: &Path,
    token: &str,
    local: Option<&Path>,
    index: Option<usize>,
    page: Option<i64>,
    raster: Option<&str>,
    clear: bool,
) -> AppResult<()> {
    let choices = usize::from(local.is_some())
        + usize::from(index.is_some())
        + usize::from(page.is_some())
        + usize::from(raster.is_some())
        + usize::from(clear);
    if choices > 1 {
        return Err("COVER_SELECTION_INVALID".into());
    }
    let t = ticket(db, token)?;
    let operation = cache::begin_cover_cache_operation();
    let bytes = if clear {
        None
    } else if let Some(path) = local {
        Some(cache::read_cover_payload(path)?)
    } else if let Some(index) = index {
        let item = t.items.get(index).ok_or("COVER_SELECTION_INVALID")?;
        Some(if item.url.starts_with("https://image.tmdb.org/") {
            crate::tmdb::candidate_bytes(&item.url)?
        } else {
            crate::bangumi::candidate_bytes(&item.url)?
        })
    } else if let Target::Book(id) = t.target {
        let opened = crate::comic_reader::open(db, id)?;
        if let Some(index) = page {
            if opened.book.document_format.is_some() {
                return Err("COVER_SELECTION_INVALID".into());
            }
            Some(crate::comic_reader::read_page_at_revision(
                db,
                id,
                index,
                Some(&opened.book.revision),
            )?)
        } else if let Some(raw) = raster {
            if opened.book.document_format.as_deref() != Some("PDF") || raw.len() > 3 * 1024 * 1024
            {
                return Err("COVER_SELECTION_INVALID".into());
            }
            use base64::Engine;
            Some(
                base64::engine::general_purpose::STANDARD
                    .decode(raw)
                    .map_err(|_| "COVER_SELECTION_INVALID")?,
            )
        } else {
            crate::comic_reader::read_book_cover(db, id, &opened.book.revision)?
        }
    } else {
        return Err("COVER_SELECTION_INVALID".into());
    };
    if !clear && bytes.is_none() {
        return Err("COVER_SELECTION_INVALID".into());
    }
    let mut restored = None;
    if clear {
        if let Target::Node(id) = t.target {
            if crate::tmdb::binding(&db.connect()?, id)?.is_none_or(|b| !b.active) {
                if let Some(b) = db.get_binding(id)? {
                    let subject = crate::models::BangumiSubject {
                        subject_id: b.provider_subject_id,
                        title: b.provider_title,
                        title_cn: b.provider_title_cn,
                        title_en: None,
                        title_ja: None,
                        title_ko: None,
                        match_aliases: vec![],
                        date: b.provider_date,
                        image_url: b.provider_image_url,
                        summary: None,
                        subject_type: b.provider_subject_type,
                    };
                    restored = crate::bangumi::download_cover(&operation, root, &subject)?;
                }
            }
        }
    }
    let mut c = db.connect()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    if fingerprint(&tx, t.target)? != t.fingerprint {
        return Err("COVER_SELECTION_EXPIRED".into());
    }
    let destination = bytes
        .map(|b| cache::store_manual_payload(&operation, root, t.target, &b))
        .transpose()?;
    match t.target {
        Target::Node(id) => {
            tx.execute("UPDATE nodes SET cover_source=?1,cover_cache_path=?2,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?3",params![if restored.is_some(){"BANGUMI"}else if clear {"PLACEHOLDER"}else{"MANUAL"},destination.as_ref().or(restored.as_ref()).map(|p|p.to_string_lossy()),id]).map_err(|e|e.to_string())?;
        }
        Target::Book(id) => {
            if let Some(path)=destination {tx.execute("INSERT INTO book_manual_covers(book_id,cover_cache_path) VALUES(?1,?2) ON CONFLICT(book_id) DO UPDATE SET cover_cache_path=excluded.cover_cache_path",params![id,path.to_string_lossy()])}else{tx.execute("DELETE FROM book_manual_covers WHERE book_id=?1",[id])}.map_err(|e|e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    discard(token);
    crate::poster_cache::schedule_warmup(db, root);
    Ok(())
}
pub(crate) fn book_manual(db: &Database, id: i64) -> AppResult<Option<PathBuf>> {
    let c = db.connect()?;
    crate::collection::validate(&c, Target::Book(id))?;
    let path: Option<String> = c
        .query_row(
            "SELECT cover_cache_path FROM book_manual_covers WHERE book_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(path) = &path {
        for root in db.library_paths()? {
            if cache::is_equal_or_within_checked(Path::new(path), &root)? {
                return Err("COVER_SELECTION_INVALID".into());
            }
        }
    }
    Ok(path.map(PathBuf::from))
}

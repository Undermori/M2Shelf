//! Atomic path remapping for an unchanged collection. Never writes, copies or removes media.
use crate::{
    db::{AppResult, Database},
    library_paths,
};
use rusqlite::{params, Connection, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};
#[derive(Clone, Serialize)]
struct Row {
    table: &'static str,
    field: &'static str,
    id: i64,
    path: String,
    size: Option<u64>,
    stamp: Option<String>,
    directory: bool,
}
#[derive(Clone)]
struct Plan {
    db: PathBuf,
    root: i64,
    old: String,
    new: String,
    rows: Vec<Row>,
    hashes: Vec<(String, String)>,
    fingerprint: String,
    at: Instant,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub token: Option<String>,
    pub old_path: String,
    pub new_path: String,
    pub checked: usize,
    pub missing: usize,
    pub samples: Vec<String>,
    pub issues: Vec<String>,
    pub verified_samples: usize,
    pub old_available: bool,
}
static PLANS: LazyLock<Mutex<HashMap<String, Plan>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
fn err<E>(_: E) -> String {
    "RELOCATE_INVALID".into()
}
fn rows(c: &Connection, root: i64) -> AppResult<Vec<Row>> {
    let mut all = vec![];
    for(table,field,query)in[
  ("nodes","absolute_path","SELECT id,absolute_path,NULL,NULL,0 FROM nodes WHERE library_root_id=?1"),
  ("media_files","absolute_path","SELECT f.id,f.absolute_path,f.file_size,f.modified_at,0 FROM media_files f JOIN nodes n ON n.id=f.node_id WHERE n.library_root_id=?1"),
  ("resource_files","absolute_path","SELECT f.id,f.absolute_path,f.file_size,f.modified_at,0 FROM resource_files f JOIN nodes n ON n.id=f.node_id WHERE n.library_root_id=?1"),
  ("comic_books","source_path","SELECT b.id,b.source_path,CASE WHEN b.source_kind='IMAGE_FOLDER' THEN NULL ELSE b.file_size END,CASE WHEN b.source_kind='IMAGE_FOLDER' THEN NULL ELSE b.modified_at END,b.source_kind='IMAGE_FOLDER' FROM comic_books b JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=?1"),
  ("comic_pages","source_locator","SELECT p.id,p.source_locator,p.file_size,p.modified_at,0 FROM comic_pages p JOIN comic_books b ON b.id=p.comic_book_id JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=?1 AND b.source_kind='IMAGE_FOLDER'"),
  ("indexed_media_metadata","source_key","SELECT m.rowid,m.source_key,NULL,NULL,0 FROM indexed_media_metadata m JOIN nodes n ON n.id=m.node_id WHERE n.library_root_id=?1")
 ]{let mut s=c.prepare(query).map_err(err)?;for row in s.query_map([root],|r|Ok(Row{table,field,id:r.get(0)?,path:r.get(1)?,size:r.get(2)?,stamp:r.get(3)?,directory:r.get(4)?})).map_err(err)?{all.push(row.map_err(err)?);if all.len()>200000{return Err("RELOCATE_LIMIT".into());}}}
    all.sort_by(|a, b| (a.table, a.id).cmp(&(b.table, b.id)));
    Ok(all)
}
fn fingerprint(rows: &[Row]) -> AppResult<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(rows).map_err(err)?)
    ))
}
/// Component-aware, case-insensitive mapping; textual siblings and `..` never qualify.
pub(crate) fn mapped(path: &str, old: &str, new: &str) -> AppResult<String> {
    let clean = crate::db::display_path(Path::new(path));
    let origin = crate::db::display_path(Path::new(old));
    let p = Path::new(&clean);
    let o = Path::new(&origin);
    let components = p.components().collect::<Vec<_>>();
    let root = o.components().collect::<Vec<_>>();
    if !p.is_absolute()
        || components.len() < root.len()
        || components
            .iter()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        || !components.iter().zip(&root).all(|(a, b)| {
            a.as_os_str().to_string_lossy().to_lowercase()
                == b.as_os_str().to_string_lossy().to_lowercase()
        })
    {
        return Err("RELOCATE_INVALID".into());
    }
    let mut destination = PathBuf::from(new);
    for part in components.iter().skip(root.len()) {
        destination.push(part.as_os_str());
    }
    Ok(destination.to_string_lossy().into_owned())
}
fn sample(path: &Path, root: &Path) -> AppResult<String> {
    let m = fs::metadata(path).map_err(err)?;
    let canonical_root = fs::canonicalize(root).map_err(err)?;
    let mut file = crate::comic_reader::checked_file(
        path,
        &canonical_root,
        m.len(),
        &crate::comics::modified(&m),
    )?;
    let mut hash = Sha256::new();
    hash.update(m.len().to_le_bytes());
    let mut data = vec![0; 65536];
    let n = file.read(&mut data).map_err(err)?;
    hash.update(&data[..n]);
    if m.len() > 65536 {
        file.seek(SeekFrom::Start(m.len().saturating_sub(65536)))
            .map_err(err)?;
        let n = file.read(&mut data).map_err(err)?;
        hash.update(&data[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn check_files(
    rows: &[Row],
    old: &str,
    new: &str,
) -> AppResult<(usize, Vec<String>, BTreeMap<String, String>)> {
    let start = Instant::now();
    let mut invalid = 0;
    let mut issues = vec![];
    let mut files = BTreeMap::new();
    let mut parents = HashSet::new();
    for row in rows {
        if start.elapsed() > Duration::from_secs(30) {
            return Err("RELOCATE_LIMIT".into());
        }
        let target = mapped(&row.path, old, new)?;
        let check: AppResult<()> = (|| {
            let p = Path::new(&target);
            let parent = p.parent().ok_or("RELOCATE_INVALID")?;
            if parents.insert(parent.to_path_buf()) {
                library_paths::plain_directory(parent)?;
            }
            let m = fs::symlink_metadata(p).map_err(err)?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if m.file_attributes() & 0x400 != 0 {
                    return Err("RELOCATE_INVALID".into());
                }
            }
            if m.file_type().is_symlink() || row.directory && !m.is_dir() {
                return Err("RELOCATE_INVALID".into());
            }
            if let Some(size) = row.size {
                if !m.is_file() || m.len() != size {
                    return Err("RELOCATE_INVALID".into());
                }
                if let Some(expected) = &row.stamp {
                    let expected = chrono::DateTime::parse_from_rfc3339(expected).map_err(err)?;
                    let actual = chrono::DateTime::<chrono::Utc>::from(m.modified().map_err(err)?);
                    if expected.timestamp() != actual.timestamp()
                        || expected.timestamp_subsec_nanos() != 0
                            && expected.timestamp_subsec_nanos() != actual.timestamp_subsec_nanos()
                    {
                        return Err("RELOCATE_INVALID".into());
                    }
                }
                files.insert(target.clone(), row.path.clone());
            }
            Ok(())
        })();
        if check.is_err() {
            invalid += 1;
            if issues.len() < 16 {
                issues.push(mapped(&row.path, old, "")?);
            }
        }
    }
    Ok((invalid, issues, files))
}
pub fn preview(db: &Database, root: i64, path: &Path) -> AppResult<Preview> {
    let new = library_paths::plain_directory(path)?;
    let new = crate::db::display_path(&db.validate_new_root_path(&new)?);
    let (old, rows) = db.read_snapshot(|c| {
        let old = c
            .query_row("SELECT path FROM library_roots WHERE id=?1", [root], |r| {
                r.get::<_, String>(0)
            })
            .map_err(err)?;
        Ok((old, rows(c, root)?))
    })?;
    if rows.is_empty() {
        return Err("RELOCATE_INVALID".into());
    }
    let fingerprint = fingerprint(&rows)?;
    let (missing, issues, files) = check_files(&rows, &old, &new)?;
    let old_available = Path::new(&old).is_dir();
    let mut verified_samples = 0;
    let mut hashes = vec![];
    let list = files.iter().collect::<Vec<_>>();
    if missing == 0 {
        for index in (0..list.len()).step_by((list.len() / 32).max(1)).take(32) {
            let (target, source) = list[index];
            let hash = sample(Path::new(target), Path::new(&new))?;
            if old_available {
                let old_root = fs::canonicalize(&old).map_err(err)?;
                if sample(Path::new(source), &old_root)? != hash {
                    return Err("RELOCATE_CONTENT_MISMATCH".into());
                }
                verified_samples += 1;
            }
            hashes.push((target.clone(), hash));
        }
    }
    let samples = list
        .iter()
        .take(8)
        .map(|(_, old_path)| mapped(old_path, &old, ""))
        .collect::<Result<Vec<_>, _>>()?;
    let token = if missing == 0 {
        let token = uuid::Uuid::new_v4().simple().to_string();
        let mut plans = PLANS.lock().unwrap_or_else(|p| p.into_inner());
        plans.retain(|_, p| p.at.elapsed() < Duration::from_secs(300));
        if plans.len() >= 4 {
            return Err("RELOCATE_BUSY".into());
        }
        plans.insert(
            token.clone(),
            Plan {
                db: db.path().into(),
                root,
                old: old.clone(),
                new: new.clone(),
                rows: rows.clone(),
                hashes,
                fingerprint,
                at: Instant::now(),
            },
        );
        Some(token)
    } else {
        None
    };
    Ok(Preview {
        token,
        old_path: old,
        new_path: new,
        checked: files.len(),
        missing,
        samples,
        issues,
        verified_samples,
        old_available,
    })
}
pub fn discard(token: &str) {
    PLANS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(token);
}
pub fn apply(db: &Database, token: &str) -> AppResult<()> {
    let plan = PLANS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(token)
        .cloned()
        .ok_or("RELOCATE_EXPIRED")?;
    if plan.db != db.path() || plan.at.elapsed() > Duration::from_secs(300) {
        return Err("RELOCATE_EXPIRED".into());
    }
    let canonical = library_paths::plain_directory(Path::new(&plan.new))?;
    db.validate_new_root_path(&canonical)?;
    if check_files(&plan.rows, &plan.old, &plan.new)?.0 != 0 {
        return Err("RELOCATE_INVALID".into());
    }
    for (path, hash) in &plan.hashes {
        if sample(Path::new(path), &canonical)? != *hash {
            return Err("RELOCATE_CONTENT_MISMATCH".into());
        }
    }
    let mut c = db.connect()?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    let old: String = tx
        .query_row(
            "SELECT path FROM library_roots WHERE id=?1",
            [plan.root],
            |r| r.get(0),
        )
        .map_err(err)?;
    if old != plan.old || fingerprint(&rows(&tx, plan.root)?)? != plan.fingerprint {
        return Err("RELOCATE_EXPIRED".into());
    }
    for row in &plan.rows {
        let path = mapped(&row.path, &plan.old, &plan.new)?;
        tx.execute(
            &format!(
                "UPDATE {} SET {}=?1 WHERE rowid=?2 AND {}=?3",
                row.table, row.field, row.field
            ),
            params![path, row.id, row.path],
        )
        .map_err(err)?;
    }
    tx.execute(
        "UPDATE library_roots SET path=?1 WHERE id=?2",
        params![plan.new, plan.root],
    )
    .map_err(err)?;
    // Image-folder locators are absolute and participate in the existing book revision digest.
    let mut statement=tx.prepare("SELECT b.id,b.file_size,b.modified_at FROM comic_books b JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=?1 AND b.source_kind='IMAGE_FOLDER'").map_err(err)?;
    let books = statement
        .query_map([plan.root], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, u64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    drop(statement);
    for (id, size, stamp) in books {
        let mut hash = Sha256::new();
        hash.update(serde_json::to_vec(&(size, &stamp)).map_err(err)?);
        let mut pages=tx.prepare("SELECT source_locator,file_size,modified_at,crc32 FROM comic_pages WHERE comic_book_id=?1 ORDER BY page_index").map_err(err)?;
        for page in pages
            .query_map([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<u32>>(3)?,
                ))
            })
            .map_err(err)?
        {
            let value = serde_json::to_vec(&page.map_err(err)?).map_err(err)?;
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value);
        }
        tx.execute(
            "UPDATE comic_books SET revision=?1 WHERE id=?2",
            params![format!("{:x}", hash.finalize()), id],
        )
        .map_err(err)?;
    }
    // Baseline keys are absolute. Reset only this library's read-only enumeration baseline.
    tx.execute(
        "DELETE FROM library_scan_snapshots WHERE library_root_id=?1",
        [plan.root],
    )
    .map_err(err)?;
    let mut fk = tx.prepare("PRAGMA foreign_key_check").map_err(err)?;
    if fk.exists([]).map_err(err)? {
        return Err("RELOCATE_INVALID".into());
    }
    drop(fk);
    #[cfg(test)]
    if FAIL.with(|f| f.replace(false)) {
        return Err("RELOCATE_TEST_INTERRUPTED".into());
    }
    tx.commit().map_err(err)?;
    discard(token);
    Ok(())
}
#[cfg(test)]
thread_local! {pub(crate) static FAIL:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}

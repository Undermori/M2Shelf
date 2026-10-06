use rusqlite::OptionalExtension;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
};

use crate::{
    bangumi,
    db::{AppResult, Database},
    models::BangumiSubject,
};

static ACTIVE: AtomicBool = AtomicBool::new(false);
const STARTUP_SUBJECT_BUDGET: usize = 32;
#[derive(Default)]
struct SessionBudget {
    attempted: HashSet<(i64, i64)>,
    provider_stopped: bool,
}
static SESSION: OnceLock<Mutex<SessionBudget>> = OnceLock::new();
struct ActiveGuard;
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::Release);
    }
}

/// One resumable, deduplicated pass. Completed Subjects (including empty aliases) never refetch.
pub fn sync_pending(database: &Database) -> AppResult<bool> {
    if ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Ok(false);
    }
    let _guard = ActiveGuard;
    let mut session = SESSION
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    sync_with_budget(database, &mut session, bangumi::enrich_subject)
}

#[cfg(test)]
fn sync_with(
    database: &Database,
    fetch: impl FnMut(&BangumiSubject) -> AppResult<BangumiSubject>,
) -> AppResult<bool> {
    sync_with_budget(database, &mut SessionBudget::default(), fetch)
}

fn sync_with_budget(
    database: &Database,
    session: &mut SessionBudget,
    mut fetch: impl FnMut(&BangumiSubject) -> AppResult<BangumiSubject>,
) -> AppResult<bool> {
    if session.provider_stopped {
        return Ok(false);
    }
    let connection = database.connect()?;
    let cursor = connection
        .query_row(
            "SELECT value FROM settings WHERE key='alias_backfill_cursor'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .and_then(|value| serde_json::from_str::<(i64, i64)>(&value).ok());
    let mut pending = database.pending_alias_subjects()?;
    pending.sort_by_key(|subject| {
        let key = (subject.subject_type, subject.subject_id);
        (cursor.is_some_and(|cursor| key <= cursor), key)
    });
    let mut changed = false;
    for subject in pending {
        if session.attempted.len() >= STARTUP_SUBJECT_BUDGET {
            break;
        }
        let enabled = connection
            .query_row(
                "SELECT value FROM settings WHERE key='bangumi_search_enabled'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .is_none_or(|value| value != "false");
        if !enabled {
            break;
        }
        let key = (subject.subject_type, subject.subject_id);
        if !session.attempted.insert(key) {
            continue;
        }
        connection.execute("INSERT INTO settings(key,value) VALUES('alias_backfill_cursor',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(&key).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        // Keep progress on network failure; retry only unfinished Subjects on a later launch.
        let detail = match fetch(&subject) {
            Ok(detail) => detail,
            Err(error) if bangumi::is_provider_wide_detail_error(&error) => {
                session.provider_stopped = true;
                break;
            }
            Err(_) => continue,
        };
        if detail.subject_id != subject.subject_id || detail.subject_type != subject.subject_type {
            break;
        }
        changed |= database.complete_provider_alias_sync(&detail)?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};
    use tempfile::TempDir;

    fn fixture() -> (TempDir, Database) {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.db");
        let database = Database::new(path.clone());
        database.migrate().unwrap();
        let root_path = temp.path().join("media");
        std::fs::create_dir(&root_path).unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = Connection::open(path).unwrap();
        for (id, subject) in [(1, 10), (2, 10), (3, 20), (4, 30)] {
            connection.execute("INSERT INTO nodes(id,library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(?1,?2,?3,'Show','Show','WORK')", params![id,root.id,format!("fixture-{id}")]).unwrap();
            connection.execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(?1,'BANGUMI',?2,2,'Show')", params![id,subject]).unwrap();
        }
        (temp, database)
    }

    #[test]
    fn backfill_deduplicates_persists_empty_results_and_resumes_only_unfinished_subjects() {
        let (_temp, database) = fixture();
        let mut calls = Vec::new();
        assert!(sync_with(&database, |subject| {
            calls.push(subject.subject_id);
            if subject.subject_id == 30 {
                return Err("api.bgm.tv 返回 HTTP 503".into());
            }
            let mut detail = subject.clone();
            if subject.subject_id == 10 {
                detail.match_aliases = vec!["Official alias".into()];
            }
            Ok(detail)
        })
        .unwrap());
        assert_eq!(calls, vec![10, 20, 30]);
        assert_eq!(database.search("Official alias", None).unwrap().len(), 2);
        assert_eq!(
            database
                .pending_alias_subjects()
                .unwrap()
                .iter()
                .map(|s| s.subject_id)
                .collect::<Vec<_>>(),
            vec![30]
        );
        assert!(!sync_with(&database, |subject| Ok(subject.clone())).unwrap());
        assert!(!sync_with(&database, |_| panic!("completed Subjects must not refetch")).unwrap());
        // A later binding of the same Subject reuses completed aliases, even when its search row
        // omitted aliases (e.g. optional network enrichment failed).
        let subject = BangumiSubject {
            subject_id: 10,
            subject_type: 2,
            title: "Show".into(),
            title_cn: None,
            title_en: None,
            title_ja: None,
            title_ko: None,
            match_aliases: vec![],
            date: None,
            image_url: None,
            summary: None,
        };
        database.save_confirmed_binding(4, &subject).unwrap();
        assert_eq!(database.search("Official alias", None).unwrap().len(), 3);
        assert!(database.pending_alias_subjects().unwrap().is_empty());
    }

    #[test]
    fn stale_subject_does_not_block_others_and_provider_failure_preserves_pending_work() {
        let (_temp, database) = fixture();
        let mut calls = Vec::new();
        assert!(!sync_with(&database, |subject| {
            calls.push(subject.subject_id);
            if subject.subject_id == 10 {
                Err("api.bgm.tv 返回 HTTP 404".into())
            } else {
                Ok(subject.clone())
            }
        })
        .unwrap());
        assert_eq!(calls, vec![10, 20, 30]);
        assert_eq!(database.pending_alias_subjects().unwrap().len(), 1);
        assert!(!sync_with(&database, |_| Err("api.bgm.tv 返回 HTTP 503".into())).unwrap());
        assert_eq!(database.pending_alias_subjects().unwrap().len(), 1);
    }
    #[test]
    fn process_budget_counts_failures_repeated_calls_and_rotates_on_restart() {
        let (_temp, database) = fixture();
        let connection = database.connect().unwrap();
        for id in 5..=44 {
            connection.execute("INSERT INTO nodes(id,library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(?1,1,?2,'Show','Show','WORK')",params![id,format!("fixture-{id}")]).unwrap();
            connection.execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(?1,'BANGUMI',?2,2,'Show')",params![id,id+95]).unwrap();
        }
        let mut session = SessionBudget::default();
        let mut calls = Vec::new();
        sync_with_budget(&database, &mut session, |subject| {
            calls.push(subject.subject_id);
            Err("api.bgm.tv 返回 HTTP 404".into())
        })
        .unwrap();
        assert_eq!(calls.len(), 32);
        sync_with_budget(&database, &mut session, |_| {
            panic!("refresh must not reset process budget")
        })
        .unwrap();
        let last = *calls.last().unwrap();
        let mut next = Vec::new();
        sync_with_budget(&database, &mut SessionBudget::default(), |subject| {
            next.push(subject.subject_id);
            Err("api.bgm.tv 返回 HTTP 404".into())
        })
        .unwrap();
        assert_eq!(next.len(), 32);
        assert!(
            next[0] > last,
            "rotation must let later subjects run before old failures"
        );
        connection.execute("INSERT INTO settings(key,value) VALUES('bangumi_search_enabled','false') ON CONFLICT(key) DO UPDATE SET value='false'",[]).unwrap();
        sync_with_budget(&database, &mut SessionBudget::default(), |_| {
            panic!("disabled feature must start no requests")
        })
        .unwrap();
    }

    #[test]
    fn provider_breaker_survives_another_ipc_call() {
        let (_temp, database) = fixture();
        let mut session = SessionBudget::default();
        let mut calls = 0;
        sync_with_budget(&database, &mut session, |_| {
            calls += 1;
            Err("api.bgm.tv 返回 HTTP 503".into())
        })
        .unwrap();
        assert_eq!(calls, 1);
        sync_with_budget(&database, &mut session, |_| {
            panic!("provider breaker must survive refresh")
        })
        .unwrap();
    }
}

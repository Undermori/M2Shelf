//! Independent acceptance fixtures for the four additive features. No personal data/network.
use crate::{
    db::Database,
    models::{LibraryMediaKind, LibraryRecognitionMode, LibraryRoot, ScanProgress},
    scanner::{ScanControl, ScanTarget},
    search_filters::SearchFilters,
};
use rusqlite::params;
use std::{
    fs,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

fn scan(db: &Database, root: &LibraryRoot, path: Option<std::path::PathBuf>, cancelled: bool) {
    let target = path.unwrap_or_else(|| root.path.clone().into());
    let parent = if target.to_string_lossy() == root.path {
        None
    } else {
        Some(
            db.connect()
                .unwrap()
                .query_row(
                    "SELECT parent_node_id FROM nodes WHERE absolute_path=?1",
                    [target.to_string_lossy()],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap(),
        )
    };
    let progress:ScanProgress=serde_json::from_value(serde_json::json!({"scanId":"delta-test","rootId":root.id,"currentPath":"","foldersScanned":0,"videosFound":0,"status":"RUNNING","errors":0,"message":null})).unwrap();
    let control = ScanControl {
        unchanged_directories: Default::default(),
        scan_id: "delta-test".into(),
        cancel: Arc::new(AtomicBool::new(cancelled)),
        progress: Arc::new(Mutex::new(progress)),
    };
    crate::scanner::run_scan(
        None,
        db,
        vec![ScanTarget {
            root: root.clone(),
            path: target,
            parent_node_id: parent,
        }],
        &control,
        &crate::db::default_video_extensions(),
    );
}
fn setup(kind: LibraryMediaKind) -> (tempfile::TempDir, Database, LibraryRoot) {
    let t = crate::db::test_temp_dir();
    let path = t.path().join("Root_百分比% 漫画");
    fs::create_dir(&path).unwrap();
    let db = Database::new(t.path().join("fixture.db"));
    db.migrate().unwrap();
    let root = db
        .add_root_with_kind(&path, None, kind, LibraryRecognitionMode::Folder)
        .unwrap();
    (t, db, root)
}
fn changes(db: &Database, id: i64) -> crate::scan_changes::ScanChanges {
    crate::scan_changes::read(&db.connect().unwrap(), id)
        .unwrap()
        .unwrap()
}
fn node(db: &Database, name: &str) -> i64 {
    db.connect()
        .unwrap()
        .query_row("SELECT id FROM nodes WHERE folder_name=?1", [name], |r| {
            r.get(0)
        })
        .unwrap()
}

#[test]
fn recent_capacity_migration_removal_and_reopen_do_not_change_watch_counts() {
    use crate::collection::{self, Target};
    let (_temp, db, root) = setup(LibraryMediaKind::Animation);
    for i in 0..53 {
        let dir = std::path::Path::new(&root.path).join(format!("Work {i}"));
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("episode.mkv"), b"video").unwrap();
    }
    scan(&db, &root, None, false);
    let c = db.connect().unwrap();
    for i in 0..53 {
        c.execute(
            "INSERT INTO watch_history(node_id,last_watched_at,watch_count) VALUES(?1,?2,7)",
            params![
                node(&db, &format!("Work {i}")),
                format!("2026-10-22T00:00:{i:02}.000Z")
            ],
        )
        .unwrap();
    }
    c.execute_batch(
        "DROP TABLE recent_activity; DELETE FROM mediashelf_schema_migrations WHERE version=31;",
    )
    .unwrap();
    db.migrate().unwrap();
    let recent = db.list_recently_watched().unwrap();
    assert_eq!(
        serde_json::to_value(&recent).unwrap(),
        serde_json::to_value(db.list_recently_watched_reference().unwrap()).unwrap()
    );
    assert_eq!(recent.len(), 30);
    assert_eq!(recent[0].node.folder_name, "Work 52");
    collection::remove_recent(
        &db,
        &[
            Target::Node(recent[0].node.id),
            Target::Node(recent[1].node.id),
        ],
    )
    .unwrap();
    assert_eq!(
        Database::new(db.path().into())
            .list_recently_watched()
            .unwrap()
            .len(),
        28
    );
    scan(&db, &root, None, false);
    assert_eq!(db.list_recently_watched().unwrap().len(), 28);
    let id = recent[0].node.id;
    assert_eq!(
        c.query_row(
            "SELECT watch_count FROM watch_history WHERE node_id=?1",
            [id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        7
    );
    db.record_node_watched(id).unwrap();
    assert_eq!(db.list_recently_watched().unwrap()[0].node.id, id);
    collection::clear_recent(&db).unwrap();
    assert!(db.list_recently_watched().unwrap().is_empty());
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM watch_history", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        53
    );
    for i in 0..31 {
        db.record_node_watched(node(&db, &format!("Work {i}")))
            .unwrap();
    }
    assert_eq!(db.list_recently_watched().unwrap().len(), 30);
    assert!(db
        .list_recently_watched()
        .unwrap()
        .iter()
        .all(|r| r.node.folder_name != "Work 0"));
}

#[test]
fn recent_books_share_no_node_identity_and_removal_keeps_progress_and_bookmarks() {
    use crate::collection::{self, Target};
    let (_temp, db, root) = setup(LibraryMediaKind::Ebook);
    let dir = std::path::Path::new(&root.path).join("Two books");
    fs::create_dir(&dir).unwrap();
    vector_pdf(&dir.join("One.pdf"));
    vector_pdf(&dir.join("Two.pdf"));
    scan(&db, &root, None, false);
    let c = db.connect().unwrap();
    let ids = c
        .prepare("SELECT id FROM comic_books ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get::<_, i64>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(ids.len(), 2);
    for id in &ids {
        crate::comic_reader::progress(&db, *id, 0).unwrap();
        c.execute(
            "INSERT INTO comic_bookmarks(comic_book_id,page_index) VALUES(?1,0)",
            [id],
        )
        .unwrap();
    }
    assert_eq!(db.list_recently_watched().unwrap().len(), 2);
    assert_eq!(
        serde_json::to_value(db.list_recently_watched().unwrap()).unwrap(),
        serde_json::to_value(db.list_recently_watched_reference().unwrap()).unwrap()
    );
    assert_eq!(
        db.list_recently_watched().unwrap()[0].comic_book_id,
        Some(ids[1])
    );
    collection::remove_recent(&db, &[Target::Book(ids[0])]).unwrap();
    assert_eq!(db.list_recently_watched().unwrap().len(), 1);
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM comic_reading_progress", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM comic_bookmarks", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    db.migrate().unwrap();
    assert_eq!(db.list_recently_watched().unwrap().len(), 1);
    crate::comic_reader::progress(&db, ids[0], 0).unwrap();
    assert_eq!(
        db.list_recently_watched().unwrap()[0].comic_book_id,
        Some(ids[0])
    );
}

#[test]
fn recent_batched_snapshot_matches_reference_across_roots_hidden_ancestors_and_shared_books() {
    let (temp, db, book_root) = setup(LibraryMediaKind::Ebook);
    let books = std::path::Path::new(&book_root.path).join("Shared books");
    fs::create_dir(&books).unwrap();
    vector_pdf(&books.join("One.pdf"));
    vector_pdf(&books.join("Two.pdf"));
    scan(&db, &book_root, None, false);
    for kind in [LibraryMediaKind::Animation, LibraryMediaKind::LiveAction] {
        let path = temp.path().join(format!("Video {kind:?}"));
        fs::create_dir_all(path.join("Series/Season 1")).unwrap();
        fs::write(
            path.join("Series/Season 1/Show S01E01.mkv"),
            b"synthetic video",
        )
        .unwrap();
        let root = db
            .add_root_with_kind(&path, None, kind, LibraryRecognitionMode::Folder)
            .unwrap();
        scan(&db, &root, None, false);
        let connection = db.connect().unwrap();
        let ids = connection
            .prepare(
                "SELECT id FROM nodes WHERE library_root_id=?1 AND total_video_count>0 ORDER BY id",
            )
            .unwrap()
            .query_map([root.id], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for id in ids {
            db.record_node_watched(id).unwrap();
        }
    }
    let connection = db.connect().unwrap();
    let ids = connection
        .prepare("SELECT id FROM comic_books ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get::<_, i64>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for id in &ids {
        crate::comic_reader::progress(&db, *id, 0).unwrap();
    }
    connection
        .execute(
            "UPDATE recent_activity SET opened_at='2026-10-28T00:00:00Z'",
            [],
        )
        .unwrap();
    for hide in [false, true] {
        if hide {
            connection
                .execute(
                    "UPDATE nodes SET node_type='IGNORED' WHERE folder_name='Series'",
                    [],
                )
                .unwrap();
        }
        let before = db.list_recently_watched_reference().unwrap();
        for _ in 0..2 {
            assert_eq!(
                serde_json::to_value(db.list_recently_watched().unwrap()).unwrap(),
                serde_json::to_value(&before).unwrap()
            );
        }
        assert_eq!(
            before
                .iter()
                .filter(|entry| entry.comic_book_id.is_some())
                .count(),
            2
        );
    }
}

#[test]
fn collection_states_are_typed_manual_and_preserved_by_scan() {
    use crate::collection::{self, Status, Target};
    let (_temp, db, root) = setup(LibraryMediaKind::Ebook);
    let dir = std::path::Path::new(&root.path).join("Two books");
    fs::create_dir(&dir).unwrap();
    vector_pdf(&dir.join("One.pdf"));
    vector_pdf(&dir.join("Two.pdf"));
    scan(&db, &root, None, false);
    let c = db.connect().unwrap();
    let ids = c
        .prepare("SELECT id FROM comic_books ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get::<_, i64>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    crate::comic_reader::progress(&db, ids[0], 0).unwrap();
    collection::set_status(&db, Target::Book(ids[0]), Some(Status::Completed)).unwrap();
    collection::set_status(&db, Target::Book(ids[1]), Some(Status::Want)).unwrap();
    assert_eq!(collection::statuses(&db).unwrap().entries.len(), 2);
    assert!(collection::set_status(
        &db,
        Target::Node(node(&db, "Two books")),
        Some(Status::Paused)
    )
    .is_ok());
    assert!(collection::set_status(&db, Target::Node(-1), Some(Status::Paused)).is_err());
    let before: String = c
        .query_row(
            "SELECT last_read_at FROM comic_reading_progress WHERE comic_book_id=?1",
            [ids[0]],
            |r| r.get(0),
        )
        .unwrap();
    scan(&db, &root, None, false);
    db.migrate().unwrap();
    assert_eq!(
        collection::statuses(&Database::new(db.path().into()))
            .unwrap()
            .entries
            .len(),
        3
    );
    assert_eq!(
        c.query_row(
            "SELECT last_read_at FROM comic_reading_progress WHERE comic_book_id=?1",
            [ids[0]],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        before
    );
    collection::set_status(&db, Target::Book(ids[0]), None).unwrap();
    assert_eq!(collection::statuses(&db).unwrap().entries.len(), 2);
}

#[test]
fn series_and_book_work_metadata_keep_children_independent() {
    use crate::collection::{self, Status, Target};
    for kind in [
        LibraryMediaKind::Animation,
        LibraryMediaKind::Comic,
        LibraryMediaKind::Ebook,
    ] {
        let (_t, db, root) = setup(kind);
        for name in ["Series/Work A", "Series/Work B"] {
            let dir = std::path::Path::new(&root.path).join(name);
            fs::create_dir_all(&dir).unwrap();
            if kind.is_book() {
                vector_pdf(&dir.join("Book.pdf"));
            } else {
                fs::write(dir.join("Episode S01E01.mkv"), b"synthetic").unwrap();
            }
        }
        scan(&db, &root, None, false);
        let series = node(&db, "Series");
        let child = node(&db, "Work A");
        db.connect()
            .unwrap()
            .execute(
                "UPDATE nodes SET node_type='CONTAINER',manual_type_override=1 WHERE id=?1",
                [series],
            )
            .unwrap();
        collection::set_status(&db, Target::Node(series), Some(Status::Want)).unwrap();
        crate::notes::save(&db, Target::Node(series), "Series note".into(), 0).unwrap();
        assert!(crate::notes::get(&db, Target::Node(child))
            .unwrap()
            .body
            .is_empty());
        assert!(collection::statuses(&db)
            .unwrap()
            .entries
            .iter()
            .all(|entry| entry.target == Target::Node(series)));
        let token = crate::cover_selection::begin(&db, Target::Node(series)).unwrap();
        crate::cover_selection::discard(&token);
        let hidden = db
            .connect()
            .unwrap()
            .query_row(
                "SELECT id FROM nodes WHERE library_root_id=?1 AND parent_node_id IS NULL",
                [root.id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        assert!(
            collection::set_status(&db, Target::Node(hidden), Some(Status::Completed)).is_err()
        );
        scan(&db, &root, None, false);
        assert_eq!(
            crate::notes::get(&db, Target::Node(series)).unwrap().body,
            "Series note"
        );
        assert_eq!(collection::statuses(&db).unwrap().entries.len(), 1);
    }
}

#[test]
fn collection_same_numeric_ids_mixed_removal_is_atomic_and_cascades() {
    use crate::collection::{self, Status, Target};
    let (_temp, db, root) = setup(LibraryMediaKind::Animation);
    let dir = std::path::Path::new(&root.path).join("Work");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("01.mkv"), b"video").unwrap();
    scan(&db, &root, None, false);
    let id = node(&db, "Work");
    let c = db.connect().unwrap();
    c.execute("INSERT INTO comic_books(id,node_id,source_path,source_kind,display_name,modified_at,page_count) VALUES(?1,?1,'synthetic.pdf','ZIP_ARCHIVE','Book','2026-10-22',1)",[id]).unwrap();
    c.execute(
        "INSERT INTO comic_reading_progress(comic_book_id,last_page_index) VALUES(?1,0)",
        [id],
    )
    .unwrap();
    c.execute(
        "INSERT INTO comic_bookmarks(comic_book_id,page_index) VALUES(?1,0)",
        [id],
    )
    .unwrap();
    collection::set_status(&db, Target::Node(id), Some(Status::Want)).unwrap();
    collection::set_status(&db, Target::Book(id), Some(Status::Completed)).unwrap();
    assert_eq!(collection::statuses(&db).unwrap().entries.len(), 2);
    db.record_node_watched(id).unwrap();
    collection::record_recent(&c, Target::Book(id)).unwrap();
    assert!(collection::remove_recent(&db, &[Target::Node(id), Target::Book(i64::MAX)]).is_err());
    assert_eq!(db.list_recently_watched().unwrap().len(), 2);
    collection::remove_recent(&db, &[Target::Node(id)]).unwrap();
    let recent = db.list_recently_watched().unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].comic_book_id, Some(id));
    collection::remove_recent(&db, &[Target::Book(id), Target::Node(id), Target::Book(id)])
        .unwrap();
    assert!(db.list_recently_watched().unwrap().is_empty());
    assert_eq!(
        c.query_row(
            "SELECT watch_count FROM watch_history WHERE node_id=?1",
            [id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM comic_bookmarks", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    collection::record_recent(&c, Target::Book(id)).unwrap();
    c.execute("DELETE FROM comic_books WHERE id=?1", [id])
        .unwrap();
    assert!(db.list_recently_watched().unwrap().is_empty());
    assert_eq!(collection::statuses(&db).unwrap().entries.len(), 1);
}

#[test]
fn format_choices_skip_extensionless_and_blank_values_without_removing_indexed_files() {
    let (_temp, db, root) = setup(LibraryMediaKind::Animation);
    let work = std::path::Path::new(&root.path).join("Format sample");
    fs::create_dir(&work).unwrap();
    fs::write(work.join("LICENSE"), b"extensionless attachment").unwrap();
    fs::write(work.join("episode.mkv"), b"synthetic video").unwrap();
    scan(&db, &root, None, false);
    assert_eq!(
        crate::search_filters::extensions(&db, Some(root.id)).unwrap(),
        vec!["MKV"]
    );
    let connection = db.connect().unwrap();
    let file: i64 = connection
        .query_row(
            "SELECT id FROM resource_files WHERE file_name='LICENSE'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    connection
        .execute(
            "UPDATE resource_files SET extension=?1 WHERE id=?2",
            params![" \t\r\n", file],
        )
        .unwrap();
    assert_eq!(
        crate::search_filters::extensions(&db, None).unwrap(),
        vec!["MKV"]
    );
    assert!(crate::search_filters::extensions(&db, Some(root.id + 100))
        .unwrap()
        .is_empty());
    assert_eq!(
        connection
            .query_row("SELECT id FROM resource_files WHERE id=?1", [file], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        file
    );
}

#[test]
fn scan_deltas_first_import_no_change_source_update_and_metadata_are_distinct() {
    let (_t, db, root) = setup(LibraryMediaKind::Animation);
    let work = std::path::Path::new(&root.path).join("Example");
    fs::create_dir(&work).unwrap();
    fs::write(work.join("01.mkv"), b"original").unwrap();
    scan(&db, &root, None, false);
    let delta = changes(&db, root.id);
    assert_eq!(
        (delta.added, delta.updated, delta.unchanged, delta.removed),
        (1, 0, 0, Some(0))
    );
    let id = node(&db, "Example");
    db.connect()
        .unwrap()
        .execute(
            "UPDATE nodes SET display_name='Custom',manual_display_name=1 WHERE id=?1",
            [id],
        )
        .unwrap();
    scan(&db, &root, None, false);
    let delta = changes(&db, root.id);
    assert_eq!((delta.added, delta.updated, delta.unchanged), (0, 0, 1));
    fs::write(work.join("01.mkv"), b"longer changed source").unwrap();
    scan(&db, &root, None, false);
    assert_eq!(changes(&db, root.id).updated, 1);
    assert_eq!(db.get_node(id).unwrap().display_name, "Custom");
    fs::remove_file(work.join("01.mkv")).unwrap();
    fs::remove_dir(&work).unwrap();
    scan(&db, &root, None, false);
    assert_eq!(changes(&db, root.id).removed, Some(1));
}
#[test]
fn scan_deltas_scope_cancellation_and_offline_never_claim_unverified_removal() {
    let (_t, db, root) = setup(LibraryMediaKind::Animation);
    for name in ["A", "B"] {
        fs::create_dir(std::path::Path::new(&root.path).join(name)).unwrap();
        fs::write(
            std::path::Path::new(&root.path).join(name).join("01.mp4"),
            b"test",
        )
        .unwrap();
    }
    scan(&db, &root, None, false);
    fs::write(std::path::Path::new(&root.path).join("A/02.mp4"), b"test").unwrap();
    scan(
        &db,
        &root,
        Some(std::path::Path::new(&root.path).join("A")),
        false,
    );
    let delta = changes(&db, root.id);
    assert_eq!(
        (delta.updated, delta.unchanged, delta.removed),
        (1, 0, Some(0))
    );
    assert_eq!(delta.scope.len(), 1);
    db.connect()
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO library_scan_snapshots VALUES(?1,'retained-baseline')",
            [root.id],
        )
        .unwrap();
    scan(&db, &root, None, true);
    assert_eq!(changes(&db, root.id).removed, None);
    assert_eq!(
        db.connect()
            .unwrap()
            .query_row(
                "SELECT snapshot_json FROM library_scan_snapshots WHERE library_root_id=?1",
                [root.id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "retained-baseline"
    );
    assert!(db.get_node(node(&db, "B")).is_ok());
    let offline = std::path::Path::new(&root.path).with_file_name("offline");
    fs::rename(&root.path, &offline).unwrap();
    scan(&db, &root, None, false);
    assert_eq!(changes(&db, root.id).removed, None);
    assert_eq!(
        db.connect()
            .unwrap()
            .query_row(
                "SELECT snapshot_json FROM library_scan_snapshots WHERE library_root_id=?1",
                [root.id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "retained-baseline"
    );
    assert!(db.get_node(node(&db, "B")).is_ok());
}
#[test]
fn sql_filters_apply_before_limits_and_use_descendant_formats_and_binding_state() {
    let (_t, db, root) = setup(LibraryMediaKind::Animation);
    let c = db.connect().unwrap();
    c.execute("INSERT INTO nodes(library_root_id,absolute_path,folder_name,display_name) VALUES(?1,?2,'Root','Root')",params![root.id,root.path]).unwrap();
    let parent = c.last_insert_rowid();
    for i in 0..230 {
        c.execute("INSERT INTO nodes(library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type) VALUES(?1,?2,?3,?4,?4,'WORK')",params![root.id,parent,format!("{}/Match{i}",root.path),format!("Match{i}")]).unwrap();
    }
    let id = c.last_insert_rowid();
    c.execute("INSERT INTO media_files(node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(?1,?2,'special.mkv','.mkv',1,'stamp')",params![id,format!("{}/special.mkv",root.path)]).unwrap();
    let f = SearchFilters {
        extension: Some("MKV".into()),
        media_kind: Some("ANIMATION".into()),
        binding: Some("unbound".into()),
        ..Default::default()
    };
    let hits = db.search_filtered("Match", None, &f).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].node.id, id);
    assert!(db.search_filtered("", None, &f).unwrap().is_empty());
    assert!(db
        .search_filtered(
            "Match",
            None,
            &SearchFilters {
                binding: Some("bound".into()),
                ..f.clone()
            }
        )
        .unwrap()
        .is_empty());
    assert!(db
        .search_filtered(
            "Match",
            None,
            &SearchFilters {
                media_kind: Some("LIVE_ACTION".into()),
                ..f
            }
        )
        .unwrap()
        .is_empty());
    assert!(crate::search_filters::extensions(&db, None)
        .unwrap()
        .contains(&"MKV".into()));
}

#[test]
fn legacy_video_filters_respect_active_tmdb_and_report_invalid_binding_types() {
    let (_t, db, root) = setup(LibraryMediaKind::Video);
    let work = std::path::Path::new(&root.path).join("Legacy Movie");
    fs::create_dir(&work).unwrap();
    fs::write(work.join("movie.mkv"), b"fixture").unwrap();
    scan(&db, &root, None, false);
    let id = node(&db, "Legacy Movie");
    let c = db.connect().unwrap();
    let movie = serde_json::json!({"id":42,"title":"Legacy Movie","originalTitle":"Legacy Movie","releaseDate":"2020-01-01","overview":"","posterPath":null});
    c.execute(
        "INSERT INTO tmdb_movie_bindings(node_id,movie_id,payload_json) VALUES(?1,42,?2)",
        params![id, movie.to_string()],
    )
    .unwrap();
    let animation = SearchFilters {
        media_kind: Some("ANIMATION".into()),
        ..Default::default()
    };
    let live = SearchFilters {
        media_kind: Some("LIVE_ACTION".into()),
        ..Default::default()
    };
    assert!(db
        .search_filtered("Legacy", None, &animation)
        .unwrap()
        .is_empty());
    assert_eq!(db.search_filtered("Legacy", None, &live).unwrap().len(), 1);
    c.execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_title,provider_subject_type) VALUES(?1,'BANGUMI',77,'Legacy Movie',2)",[id]).unwrap();
    assert!(db
        .search_filtered("Legacy", None, &animation)
        .unwrap()
        .is_empty());
    c.execute(
        "UPDATE tmdb_movie_bindings SET active=0 WHERE node_id=?1",
        [id],
    )
    .unwrap();
    assert_eq!(
        db.search_filtered("Legacy", None, &animation)
            .unwrap()
            .len(),
        1
    );
    c.execute(
        "UPDATE metadata_bindings SET provider_subject_type=1 WHERE node_id=?1",
        [id],
    )
    .unwrap();
    assert!(crate::organize::list(&db)
        .unwrap()
        .iter()
        .any(|issue| issue.node.id == id && issue.kind == "binding" && issue.reason == "invalid"));
}

#[test]
fn search_collection_status_filters_before_limit_and_preserves_explicit_series_precedence() {
    let (_t, db, root) = setup(LibraryMediaKind::Animation);
    let c = db.connect().unwrap();
    c.execute("INSERT INTO nodes(library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(?1,?2,'Root','Root','CONTAINER')",params![root.id,root.path]).unwrap();
    let parent = c.last_insert_rowid();
    for i in 0..225 {
        c.execute("INSERT INTO nodes(library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,total_video_count) VALUES(?1,?2,?3,?4,?4,'WORK',2)",params![root.id,parent,format!("{}/Match{i}",root.path),format!("Match{i}")]).unwrap();
    }
    let selected = c.last_insert_rowid();
    c.execute(
        "INSERT INTO collection_states(kind,node_id,state) VALUES('NODE',?1,'WANT')",
        [selected],
    )
    .unwrap();
    c.execute("INSERT INTO media_files(node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(?1,?2,'Match S01E01.mkv','mkv',1,'stamp')",params![selected,format!("{}/Match.mkv",root.path)]).unwrap();
    let wanted = SearchFilters {
        collection_status: Some("WANT".into()),
        ..Default::default()
    };
    let hits = db.search_filtered("Match", Some(root.id), &wanted).unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|hit| hit.node.id == selected));
    assert!(db
        .search_filtered(
            "Match",
            None,
            &SearchFilters {
                collection_status: Some("COMPLETED".into()),
                ..Default::default()
            }
        )
        .unwrap()
        .is_empty());
    let unset = db
        .search_filtered(
            "Match",
            None,
            &SearchFilters {
                collection_status: Some("UNSET".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!unset.is_empty());
    assert!(unset.iter().all(|hit| hit.node.id != selected));
    c.execute(
        "UPDATE nodes SET display_name='Match series' WHERE id=?1",
        [parent],
    )
    .unwrap();
    assert!(db
        .search_filtered("Match series", None, &wanted)
        .unwrap()
        .is_empty()); // hidden Root
    assert!(db
        .search_filtered(
            "Match",
            None,
            &SearchFilters {
                collection_status: Some("INVALID".into()),
                ..Default::default()
            }
        )
        .is_err());
}

#[test]
fn search_book_status_uses_real_book_identity_and_explicit_parent_override() {
    let (_t, db, root) = setup(LibraryMediaKind::Ebook);
    let work = std::path::Path::new(&root.path).join("Books");
    fs::create_dir(&work).unwrap();
    fs::write(work.join("First.txt"), b"First readable book").unwrap();
    fs::write(work.join("Second.txt"), b"Second readable book").unwrap();
    scan(&db, &root, None, false);
    let c = db.connect().unwrap();
    let (book, owner): (i64, i64) = c
        .query_row(
            "SELECT id,node_id FROM comic_books WHERE display_name LIKE '%First%'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    c.execute(
        "INSERT INTO collection_states(kind,book_id,state) VALUES('BOOK',?1,'WANT')",
        [book],
    )
    .unwrap();
    let f = SearchFilters {
        collection_status: Some("WANT".into()),
        ..Default::default()
    };
    let hits = db.search_filtered("First", None, &f).unwrap();
    assert!(hits
        .iter()
        .any(|hit| hit.comic_book.as_ref().is_some_and(|b| b.id == book)));
    assert!(db.search_filtered("Second", None, &f).unwrap().is_empty());
    c.execute(
        "INSERT INTO collection_states(kind,node_id,state) VALUES('NODE',?1,'COMPLETED')",
        [owner],
    )
    .unwrap();
    assert!(db.search_filtered("First", None, &f).unwrap().is_empty());
    assert!(!db
        .search_filtered(
            "First",
            None,
            &SearchFilters {
                collection_status: Some("COMPLETED".into()),
                ..Default::default()
            }
        )
        .unwrap()
        .is_empty());
}
#[test]
fn added_nodes_search_and_issue_dismissals_are_persistent_and_do_not_infer_book_failures() {
    let (_t, db, root) = setup(LibraryMediaKind::Animation);
    fs::create_dir(std::path::Path::new(&root.path).join("Example")).unwrap();
    fs::write(
        std::path::Path::new(&root.path).join("Example/01.mkv"),
        b"video",
    )
    .unwrap();
    scan(&db, &root, None, false);
    let id = node(&db, "Example");
    let hits = db
        .search_filtered(
            "",
            Some(root.id),
            &SearchFilters {
                scan_root_id: Some(root.id),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
    let c = db.connect().unwrap();
    c.execute(
        "INSERT INTO match_issue_outcomes VALUES(?1,'BANGUMI','multiple-candidates','now')",
        [id],
    )
    .unwrap();
    let issues = crate::organize::list(&db).unwrap();
    assert!(issues.iter().any(|i| i.reason == "multiple-candidates"));
    crate::organize::remind(&db, id, "match", true).unwrap();
    assert!(crate::organize::list(&Database::new(db.path().into()))
        .unwrap()
        .iter()
        .any(|i| i.kind == "match" && i.ignored));
    crate::organize::remind(&db, id, "match", false).unwrap();
    assert!(crate::organize::list(&db)
        .unwrap()
        .iter()
        .all(|i| !i.ignored));
    db.connect()
        .unwrap()
        .execute(
            "UPDATE library_roots SET auto_bangumi=0 WHERE id=?1",
            [root.id],
        )
        .unwrap();
    assert!(crate::organize::list(&db)
        .unwrap()
        .iter()
        .all(|i| i.kind != "match"));
    let (_t, book_db, book_root) = setup(LibraryMediaKind::Ebook);
    fs::write(
        std::path::Path::new(&book_root.path).join("A.txt"),
        b"chapter text",
    )
    .unwrap();
    scan(&book_db, &book_root, None, false);
    assert!(crate::organize::list(&book_db).unwrap().is_empty());
}

pub(crate) fn vector_pdf(path: &std::path::Path) {
    use lopdf::dictionary;
    let mut doc = lopdf::Document::with_version("1.5");
    let pages = doc.new_object_id();
    let content = doc.add_object(lopdf::Stream::new(
        dictionary! {},
        b"0.7 0.2 0.2 rg 20 20 160 260 re f".to_vec(),
    ));
    let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),200.into(),300.into()],"Contents"=>content});
    doc.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![lopdf::Object::Reference(page)],"Count"=>1}
            .into(),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", catalog);
    doc.save(path).unwrap();
}

#[test]
fn pdf_embedded_jpeg_and_invalid_documents_never_enter_the_new_raster_branch() {
    use lopdf::dictionary;
    let (t, db, root) = setup(LibraryMediaKind::Ebook);
    for name in ["JPEG", "Broken", "Encrypted"] {
        fs::create_dir(std::path::Path::new(&root.path).join(name)).unwrap();
    }
    let path = std::path::Path::new(&root.path).join("JPEG/book.pdf");
    vector_pdf(&path);
    let mut doc = lopdf::Document::load(&path).unwrap();
    let page = *doc.get_pages().values().next().unwrap();
    let mut jpeg = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(24, 36)
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    let image=doc.add_object(lopdf::Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Image","Width"=>24,"Height"=>36,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8,"Filter"=>"DCTDecode"},jpeg.into_inner()));
    doc.get_object_mut(page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set(
            "Resources",
            dictionary! {"XObject"=>dictionary!{"Cover"=>image}},
        );
    doc.save(&path).unwrap();
    fs::write(
        std::path::Path::new(&root.path).join("Broken/book.pdf"),
        b"%PDF broken data",
    )
    .unwrap();
    let encrypted = std::path::Path::new(&root.path).join("Encrypted/book.pdf");
    vector_pdf(&encrypted);
    let mut doc = lopdf::Document::load(&encrypted).unwrap();
    let cipher=doc.add_object(dictionary!{"Filter"=>"Standard","V"=>2,"R"=>3,"Length"=>128,"O"=>lopdf::Object::string_literal(vec![0;32]),"U"=>lopdf::Object::string_literal(vec![0;32]),"P"=>-4});
    doc.trailer.set("Encrypt", cipher);
    doc.save(encrypted).unwrap();
    scan(&db, &root, None, false);
    let cache = t.path().join("cache/covers");
    crate::cache::ensure_directories(&cache).unwrap();
    let jpeg = node(&db, "JPEG");
    assert!(crate::poster_cache::cover_data_url(&db, &cache, jpeg, 384)
        .unwrap()
        .is_some());
    assert!(crate::pdf_cover::begin(&db, &cache, Some(jpeg), None, None)
        .unwrap()
        .is_none());
    for name in ["Broken", "Encrypted"] {
        let id = node(&db, name);
        let request = crate::pdf_cover::begin(&db, &cache, Some(id), None, None);
        assert!(request.is_err() || request.unwrap().is_none());
    }
}

#[test]
fn pdf_fallback_rejects_oversize_documents_before_raster_authorization() {
    let (t, db, root) = setup(LibraryMediaKind::Ebook);
    let directory = std::path::Path::new(&root.path).join("Oversize");
    fs::create_dir(&directory).unwrap();
    let path = directory.join("large.pdf");
    vector_pdf(&path);
    // Extend an owned synthetic fixture, never allocate/read a PDF-sized WebView payload.
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(crate::ebooks::MAX_PDF_BYTES + 1)
        .unwrap();
    db.connect()
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO library_scan_snapshots VALUES(?1,'retained-baseline')",
            [root.id],
        )
        .unwrap();
    scan(&db, &root, None, false);
    assert_eq!(
        db.connect()
            .unwrap()
            .query_row(
                "SELECT snapshot_json FROM library_scan_snapshots WHERE library_root_id=?1",
                [root.id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "retained-baseline"
    );
    let id = node(&db, "Oversize");
    let cache = t.path().join("cache/covers");
    crate::cache::ensure_directories(&cache).unwrap();
    let result = crate::pdf_cover::begin(&db, &cache, Some(id), None, None);
    assert!(result.is_err() || result.unwrap().is_none());
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        crate::ebooks::MAX_PDF_BYTES + 1
    );
    assert_eq!(fs::read_dir(cache.join("posters")).unwrap().count(), 0);
}

#[test]
fn search_book_kinds_real_formats_and_cached_cover_states_are_independent() {
    for kind in [
        LibraryMediaKind::Comic,
        LibraryMediaKind::Ebook,
        LibraryMediaKind::Doujin,
        LibraryMediaKind::Artbook,
    ] {
        let (t, db, root) = setup(kind);
        let work = std::path::Path::new(&root.path).join("Book");
        fs::create_dir(&work).unwrap();
        fs::write(work.join("Book.txt"), b"A readable book").unwrap();
        scan(&db, &root, None, false);
        let media = kind.as_db().to_owned();
        let filters = SearchFilters {
            media_kind: Some(media),
            extension: Some("TXT".into()),
            ..Default::default()
        };
        let hits = db.search_filtered("Book", None, &filters).unwrap();
        assert!(!hits.is_empty());
        let wrong = SearchFilters {
            media_kind: Some("ANIMATION".into()),
            ..filters.clone()
        };
        assert!(db.search_filtered("Book", None, &wrong).unwrap().is_empty());
        let id = node(&db, "Book");
        let image = t.path().join("cover.png");
        image::DynamicImage::new_rgb8(64, 96).save(&image).unwrap();
        db.connect()
            .unwrap()
            .execute(
                "UPDATE nodes SET cover_source='MANUAL',cover_cache_path=?2 WHERE id=?1",
                params![id, image.to_string_lossy()],
            )
            .unwrap();
        assert!(!db
            .search_filtered(
                "Book",
                None,
                &SearchFilters {
                    cover: Some("available".into()),
                    ..filters.clone()
                }
            )
            .unwrap()
            .is_empty());
        fs::remove_file(image).unwrap();
        assert!(db
            .search_filtered(
                "Book",
                None,
                &SearchFilters {
                    cover: Some("available".into()),
                    ..filters
                }
            )
            .unwrap()
            .is_empty());
    }
}
#[test]
fn pdf_fallback_uses_existing_tiers_revision_guards_and_manual_priority() {
    use base64::Engine;
    let (t, db, root) = setup(LibraryMediaKind::Ebook);
    let dir = std::path::Path::new(&root.path).join("Vector");
    fs::create_dir(&dir).unwrap();
    let path = dir.join("first.pdf");
    vector_pdf(&path);
    scan(&db, &root, None, false);
    let id = node(&db, "Vector");
    let book = crate::comics::books(&db.connect().unwrap(), id)
        .unwrap()
        .remove(0);
    let cache = t.path().join("cache/covers");
    crate::cache::ensure_directories(&cache).unwrap();
    assert!(crate::poster_cache::cover_data_url(&db, &cache, id, 384)
        .unwrap()
        .is_none());
    let ticket = crate::pdf_cover::begin(&db, &cache, Some(id), None, None)
        .unwrap()
        .unwrap();
    assert_eq!(ticket.book_id, book.id);
    let cancelled = crate::pdf_cover::begin(&db, &cache, Some(id), None, None)
        .unwrap()
        .unwrap()
        .ticket
        .unwrap();
    crate::pdf_cover::cancel(&cancelled);
    assert!(crate::pdf_cover::commit(&db, &cache, &cancelled, "AA==").is_err());
    let stale = crate::pdf_cover::begin(&db, &cache, Some(id), None, None)
        .unwrap()
        .unwrap()
        .ticket
        .unwrap();
    let bytes = {
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(768, 1152)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    };
    assert!(crate::pdf_cover::commit(
        &db,
        &cache,
        ticket.ticket.as_deref().unwrap(),
        &base64::engine::general_purpose::STANDARD.encode(&bytes)
    )
    .unwrap());
    for width in [256, 384, 512, 768] {
        assert!(crate::poster_cache::cover_data_url(&db, &cache, id, width)
            .unwrap()
            .is_some());
    }
    let reopened = Database::new(db.path().into());
    assert!(crate::poster_cache::book_cover_data_url(
        &reopened,
        &cache,
        book.id,
        &book.revision,
        512
    )
    .unwrap()
    .is_some());
    scan(&db, &root, None, false);
    assert!(crate::poster_cache::cover_data_url(&db, &cache, id, 384)
        .unwrap()
        .is_some());
    let ready = crate::pdf_cover::begin(&db, &cache, Some(id), None, None)
        .unwrap()
        .unwrap();
    assert!(ready.ready && ready.ticket.is_none());
    fs::write(&path, b"changed PDF").unwrap();
    assert!(crate::pdf_cover::commit(
        &db,
        &cache,
        &stale,
        &base64::engine::general_purpose::STANDARD.encode(&bytes)
    )
    .is_err());
    db.connect()
        .unwrap()
        .execute(
            "UPDATE nodes SET cover_source='MANUAL',cover_cache_path='retained.png' WHERE id=?1",
            [id],
        )
        .unwrap();
    assert!(crate::pdf_cover::begin(&db, &cache, Some(id), None, None)
        .unwrap()
        .is_none());
}
#[test]
fn next6_notes_are_typed_searchable_cas_bounded_and_survive_scan_backup() {
    use crate::{collection::Target, notes};
    let (temp, db, root) = setup(LibraryMediaKind::Animation);
    let dir = std::path::Path::new(&root.path).join("Film");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("one.mkv"), b"video").unwrap();
    scan(&db, &root, None, false);
    let id = node(&db, "Film");
    let original = notes::get(&db, Target::Node(id)).unwrap();
    assert_eq!(original.revision, 0);
    let saved = notes::save(
        &db,
        Target::Node(id),
        "私人\n東京 <script>literal</script> 100%".into(),
        0,
    )
    .unwrap();
    assert!(notes::save(&db, Target::Node(id), "stale".into(), 0).is_err());
    assert!(notes::save(&db, Target::Node(id), "x".repeat(10001), saved.revision).is_err());
    assert!(notes::save(&db, Target::Book(id + 9999), "wrong".into(), 0).is_err());
    let hits = db.search("東京", Some(root.id)).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].note_excerpt.as_ref().unwrap().contains("東京"));
    assert!(db.search("100%", None).unwrap().len() == 1);
    scan(&db, &root, None, false);
    assert_eq!(notes::get(&db, Target::Node(id)).unwrap().body, saved.body);
    let app = temp.path().join("app");
    fs::create_dir(&app).unwrap();
    let archive = temp.path().join("notes.m2backup");
    crate::collection_backup::export(&db, &app, &archive).unwrap();
    let preview = crate::collection_backup::prepare(&app, &archive).unwrap();
    crate::collection_backup::discard(&app, &preview.token).unwrap();
    notes::save(&db, Target::Node(id), String::new(), saved.revision).unwrap();
    assert!(db.search("東京", None).unwrap().is_empty());
}

fn next6_png(path: &std::path::Path, color: u8) {
    image::RgbImage::from_pixel(40, 60, image::Rgb([color, 90, 140]))
        .save(path)
        .unwrap();
}

#[test]
fn next6_book_notes_covers_backup_and_relocation_preserve_real_identity() {
    use crate::{
        collection::{self, Status, Target},
        comic_reader, cover_selection, library_relocation, notes,
    };
    let temp = crate::db::test_temp_dir();
    let app = temp.path().join("app");
    fs::create_dir(&app).unwrap();
    let source = temp.path().join("Old 漫画");
    fs::create_dir(&source).unwrap();
    let images = source.join("Images");
    fs::create_dir(&images).unwrap();
    next6_png(&images.join("001.png"), 20);
    next6_png(&images.join("002.png"), 40);
    fs::write(source.join("Text.txt"), "東京 中文\nPrivate test").unwrap();
    vector_pdf(&source.join("Vector.pdf"));
    let db = Database::new(app.join("morimediashelf.db"));
    db.migrate().unwrap();
    let root = db
        .add_root_with_kind(
            &source,
            None,
            LibraryMediaKind::Comic,
            LibraryRecognitionMode::Folder,
        )
        .unwrap();
    scan(&db, &root, None, false);
    let c = db.connect().unwrap();
    let books: Vec<(i64, String)> = c
        .prepare("SELECT id,source_path FROM comic_books ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    drop(c);
    assert_eq!(books.len(), 3);
    for (id, path) in &books {
        notes::save(&db, Target::Book(*id), format!("独立 {path}"), 0).unwrap();
        collection::set_status(&db, Target::Book(*id), Some(Status::Paused)).unwrap();
        let opened = comic_reader::open(&db, *id).unwrap();
        comic_reader::progress_at_revision(&db, *id, 0, Some(&opened.book.revision)).unwrap();
        comic_reader::bookmark_at_revision(&db, *id, 0, true, Some(&opened.book.revision)).unwrap();
    }
    let image_id = books.iter().find(|(_, p)| p.ends_with("Images")).unwrap().0;
    let pdf_id = books
        .iter()
        .find(|(_, p)| p.ends_with("Vector.pdf"))
        .unwrap()
        .0;
    let cache = app.join("cache/covers");
    crate::cache::ensure_directories(&cache).unwrap();
    let cover = temp.path().join("manual.png");
    next6_png(&cover, 200);
    let before = comic_reader::open(&db, pdf_id).unwrap();
    let token = cover_selection::begin(&db, Target::Book(pdf_id)).unwrap();
    assert!(fs::read_dir(cache.join("manual")).unwrap().next().is_none());
    cover_selection::discard(&token);
    let token = cover_selection::begin(&db, Target::Book(pdf_id)).unwrap();
    cover_selection::apply(&db, &cache, &token, Some(&cover), None, None, None, false).unwrap();
    let manual = cover_selection::book_manual(&db, pdf_id).unwrap().unwrap();
    let original = fs::read(&manual).unwrap();
    let invalid = temp.path().join("invalid.png");
    fs::write(&invalid, &original[..24]).unwrap();
    let token = cover_selection::begin(&db, Target::Book(pdf_id)).unwrap();
    assert!(
        cover_selection::apply(&db, &cache, &token, Some(&invalid), None, None, None, false)
            .is_err()
    );
    assert_eq!(fs::read(&manual).unwrap(), original);
    cover_selection::discard(&token);
    let token = cover_selection::begin(&db, Target::Book(image_id)).unwrap();
    cover_selection::apply(&db, &cache, &token, None, None, Some(1), None, false).unwrap();
    let opened = comic_reader::open(&db, image_id).unwrap();
    assert!(crate::poster_cache::book_cover_data_url(
        &db,
        &cache,
        image_id,
        &opened.book.revision,
        256
    )
    .unwrap()
    .is_some());
    assert_eq!(
        comic_reader::open(&db, pdf_id)
            .unwrap()
            .book
            .progress
            .unwrap()
            .last_read_at,
        before.book.progress.unwrap().last_read_at
    );
    let target = temp.path().join("New 漫画");
    fs::rename(&source, &target).unwrap();
    let plan = library_relocation::preview(&db, root.id, &target).unwrap();
    assert_eq!(plan.missing, 0);
    assert!(!plan.old_available);
    library_relocation::FAIL.with(|v| v.set(true));
    assert!(library_relocation::apply(&db, plan.token.as_ref().unwrap()).is_err());
    assert_eq!(db.library_paths().unwrap(), vec![source.clone()]);
    library_relocation::apply(&db, plan.token.as_ref().unwrap()).unwrap();
    assert_eq!(db.library_paths().unwrap(), vec![target.clone()]);
    for (id, _) in &books {
        let opened = comic_reader::open(&db, *id).unwrap();
        assert!(opened
            .book
            .source_path
            .starts_with(&target.to_string_lossy().to_string()));
        assert_eq!(opened.bookmarks, vec![0]);
        assert_eq!(opened.book.progress.unwrap().last_page_index, 0);
        assert!(notes::get(&db, Target::Book(*id))
            .unwrap()
            .body
            .starts_with("独立"));
    }
    assert!(!comic_reader::read_page(&db, image_id, 0)
        .unwrap()
        .is_empty());
    assert!(!comic_reader::read_pdf_range(
        &db,
        pdf_id,
        0,
        30,
        &comic_reader::open(&db, pdf_id).unwrap().book.revision
    )
    .unwrap()
    .is_empty());
    let states = collection::statuses(&db).unwrap();
    assert_eq!(states.entries.len(), 3);
    let archive = temp.path().join("roundtrip.m2backup");
    crate::collection_backup::export(&db, &app, &archive).unwrap();
    let preview = crate::collection_backup::prepare(&app, &archive).unwrap();
    assert_eq!(preview.manual_covers, 2);
    crate::collection_backup::arm(&app, &preview.token, &db).unwrap();
    assert_eq!(
        crate::collection_backup::startup(&app).unwrap(),
        Some("BACKUP_RESTORED")
    );
    assert_eq!(
        fs::read(cover_selection::book_manual(&db, pdf_id).unwrap().unwrap()).unwrap(),
        original
    );
    assert!(notes::get(&db, Target::Book(pdf_id))
        .unwrap()
        .body
        .starts_with("独立"));
    let fresh = db.list_roots().unwrap().remove(0);
    scan(&db, &fresh, None, false);
    assert!(notes::get(&db, Target::Book(pdf_id)).is_ok());
    assert_eq!(comic_reader::bookmarks(&db, image_id).unwrap(), vec![0]);
}

#[test]
fn next6_relocation_rejects_wrong_collection_overlap_stale_and_missing() {
    let (temp, db, root) = setup(LibraryMediaKind::Animation);
    let directory = std::path::Path::new(&root.path).join("Film");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("one.mkv"), b"one content").unwrap();
    scan(&db, &root, None, false);
    let id = node(&db, "Film");
    let dest = temp.path().join("Different");
    fs::create_dir_all(dest.join("Film")).unwrap();
    let file = dest.join("Film/one.mkv");
    fs::write(&file, b"bad content").unwrap();
    let stamp = fs::metadata(directory.join("one.mkv"))
        .unwrap()
        .modified()
        .unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(stamp))
        .unwrap();
    assert!(crate::library_relocation::preview(&db, root.id, &dest).is_err());
    fs::write(&file, b"one content").unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(stamp))
        .unwrap();
    let plan = crate::library_relocation::preview(&db, root.id, &dest).unwrap();
    assert_eq!(plan.verified_samples, 1);
    db.connect()
        .unwrap()
        .execute(
            "UPDATE media_files SET file_size=file_size+1 WHERE node_id=?1",
            [id],
        )
        .unwrap();
    assert!(crate::library_relocation::apply(&db, plan.token.as_ref().unwrap()).is_err());
    assert_eq!(
        db.library_paths().unwrap(),
        vec![std::path::PathBuf::from(&root.path)]
    );
    assert!(crate::library_relocation::preview(&db, root.id, &directory).is_err());
    assert!(crate::library_relocation::mapped(
        "D:/Collection-other/a",
        "D:/Collection",
        "E:/Collection"
    )
    .is_err());
    assert!(crate::library_paths::validate(&db, &file).is_err());
    assert!(crate::library_paths::validate(&db, &dest.join("absent")).is_err());
}

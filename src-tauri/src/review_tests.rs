//! Regression fixtures contain only synthetic names and bytes, never personal media.
use crate::{
    db::Database,
    models::{
        BangumiSubject, CoverSource, LibraryRecognitionMode, ScanPhase, ScanProgress, ScanStatus,
    },
    scanner::{ScanControl, ScanTarget},
};
use std::{
    fs,
    sync::{atomic::AtomicBool, Arc, Mutex},
};
use tempfile::TempDir;

fn fixture(files: &[&str], mode: LibraryRecognitionMode) -> (TempDir, Database, i64) {
    let temp = crate::db::test_temp_dir();
    let root_path = temp.path().join("library");
    fs::create_dir(&root_path).unwrap();
    for file in files {
        let path = root_path.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"fixture").unwrap();
    }
    let database = Database::new(temp.path().join("test.db"));
    database.migrate().unwrap();
    let root = database.add_root_with_mode(&root_path, None, mode).unwrap();
    let control = ScanControl {
        unchanged_directories: Default::default(),
        scan_id: "fixture".into(),
        cancel: Arc::new(AtomicBool::new(false)),
        progress: Arc::new(Mutex::new(ScanProgress {
            background: false,
            library_changed: None,
            scan_id: "fixture".into(),
            root_id: root.id,
            current_path: String::new(),
            folders_scanned: 0,
            videos_found: 0,
            comic_books_found: 0,
            status: ScanStatus::Running,
            errors: 0,
            message: None,
            phase: ScanPhase::Scanning,
            auto_match_current: 0,
            auto_match_total: 0,
            auto_match_matched: 0,
            auto_match_pending: 0,
            auto_match_unmatched: 0,
            auto_match_errors: 0,
        })),
    };
    crate::scanner::run_scan(
        None,
        &database,
        vec![ScanTarget {
            root: root.clone(),
            path: root_path,
            parent_node_id: None,
        }],
        &control,
        &crate::db::default_video_extensions(),
    );
    assert_eq!(control.progress().status, ScanStatus::Completed);
    (temp, database, root.id)
}

fn subject(id: i64) -> BangumiSubject {
    BangumiSubject {
        subject_id: id,
        subject_type: 2,
        title: "Example Show".into(),
        title_cn: None,
        title_en: None,
        title_ja: None,
        title_ko: None,
        match_aliases: vec![],
        date: None,
        image_url: None,
        summary: None,
    }
}

#[test]
fn numbered_volumes_are_one_work_with_flat_nested_videos_and_original_file_ids() {
    let (_temp, db, _) = fixture(
        &[
            "Example Show/CD1/01.mkv",
            "Example Show/CD2/02.mkv",
            "Example Show/SPs/PV/03.mkv",
            "Example Show/CDs/music.flac",
            "Example Show/Scans/image.png",
        ],
        LibraryRecognitionMode::Folder,
    );
    let works = db.list_all_resources().unwrap().works;
    assert_eq!(
        works.len(),
        1,
        "volumes and supplements must not become separate works"
    );
    assert_eq!(works[0].node.folder_name, "Example Show");
    assert_eq!(works[0].node.total_video_count, 3);
    let detail = crate::works::work_detail(&db, works[0].node.id).unwrap();
    assert!(detail.media_files.is_empty());
    assert_eq!(detail.nested_media_files.len(), 3);
    assert_eq!(detail.expanded_folder_ids.len(), 3);
    for entry in detail.nested_media_files {
        assert_ne!(entry.file.node_id, detail.node.id);
        assert!(entry.file.absolute_path.contains(&entry.relative_directory));
    }
    let candidates = db.list_bangumi_match_candidates(None, false).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, detail.node.id);
}

#[test]
fn disc_like_structure_with_different_titles_is_not_combined() {
    let (_temp, db, _) = fixture(
        &[
            "Example Show/Disc1/First Movie - 01.mkv",
            "Example Show/Disc2/Second Movie - 01.mkv",
        ],
        LibraryRecognitionMode::Folder,
    );
    let works = db.list_all_resources().unwrap().works;
    assert_eq!(works.len(), 2);
    assert!(works
        .iter()
        .all(|work| work.node.folder_name.starts_with("Disc")));
}

#[test]
fn independent_and_hidden_subtrees_are_not_flattened_into_a_manual_parent_work() {
    let (_temp, db, _) = fixture(
        &[
            "Example Show/01.mkv",
            "Example Show/SPs/PV/02.mkv",
            "Example Show/OVA/03.mkv",
            "Example Show/Hidden/04.mkv",
        ],
        LibraryRecognitionMode::Folder,
    );
    let connection = db.connect().unwrap();
    let find = |name: &str| {
        connection
            .query_row("SELECT id FROM nodes WHERE folder_name=?1", [name], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
    };
    let parent = find("Example Show");
    let ova = find("OVA");
    let hidden = find("Hidden");
    db.set_node_type(parent, crate::models::NodeType::Work)
        .unwrap();
    db.set_node_type(ova, crate::models::NodeType::Work)
        .unwrap();
    db.set_node_type(hidden, crate::models::NodeType::Ignored)
        .unwrap();
    let detail = crate::works::work_detail(&db, parent).unwrap();
    assert_eq!(detail.media_files.len(), 1);
    assert_eq!(detail.nested_media_files.len(), 1);
    assert_eq!(detail.nested_media_files[0].file.file_name, "02.mkv");
    assert!(detail.children.iter().any(|child| child.id == ova));
    assert!(!detail.children.iter().any(|child| child.id == hidden));
}

#[test]
fn stale_details_cannot_read_an_ignored_node_or_its_descendants() {
    let (_temp, db, _) = fixture(
        &["Example Show/01.mkv", "Example Show/SPs/PV/02.mkv"],
        LibraryRecognitionMode::Folder,
    );
    let connection = db.connect().unwrap();
    let id = |name| {
        connection
            .query_row("SELECT id FROM nodes WHERE folder_name=?1", [name], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
    };
    let parent = id("Example Show");
    let child = id("PV");
    assert!(crate::works::node_detail(&db, child).is_ok());
    db.set_node_type(parent, crate::models::NodeType::Ignored)
        .unwrap();
    for node_id in [parent, child] {
        assert!(crate::works::node_detail(&db, node_id)
            .unwrap_err()
            .contains("NODE_NOT_VISIBLE"));
        assert!(crate::db::ensure_node_visible_conn(&connection, node_id).is_err());
    }
    // Hidden-items management still reads the original Node and its retained metadata.
    assert_eq!(db.list_hidden_nodes().unwrap()[0].id, parent);
    assert_eq!(db.get_node(child).unwrap().id, child);
    db.reset_node_type(parent).unwrap();
    assert!(crate::works::node_detail(&db, child).is_ok());
}

#[test]
fn whole_work_binding_is_atomic_rejects_stale_targets_and_preserves_manual_covers() {
    let (temp, db, _) = fixture(
        &["Example Show - 01.mkv", "Example Show - 02.mkv"],
        LibraryRecognitionMode::VideoFile,
    );
    let group = db.list_all_resources().unwrap().works.remove(0);
    assert_eq!(group.sources.len(), 2);
    db.change_work_binding(&group.target, Some(&subject(42)))
        .unwrap();
    for source in &group.sources {
        assert_eq!(
            db.get_binding(source.id)
                .unwrap()
                .unwrap()
                .provider_subject_id,
            42
        );
    }
    assert!(db
        .change_work_binding(&group.target, None)
        .unwrap_err()
        .contains("STALE"));
    let manual = temp.path().join("manual.jpg");
    db.set_node_cover(group.sources[0].id, CoverSource::Manual, Some(&manual))
        .unwrap();
    let target = db
        .capture_work_sources(&group.target.source_node_ids)
        .unwrap();
    let automatic = temp.path().join("automatic.jpg");
    db.apply_work_cover(&target, &subject(42), Some(&automatic), None, false)
        .unwrap();
    assert_eq!(
        db.get_node(group.sources[0].id)
            .unwrap()
            .cover_cache_path
            .as_deref(),
        Some(manual.to_str().unwrap())
    );
    let group = db.list_all_resources().unwrap().works.remove(0);
    db.change_work_binding(&group.target, None).unwrap();
    for source in &group.sources {
        assert!(db.get_binding(source.id).unwrap().is_none());
    }
    assert_eq!(
        db.list_all_resources().unwrap().works.len(),
        1,
        "local episode evidence remains after clear"
    );
}

#[test]
fn scan_health_failure_is_durable_without_advancing_last_success() {
    let (_temp, db, root) = fixture(&["Example Show/01.mkv"], LibraryRecognitionMode::Folder);
    db.record_auto_scan_health(root, "SUCCESS", 0, None)
        .unwrap();
    let success = db
        .get_root(root)
        .unwrap()
        .scan_health
        .unwrap()
        .last_success_at;
    db.record_auto_scan_health(root, "PARTIAL", 1, Some("synthetic permission failure"))
        .unwrap();
    let health = db.list_roots().unwrap()[0].scan_health.clone().unwrap();
    assert_eq!(health.outcome, "PARTIAL");
    assert_eq!(health.last_success_at, success);
    assert_eq!(health.error_count, 1);
    assert!(!health.warnings_ignored);
    let ignored = db.set_scan_warnings_ignored(root, true).unwrap();
    assert!(ignored.scan_health.unwrap().warnings_ignored);
    db.record_auto_scan_health(root, "PARTIAL", 2, Some("another failure"))
        .unwrap();
    let health = db.get_root(root).unwrap().scan_health.unwrap();
    assert!(health.warnings_ignored);
    assert_eq!(health.error_count, 2);
    assert_eq!(health.outcome, "PARTIAL");
    assert_eq!(health.last_success_at, success);
    assert!(
        !db.set_scan_warnings_ignored(root, false)
            .unwrap()
            .scan_health
            .unwrap()
            .warnings_ignored
    );
    assert!(db.set_scan_warnings_ignored(i64::MAX, true).is_err());
}

#[test]
fn old_database_missing_startup_preference_defaults_off_and_explicit_values_survive() {
    let (_temp, db, _) = fixture(&["Example Show/01.mkv"], LibraryRecognitionMode::Folder);
    let connection = db.connect().unwrap();
    connection
        .execute("DELETE FROM settings WHERE key='auto_scan_on_startup'", [])
        .unwrap();
    db.migrate().unwrap();
    let value: String = connection
        .query_row(
            "SELECT value FROM settings WHERE key='auto_scan_on_startup'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(value, "false");
    connection
        .execute(
            "UPDATE settings SET value='true' WHERE key='auto_scan_on_startup'",
            [],
        )
        .unwrap();
    db.migrate().unwrap();
    let value: String = connection
        .query_row(
            "SELECT value FROM settings WHERE key='auto_scan_on_startup'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(value, "true");
}

#[test]
fn mixed_with_direct_videos_and_specials_without_a_main_work_stay_out_of_catalogue() {
    let (_temp, db, _) = fixture(
        &[
            "Collection/01.mkv",
            "Collection/Another Work/02.mkv",
            "Only Specials/SPs/03.mkv",
        ],
        LibraryRecognitionMode::Folder,
    );
    let works = db.list_all_resources().unwrap().works;
    assert!(!works
        .iter()
        .any(|work| ["Collection", "Only Specials"].contains(&work.node.folder_name.as_str())));
    assert!(!db
        .list_bangumi_match_candidates(None, false)
        .unwrap()
        .iter()
        .any(|node| node.folder_name == "Collection"));
}

#[test]
fn disc_seasons_and_conflicting_bindings_keep_independent_boundaries_after_reset() {
    let (_temp, db, _) = fixture(
        &[
            "Example Show/BD1/Example Show S01E01.mkv",
            "Example Show/BD2/Example Show S02E01.mkv",
        ],
        LibraryRecognitionMode::Folder,
    );
    assert_eq!(db.list_all_resources().unwrap().works.len(), 2);
    let (_temp, db, _) = fixture(
        &["Example Show/CD1/01.mkv", "Example Show/CD2/02.mkv"],
        LibraryRecognitionMode::Folder,
    );
    let connection = db.connect().unwrap();
    let child = connection
        .query_row("SELECT id FROM nodes WHERE folder_name='CD1'", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
    db.save_confirmed_binding(child, &subject(100)).unwrap();
    assert_eq!(db.list_all_resources().unwrap().works.len(), 2);
    assert_eq!(
        db.get_binding(child).unwrap().unwrap().provider_subject_id,
        100
    );
    let parent = connection
        .query_row(
            "SELECT id FROM nodes WHERE folder_name='Example Show'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    db.reset_node_type(parent).unwrap();
    assert_eq!(db.list_all_resources().unwrap().works.len(), 2);
    assert!(!db
        .list_all_resources()
        .unwrap()
        .recognition_warnings
        .is_empty());
}

#[test]
fn group_rebind_rejects_deleted_or_changed_sources_without_partial_writes() {
    let (_temp, db, _) = fixture(
        &["Example Show - 01.mkv", "Example Show - 02.mkv"],
        LibraryRecognitionMode::VideoFile,
    );
    let group = db.list_all_resources().unwrap().works.remove(0);
    db.save_confirmed_binding(group.sources[1].id, &subject(90))
        .unwrap();
    assert_eq!(
        db.change_work_binding(&group.target, Some(&subject(42)))
            .unwrap_err(),
        "WORK_TARGET_STALE"
    );
    assert!(db.get_binding(group.sources[0].id).unwrap().is_none());
    assert_eq!(
        db.get_binding(group.sources[1].id)
            .unwrap()
            .unwrap()
            .provider_subject_id,
        90
    );
    db.clear_binding(group.sources[1].id).unwrap();
    let group = db.list_all_resources().unwrap().works.remove(0);
    db.connect()
        .unwrap()
        .execute("DELETE FROM nodes WHERE id=?1", [group.sources[1].id])
        .unwrap();
    assert_eq!(
        db.change_work_binding(&group.target, Some(&subject(42)))
            .unwrap_err(),
        "WORK_TARGET_STALE"
    );
    assert!(db.get_binding(group.sources[0].id).unwrap().is_none());
}

#[test]
fn cover_failure_keeps_all_bindings_and_retry_preserves_already_valid_auto_covers() {
    let (temp, db, _) = fixture(
        &["Example Show - 01.mkv", "Example Show - 02.mkv"],
        LibraryRecognitionMode::VideoFile,
    );
    let group = db.list_all_resources().unwrap().works.remove(0);
    db.change_work_binding(&group.target, Some(&subject(42)))
        .unwrap();
    let target = db
        .capture_work_sources(&group.target.source_node_ids)
        .unwrap();
    db.apply_work_cover(
        &target,
        &subject(42),
        None,
        Some("synthetic download failure"),
        false,
    )
    .unwrap();
    for id in &target.source_node_ids {
        let binding = db.get_binding(*id).unwrap().unwrap();
        assert_eq!(binding.provider_subject_id, 42);
        assert!(binding.cover_download_error.is_some());
    }
    let first = temp.path().join("first.jpg");
    fs::write(&first, valid_cover()).unwrap();
    db.set_node_cover(
        target.source_node_ids[0],
        CoverSource::Bangumi,
        Some(&first),
    )
    .unwrap();
    db.set_binding_cover_error_if_subject(target.source_node_ids[0], 42, None)
        .unwrap();
    let target = db.capture_work_sources(&target.source_node_ids).unwrap();
    let second = temp.path().join("second.jpg");
    db.apply_work_cover(&target, &subject(42), Some(&second), None, true)
        .unwrap();
    assert_eq!(
        db.get_node(target.source_node_ids[0])
            .unwrap()
            .cover_cache_path
            .as_deref(),
        first.to_str()
    );
    assert_eq!(
        db.get_node(target.source_node_ids[1])
            .unwrap()
            .cover_cache_path
            .as_deref(),
        second.to_str()
    );
}

#[test]
fn bdmv_numbered_streams_do_not_conflict_with_disc_title_or_repeat_in_detail() {
    let (_temp, db, _) = fixture(
        &[
            "Example Show/Disc1/BDMV/index.bdmv",
            "Example Show/Disc1/BDMV/STREAM/00001.m2ts",
            "Example Show/Disc2/BDMV/index.bdmv",
            "Example Show/Disc2/BDMV/STREAM/00002.m2ts",
        ],
        LibraryRecognitionMode::Folder,
    );
    let works = db.list_all_resources().unwrap().works;
    assert_eq!(works.len(), 1);
    let detail = crate::works::work_detail(&db, works[0].node.id).unwrap();
    assert_eq!(detail.nested_media_files.len(), 2);
    let mut ids = detail
        .nested_media_files
        .iter()
        .map(|entry| entry.file.id)
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 2);
}

#[test]
fn deeply_nested_independent_work_keeps_a_visible_entry_and_owned_statistics_do_not_repeat() {
    let (_temp, db, _) = fixture(
        &[
            "Example Show/01.mkv",
            "Example Show/SPs/PV/02.mkv",
            "Example Show/SPs/Independent/03.mkv",
        ],
        LibraryRecognitionMode::Folder,
    );
    let connection = db.connect().unwrap();
    let id = connection
        .query_row(
            "SELECT id FROM nodes WHERE folder_name='Independent'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    db.set_node_type(id, crate::models::NodeType::Work).unwrap();
    let parent = connection
        .query_row(
            "SELECT id FROM nodes WHERE folder_name='Example Show'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    let detail = crate::works::work_detail(&db, parent).unwrap();
    assert_eq!(detail.node.total_video_count, 2);
    assert_eq!(detail.nested_media_files.len(), 1);
    assert!(detail.children.iter().any(|node| node.id == id));
    let works = db.list_all_resources().unwrap().works;
    assert_eq!(
        works
            .iter()
            .map(|work| work.node.total_video_count)
            .sum::<i64>(),
        3
    );
}

#[test]
fn ordinary_work_detail_uses_owned_count_and_activity_times() {
    let (_temp, db, _) = fixture(
        &["Example Show/01.mkv", "Example Show/Independent/02.mkv"],
        LibraryRecognitionMode::Folder,
    );
    let connection = db.connect().unwrap();
    let find = |name: &str| {
        connection
            .query_row("SELECT id FROM nodes WHERE folder_name=?1", [name], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
    };
    let parent = find("Example Show");
    let independent = find("Independent");
    db.set_node_type(parent, crate::models::NodeType::Work)
        .unwrap();
    db.set_node_type(independent, crate::models::NodeType::Work)
        .unwrap();
    for (id, date) in [(parent, "2026-01-01"), (independent, "2026-02-01")] {
        connection
            .execute(
                "UPDATE media_files SET modified_at=?1 WHERE node_id=?2",
                rusqlite::params![date, id],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO watch_history(node_id,last_watched_at,watch_count) VALUES(?1,?2,1)",
                rusqlite::params![id, date],
            )
            .unwrap();
    }
    let mut detail = crate::models::NodeDetail {
        comic_books: None,
        node: db.get_node(parent).unwrap(),
        children: db.list_children(parent).unwrap(),
        media_files: db.list_media(parent).unwrap(),
        resource_files: Vec::new(),
        breadcrumbs: Vec::new(),
        binding: None,
        work_sources: None,
        work_target: None,
        nested_media_files: Vec::new(),
        expanded_folder_ids: Vec::new(),
        recognition_warnings: Vec::new(),
    };
    let sources = vec![detail.node.clone()];
    crate::works::populate_owned_content(&db, &mut detail, &sources).unwrap();
    assert_eq!(detail.node.total_video_count, 1);
    assert!(detail.nested_media_files.is_empty());
    assert_eq!(
        detail.node.last_watched_at.as_deref(),
        Some("2026-01-01T00:00:00.000Z")
    );
    assert_eq!(
        detail.node.latest_file_modified_at.as_deref(),
        Some("2026-01-01T00:00:00.000Z")
    );
}

#[test]
fn video_file_mode_keeps_supplement_and_disc_named_files_as_individual_works() {
    let (_temp, db, _) = fixture(&["OVA.mkv", "CD1.mkv"], LibraryRecognitionMode::VideoFile);
    assert_eq!(db.list_all_resources().unwrap().works.len(), 2);
    assert_eq!(
        db.list_bangumi_match_candidates(None, false).unwrap().len(),
        2
    );
}

#[test]
fn cover_commit_keeps_original_sources_when_binding_itself_merges_a_disc_boundary() {
    let (temp, db, _) = fixture(
        &["Example Show/01.mkv", "Example Show/CD1/02.mkv"],
        LibraryRecognitionMode::Folder,
    );
    let connection = db.connect().unwrap();
    let parent = connection
        .query_row(
            "SELECT id FROM nodes WHERE folder_name='Example Show'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    let disc = connection
        .query_row("SELECT id FROM nodes WHERE folder_name='CD1'", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
    db.set_node_type(parent, crate::models::NodeType::Work)
        .unwrap();
    db.save_confirmed_binding(parent, &subject(42)).unwrap();
    db.save_confirmed_binding(disc, &subject(90)).unwrap();
    let group = db
        .list_all_resources()
        .unwrap()
        .works
        .into_iter()
        .find(|work| work.node.id == disc)
        .unwrap();
    db.change_work_binding(&group.target, Some(&subject(42)))
        .unwrap();
    let target = db.capture_work_sources(&[disc]).unwrap();
    db.apply_work_cover(
        &target,
        &subject(42),
        Some(&temp.path().join("fixture.jpg")),
        None,
        false,
    )
    .unwrap();
    assert_eq!(
        db.get_binding(disc).unwrap().unwrap().provider_subject_id,
        42
    );
    assert!(db.get_node(disc).unwrap().cover_cache_path.is_some());
    assert_eq!(db.list_all_resources().unwrap().works.len(), 1);
}

fn valid_cover() -> Vec<u8> {
    // Small synthetic PNG header accepted by the bounded cache transport validator.
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&13_u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes
}

#[test]
fn large_work_groups_validate_and_write_atomically_without_the_edit_batch_limit() {
    let names = (1..=1000)
        .map(|id| format!("Example Show S01E{id:04}.mkv"))
        .collect::<Vec<_>>();
    let files = names.iter().map(String::as_str).collect::<Vec<_>>();
    let (_temp, db, _) = fixture(&files, LibraryRecognitionMode::VideoFile);
    let group = db.list_all_resources().unwrap().works.remove(0);
    assert_eq!(group.sources.len(), 1000);
    let mut tampered = group.target.clone();
    tampered.source_node_ids.push(i64::MAX);
    assert!(db
        .change_work_binding(&tampered, Some(&subject(42)))
        .is_err());
    tampered = group.target.clone();
    tampered.source_node_ids[1] = tampered.source_node_ids[0];
    assert!(db
        .change_work_binding(&tampered, Some(&subject(42)))
        .is_err());
    let connection = db.connect().unwrap();
    let fail_id = group.target.source_node_ids[501];
    connection.execute_batch(&format!("CREATE TRIGGER fail_binding BEFORE INSERT ON metadata_bindings WHEN NEW.node_id={fail_id} BEGIN SELECT RAISE(ABORT,'injected'); END;")).unwrap();
    assert!(db
        .change_work_binding(&group.target, Some(&subject(42)))
        .is_err());
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM metadata_bindings", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    connection
        .execute_batch("DROP TRIGGER fail_binding;")
        .unwrap();
    let changed = db
        .change_work_binding(&group.target, Some(&subject(42)))
        .unwrap();
    assert_eq!(changed.cover_target.source_node_ids.len(), 1000);
    db.apply_work_cover(
        &changed.cover_target,
        &subject(42),
        None,
        Some("fixture"),
        false,
    )
    .unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM metadata_bindings WHERE provider_subject_id=42",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1000
    );
    let current = db.list_all_resources().unwrap().works.remove(0);
    db.change_work_binding(&current.target, None).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM metadata_bindings", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn work_snapshot_ignores_activity_but_detects_boundaries_and_cover_phase_races() {
    let (_temp, db, _) = fixture(
        &["Example Show/01.mkv", "Example Show/SPs/02.mkv"],
        LibraryRecognitionMode::Folder,
    );
    let initial = db.list_all_resources().unwrap().works.remove(0);
    db.set_node_type(initial.node.id, crate::models::NodeType::Work)
        .unwrap();
    let group = db.list_all_resources().unwrap().works.remove(0);
    db.record_node_watched(group.node.id).unwrap();
    db.connect()
        .unwrap()
        .execute(
            "UPDATE nodes SET updated_at='2099-01-01' WHERE id=?1",
            [group.node.id],
        )
        .unwrap();
    assert!(db.validate_work_target(&group.target).is_ok());
    let special: i64 = db
        .connect()
        .unwrap()
        .query_row("SELECT id FROM nodes WHERE folder_name='SPs'", [], |row| {
            row.get(0)
        })
        .unwrap();
    db.set_node_type(special, crate::models::NodeType::Work)
        .unwrap();
    assert_eq!(
        db.validate_work_target(&group.target).unwrap_err(),
        "WORK_TARGET_STALE"
    );
    let current = db
        .list_all_resources()
        .unwrap()
        .works
        .into_iter()
        .find(|work| work.node.id == group.node.id)
        .unwrap();
    let changed = db
        .change_work_binding(&current.target, Some(&subject(42)))
        .unwrap();
    db.set_node_cover(
        group.node.id,
        CoverSource::Manual,
        Some(std::path::Path::new("X:/Fixtures/manual.png")),
    )
    .unwrap();
    assert_eq!(
        db.apply_work_cover(
            &changed.cover_target,
            &subject(42),
            None,
            Some("fixture"),
            false
        )
        .unwrap_err(),
        "WORK_TARGET_STALE"
    );
    assert_eq!(
        db.get_binding(group.node.id)
            .unwrap()
            .unwrap()
            .provider_subject_id,
        42
    );
    assert_eq!(
        db.get_node(group.node.id).unwrap().cover_source,
        CoverSource::Manual
    );
}

#[test]
fn cover_retry_repairs_corruption_and_explicit_decode_failure_without_touching_other_sources() {
    let (temp, db, _) = fixture(
        &[
            "Example Show - 01.mkv",
            "Example Show - 02.mkv",
            "Example Show - 03.mkv",
        ],
        LibraryRecognitionMode::VideoFile,
    );
    let group = db.list_all_resources().unwrap().works.remove(0);
    db.change_work_binding(&group.target, Some(&subject(42)))
        .unwrap();
    let valid = temp.path().join("valid.png");
    let corrupt = temp.path().join("corrupt.png");
    let repaired = temp.path().join("repaired.png");
    fs::write(&valid, valid_cover()).unwrap();
    fs::write(&corrupt, b"bad image").unwrap();
    fs::write(&repaired, valid_cover()).unwrap();
    for (id, path, source) in [
        (group.sources[0].id, &valid, CoverSource::Bangumi),
        (group.sources[1].id, &corrupt, CoverSource::Bangumi),
        (group.sources[2].id, &corrupt, CoverSource::Manual),
    ] {
        db.set_node_cover(id, source, Some(path)).unwrap();
        db.set_binding_cover_error_if_subject(id, 42, None).unwrap();
    }
    let target = db.list_all_resources().unwrap().works.remove(0).target;
    db.apply_work_cover_with_failures(&target, &subject(42), Some(&repaired), None, true, &[])
        .unwrap();
    assert_eq!(
        db.get_node(group.sources[0].id)
            .unwrap()
            .cover_cache_path
            .as_deref(),
        valid.to_str()
    );
    assert_eq!(
        db.get_node(group.sources[1].id)
            .unwrap()
            .cover_cache_path
            .as_deref(),
        repaired.to_str()
    );
    assert_eq!(
        db.get_node(group.sources[2].id)
            .unwrap()
            .cover_cache_path
            .as_deref(),
        corrupt.to_str()
    );
    let target = db.list_all_resources().unwrap().works.remove(0).target;
    assert!(db
        .apply_work_cover_with_failures(
            &target,
            &subject(42),
            Some(&repaired),
            None,
            true,
            &[i64::MAX]
        )
        .is_err());
    db.apply_work_cover_with_failures(
        &target,
        &subject(42),
        Some(&repaired),
        None,
        true,
        &[group.sources[0].id],
    )
    .unwrap();
    assert_eq!(
        db.get_node(group.sources[0].id)
            .unwrap()
            .cover_cache_path
            .as_deref(),
        repaired.to_str()
    );
}

#[test]
fn single_classification_and_ancestor_refresh_roll_back_together() {
    let (_temp, db, _) = fixture(
        &["Example Show/CD1/01.mkv", "Example Show/CD2/02.mkv"],
        LibraryRecognitionMode::Folder,
    );
    let connection = db.connect().unwrap();
    let parent: i64 = connection
        .query_row(
            "SELECT id FROM nodes WHERE folder_name='Example Show'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let disc: i64 = connection
        .query_row("SELECT id FROM nodes WHERE folder_name='CD1'", [], |row| {
            row.get(0)
        })
        .unwrap();
    connection.execute_batch(&format!("CREATE TRIGGER fail_parent BEFORE UPDATE OF node_type ON nodes WHEN NEW.id={parent} BEGIN SELECT RAISE(ABORT,'injected'); END;")).unwrap();
    assert!(db
        .set_node_type(disc, crate::models::NodeType::Work)
        .is_err());
    assert!(!db.get_node(disc).unwrap().manual_type_override);
    connection
        .execute_batch("DROP TRIGGER fail_parent;")
        .unwrap();
    db.set_node_type(disc, crate::models::NodeType::Work)
        .unwrap();
    connection.execute_batch(&format!("CREATE TRIGGER fail_parent BEFORE UPDATE OF node_type ON nodes WHEN NEW.id={parent} BEGIN SELECT RAISE(ABORT,'injected'); END;")).unwrap();
    assert!(db.reset_node_type(disc).is_err());
    assert!(db.get_node(disc).unwrap().manual_type_override);
}

#[test]
fn deferred_read_snapshot_remains_consistent_while_another_connection_writes() {
    let (_temp, db, _) = fixture(&["Example Show/01.mkv"], LibraryRecognitionMode::Folder);
    let group = db.list_all_resources().unwrap().works.remove(0);
    db.read_snapshot(|connection| {
        let before = crate::db::get_node_conn(connection, group.node.id)?;
        db.connect()
            .unwrap()
            .execute(
                "UPDATE nodes SET display_name='Concurrent title' WHERE id=?1",
                [group.node.id],
            )
            .unwrap();
        let after = crate::db::get_node_conn(connection, group.node.id)?;
        assert_eq!(before.display_name, after.display_name);
        Ok(())
    })
    .unwrap();
    assert_eq!(
        db.get_node(group.node.id).unwrap().display_name,
        "Concurrent title"
    );
}

#[test]
fn deep_directory_failure_preserves_old_index_and_baseline_while_another_root_updates() {
    let (temp, db, root_id) = fixture(
        &["Example Show/01.mkv", "Example Show/SPs/PV/02.mkv"],
        LibraryRecognitionMode::Folder,
    );
    let healthy_path = temp.path().join("healthy");
    fs::create_dir_all(healthy_path.join("Other Show")).unwrap();
    fs::write(healthy_path.join("Other Show/01.mkv"), b"fixture").unwrap();
    let healthy = db.add_root(&healthy_path, None).unwrap();
    let make_control = || ScanControl {
        unchanged_directories: Default::default(),
        scan_id: "deep-failure".into(),
        cancel: Arc::new(AtomicBool::new(false)),
        progress: Arc::new(Mutex::new(ScanProgress {
            background: true,
            library_changed: None,
            scan_id: "deep-failure".into(),
            root_id,
            current_path: String::new(),
            folders_scanned: 0,
            videos_found: 0,
            comic_books_found: 0,
            status: ScanStatus::Running,
            errors: 0,
            message: None,
            phase: ScanPhase::Scanning,
            auto_match_current: 0,
            auto_match_total: 0,
            auto_match_matched: 0,
            auto_match_pending: 0,
            auto_match_unmatched: 0,
            auto_match_errors: 0,
        })),
    };
    let run = |control: &ScanControl| {
        let targets = db
            .list_roots()
            .unwrap()
            .into_iter()
            .map(|root| ScanTarget {
                path: std::path::PathBuf::from(&root.path),
                root,
                parent_node_id: None,
            })
            .collect();
        crate::scanner::run_scan(
            None,
            &db,
            targets,
            control,
            &crate::db::default_video_extensions(),
        );
    };
    run(&make_control());
    let connection = db.connect().unwrap();
    let baseline = |id| {
        connection
            .query_row(
                "SELECT snapshot_json FROM library_scan_snapshots WHERE library_root_id=?1",
                [id],
                |row| row.get::<_, String>(0),
            )
            .unwrap()
    };
    let old_failed_baseline = baseline(root_id);
    let old_healthy_baseline = baseline(healthy.id);
    let previous_success = db
        .get_root(root_id)
        .unwrap()
        .scan_health
        .unwrap()
        .last_success_at;
    let failed_root = db.get_root(root_id).unwrap();
    let deep = std::path::Path::new(&failed_root.path).join("Example Show/SPs/PV");
    fs::remove_file(deep.join("02.mkv")).unwrap();
    fs::write(healthy_path.join("Other Show/02.mkv"), b"new fixture").unwrap();
    let _fault = crate::incremental::fail_directory_read(deep);
    run(&make_control());
    assert_eq!(baseline(root_id), old_failed_baseline);
    assert_ne!(baseline(healthy.id), old_healthy_baseline);
    let failed_health = db.get_root(root_id).unwrap().scan_health.unwrap();
    assert_eq!(failed_health.outcome, "PARTIAL");
    assert_eq!(failed_health.last_success_at, previous_success);
    assert!(failed_health
        .detail
        .unwrap()
        .contains("Synthetic deep-directory read failure"));
    assert_eq!(
        db.get_root(healthy.id)
            .unwrap()
            .scan_health
            .unwrap()
            .outcome,
        "SUCCESS"
    );
    let retained: i64 = connection.query_row("SELECT COUNT(*) FROM media_files f JOIN nodes n ON n.id=f.node_id WHERE n.library_root_id=?1", [root_id], |row| row.get(0)).unwrap();
    assert_eq!(retained, 2);
}

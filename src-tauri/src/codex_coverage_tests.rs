//! Offline regression fixtures for indexed data boundaries. No personal data or network.
use crate::{
    db::Database,
    library_relocation,
    logical_works::LogicalWorkIndex,
    models::{
        LibraryMediaKind, LibraryRecognitionMode, LibraryRoot, MediaFile, NestedMediaFile,
        NodeDetail, NodeType,
    },
    scan_changes,
    scanner::ScanTarget,
    search_filters::{self, SearchFilters},
    video_presentation::{self, VideoSection},
    works,
};
use rusqlite::{params, Connection};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

// Production permits four outstanding relocation plans. Serialize only these new
// fixtures so the existing independent relocation tests retain their own slots.
static RELOCATION_TESTS: Mutex<()> = Mutex::new(());

mod round3_scan_and_probe {
    use super::*;
    use crate::{
        models::{ScanPhase, ScanProgress, ScanStatus},
        player,
        scanner::{self, ScanControl},
    };
    use std::{
        io::{Read, Write},
        process::{Command, Stdio},
        sync::{atomic::Ordering, mpsc, Arc},
        thread,
        time::{Duration, Instant},
    };

    fn control(root: &LibraryRoot, name: &str) -> ScanControl {
        ScanControl {
            unchanged_directories: Default::default(),
            scan_id: name.into(),
            cancel: Default::default(),
            progress: Arc::new(Mutex::new(ScanProgress {
                background: false,
                library_changed: None,
                scan_id: name.into(),
                root_id: root.id,
                current_path: root.path.clone(),
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
        }
    }

    fn scan(f: &Fixture, target: ScanTarget, name: &str) -> ScanProgress {
        f.db.start_scan_run(name, f.root.id).unwrap();
        let control = control(&f.root, name);
        scanner::run_scan(
            None,
            &f.db,
            vec![target],
            &control,
            &crate::db::default_video_extensions(),
        );
        control.progress()
    }

    fn deep_scan(kind: LibraryMediaKind) {
        let f = Fixture::new(kind);
        let mut deepest = PathBuf::from(&f.root.path);
        for _ in 0..48 {
            deepest.push("d");
        }
        let series = deepest.join("合成系列");
        for leaf in ["卷10", "卷2"] {
            let dir = series.join(leaf);
            fs::create_dir_all(&dir).unwrap();
            match kind {
                LibraryMediaKind::Video => fs::write(dir.join("episode01.mkv"), b"video").unwrap(),
                LibraryMediaKind::Comic => {
                    image::RgbImage::from_pixel(2, 3, image::Rgb([12, 34, 56]))
                        .save(dir.join("001.png"))
                        .unwrap();
                }
                LibraryMediaKind::Ebook => {
                    fs::write(dir.join("正文.txt"), "合成正文，仅用于离线扫描回归。\n").unwrap();
                }
                _ => unreachable!(),
            }
        }
        let original_files = media_snapshot(Path::new(&f.root.path));
        let first = scan(&f, f.target(None), "round3-deep-first");
        assert_eq!(first.status, ScanStatus::Completed);
        assert_eq!(first.errors, 0, "{:?}", first.message);
        assert_eq!(first.folders_scanned, 52);
        assert_eq!(first.videos_found + first.comic_books_found, 2);
        let c = f.db.connect().unwrap();
        let node = |path: &Path| -> i64 {
            c.query_row(
                "SELECT id FROM nodes WHERE absolute_path=?1",
                [path.to_string_lossy()],
                |r| r.get(0),
            )
            .unwrap()
        };
        let second = node(&series.join("卷2"));
        let tenth = node(&series.join("卷10"));
        assert!(second < tenth, "natural traversal must create 2 before 10");
        let series_id = node(&series);
        let total = || -> i64 {
            c.query_row(
                if kind.is_book() {
                    "SELECT total_comic_book_count FROM nodes WHERE id=?1"
                } else {
                    "SELECT total_video_count FROM nodes WHERE id=?1"
                },
                [series_id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(total(), 2, "parents must finalize after their descendants");
        c.execute("UPDATE nodes SET display_name='私有测试名称',manual_display_name=1,manual_type_override=1,node_type='WORK' WHERE id=?1", [second]).unwrap();
        f.bind(second, 123, if kind.is_book() { 1 } else { 2 });
        let preserved = curated_rows(&c);
        let rescanned = scan(&f, f.target(None), "round3-deep-rescan");
        assert_eq!(rescanned.errors, 0);
        assert_eq!(curated_rows(&c), preserved);
        assert_eq!(node(&series.join("卷2")), second);
        assert_eq!(node(&series.join("卷10")), tenth);
        let partial = scan(
            &f,
            ScanTarget {
                root: f.root.clone(),
                path: series.join("卷2"),
                parent_node_id: Some(series_id),
            },
            "round3-deep-partial",
        );
        assert_eq!(partial.errors, 0);
        assert_eq!(
            total(),
            2,
            "partial scan must refresh ancestors, retain siblings"
        );
        assert_eq!(curated_rows(&c), preserved);

        let cancelled = control(&f.root, "round3-deep-cancel");
        cancelled.cancel.store(true, Ordering::Relaxed);
        scanner::run_scan(
            None,
            &f.db,
            vec![f.target(None)],
            &cancelled,
            &crate::db::default_video_extensions(),
        );
        assert_eq!(cancelled.progress().status, ScanStatus::Cancelled);
        assert_eq!(curated_rows(&c), preserved);
        assert_eq!(media_snapshot(Path::new(&f.root.path)), original_files);
    }

    #[test]
    fn deep_video_scan_keeps_order_parent_counts_partial_refresh_and_metadata() {
        deep_scan(LibraryMediaKind::Video);
    }

    #[test]
    fn deep_comic_scan_keeps_order_parent_counts_partial_refresh_and_metadata() {
        deep_scan(LibraryMediaKind::Comic);
    }

    #[test]
    fn deep_ebook_scan_keeps_order_parent_counts_partial_refresh_and_metadata() {
        deep_scan(LibraryMediaKind::Ebook);
    }

    #[test]
    fn book_descendant_failure_preserves_missing_siblings_at_ancestor_cleanup() {
        for kind in [LibraryMediaKind::Comic, LibraryMediaKind::Ebook] {
            let f = Fixture::new(kind);
            let series = Path::new(&f.root.path).join("系列");
            let missing = series.join("卷2");
            let invalid = series.join("卷10");
            fs::create_dir_all(&missing).unwrap();
            fs::create_dir_all(&invalid).unwrap();
            fs::write(missing.join("正文.txt"), "原有正文").unwrap();
            fs::write(invalid.join("正文.txt"), "另一卷正文").unwrap();
            assert_eq!(scan(&f, f.target(None), "round3-before-failure").errors, 0);
            let c = f.db.connect().unwrap();
            let missing_id: i64 = c
                .query_row(
                    "SELECT id FROM nodes WHERE absolute_path=?1",
                    [missing.to_string_lossy()],
                    |r| r.get(0),
                )
                .unwrap();
            let book_id: i64 = c
                .query_row(
                    "SELECT id FROM comic_books WHERE node_id=?1",
                    [missing_id],
                    |r| r.get(0),
                )
                .unwrap();
            // Only synthetic fixture content is removed; scanning must not infer an entirely
            // observed parent when its later child fails to index this invalid archive.
            fs::remove_file(missing.join("正文.txt")).unwrap();
            fs::remove_dir(&missing).unwrap();
            fs::write(invalid.join("broken.cbz"), b"not an archive").unwrap();
            let failed = scan(&f, f.target(None), "round3-descendant-failure");
            assert!(failed.errors > 0);
            assert_eq!(
                c.query_row(
                    "SELECT id FROM comic_books WHERE node_id=?1",
                    [missing_id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                book_id
            );
            let health = f.db.list_roots().unwrap();
            assert_eq!(health[0].scan_health.as_ref().unwrap().outcome, "PARTIAL");
        }
    }

    struct HeldPipe {
        release: mpsc::Receiver<()>,
        prefix: bool,
    }
    impl Read for HeldPipe {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            if self.prefix {
                self.prefix = false;
                let message = b"Error parsing option synthetic-test\n";
                bytes[..message.len()].copy_from_slice(message);
                return Ok(message.len());
            }
            self.release.recv().unwrap();
            Ok(0)
        }
    }

    #[test]
    fn probe_output_limits_bytes_and_shares_one_deadline_for_held_pipes() {
        let capture = player::probe_output(std::io::Cursor::new(vec![b'x'; 200_000]));
        assert_eq!(
            player::collect_probe_output(&capture, Instant::now() + Duration::from_secs(1)).len(),
            8192
        );
        let (release_out, held_out) = mpsc::channel();
        let (release_err, held_err) = mpsc::channel();
        let stdout = player::probe_output(HeldPipe {
            release: held_out,
            prefix: true,
        });
        let stderr = player::probe_output(HeldPipe {
            release: held_err,
            prefix: true,
        });
        let start = Instant::now();
        let deadline = start + Duration::from_millis(50);
        assert_eq!(
            player::collect_probe_output(&stdout, deadline),
            b"Error parsing option synthetic-test\n"
        );
        assert_eq!(
            player::collect_probe_output(&stderr, deadline),
            b"Error parsing option synthetic-test\n"
        );
        assert!(start.elapsed() < Duration::from_millis(200));
        release_out.send(()).unwrap();
        release_err.send(()).unwrap();
        // Finish both simulated inherited-handle readers; the test leaves no blocked thread.
        assert!(matches!(
            stdout.recv_timeout(Duration::from_secs(1)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        assert!(matches!(
            stderr.recv_timeout(Duration::from_secs(1)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn probe_subprocess_fixture() {
        match std::env::var("M2SHELF_SYNTHETIC_PROBE_MODE").as_deref() {
            Ok("wait") => thread::sleep(Duration::from_secs(10)),
            Ok("output") => {
                let _ = std::io::stdout().write_all(&vec![b'x'; 200_000]);
                let mut bytes = b"Error parsing option synthetic-test\n".to_vec();
                bytes.extend(vec![b'y'; 200_000]);
                let _ = std::io::stderr().write_all(&bytes);
            }
            _ => {}
        }
    }

    #[test]
    fn probe_process_timeout_and_noisy_output_return_with_bounded_diagnostics() {
        for mode in ["wait", "output"] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "codex_coverage_tests::round3_scan_and_probe::probe_subprocess_fixture",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("M2SHELF_SYNTHETIC_PROBE_MODE", mode)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let started = Instant::now();
            let timeout = if mode == "wait" {
                Duration::from_millis(200)
            } else {
                Duration::from_secs(3)
            };
            let result = player::run_version_probe(&mut command, timeout).unwrap();
            assert!(started.elapsed() < timeout + Duration::from_secs(1));
            assert!(!result.ok);
            if mode == "wait" {
                assert!(result.message.contains("超时"));
            } else {
                assert!(result.message.starts_with("PLAYER_CONFIG_ERROR"));
                assert!(result.message.len() <= 2 * 8192 + 200);
            }
        }
    }
}

struct Fixture {
    db: Database,
    root: LibraryRoot,
    hidden: i64,
    temp: tempfile::TempDir,
}
impl Fixture {
    fn new(kind: LibraryMediaKind) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Root_百分比%_合成");
        fs::create_dir(&path).unwrap();
        let db = Database::new(temp.path().join("fixture.sqlite"));
        db.migrate().unwrap();
        let root = db
            .add_root_with_kind(&path, None, kind, LibraryRecognitionMode::Folder)
            .unwrap();
        let c = db.connect().unwrap();
        c.execute("INSERT INTO nodes(library_root_id,absolute_path,folder_name,display_name) VALUES(?1,?2,'Root','Root')", params![root.id, root.path]).unwrap();
        let hidden = c.last_insert_rowid();
        assert_eq!(
            c.query_row(
                "SELECT max(version) FROM mediashelf_schema_migrations",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            38
        );
        Self {
            db,
            root,
            hidden,
            temp,
        }
    }
    fn node(&self, parent: i64, relative: &str, kind: &str, count: i64) -> i64 {
        let path = Path::new(&self.root.path).join(relative);
        let name = path.file_name().unwrap().to_string_lossy();
        let c = self.db.connect().unwrap();
        c.execute("INSERT INTO nodes(library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,direct_video_count,total_video_count) VALUES(?1,?2,?3,?4,?4,?5,?6,?6)", params![self.root.id,parent,path.to_string_lossy(),name,kind,count]).unwrap();
        c.last_insert_rowid()
    }
    fn file(&self, node: i64, name: &str, extension: &str) -> i64 {
        let c = self.db.connect().unwrap();
        let path: String = c
            .query_row("SELECT absolute_path FROM nodes WHERE id=?1", [node], |r| {
                r.get(0)
            })
            .unwrap();
        c.execute("INSERT INTO media_files(node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(?1,?2,?3,?4,7,'2026-01-01T00:00:00Z')", params![node,Path::new(&path).join(name).to_string_lossy(),name,extension]).unwrap();
        c.last_insert_rowid()
    }
    fn bind(&self, node: i64, subject: i64, subject_type: i64) {
        self.db.connect().unwrap().execute("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(?1,'BANGUMI',?2,?3,'Fixture official title')", params![node,subject,subject_type]).unwrap();
    }
    fn target(&self, relative: Option<&str>) -> ScanTarget {
        ScanTarget {
            root: self.root.clone(),
            path: relative.map_or_else(
                || PathBuf::from(&self.root.path),
                |r| Path::new(&self.root.path).join(r),
            ),
            parent_node_id: relative.map(|_| self.hidden),
        }
    }
}

#[test]
#[cfg(windows)]
fn relocation_maps_windows_components_and_rejects_sibling_and_parent_escape() {
    assert_eq!(
        library_relocation::mapped(r"x:\ANIME\作品\Disc 1", r"X:\Anime", r"Y:\New").unwrap(),
        r"Y:\New\作品\Disc 1"
    );
    assert_eq!(
        library_relocation::mapped(r"\\?\X:\Anime\作品", r"X:\Anime", r"Y:\New").unwrap(),
        r"Y:\New\作品"
    );
    for path in [
        r"X:\Anime2\Work",
        r"X:\Anime-old\Work",
        r"X:\Anime\..\Secret",
        r"X:\Other\Work",
        r"Anime\Work",
    ] {
        assert_eq!(
            library_relocation::mapped(path, r"X:\Anime", r"Y:\New").unwrap_err(),
            "RELOCATE_INVALID",
            "{path}"
        );
    }
}

fn path_rows(c: &Connection) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for (table, field) in [
        ("nodes", "absolute_path"),
        ("media_files", "absolute_path"),
        ("resource_files", "absolute_path"),
        ("comic_books", "source_path"),
        ("comic_pages", "source_locator"),
        ("indexed_media_metadata", "source_key"),
    ] {
        let mut s = c
            .prepare(&format!("SELECT rowid,{field} FROM {table} ORDER BY rowid"))
            .unwrap();
        for row in s
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
        {
            let (id, path) = row.unwrap();
            result.insert(format!("{table}:{id}"), path);
        }
    }
    result
}
fn curated_rows(c: &Connection) -> BTreeMap<String, Vec<String>> {
    let mut result = BTreeMap::new();
    for (label, sql) in [
        ("names", "SELECT json_array(id,display_name,manual_display_name,manual_type_override,cover_source,cover_cache_path) FROM nodes ORDER BY id"),
        ("bangumi", "SELECT json_array(id,node_id,provider_subject_id,provider_subject_type,provider_title) FROM metadata_bindings ORDER BY id"),
        ("tmdb", "SELECT json_array(node_id,movie_id,payload_json,active) FROM tmdb_movie_bindings ORDER BY node_id"),
        ("progress", "SELECT json_array(comic_book_id,last_page_index,last_read_at) FROM comic_reading_progress ORDER BY comic_book_id"),
        ("bookmarks", "SELECT json_array(id,comic_book_id,page_index,created_at) FROM comic_bookmarks ORDER BY id"),
        ("notes", "SELECT json_array(id,node_id,book_id,body,revision) FROM private_notes ORDER BY id"),
        ("metadata", "SELECT json_array(rowid,node_id,book_id,resource_id,revision,payload_json) FROM indexed_media_metadata ORDER BY rowid"),
    ] {
        result.insert(label.into(), c.prepare(sql).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_,_>>().unwrap());
    }
    result
}
fn media_snapshot(root: &Path) -> BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = BTreeMap::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                files.insert(
                    entry.path().strip_prefix(root).unwrap().to_path_buf(),
                    (
                        fs::read(entry.path()).unwrap(),
                        entry.metadata().unwrap().modified().unwrap(),
                    ),
                );
            }
        }
    }
    files
}

#[test]
fn relocation_updates_every_path_atomically_preserving_identity_and_personal_data() {
    let _serial = RELOCATION_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let f = Fixture::new(LibraryMediaKind::Comic);
    let source = Path::new(&f.root.path);
    let new = f.temp.path().join("New 合成");
    let work = f.node(f.hidden, "Work", "WORK", 1);
    let c = f.db.connect().unwrap();
    for root in [source, new.as_path()] {
        fs::create_dir_all(root.join("Work/Images")).unwrap();
        fs::create_dir_all(root.join("Film")).unwrap();
        fs::write(root.join("Work/episode.mkv"), b"synthetic media").unwrap();
        fs::write(root.join("Work/notes.txt"), b"synthetic attachment").unwrap();
        image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]))
            .save(root.join("Work/Images/001.png"))
            .unwrap();
    }
    // Only fixture construction aligns timestamps; the relocation operations remain read-only.
    for rel in ["Work/episode.mkv", "Work/notes.txt", "Work/Images/001.png"] {
        let stamp = fs::metadata(source.join(rel)).unwrap().modified().unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(new.join(rel))
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(stamp))
            .unwrap();
    }
    for (table, rel, name, extension) in [
        ("media_files", "Work/episode.mkv", "episode.mkv", "mkv"),
        ("resource_files", "Work/notes.txt", "notes.txt", "txt"),
    ] {
        let meta = fs::metadata(source.join(rel)).unwrap();
        c.execute(&format!("INSERT INTO {table}(node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(?1,?2,?3,?4,?5,?6)"), params![work,source.join(rel).to_string_lossy(),name,extension,meta.len(),crate::comics::modified(&meta)]).unwrap();
    }
    let resource = c.last_insert_rowid();
    c.execute("INSERT INTO comic_books(node_id,source_path,source_kind,display_name,file_size,modified_at,page_count,revision) VALUES(?1,?2,'IMAGE_FOLDER','Synthetic book',0,'2026-01-01T00:00:00Z',1,'before-relocation')", params![work,source.join("Work/Images").to_string_lossy()]).unwrap();
    let book = c.last_insert_rowid();
    let page = source.join("Work/Images/001.png");
    let meta = fs::metadata(&page).unwrap();
    c.execute("INSERT INTO comic_pages(comic_book_id,page_index,page_name,source_locator,file_size,modified_at) VALUES(?1,0,'001.png',?2,?3,?4)",params![book,page.to_string_lossy(),meta.len(),crate::comics::modified(&meta)]).unwrap();
    c.execute("INSERT INTO indexed_media_metadata(source_key,node_id,book_id,resource_id,revision,payload_json) VALUES(?1,?2,?3,?4,'metadata-revision','{}')",params![source.join("Work/notes.txt").to_string_lossy(),work,book,resource]).unwrap();
    c.execute("UPDATE nodes SET display_name='My unchanged display name',manual_display_name=1,manual_type_override=1 WHERE id=?1",[work]).unwrap();
    f.bind(work, 42, 1);
    c.execute(
        "INSERT INTO comic_reading_progress(comic_book_id,last_page_index) VALUES(?1,0)",
        [book],
    )
    .unwrap();
    c.execute(
        "INSERT INTO comic_bookmarks(comic_book_id,page_index) VALUES(?1,0)",
        [book],
    )
    .unwrap();
    c.execute(
        "INSERT INTO private_notes(node_id,body,revision) VALUES(?1,'Node note',4)",
        [work],
    )
    .unwrap();
    c.execute(
        "INSERT INTO private_notes(book_id,body,revision) VALUES(?1,'Book note',5)",
        [book],
    )
    .unwrap();
    c.execute(
        "INSERT INTO library_scan_snapshots(library_root_id,snapshot_json) VALUES(?1,'{}')",
        [f.root.id],
    )
    .unwrap();
    let before_paths = path_rows(&c);
    let before_curated = curated_rows(&c);
    let before_source = media_snapshot(source);
    let before_target = media_snapshot(&new);
    let plan = library_relocation::preview(&f.db, f.root.id, &new).unwrap();
    assert_eq!(plan.missing, 0);
    assert_eq!(plan.checked, 3);
    assert_eq!(plan.verified_samples, 3);
    assert!(plan.old_available);
    let token = plan.token.unwrap();
    library_relocation::FAIL.with(|v| v.set(true));
    assert_eq!(
        library_relocation::apply(&f.db, &token).unwrap_err(),
        "RELOCATE_TEST_INTERRUPTED"
    );
    assert_eq!(path_rows(&c), before_paths);
    assert_eq!(curated_rows(&c), before_curated);
    assert_eq!(f.db.list_roots().unwrap()[0].path, f.root.path);
    library_relocation::apply(&f.db, &token).unwrap();
    let after_paths = path_rows(&c);
    for (key, old) in before_paths {
        assert_eq!(
            after_paths[&key],
            library_relocation::mapped(&old, &f.root.path, &new.to_string_lossy()).unwrap(),
            "{key}"
        );
    }
    assert_eq!(curated_rows(&c), before_curated);
    assert_ne!(
        c.query_row(
            "SELECT revision FROM comic_books WHERE id=?1",
            [book],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "before-relocation"
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM library_scan_snapshots WHERE library_root_id=?1",
            [f.root.id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(media_snapshot(source), before_source);
    assert_eq!(media_snapshot(&new), before_target);
    assert_eq!(
        library_relocation::apply(&f.db, &token).unwrap_err(),
        "RELOCATE_EXPIRED"
    );
}

#[test]
fn relocation_missing_samples_and_stale_plan_never_change_registered_paths() {
    let _serial = RELOCATION_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let f = Fixture::new(LibraryMediaKind::Video);
    let work = f.node(f.hidden, "Work", "WORK", 1);
    let target = f.temp.path().join("New");
    fs::create_dir(&target).unwrap();
    let missing = library_relocation::preview(&f.db, f.root.id, &target).unwrap();
    assert!(missing.token.is_none());
    assert_eq!(missing.missing, 1);
    assert!(missing.issues.iter().any(|v| v == "Work"));
    fs::create_dir(target.join("Work")).unwrap();
    let plan = library_relocation::preview(&f.db, f.root.id, &target).unwrap();
    let token = plan.token.unwrap();
    let other = Database::new(f.temp.path().join("different.sqlite"));
    other.migrate().unwrap();
    assert_eq!(
        library_relocation::apply(&other, &token).unwrap_err(),
        "RELOCATE_EXPIRED"
    );
    f.db.connect()
        .unwrap()
        .execute(
            "UPDATE nodes SET absolute_path=?1 WHERE id=?2",
            params![
                Path::new(&f.root.path).join("Changed").to_string_lossy(),
                work
            ],
        )
        .unwrap();
    assert_eq!(
        library_relocation::apply(&f.db, &token).unwrap_err(),
        "RELOCATE_EXPIRED"
    );
    assert_eq!(f.db.list_roots().unwrap()[0].path, f.root.path);
    library_relocation::discard(&token);
}

fn video(f: &Fixture, node: i64, name: &str, id: i64) -> MediaFile {
    MediaFile {
        id,
        node_id: node,
        absolute_path: Path::new(&f.root.path)
            .join(name)
            .to_string_lossy()
            .into_owned(),
        file_name: name.into(),
        extension: "mkv".into(),
        file_size: 7,
        modified_at: "2026-01-01T00:00:00Z".into(),
        duration_ms: None,
        width: None,
        height: None,
        codec: None,
        last_seen_at: "fixture".into(),
    }
}
#[test]
fn video_sections_classify_specials_season_zero_discs_and_nested_unknowns() {
    let f = Fixture::new(LibraryMediaKind::Animation);
    let id = f.node(f.hidden, "Example Show", "WORK", 1);
    let owner = f.db.get_node(id).unwrap();
    for (i, (name, relative, expected)) in [
        ("Example Show 01.mkv", "", VideoSection::Main),
        ("NCOP.mkv", "", VideoSection::Sps),
        ("NCED.mkv", "", VideoSection::Sps),
        ("CM.mkv", "", VideoSection::Sps),
        ("Example Show S00E01.mkv", "", VideoSection::Sps),
        ("unlabelled.mkv", "CD1", VideoSection::Main),
        ("unlabelled.mkv", "CD 01", VideoSection::Main),
        ("unlabelled.mkv", "Disc 2", VideoSection::Main),
        ("unlabelled.mkv", "CDs", VideoSection::Cds),
        ("unlabelled.mkv", "CD", VideoSection::Cds),
        ("NCOP.mkv", "CDs", VideoSection::Cds),
        ("unlabelled.mkv", "OST/Disc 1", VideoSection::Cds),
        ("unlabelled.mkv", "Soundtrack", VideoSection::Cds),
        ("unlabelled.mkv", "特典CD", VideoSection::Cds),
        ("Example Show OST 01.mkv", "", VideoSection::Cds),
        ("Example Show Drama CD.mkv", "", VideoSection::Cds),
        ("Example Show 特典CD.mkv", "", VideoSection::Cds),
        ("Example Show OST NCOP.mkv", "", VideoSection::Sps),
        ("Example Show OST S00E01.mkv", "", VideoSection::Sps),
        ("Example Show CD 01.mkv", "", VideoSection::Main),
        ("unlabelled.mkv", "Extras/NCOP", VideoSection::Sps),
        ("unlabelled.mkv", "Unknown/Nested", VideoSection::Other),
        (
            "Example Show S01E02.mkv",
            "Unknown/Nested",
            VideoSection::Main,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            video_presentation::classify(&video(&f, id, name, i as i64), &owner, relative),
            expected,
            "{name} {relative}"
        );
    }
}
#[test]
fn sparse_video_projection_preserves_defaults_and_protects_special_bonus_titles() {
    let f = Fixture::new(LibraryMediaKind::Animation);
    let id = f.node(f.hidden, "Special Bonus Adventure", "WORK", 1);
    let owner = f.db.get_node(id).unwrap();
    for title in [
        "Special Bonus Adventure 01.mkv",
        "Special Bonus Adventure S01E02.mkv",
    ] {
        assert_eq!(
            video_presentation::classify(&video(&f, id, title, 1), &owner, ""),
            VideoSection::Main
        );
    }
    let mut detail = NodeDetail {
        node: owner,
        binding: None,
        children: vec![],
        resource_files: vec![],
        breadcrumbs: vec![],
        work_sources: None,
        work_target: None,
        expanded_folder_ids: vec![],
        recognition_warnings: vec![],
        cds_folders: vec![],
        cds_files: vec![],
        sp_folders: Vec::new(),
        sp_files: Vec::new(),
        comic_books: None,
        video_sections: vec![],
        media_files: vec![
            video(&f, id, "Special Bonus Adventure 01.mkv", 1),
            video(&f, id, "NCOP.mkv", 2),
        ],
        nested_media_files: vec![
            NestedMediaFile {
                file: video(&f, id, "mystery.mkv", 3),
                source_node_id: id,
                source_name: "Source".into(),
                relative_directory: "Unlabelled".into(),
            },
            NestedMediaFile {
                file: video(&f, id, "02.mkv", 4),
                source_node_id: id,
                source_name: "Source".into(),
                relative_directory: "Disc 2".into(),
            },
        ],
    };
    video_presentation::populate(&mut detail);
    assert_eq!(
        detail
            .video_sections
            .iter()
            .map(|v| (v.file_id, v.section))
            .collect::<Vec<_>>(),
        vec![(2, VideoSection::Sps), (4, VideoSection::Main)]
    );
    assert_eq!(
        detail.media_files.len() + detail.nested_media_files.len(),
        4
    );
}

#[test]
fn music_words_in_work_titles_do_not_reclassify_ordinary_episodes() {
    let f = Fixture::new(LibraryMediaKind::Animation);
    for title in [
        "Soundtrack",
        "OST Adventure",
        "Drama CD Story",
        "特典CD物语",
    ] {
        let id = f.node(f.hidden, title, "WORK", 1);
        let owner = f.db.get_node(id).unwrap();
        let name = format!("{title} S01E01.mkv");
        assert_eq!(
            video_presentation::classify(&video(&f, id, &name, 1), &owner, ""),
            VideoSection::Main,
            "{title}"
        );
    }
}

#[test]
fn search_combines_scope_type_binding_status_extension_before_node_and_file_limits() {
    let f = Fixture::new(LibraryMediaKind::Animation);
    let c = f.db.connect().unwrap();
    for n in 0..205 {
        let id = f.node(f.hidden, &format!("Needle decoy {n:03}"), "WORK", 1);
        f.file(id, &format!("Needle decoy {n:03}.mp4"), "mp4");
    }
    let wanted = f.node(f.hidden, "Needle wanted", "WORK", 1);
    f.file(wanted, "Needle wanted.mkv", ".MKV");
    f.bind(wanted, 42, 2);
    c.execute(
        "INSERT INTO collection_states(kind,node_id,state) VALUES('NODE',?1,'COMPLETED')",
        [wanted],
    )
    .unwrap();
    let filters = SearchFilters {
        media_kind: Some("ANIMATION".into()),
        binding: Some("bound".into()),
        collection_status: Some("COMPLETED".into()),
        extension: Some("mkv".into()),
        ..Default::default()
    };
    let hits =
        f.db.search_filtered("Needle", Some(f.root.id), &filters)
            .unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| h.node.id == wanted));
    assert!(hits.iter().any(|h| h.media_file.is_some()));
    assert!(f
        .db
        .search_filtered("Needle", Some(f.root.id + 999), &filters)
        .unwrap()
        .is_empty());
    for rejected in [
        SearchFilters {
            media_kind: Some("LIVE_ACTION".into()),
            ..filters.clone()
        },
        SearchFilters {
            binding: Some("unbound".into()),
            ..filters.clone()
        },
        SearchFilters {
            collection_status: Some("PAUSED".into()),
            ..filters.clone()
        },
        SearchFilters {
            extension: Some("mp4".into()),
            ..filters.clone()
        },
    ] {
        assert!(f
            .db
            .search_filtered("Needle", None, &rejected)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn search_book_filter_runs_before_limit_and_respects_reading_status_and_scan_scope() {
    let f = Fixture::new(LibraryMediaKind::Comic);
    let c = f.db.connect().unwrap();
    let owner = f.node(f.hidden, "Books", "WORK", 0);
    c.execute(
        "UPDATE nodes SET total_comic_book_count=206,direct_comic_book_count=206 WHERE id=?1",
        [owner],
    )
    .unwrap();
    for n in 0..206 {
        c.execute("INSERT INTO comic_books(node_id,source_path,source_kind,display_name,modified_at,reader_format,page_count,revision) VALUES(?1,?2,'ZIP_ARCHIVE',?3,'2026-01-01T00:00:00Z',?4,1,'fixture')",params![owner,Path::new(&f.root.path).join(format!("book{n}.pdf")).to_string_lossy(),format!("Needle book {n}"),if n==205 {"PDF"} else {"EPUB"}]).unwrap();
    }
    let book = c.last_insert_rowid();
    c.execute(
        "INSERT INTO collection_states(kind,book_id,state) VALUES('BOOK',?1,'IN_PROGRESS')",
        [book],
    )
    .unwrap();
    let filters = SearchFilters {
        media_kind: Some("COMIC".into()),
        binding: Some("unbound".into()),
        collection_status: Some("IN_PROGRESS".into()),
        extension: Some("PDF".into()),
        ..Default::default()
    };
    let hits =
        f.db.search_filtered("Needle", Some(f.root.id), &filters)
            .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].comic_book.as_ref().unwrap().id, book);
    c.execute(
        "INSERT INTO library_scan_changes(root_id,summary_json) VALUES(?1,?2)",
        params![
            f.root.id,
            serde_json::to_string(&scan_changes::ScanChanges {
                added_node_ids: vec![owner],
                ..Default::default()
            })
            .unwrap()
        ],
    )
    .unwrap();
    let scoped = SearchFilters {
        scan_root_id: Some(f.root.id),
        ..filters
    };
    assert!(f
        .db
        .search_filtered("", None, &scoped)
        .unwrap()
        .iter()
        .any(|h| h.comic_book.as_ref().is_some_and(|b| b.id == book)));
    assert!(f
        .db
        .search_filtered("", None, &SearchFilters::default())
        .unwrap()
        .is_empty());
    c.execute("UPDATE nodes SET node_type='IGNORED' WHERE id=?1", [owner])
        .unwrap();
    assert!(f
        .db
        .search_filtered("Needle", None, &scoped)
        .unwrap()
        .is_empty());
}

#[test]
fn extension_options_drop_blank_values_and_filter_validation_rejects_malformed_input() {
    let f = Fixture::new(LibraryMediaKind::Video);
    let node = f.node(f.hidden, "Work", "WORK", 1);
    for (n, ext) in ["", ".", " ", "\t\r\n", "..mKv", "PDF"]
        .into_iter()
        .enumerate()
    {
        f.file(node, &format!("synthetic-{n}"), ext);
    }
    assert_eq!(
        search_filters::extensions(&f.db, Some(f.root.id)).unwrap(),
        vec!["MKV", "PDF"]
    );
    assert!(search_filters::extensions(&f.db, Some(f.root.id + 99))
        .unwrap()
        .is_empty());
    for extension in [" ", ".mkv", "mkv;DROP", "mkv\n", "abcdefghijklmnopq"] {
        assert_eq!(
            SearchFilters {
                extension: Some(extension.into()),
                ..Default::default()
            }
            .validate()
            .unwrap_err(),
            "SEARCH_FILTER_INVALID"
        );
    }
    for filters in [
        SearchFilters {
            media_kind: Some("VIDEO".into()),
            ..Default::default()
        },
        SearchFilters {
            binding: Some("yes".into()),
            ..Default::default()
        },
        SearchFilters {
            collection_status: Some("WATCHED".into()),
            ..Default::default()
        },
        SearchFilters {
            cover: Some("anything".into()),
            ..Default::default()
        },
    ] {
        assert!(filters.validate().is_err());
    }
}

#[test]
fn scan_delta_counts_physical_changes_without_counting_personal_edits() {
    let f = Fixture::new(LibraryMediaKind::Video);
    let same = f.node(f.hidden, "Same", "WORK", 1);
    f.file(same, "01.mkv", "mkv");
    let updated = f.node(f.hidden, "Updated", "WORK", 1);
    let file = f.file(updated, "01.mkv", "mkv");
    let removed = f.node(f.hidden, "Removed", "WORK", 1);
    f.file(removed, "01.mkv", "mkv");
    let targets = vec![f.target(None)];
    let before = scan_changes::capture(&f.db, &targets).unwrap();
    let added = f.node(f.hidden, "Added", "WORK", 1);
    f.file(added, "01.mkv", "mkv");
    let c = f.db.connect().unwrap();
    c.execute(
        "UPDATE media_files SET file_size=file_size+1 WHERE id=?1",
        [file],
    )
    .unwrap();
    c.execute("DELETE FROM nodes WHERE id=?1", [removed])
        .unwrap();
    c.execute(
        "UPDATE nodes SET display_name='Personal title',manual_display_name=1 WHERE id=?1",
        [same],
    )
    .unwrap();
    f.bind(same, 42, 2);
    scan_changes::finish(&f.db, &targets, &before, "complete", true, 0).unwrap();
    let changes = scan_changes::read(&c, f.root.id).unwrap().unwrap();
    assert_eq!(
        (
            changes.added,
            changes.updated,
            changes.unchanged,
            changes.removed
        ),
        (1, 1, 1, Some(1))
    );
    assert_eq!(changes.added_node_ids, vec![added]);
    assert!(changes.complete);
}

#[test]
fn incomplete_scan_deltas_never_confirm_removals_or_include_prefix_siblings() {
    let f = Fixture::new(LibraryMediaKind::Video);
    let branch = f.node(f.hidden, "Branch%_", "CONTAINER", 0);
    let child = f.node(branch, "Branch%_/Child", "WORK", 1);
    f.file(child, "01.mkv", "mkv");
    f.node(f.hidden, "Branch%_other", "WORK", 1);
    let targets = vec![f.target(Some("Branch%_"))];
    let before = scan_changes::capture(&f.db, &targets).unwrap();
    assert_eq!(before.len(), 2);
    let c = f.db.connect().unwrap();
    c.execute("DELETE FROM nodes WHERE id=?1", [child]).unwrap();
    for (label, errors) in [("partial", 0), ("cancelled", 0), ("failed", 1)] {
        scan_changes::finish(&f.db, &targets, &before, label, false, errors).unwrap();
        let changes = scan_changes::read(&c, f.root.id).unwrap().unwrap();
        assert_eq!(changes.removed, None);
        assert!(!changes.complete);
        assert_eq!(changes.errors, errors);
        assert_eq!(changes.unchanged, 1);
        assert_eq!(
            changes.scope,
            vec![targets[0].path.to_string_lossy().into_owned()]
        );
    }
    // A successfully observed partial scope may confirm only its own deletion, never siblings.
    scan_changes::finish(&f.db, &targets, &before, "scoped-success", true, 0).unwrap();
    assert_eq!(
        scan_changes::read(&c, f.root.id).unwrap().unwrap().removed,
        Some(1)
    );
}

#[test]
fn logical_multidisc_ownership_merges_cd_and_disc_names_without_mutating_identity() {
    for names in [["CD1", "CD2"], ["Disc 1", "Disc 2"]] {
        let f = Fixture::new(LibraryMediaKind::Animation);
        let parent = f.node(f.hidden, "Steins;Gate", "CONTAINER", 0);
        let mut children = vec![];
        for (i, name) in names.into_iter().enumerate() {
            let child = f.node(parent, &format!("Steins;Gate/{name}"), "AUTO_WORK", 1);
            // Number-only disc files contribute no conflicting title; the named
            // parent plus complete numbered disc directories supply the evidence.
            f.file(child, &format!("{:02}.mkv", i + 1), "mkv");
            children.push(child);
        }
        let c = f.db.connect().unwrap();
        c.execute(
            "UPDATE nodes SET total_video_count=2,child_media_branch_count=2 WHERE id=?1",
            [parent],
        )
        .unwrap();
        LogicalWorkIndex::reclassify(&c, Some(f.root.id)).unwrap();
        let graph = LogicalWorkIndex::load(&c).unwrap();
        assert!(graph.is_source(parent));
        assert_eq!(graph.nodes[&parent].node_type, NodeType::AutoWork);
        for child in children {
            assert_eq!(graph.owners.get(&child), Some(&parent));
            assert!(!graph.is_source(child));
        }
        assert_eq!(
            c.query_row("SELECT count(*) FROM nodes", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            4
        );
        let detail = works::node_detail(&f.db, parent).unwrap();
        assert_eq!(detail.nested_media_files.len(), 2);
    }
}

#[test]
fn logical_multidisc_rejects_season_manual_and_binding_boundaries_and_skips_ignored() {
    for boundary in ["season", "manual", "binding", "ignored"] {
        let f = Fixture::new(LibraryMediaKind::Animation);
        let parent = f.node(f.hidden, "Steins;Gate S01", "CONTAINER", 0);
        let a = f.node(parent, "Steins;Gate S01/CD1", "AUTO_WORK", 1);
        f.file(a, "Steins;Gate S01E01.mkv", "mkv");
        let b = f.node(parent, "Steins;Gate S01/CD2", "AUTO_WORK", 1);
        f.file(
            b,
            if boundary == "season" {
                "Steins;Gate S02E01.mkv"
            } else {
                "Steins;Gate S01E02.mkv"
            },
            "mkv",
        );
        let c = f.db.connect().unwrap();
        match boundary {
            "manual" => {
                c.execute(
                    "UPDATE nodes SET manual_type_override=1,node_type='WORK' WHERE id=?1",
                    [b],
                )
                .unwrap();
            }
            "binding" => {
                f.bind(parent, 42, 2);
                f.bind(b, 43, 2);
            }
            "ignored" => {
                c.execute(
                    "UPDATE nodes SET manual_type_override=1,node_type='IGNORED' WHERE id=?1",
                    [b],
                )
                .unwrap();
            }
            _ => {}
        }
        let graph = LogicalWorkIndex::load(&c).unwrap();
        if boundary == "ignored" {
            assert!(!graph.owners.contains_key(&b));
            assert!(!graph.owned_nodes(parent).contains(&b));
        } else {
            assert!(!graph.is_source(parent), "{boundary}");
            assert!(graph.is_source(a), "{boundary}");
            assert!(graph.is_source(b), "{boundary}");
        }
    }
}

#[test]
fn cds_supplements_join_main_discs_without_stealing_independent_boundaries() {
    for music_name in ["CD", "CDs", "OST", "Soundtrack", "特典CD"] {
        let f = Fixture::new(LibraryMediaKind::Animation);
        let parent = f.node(f.hidden, "Example Show", "CONTAINER", 0);
        let disc = f.node(parent, "Example Show/CD1", "AUTO_WORK", 1);
        let disc_file = f.file(disc, "01.mkv", "mkv");
        let music = f.node(
            parent,
            &format!("Example Show/{music_name}"),
            "AUTO_WORK",
            1,
        );
        let music_file = f.file(music, "concert.mkv", "mkv");
        let c = f.db.connect().unwrap();
        c.execute(
            "UPDATE nodes SET total_video_count=2,child_media_branch_count=2 WHERE id=?1",
            [parent],
        )
        .unwrap();
        LogicalWorkIndex::reclassify(&c, Some(f.root.id)).unwrap();
        let graph = LogicalWorkIndex::load(&c).unwrap();
        assert!(graph.is_source(parent), "{music_name}");
        assert_eq!(graph.owners.get(&disc), Some(&parent));
        assert_eq!(graph.owners.get(&music), Some(&parent));
        assert!(!graph.is_source(music));
        let detail = works::node_detail(&f.db, parent).unwrap();
        assert_eq!(detail.nested_media_files.len(), 2);
        assert!(detail
            .video_sections
            .iter()
            .any(|entry| entry.file_id == disc_file && entry.section == VideoSection::Main));
        assert!(detail
            .video_sections
            .iter()
            .any(|entry| entry.file_id == music_file && entry.section == VideoSection::Cds));
        // A later explicit independent classification retains the real child identity.
        c.execute(
            "UPDATE nodes SET manual_type_override=1,node_type='WORK' WHERE id=?1",
            [music],
        )
        .unwrap();
        let graph = LogicalWorkIndex::load(&c).unwrap();
        assert_eq!(graph.owners.get(&music), Some(&music));
        assert!(graph.is_source(music));
    }
}

#[test]
fn cds_detail_keeps_resource_and_video_identities_separate_without_disk_reads() {
    for with_video in [false, true] {
        let f = Fixture::new(LibraryMediaKind::Animation);
        let work = f.node(f.hidden, "Synthetic Work", "WORK", i64::from(with_video));
        let music = f.node(
            work,
            "Synthetic Work/CDs",
            "AUTO_WORK",
            i64::from(with_video),
        );
        let c = f.db.connect().unwrap();
        let mut resources = Vec::new();
        for (owner, relative, name, extension, kind) in [
            (
                work,
                "Synthetic Work/Theme.flac",
                "Theme.flac",
                "flac",
                "AUDIO",
            ),
            (
                music,
                "Synthetic Work/CDs/Booklet.pdf",
                "Booklet.pdf",
                "pdf",
                "DOCUMENT",
            ),
        ] {
            let path = Path::new(&f.root.path).join(relative);
            assert!(!path.exists(), "only the index is needed to open details");
            c.execute("INSERT INTO resource_files(node_id,absolute_path,file_name,extension,file_size,modified_at,resource_type) VALUES(?1,?2,?3,?4,10,'2026-01-01T00:00:00Z',?5)", params![owner,path.to_string_lossy(),name,extension,kind]).unwrap();
            resources.push(c.last_insert_rowid());
        }
        let videos = with_video.then(|| {
            let main = f.file(work, "Episode.mkv", "mkv");
            let concert = f.file(music, "Concert.mkv", "mkv");
            c.execute(
                "UPDATE nodes SET total_video_count=2,child_media_branch_count=1 WHERE id=?1",
                [work],
            )
            .unwrap();
            (main, concert)
        });
        let detail = works::node_detail(&f.db, work).unwrap();
        assert_eq!(detail.resource_files.len(), 1);
        assert_eq!(detail.resource_files[0].id, resources[0]);
        assert_eq!(detail.resource_files[0].node_id, work);
        assert_eq!(detail.cds_files.len(), 1);
        assert_eq!(detail.cds_files[0].id, resources[1]);
        assert!(detail.cds_folders.is_empty());
        assert_eq!(detail.children.len(), 1);
        assert_eq!(detail.children[0].id, music);
        // An unexpanded resource belongs to its original child and stays reachable
        // through that folder; opening the tab does not promote or copy the row.
        let music_detail = works::node_detail(&f.db, music).unwrap();
        assert_eq!(music_detail.resource_files.len(), 1);
        assert_eq!(music_detail.resource_files[0].id, resources[1]);
        assert_eq!(music_detail.resource_files[0].node_id, music);
        if let Some((main, concert)) = videos {
            assert_eq!(detail.media_files.len(), 1);
            assert_eq!(detail.media_files[0].id, main);
            assert_eq!(detail.nested_media_files.len(), 1);
            assert_eq!(detail.nested_media_files[0].file.id, concert);
            assert_eq!(detail.expanded_folder_ids, vec![music]);
            assert!(detail
                .video_sections
                .iter()
                .any(|entry| { entry.file_id == concert && entry.section == VideoSection::Cds }));
            assert_eq!(detail.node.total_video_count, 2);
        } else {
            assert!(detail.media_files.is_empty());
            assert!(detail.nested_media_files.is_empty());
            assert!(detail.video_sections.is_empty());
            assert_eq!(detail.node.total_video_count, 0);
        }
        assert!(media_snapshot(Path::new(&f.root.path)).is_empty());
    }
}

#[test]
fn mixed_and_container_works_expand_supplementary_sps_and_unwrap_cds_shell() {
    let f = Fixture::new(LibraryMediaKind::Animation);
    let mixed = f.node(f.hidden, "Mixed Show", "MIXED", 1);
    let main_ep = f.file(mixed, "01.mkv", "mkv");
    let season2 = f.node(mixed, "Mixed Show/Season 2", "AUTO_WORK", 1);
    let s2_ep = f.file(season2, "S02E01.mkv", "mkv");
    let sps = f.node(mixed, "Mixed Show/SPs [BD]", "AUTO_WORK", 1);
    let sp_ep = f.file(sps, "SP01.mkv", "mkv");
    let cds = f.node(mixed, "Mixed Show/CDs [FLAC]", "CONTAINER", 0);
    let ost_dir = f.node(cds, "Mixed Show/CDs [FLAC]/OST Album", "CONTAINER", 0);
    let c = f.db.connect().unwrap();
    c.execute(
        "UPDATE nodes SET total_video_count=3,child_media_branch_count=2 WHERE id=?1",
        [mixed],
    )
    .unwrap();
    c.execute(
        "INSERT INTO resource_files(node_id,absolute_path,file_name,extension,file_size,modified_at,resource_type) VALUES(?1,?2,'Booklet.pdf','pdf',10,'2026-01-01T00:00:00Z','DOCUMENT')",
        params![cds, Path::new(&f.root.path).join("Mixed Show/CDs [FLAC]/Booklet.pdf").to_string_lossy()],
    )
    .unwrap();
    drop(c);

    let detail = works::node_detail(&f.db, mixed).unwrap();
    assert_eq!(detail.media_files.len(), 1);
    assert_eq!(detail.media_files[0].id, main_ep);
    assert_eq!(detail.nested_media_files.len(), 1);
    assert_eq!(detail.nested_media_files[0].file.id, sp_ep);
    assert!(!detail
        .nested_media_files
        .iter()
        .any(|entry| entry.file.id == s2_ep));
    assert!(detail
        .video_sections
        .iter()
        .any(|entry| entry.file_id == sp_ep && entry.section == VideoSection::Sps));
    assert!(detail.expanded_folder_ids.contains(&sps));
    assert!(!detail.expanded_folder_ids.contains(&season2));
    assert!(!detail.cds_folders.iter().any(|folder| folder.id == cds));
    assert_eq!(detail.cds_folders.len(), 1);
    assert_eq!(detail.cds_folders[0].id, ost_dir);
    assert_eq!(detail.cds_files.len(), 1);
    assert_eq!(detail.cds_files[0].file_name, "Booklet.pdf");

    let container = f.node(f.hidden, "Franchise Container", "CONTAINER", 0);
    let c_s1 = f.node(container, "Franchise Container/Season 1", "AUTO_WORK", 1);
    let c_s1_ep = f.file(c_s1, "01.mkv", "mkv");
    let c_sps = f.node(
        container,
        "Franchise Container/映像特典 [BD]",
        "AUTO_WORK",
        1,
    );
    let c_sp_ep = f.file(c_sps, "NCOP.mkv", "mkv");
    let c_detail = works::node_detail(&f.db, container).unwrap();
    assert_eq!(c_detail.nested_media_files.len(), 1);
    assert_eq!(c_detail.nested_media_files[0].file.id, c_sp_ep);
    assert!(!c_detail
        .nested_media_files
        .iter()
        .any(|entry| entry.file.id == c_s1_ep));
    assert!(c_detail
        .video_sections
        .iter()
        .any(|entry| entry.file_id == c_sp_ep && entry.section == VideoSection::Sps));
    assert!(c_detail.expanded_folder_ids.contains(&c_sps));
}

#[test]
fn cross_library_grouping_and_source_snapshots_are_order_independent() {
    let f = Fixture::new(LibraryMediaKind::Animation);
    let a = f.node(f.hidden, "Shared A", "WORK", 2);
    f.bind(a, 42, 2);
    let other_path = f.temp.path().join("Another library");
    fs::create_dir(&other_path).unwrap();
    let other =
        f.db.add_root_with_kind(
            &other_path,
            None,
            LibraryMediaKind::Animation,
            LibraryRecognitionMode::Folder,
        )
        .unwrap();
    let c = f.db.connect().unwrap();
    c.execute("INSERT INTO nodes(library_root_id,absolute_path,folder_name,display_name,node_type,total_video_count,direct_video_count) VALUES(?1,?2,'Shared B','Shared B','WORK',3,3)",params![other.id,other_path.join("Shared B").to_string_lossy()]).unwrap();
    let b = c.last_insert_rowid();
    f.bind(b, 42, 2);
    let distinct = f.node(f.hidden, "Distinct", "WORK", 1);
    f.bind(distinct, 43, 2);
    let original = vec![
        f.db.get_node(a).unwrap(),
        f.db.get_node(b).unwrap(),
        f.db.get_node(distinct).unwrap(),
    ];
    let expected = serde_json::to_value(works::group_works(original.clone())).unwrap();
    for order in [[2, 1, 0], [1, 0, 2], [1, 2, 0]] {
        assert_eq!(
            serde_json::to_value(works::group_works(
                order.map(|i| original[i].clone()).to_vec()
            ))
            .unwrap(),
            expected
        );
    }
    let groups = works::group_works(original);
    assert_eq!(groups.len(), 2);
    let shared = groups.iter().find(|g| g.sources.len() == 2).unwrap();
    assert_eq!(shared.node.total_video_count, 5);
    assert_eq!(shared.target.source_node_ids, vec![a, b]);
    assert_eq!(
        shared
            .sources
            .iter()
            .map(|n| n.library_root_id)
            .collect::<Vec<_>>(),
        vec![f.root.id, other.id]
    );
}

#[test]
fn relocation_preserves_tmdb_identity_and_manual_metadata_in_video_root() {
    let _serial = RELOCATION_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let f = Fixture::new(LibraryMediaKind::LiveAction);
    let work = f.node(f.hidden, "Film", "WORK", 1);
    let source = Path::new(&f.root.path);
    let target = f.temp.path().join("New video library");
    for root in [source, target.as_path()] {
        fs::create_dir_all(root.join("Film")).unwrap();
        fs::write(root.join("Film/movie.mkv"), b"fixture").unwrap();
    }
    let metadata = fs::metadata(source.join("Film/movie.mkv")).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(target.join("Film/movie.mkv"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
        .unwrap();
    let file = f.file(work, "movie.mkv", "mkv");
    let c = f.db.connect().unwrap();
    c.execute(
        "UPDATE media_files SET modified_at=?1 WHERE id=?2",
        params![crate::comics::modified(&metadata), file],
    )
    .unwrap();
    c.execute(
        "UPDATE nodes SET display_name='Manual film title',manual_display_name=1 WHERE id=?1",
        [work],
    )
    .unwrap();
    let payload = serde_json::json!({"id":77,"title":"Film","originalTitle":"Film","overview":"Synthetic","releaseDate":"2026-01-01","posterPath":null}).to_string();
    c.execute(
        "INSERT INTO tmdb_movie_bindings(node_id,movie_id,payload_json) VALUES(?1,77,?2)",
        params![work, payload],
    )
    .unwrap();
    c.execute(
        "INSERT INTO private_notes(node_id,body,revision) VALUES(?1,'Keep note',3)",
        [work],
    )
    .unwrap();
    let before = curated_rows(&c);
    let old_media = media_snapshot(source);
    let new_media = media_snapshot(&target);
    let plan = library_relocation::preview(&f.db, f.root.id, &target).unwrap();
    library_relocation::apply(&f.db, &plan.token.unwrap()).unwrap();
    assert_eq!(curated_rows(&c), before);
    assert_eq!(
        f.db.get_node(work).unwrap().tmdb_binding.unwrap().movie.id,
        77
    );
    assert_eq!(
        c.query_row(
            "SELECT absolute_path FROM media_files WHERE id=?1",
            [file],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        target.join("Film").join("movie.mkv").to_string_lossy()
    );
    assert_eq!(media_snapshot(source), old_media);
    assert_eq!(media_snapshot(&target), new_media);
}

#[test]
fn relocation_preserves_opened_single_image_attachment_and_reading_state() {
    let _serial = RELOCATION_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    // IMAGE_FOLDER also represents a single-image attachment. Resource identity
    // distinguishes that file from a core image directory throughout relocation.
    let f = Fixture::new(LibraryMediaKind::Video);
    let node = f.node(f.hidden, "Film", "WORK", 1);
    let source = Path::new(&f.root.path).join("Film/cover.png");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    image::RgbImage::from_pixel(2, 2, image::Rgb([20, 30, 40]))
        .save(&source)
        .unwrap();
    let metadata = fs::metadata(&source).unwrap();
    let stamp = chrono::DateTime::<chrono::Utc>::from(metadata.modified().unwrap())
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let c = f.db.connect().unwrap();
    c.execute("INSERT INTO resource_files(node_id,absolute_path,file_name,extension,file_size,modified_at,resource_type) VALUES(?1,?2,'cover.png','png',?3,?4,'IMAGE')", params![node, source.to_string_lossy(), metadata.len(), stamp]).unwrap();
    let resource = c.last_insert_rowid();
    let book = crate::comic_reader::open_resource(&f.db, resource)
        .unwrap()
        .unwrap();
    crate::comic_reader::progress_at_revision(&f.db, book.id, 0, Some(&book.revision)).unwrap();
    crate::comic_reader::bookmark_at_revision(&f.db, book.id, 0, true, Some(&book.revision))
        .unwrap();
    let target = f.temp.path().join("New image attachment library");
    fs::create_dir_all(target.join("Film")).unwrap();
    fs::copy(&source, target.join("Film/cover.png")).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(target.join("Film/cover.png"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
        .unwrap();
    let before = curated_rows(&c);
    let old_media = media_snapshot(Path::new(&f.root.path));
    let new_media = media_snapshot(&target);
    let preview = library_relocation::preview(&f.db, f.root.id, &target).unwrap();
    assert_eq!(preview.missing, 0);
    assert!(preview.issues.is_empty());
    library_relocation::apply(&f.db, &preview.token.unwrap()).unwrap();
    assert_eq!(
        c.query_row(
            "SELECT source_resource_id FROM comic_books WHERE id=?1",
            [book.id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        resource
    );
    assert_eq!(curated_rows(&c), before);
    let destination = target
        .join("Film")
        .join("cover.png")
        .to_string_lossy()
        .into_owned();
    for (query, id) in [
        (
            "SELECT absolute_path FROM resource_files WHERE id=?1",
            resource,
        ),
        ("SELECT source_path FROM comic_books WHERE id=?1", book.id),
        (
            "SELECT source_locator FROM comic_pages WHERE comic_book_id=?1",
            book.id,
        ),
    ] {
        assert_eq!(
            c.query_row(query, [id], |r| r.get::<_, String>(0)).unwrap(),
            destination
        );
    }
    let reopened = crate::comic_reader::open_resource(&f.db, resource)
        .unwrap()
        .unwrap();
    assert_eq!(reopened.id, book.id);
    assert_eq!(reopened.progress.unwrap().last_page_index, 0);
    assert_eq!(crate::comic_reader::bookmarks(&f.db, book.id).unwrap(), [0]);
    assert_eq!(
        crate::comic_reader::read_page_at_revision(&f.db, book.id, 0, Some(&reopened.revision))
            .unwrap(),
        fs::read(&source).unwrap()
    );
    assert_eq!(media_snapshot(Path::new(&f.root.path)), old_media);
    assert_eq!(media_snapshot(&target), new_media);
}

mod reader_boundaries {
    use crate::{comic_reader, comics, db::Database, pdf_cover, tmdb_credentials};
    use rusqlite::params;
    use std::{fs, path::PathBuf};

    struct Fixture {
        temp: tempfile::TempDir,
        db: Database,
        root: PathBuf,
        page: PathBuf,
        bytes: Vec<u8>,
    }
    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("synthetic-media");
            let folder = root.join("Synthetic comic");
            fs::create_dir_all(&folder).unwrap();
            let mut bytes = Vec::new();
            let image = image::RgbImage::from_pixel(2, 3, image::Rgb([32, 64, 96]));
            image::codecs::jpeg::JpegEncoder::new(&mut bytes)
                .encode_image(&image)
                .unwrap();
            let page = folder.join("01.png");
            fs::write(&page, &bytes).unwrap();
            let stamp = comics::modified(&fs::metadata(&page).unwrap());
            let db = Database::new(temp.path().join("synthetic.sqlite"));
            db.migrate().unwrap();
            let c = db.connect().unwrap();
            c.execute("INSERT INTO library_roots(id,path,display_name,media_kind) VALUES(1,?1,'Synthetic','COMIC')", [root.to_string_lossy()]).unwrap();
            c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type) VALUES(1,1,NULL,?1,'Synthetic','Synthetic','CONTAINER'),(2,1,1,?2,'Synthetic comic','Synthetic comic','WORK')", params![root.to_string_lossy(),folder.to_string_lossy()]).unwrap();
            c.execute("INSERT INTO comic_books(id,node_id,source_path,source_kind,display_name,file_size,modified_at,page_count,revision) VALUES(1,2,?1,'IMAGE_FOLDER','Synthetic comic',?2,?3,1,'revision-1')", params![folder.to_string_lossy(),bytes.len() as i64,stamp]).unwrap();
            c.execute("INSERT INTO comic_pages(comic_book_id,page_index,page_name,source_locator,file_size,modified_at) VALUES(1,0,'01.png',?1,?2,?3)", params![page.to_string_lossy(),bytes.len() as i64,stamp]).unwrap();
            drop(c);
            Self {
                temp,
                db,
                root,
                page,
                bytes,
            }
        }
    }

    #[test]
    fn comic_detail_rejects_hidden_ancestors_and_non_book_roots() {
        let f = Fixture::new();
        assert_eq!(comic_reader::detail(&f.db, 2).unwrap().len(), 1);
        let c = f.db.connect().unwrap();
        c.execute("UPDATE nodes SET node_type='IGNORED' WHERE id=1", [])
            .unwrap();
        assert!(comic_reader::detail(&f.db, 2)
            .unwrap_err()
            .starts_with("NODE_NOT_VISIBLE"));
        let video = f.temp.path().join("synthetic-video");
        c.execute("INSERT INTO library_roots(id,path,display_name,media_kind) VALUES(2,?1,'Video','VIDEO')", [video.to_string_lossy()]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type) VALUES(3,2,NULL,?1,'Video','Video','WORK')", [video.to_string_lossy()]).unwrap();
        assert_eq!(
            comic_reader::detail(&f.db, 3).unwrap_err(),
            "COMIC_BOOK_NOT_FOUND"
        );
    }

    #[test]
    fn mislabeled_jpeg_page_uses_magic_and_leaves_source_bytes_unchanged() {
        let f = Fixture::new();
        let stamp = comics::modified(&fs::metadata(&f.page).unwrap());
        let bytes = comic_reader::read_page_at_revision(&f.db, 1, 0, Some("revision-1")).unwrap();
        assert_eq!(bytes, f.bytes);
        assert_eq!(comic_reader::image_mime(&bytes).unwrap(), "image/jpeg");
        assert_eq!(comic_reader::image_dimensions(&bytes).unwrap(), (2, 3));
        assert_eq!(fs::read(&f.page).unwrap(), f.bytes);
        assert_eq!(comics::modified(&fs::metadata(&f.page).unwrap()), stamp);
    }

    #[test]
    fn corrupt_and_unsupported_image_payloads_are_rejected() {
        for bytes in [
            b"<html>synthetic</html>".as_slice(),
            &[0xff, 0xd8, 0xff],
            b"\x89PNG\r\n\x1a\n",
            b"RIFF\0\0\0\0WEBPx",
            b"",
        ] {
            assert!(comic_reader::validate_image(bytes).is_err(), "{bytes:?}");
        }
    }

    #[test]
    fn stale_revisions_reject_page_progress_and_bookmark_writes() {
        let f = Fixture::new();
        assert_eq!(
            comic_reader::read_page_at_revision(&f.db, 1, 0, Some("stale")).unwrap_err(),
            "COMIC_PAGE_CHANGED"
        );
        assert_eq!(
            comic_reader::progress_with_position_at_revision(&f.db, 1, 0, Some("stale"), None)
                .unwrap_err(),
            "COMIC_PAGE_CHANGED"
        );
        assert_eq!(
            comic_reader::bookmark_at_revision(&f.db, 1, 0, true, Some("stale")).unwrap_err(),
            "COMIC_PAGE_CHANGED"
        );
        let c = f.db.connect().unwrap();
        for table in ["comic_reading_progress", "comic_bookmarks"] {
            let count: i64 = c
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 0, "{table}");
        }
    }

    #[test]
    fn invalid_page_indices_and_missing_index_rows_are_rejected() {
        let f = Fixture::new();
        for index in [-1, 1, i64::MAX] {
            assert_eq!(
                comic_reader::read_page_at_revision(&f.db, 1, index, Some("revision-1"))
                    .unwrap_err(),
                "COMIC_PAGE_CHANGED"
            );
        }
        f.db.connect()
            .unwrap()
            .execute("DELETE FROM comic_pages", [])
            .unwrap();
        assert_eq!(
            comic_reader::read_page_at_revision(&f.db, 1, 0, Some("revision-1")).unwrap_err(),
            "COMIC_PAGE_CHANGED"
        );
    }

    #[test]
    fn empty_or_index_failed_books_cannot_open_or_read() {
        let f = Fixture::new();
        let c = f.db.connect().unwrap();
        c.execute("UPDATE comic_books SET page_count=0", [])
            .unwrap();
        assert_eq!(comic_reader::open(&f.db, 1).unwrap_err(), "COMIC_NO_PAGES");
        c.execute(
            "UPDATE comic_books SET page_count=1,index_error='SYNTHETIC_DAMAGED'",
            [],
        )
        .unwrap();
        assert_eq!(
            comic_reader::open(&f.db, 1).unwrap_err(),
            "SYNTHETIC_DAMAGED"
        );
        assert_eq!(
            comic_reader::read_page(&f.db, 1, 0).unwrap_err(),
            "SYNTHETIC_DAMAGED"
        );
    }

    #[test]
    fn archive_locator_traversal_is_rejected_before_opening_archive() {
        let f = Fixture::new();
        let c = f.db.connect().unwrap();
        c.execute("UPDATE comic_books SET source_kind='ZIP_ARCHIVE'", [])
            .unwrap();
        for locator in [
            "../escape.png",
            "/absolute.png",
            "C:/escape.png",
            "..\\escape.png",
            "folder\\..\\escape.png",
            "folder/../escape.png",
        ] {
            c.execute("UPDATE comic_pages SET source_locator=?1", [locator])
                .unwrap();
            assert_eq!(
                comic_reader::read_page_at_revision(&f.db, 1, 0, Some("revision-1")).unwrap_err(),
                "COMIC_ARCHIVE_PATH",
                "{locator}"
            );
        }
    }

    #[test]
    fn page_budget_and_opened_handle_root_and_metadata_are_enforced() {
        let f = Fixture::new();
        let root = fs::canonicalize(&f.root).unwrap();
        let stamp = comics::modified(&fs::metadata(&f.page).unwrap());
        assert!(comic_reader::checked_file(&f.page, &root, f.bytes.len() as u64, &stamp).is_ok());
        assert_eq!(
            comic_reader::checked_file(&f.page, &root, f.bytes.len() as u64 + 1, &stamp)
                .unwrap_err(),
            "COMIC_PAGE_CHANGED"
        );
        let outside = f.temp.path().join("outside.png");
        fs::write(&outside, &f.bytes).unwrap();
        let outside_stamp = comics::modified(&fs::metadata(&outside).unwrap());
        assert_eq!(
            comic_reader::checked_file(&outside, &root, f.bytes.len() as u64, &outside_stamp)
                .unwrap_err(),
            "COMIC_PATH_OUTSIDE_ROOT"
        );
        f.db.connect()
            .unwrap()
            .execute(
                "UPDATE comic_pages SET file_size=?1",
                [comics::MAX_PAGE_BYTES + 1],
            )
            .unwrap();
        assert_eq!(
            comic_reader::read_page_at_revision(&f.db, 1, 0, Some("revision-1")).unwrap_err(),
            "COMIC_PAGE_LIMIT"
        );
    }

    #[test]
    fn pdf_cover_requires_exactly_one_target_and_book_revision() {
        let f = Fixture::new();
        let cache = f.temp.path().join("synthetic-cache");
        assert_eq!(
            pdf_cover::begin(&f.db, &cache, None, None, None)
                .err()
                .unwrap(),
            "PDF_COVER_TARGET_INVALID"
        );
        assert_eq!(
            pdf_cover::begin(&f.db, &cache, Some(2), Some(1), Some("revision-1"))
                .err()
                .unwrap(),
            "PDF_COVER_TARGET_INVALID"
        );
        assert_eq!(
            pdf_cover::begin(&f.db, &cache, None, Some(1), None)
                .err()
                .unwrap(),
            "PDF_COVER_REVISION_REQUIRED"
        );
        assert!(
            pdf_cover::begin(&f.db, &cache, None, Some(1), Some("revision-1"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn credential_invalid_source_and_status_payload_keep_secrets_out_of_ipc() {
        let f = Fixture::new();
        for source in ["", " ", "personal", "DEFAULT\0", "UNSUPPORTED"] {
            assert_eq!(
                tmdb_credentials::select(&f.db, source).unwrap_err(),
                "TMDB_SOURCE_INVALID"
            );
        }
        let count: i64 =
            f.db.connect()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM settings WHERE key='tmdb_credential_source'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(count, 0);
        let synthetic = tmdb_credentials::CredentialStatus {
            source: "PERSONAL",
            default_available: false,
            personal_configured: false,
            check: "UNCHECKED",
        };
        assert_eq!(
            serde_json::to_value(synthetic).unwrap(),
            serde_json::json!({ "source": "PERSONAL", "defaultAvailable": false, "personalConfigured": false, "check": "UNCHECKED" })
        );
    }

    #[cfg(windows)]
    #[test]
    fn credential_oversized_dialog_input_is_rejected_before_native_prompt() {
        assert_eq!(
            tmdb_credentials::configure(0, &"x".repeat(121), "").unwrap_err(),
            "TMDB_DIALOG_INVALID"
        );
        assert_eq!(
            tmdb_credentials::configure(0, "", &"x".repeat(1001)).unwrap_err(),
            "TMDB_DIALOG_INVALID"
        );
    }
}

#[test]
fn round7_sps_shells_project_resources_nested_videos_and_preserve_boundaries() {
    for source_kind in ["WORK", "MIXED", "CONTAINER"] {
        let f = Fixture::new(LibraryMediaKind::Animation);
        let work = f.node(f.hidden, "Synthetic Show", source_kind, 1);
        let main = f.file(work, "01.mkv", "mkv");
        let shell = f.node(work, "Synthetic Show/SPs [BD]", "CONTAINER", 0);
        let nested = f.node(shell, "Synthetic Show/SPs [BD]/Menu", "CONTAINER", 0);
        let deep = f.node(nested, "Synthetic Show/SPs [BD]/Menu/Deep", "AUTO_WORK", 1);
        let special = f.file(deep, "menu.MKV", "mkv");
        let empty = f.node(work, "Synthetic Show/NCED", "CONTAINER", 0);
        let music = f.node(work, "Synthetic Show/CDs", "CONTAINER", 0);
        let independent = f.node(shell, "Synthetic Show/SPs [BD]/Independent", "WORK", 1);
        let independent_file = f.file(independent, "01.mkv", "mkv");
        f.bind(independent, 421, 2);
        let ignored = f.node(shell, "Synthetic Show/SPs [BD]/Ignored", "IGNORED", 1);
        f.file(ignored, "ignored.mkv", "mkv");
        let manual = f.node(work, "Synthetic Show/Specials", "WORK", 1);
        let manual_file = f.file(manual, "special.mkv", "mkv");
        let c = f.db.connect().unwrap();
        c.execute(
            "UPDATE nodes SET manual_type_override=1 WHERE id=?1",
            [manual],
        )
        .unwrap();
        // Simulate migrated path casing. The parent graph remains authoritative.
        c.execute(
            "UPDATE nodes SET absolute_path=lower(absolute_path) WHERE id=?1",
            [shell],
        )
        .unwrap();
        let mut resources = Vec::new();
        for (owner, name, extension, kind) in [
            (shell, "Commentary.mka", "mka", "AUDIO"),
            (shell, "Special.ass", "ass", "SUBTITLE"),
            (music, "Theme.flac", "flac", "AUDIO"),
        ] {
            c.execute("INSERT INTO resource_files(node_id,absolute_path,file_name,extension,file_size,modified_at,resource_type) VALUES(?1,?2,?3,?4,7,'2026-01-01T00:00:00Z',?5)", params![owner, format!("synthetic-{owner}-{name}"), name, extension, kind]).unwrap();
            resources.push(c.last_insert_rowid());
        }
        let before = curated_rows(&c);
        let verify = |detail: &NodeDetail| {
            assert_eq!(
                detail.sp_files.iter().map(|r| r.id).collect::<Vec<_>>(),
                resources[..2]
            );
            assert_eq!(
                detail.cds_files.iter().map(|r| r.id).collect::<Vec<_>>(),
                resources[2..]
            );
            assert_eq!(
                detail.sp_folders.iter().map(|n| n.id).collect::<Vec<_>>(),
                vec![nested]
            );
            assert!(detail.sp_folders[0].latest_file_modified_at.is_some());
            assert!(detail.sp_folders[0].card_progress.is_some());
            for id in [shell, empty, music] {
                assert!(
                    detail.expanded_folder_ids.contains(&id),
                    "{source_kind}: {id}"
                );
            }
            for id in [manual, independent] {
                assert!(!detail.expanded_folder_ids.contains(&id));
                assert!(detail.children.iter().any(|n| n.id == id));
            }
            let video_ids = detail
                .media_files
                .iter()
                .map(|r| r.id)
                .chain(detail.nested_media_files.iter().map(|r| r.file.id))
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(
                video_ids,
                std::collections::HashSet::from([main, special]),
                "{source_kind}"
            );
            assert!(!video_ids.contains(&independent_file));
            assert!(!video_ids.contains(&manual_file));
            let sp = detail
                .nested_media_files
                .iter()
                .find(|r| r.file.id == special)
                .unwrap();
            assert_eq!(sp.relative_directory, "SPs [BD]/Menu/Deep");
            assert!(detail
                .video_sections
                .iter()
                .any(|r| r.file_id == special && r.section == VideoSection::Sps));
        };
        verify(&works::node_detail(&f.db, work).unwrap());
        verify(
            &works::presentation_detail(&f.db, work, false)
                .unwrap()
                .detail,
        );
        if source_kind == "WORK" {
            verify(&works::work_detail(&f.db, work).unwrap());
            verify(
                &works::presentation_detail(&f.db, work, true)
                    .unwrap()
                    .detail,
            );
        }
        assert_eq!(curated_rows(&c), before);
        assert!(media_snapshot(Path::new(&f.root.path)).is_empty());
    }
}

#[test]
fn round7_nonvideo_supplement_names_expand_without_inventing_videos() {
    let f = Fixture::new(LibraryMediaKind::Animation);
    let work = f.node(f.hidden, "Synthetic Work", "WORK", 0);
    let mut expected = Vec::new();
    for name in [
        "SPs",
        "Specials",
        "NCOP",
        "NCED",
        "Menu",
        "PV",
        "CM",
        "映像特典",
    ] {
        expected.push(f.node(work, &format!("Synthetic Work/{name}"), "CONTAINER", 0));
    }
    let detail = works::node_detail(&f.db, work).unwrap();
    assert_eq!(detail.expanded_folder_ids, expected);
    assert_eq!(detail.node.total_video_count, 0);
    assert!(detail.video_sections.is_empty());
    assert!(detail.sp_files.is_empty());
    assert!(detail.sp_folders.is_empty());
}

#[test]
fn round7_extension_changes_invalidate_only_video_baselines_transactionally() {
    let f = Fixture::new(LibraryMediaKind::Animation);
    let comic_path = f.temp.path().join("Books");
    fs::create_dir(&comic_path).unwrap();
    let comic =
        f.db.add_root_with_kind(
            &comic_path,
            None,
            LibraryMediaKind::Comic,
            LibraryRecognitionMode::Folder,
        )
        .unwrap();
    let c = f.db.connect().unwrap();
    for id in [f.root.id, comic.id] {
        c.execute(
            "INSERT INTO library_scan_snapshots(library_root_id,snapshot_json) VALUES(?1,'{}')",
            [id],
        )
        .unwrap();
    }
    let snapshots = || {
        c.query_row("SELECT count(*) FROM library_scan_snapshots", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap()
    };
    let cache = f.temp.path().join("cache");
    let mut settings = f.db.get_settings(&cache).unwrap();
    settings.video_extensions = settings
        .video_extensions
        .iter()
        .rev()
        .map(|v| format!(".{}", v.to_uppercase()))
        .collect();
    settings.video_extensions.push("mkv".into());
    settings.theme = "dark".into();
    let mut settings = f.db.update_settings(&settings, &cache).unwrap();
    assert_eq!(
        snapshots(),
        2,
        "equivalent normalized policy must retain baselines"
    );
    settings.video_extensions.retain(|v| v != "mkv");
    c.execute_batch("CREATE TRIGGER round7_deny_snapshot_delete BEFORE DELETE ON library_scan_snapshots BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(f.db.update_settings(&settings, &cache).is_err());
    assert_eq!(snapshots(), 2);
    assert!(f
        .db
        .get_settings(&cache)
        .unwrap()
        .video_extensions
        .iter()
        .any(|v| v == "mkv"));
    c.execute_batch("DROP TRIGGER round7_deny_snapshot_delete;")
        .unwrap();
    f.db.update_settings(&settings, &cache).unwrap();
    assert_eq!(snapshots(), 1);
    assert_eq!(
        c.query_row(
            "SELECT library_root_id FROM library_scan_snapshots",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        comic.id
    );
    c.execute(
        "INSERT INTO library_scan_snapshots(library_root_id,snapshot_json) VALUES(?1,'{}')",
        [f.root.id],
    )
    .unwrap();
    settings.video_extensions = crate::db::default_video_extensions();
    f.db.update_settings(&settings, &cache).unwrap();
    assert_eq!(
        snapshots(),
        1,
        "restore-default must invalidate the stale video baseline too"
    );
}

#[test]
fn round7_supplement_context_does_not_hide_orphan_or_explicit_sources() {
    let f = Fixture::new(LibraryMediaKind::Animation);
    let parent = f.node(f.hidden, "Parent Series", "MIXED", 1);
    let shell = f.node(parent, "Parent Series/SPs", "CONTAINER", 0);
    let nested = f.node(shell, "Parent Series/SPs/Behind the scenes", "AUTO_WORK", 1);
    f.file(nested, "01.mkv", "mkv");
    let manual = f.node(shell, "Parent Series/SPs/Independent", "WORK", 1);
    f.file(manual, "01.mkv", "mkv");
    let bound = f.node(shell, "Parent Series/SPs/Bound", "WORK", 1);
    f.file(bound, "01.mkv", "mkv");
    f.bind(bound, 1234, 2);
    let orphan_shell = f.node(f.hidden, "SPs", "CONTAINER", 0);
    let orphan = f.node(orphan_shell, "SPs/Named independent work", "AUTO_WORK", 1);
    f.file(orphan, "01.mkv", "mkv");
    let c = f.db.connect().unwrap();
    c.execute(
        "UPDATE nodes SET manual_type_override=1 WHERE id=?1",
        [manual],
    )
    .unwrap();
    let index = LogicalWorkIndex::load(&c).unwrap();
    assert!(!index.is_source(nested));
    for id in [manual, bound, orphan] {
        assert!(index.is_source(id), "explicit or orphan source {id}");
    }
}

#![cfg(feature = "fixtures")]
use m2shelf_smart_mixed_lab::model::*;
use m2shelf_smart_mixed_shadow::{
    adapter::{relative_index_path, ReadIndex, ReadOptions},
    fixture::Factory,
    model::*,
    run,
};
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::PathBuf, sync::atomic::Ordering};

fn path(label: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/smart-mixed-phase2/tests");
    std::fs::create_dir_all(&d).unwrap();
    d.join(format!(
        "{label}-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
fn factory(label: &str, kind: LibraryKind, mode: &str) -> (Factory, PathBuf) {
    let p = path(label);
    (Factory::create(&p, kind, mode).unwrap(), p)
}
fn input(case: u32) -> Snapshot {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../docs/smart-mixed/fixtures/{case:02}.json"));
    serde_json::from_value(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(p).unwrap()).unwrap()["input"]
            .clone(),
    )
    .unwrap()
}
fn scenario(case: u32) -> (Factory, PathBuf) {
    let i = input(case);
    let (mut f, p) = factory(&format!("case-{case}"), i.media_kind, "FOLDER");
    f.import(&i).unwrap();
    (f, p)
}
fn shadow(p: &std::path::Path) -> (IndexSnapshot, ShadowReport) {
    let mut r = ReadIndex::open(p).unwrap();
    let (s, o, _) = run(&mut r, 1, &ReadOptions::default()).unwrap();
    (s, o)
}
fn private_report(name: &str, s: &IndexSnapshot, o: &ShadowReport) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/smart-mixed-phase2");
    let file = std::fs::File::create(dir.join(format!("{name}.shadow.json"))).unwrap();
    serde_json::to_writer_pretty(std::io::BufWriter::new(file), &(s, o)).unwrap();
}
fn invariant(s: &IndexSnapshot, o: &ShadowReport) {
    let proposed: BTreeSet<_> = o.source_ref_to_book_id.values().copied().collect();
    let fallback: BTreeSet<_> = o.fallback_book_ids.iter().copied().collect();
    assert!(proposed.is_disjoint(&fallback));
    assert_eq!(
        proposed.union(&fallback).copied().collect::<BTreeSet<_>>(),
        s.source_id_map.keys().copied().collect()
    );
    assert_eq!(o.summary.missing_source_ids, 0);
    assert_eq!(o.summary.duplicate_source_ids, 0);
    assert!(!o.summary.mutation_policy.contains("WRITE"));
}
fn disk_digest(p: &std::path::Path) -> String {
    format!("{:x}", Sha256::digest(std::fs::read(p).unwrap()))
}
fn logical_digest(c: &rusqlite::Connection) -> String {
    let mut hash = Sha256::new();
    let tables = c
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for t in tables {
        hash.update(t.as_bytes());
        let mut st = c
            .prepare(&format!(
                "SELECT * FROM \"{}\" ORDER BY rowid",
                t.replace('"', "\"\"")
            ))
            .unwrap();
        let cols = st.column_count();
        let mut rows = st.query([]).unwrap();
        while let Some(r) = rows.next().unwrap() {
            for n in 0..cols {
                hash.update(format!("{:?};", r.get_ref(n).unwrap()).as_bytes());
            }
        }
    }
    format!("{:x}", hash.finalize())
}
#[test]
fn two_series_and_100_independent_pdfs_are_104_real_sources() {
    let (_f, p) = scenario(3);
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert_eq!(o.summary.indexed_sources, 104);
    assert_eq!(o.summary.series, 2);
    assert_eq!(o.summary.works, 100);
    assert_eq!(o.logical_proposal.root_items.len(), 102);
    assert_eq!(o.summary.proposed_reading_units, 104);
    assert!(o.logical_proposal.retained_prior.is_empty());
    assert_eq!(s.current_browse_node_ids.len(), 2);
    assert_eq!(s.current_browse_book_ids.len(), 100);
    private_report("root-parallel", &s, &o);
}
#[test]
fn mixed_series_preserves_png_order_and_all_seven_ids() {
    let (_f, p) = scenario(49);
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    private_report("mixed-series", &s, &o);
    assert_eq!(s.source_id_map.len(), 7);
    assert_eq!(o.summary.series, 1);
    assert_eq!(o.summary.proposed_reading_units, 6);
    assert_eq!(o.fallback_book_ids.len(), 1);
    let image = &s.source_id_map[&o.fallback_book_ids[0]];
    assert_eq!(image.source_kind, "IMAGE_FOLDER");
    assert_eq!(
        image.pages.iter().map(|p| p.page_index).collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(image.status, "INDEXED_IMAGE_BINARY_VALIDATION_UNKNOWN");
    assert!(image.pages[0].source_locator.ends_with(".png"));
    assert!(o
        .logical_proposal
        .groups
        .iter()
        .any(|g| g.decision == Decision::Review));
}
#[test]
fn author_is_two_independent_works_not_a_series() {
    let (_f, p) = scenario(10);
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    private_report("author-category", &s, &o);
    assert_eq!(o.summary.series, 0);
    assert_eq!(o.summary.works, 2);
    assert_eq!(o.logical_proposal.root_items.len(), 1);
}
#[test]
fn doujin_artist_ebook_author_and_artbook_raw_images_keep_sources() {
    for (case, kind) in [
        (11, LibraryKind::Doujin),
        (45, LibraryKind::Ebook),
        (12, LibraryKind::Artbook),
    ] {
        let (_f, p) = scenario(case);
        let (s, o) = shadow(&p);
        assert_eq!(s.media_type, kind);
        invariant(&s, &o);
        assert_eq!(o.summary.series, 0);
        assert!(s.resources.iter().all(|r| !r.core_duplicate));
    }
}
#[test]
fn direct_and_nested_image_collections_remain_separate_fallback_books() {
    let (_f, p) = scenario(28);
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert_eq!(o.fallback_book_ids.len(), 2);
    assert_eq!(o.summary.proposed_reading_units, 0);
    assert_ne!(
        s.source_id_map[&o.fallback_book_ids[0]].pages[0].source_locator,
        s.source_id_map[&o.fallback_book_ids[1]].pages[0].source_locator
    );
}
#[test]
fn core_resources_dedupe_but_readme_zip_and_unopened_mobi_remain() {
    let (mut f, p) = factory("resources", LibraryKind::Comic, "FOLDER");
    f.file_book("作品/第01卷.pdf").unwrap();
    let core = f.resource("作品/第01卷.pdf").unwrap();
    let readme = f.resource("作品/README.md").unwrap();
    let zip = f.resource("作品/archive.zip").unwrap();
    let mobi = f.resource("作品/unopened.mobi").unwrap();
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert!(
        s.resources
            .iter()
            .find(|r| r.id == core)
            .unwrap()
            .core_duplicate
    );
    let d = s
        .current_details
        .iter()
        .find(|d| d.anchor_node_id == f.nodes["作品"])
        .unwrap();
    assert!(!d.attachment_resource_ids.contains(&core));
    for id in [readme, zip, mobi] {
        assert!(d.attachment_resource_ids.contains(&id));
    }
    assert_eq!(d.unopened_readable_resource_ids, vec![mobi]);
    assert_eq!(o.summary.unopened_readable_resources, 1);
}
#[test]
fn opened_attachment_retains_resource_identity_and_never_becomes_work() {
    let (mut f, p) = factory("opened", LibraryKind::Ebook, "FOLDER");
    let b = f.file_book("Novel.mobi").unwrap();
    let r = f.resource("Novel.mobi").unwrap();
    f.connection.execute("UPDATE comic_books SET source_resource_id=?1,source_resource_stamp='indexed-stamp' WHERE id=?2",params![r,b]).unwrap();
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert_eq!(o.summary.works, 0);
    assert_eq!(o.fallback_book_ids, vec![b]);
    assert_eq!(s.source_id_map[&b].resource_id, Some(r));
    assert!(!s.resources[0].core_duplicate);
    assert_eq!(o.summary.unopened_readable_resources, 0);
}
#[test]
fn ignored_subtree_never_groups_but_indexed_rows_remain() {
    let (mut f, p) = factory("ignored", LibraryKind::Comic, "FOLDER");
    let a = f.file_book("Ignored/第01卷.pdf").unwrap();
    f.file_book("Ignored/第02卷.pdf").unwrap();
    f.manual(f.nodes["Ignored"], "IGNORED").unwrap();
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert_eq!(o.summary.series, 0);
    assert_eq!(o.fallback_book_ids.len(), 2);
    assert!(o.fallback_book_ids.contains(&a));
    assert!(!s.current_details[0].book_ids.contains(&a));
}
#[test]
fn manual_work_container_mixed_are_hard_fences() {
    for kind in ["WORK", "CONTAINER", "MIXED"] {
        let (mut f, p) = factory(kind, LibraryKind::Comic, "FOLDER");
        f.file_book("Manual/第01卷.pdf").unwrap();
        f.file_book("Manual/第02卷.pdf").unwrap();
        f.manual(f.nodes["Manual"], kind).unwrap();
        let (s, o) = shadow(&p);
        invariant(&s, &o);
        assert_eq!(o.summary.series, 0);
        assert_eq!(o.summary.works, 2);
        assert_eq!(s.nodes[&f.nodes["Manual"]].node_type, kind);
    }
}
#[test]
fn differently_bound_child_edition_stays_out_of_parent_ownership() {
    let (mut f, p) = factory("binding", LibraryKind::Comic, "FOLDER");
    f.file_book("Series/第01卷.pdf").unwrap();
    let b = f.file_book("Series/Edition/第02卷.pdf").unwrap();
    f.bind(f.nodes["Series"], 11).unwrap();
    f.bind(f.nodes["Series/Edition"], 22).unwrap();
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert_eq!(o.summary.series, 0);
    let d = s
        .current_details
        .iter()
        .find(|d| d.anchor_node_id == f.nodes["Series"])
        .unwrap();
    assert!(!d.book_ids.contains(&b));
    assert!(d.remaining_child_ids.contains(&f.nodes["Series/Edition"]));
}
#[test]
fn equal_bound_children_match_existing_detail_contract_but_shadow_is_conservative() {
    let (mut f, p) = factory("equal-binding", LibraryKind::Comic, "FOLDER");
    let a = f.file_book("Series/第01卷.pdf").unwrap();
    let b = f.file_book("Series/Edition/第02卷.pdf").unwrap();
    f.bind(f.nodes["Series"], 11).unwrap();
    f.bind(f.nodes["Series/Edition"], 11).unwrap();
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    let d = s
        .current_details
        .iter()
        .find(|d| d.anchor_node_id == f.nodes["Series"])
        .unwrap();
    assert_eq!(d.book_ids, vec![a, b]);
    assert_eq!(o.summary.series, 0);
}
#[test]
fn parallel_formats_and_collection_never_merge_real_ids() {
    for case in [20, 22, 42] {
        let (_f, p) = scenario(case);
        let (s, o) = shadow(&p);
        invariant(&s, &o);
        assert_eq!(o.summary.proposed_reading_units, s.source_id_map.len());
        if case != 22 {
            assert!(o.summary.editions > 0);
        }
    }
}
#[test]
fn every_persisted_metadata_table_and_database_byte_is_unchanged() {
    let (mut f, p) = factory("metadata", LibraryKind::Ebook, "FOLDER");
    let b = f.file_book("Novel.azw3").unwrap();
    f.seed_metadata(b).unwrap();
    let before_disk = disk_digest(&p);
    let before_logic = logical_digest(&f.connection);
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert_eq!(before_disk, disk_digest(&p));
    assert_eq!(before_logic, logical_digest(&f.connection));
    assert_eq!(
        s.source_id_map[&b].revision,
        "synthetic-revision:Novel.azw3"
    );
}
#[test]
fn both_existing_recognition_modes_remain_immutable() {
    for mode in ["FOLDER", "VIDEO_FILE"] {
        let (mut f, p) = factory(mode, LibraryKind::Comic, mode);
        f.file_book("Nested/第01卷.pdf").unwrap();
        let before = logical_digest(&f.connection);
        let (s, o) = shadow(&p);
        invariant(&s, &o);
        assert_eq!(s.recognition_mode, mode);
        assert_eq!(before, logical_digest(&f.connection));
    }
}
#[test]
fn all_three_video_kinds_are_rejected_before_book_queries() {
    for kind in [
        LibraryKind::Video,
        LibraryKind::Animation,
        LibraryKind::LiveAction,
    ] {
        let (f, p) = factory("video", kind, "FOLDER");
        f.connection
            .execute_batch("DROP TABLE comic_pages;")
            .unwrap();
        let before = disk_digest(&p);
        let err = ReadIndex::open(&p)
            .unwrap()
            .read(1, &ReadOptions::default())
            .unwrap_err();
        assert_eq!(err, "OUT_OF_SCOPE_VIDEO_LIBRARY");
        assert_eq!(before, disk_digest(&p));
    }
}
#[test]
fn gaps_bare_numbers_unicode_and_same_names_in_different_parents_are_safe() {
    for case in [17, 19, 21] {
        let (_f, p) = scenario(case);
        let (s, o) = shadow(&p);
        invariant(&s, &o);
        if case == 19 {
            assert!(o.summary.diagnostics.contains_key("VOLUME_GAP"));
        } else {
            assert_eq!(o.summary.series, 0);
        }
    }
    let (mut f, p) = factory("unicode", LibraryKind::Ebook, "FOLDER");
    for name in ["é.pdf", "e\u{301}.pdf", "İ.pdf", "i.pdf"] {
        f.file_book(name).unwrap();
    }
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert_eq!(o.summary.works, 4);
}
#[test]
fn inferred_ascii_case_collision_keeps_physical_fallback() {
    let (mut f, p) = factory("case-collision", LibraryKind::Comic, "VIDEO_FILE");
    f.file_book("Series/A.pdf").unwrap();
    f.file_book("series/B.pdf").unwrap();
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert_eq!(o.fallback_book_ids.len(), 2);
    assert!(o.summary.diagnostics.contains_key("PATH_CASE_COLLISION"));
}
#[test]
fn repeated_snapshot_and_serialization_are_deterministic() {
    let (_f, p) = scenario(3);
    let (s, a) = shadow(&p);
    let (t, b) = shadow(&p);
    assert_eq!(s.index_snapshot_version, t.index_snapshot_version);
    assert_eq!(
        serde_json::to_vec(&a).unwrap(),
        serde_json::to_vec(&b).unwrap()
    );
}
#[test]
fn reversed_sql_insertion_keeps_same_path_based_structure() {
    let mut i = input(3);
    let (mut f, p) = factory("forward", i.media_kind, "FOLDER");
    f.import(&i).unwrap();
    i.entries.reverse();
    let (mut g, q) = factory("reverse", i.media_kind, "FOLDER");
    g.import(&i).unwrap();
    let (_, a) = shadow(&p);
    let (_, b) = shadow(&q);
    fn projection(o: &ShadowReport) -> Vec<(String, Vec<String>)> {
        let paths = o
            .logical_proposal
            .reading_units
            .iter()
            .map(|u| (u.source_ref.as_str(), u.path.as_str()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut v = o
            .logical_proposal
            .groups
            .iter()
            .map(|g| {
                (
                    g.title.clone(),
                    g.members
                        .iter()
                        .map(|m| paths[m.source_ref.as_str()].to_owned())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        v.sort();
        v
    }
    assert_eq!(projection(&a), projection(&b));
}
#[test]
fn shadow_overrides_have_priority_without_mutating_persisted_types() {
    let (f, p) = scenario(4);
    let before = disk_digest(&p);
    let (_, normal) = shadow(&p);
    assert_eq!(normal.summary.series, 1);
    let mut opt = ReadOptions::default();
    opt.overrides.push(ManualOverride {
        path: "东京食尸鬼".into(),
        role: Some(DirectoryRole::Category),
        series_directory: None,
        volume: None,
        chapter: None,
        no_merge: true,
    });
    let mut reader = ReadIndex::open(&p).unwrap();
    let (s, changed, _) = run(&mut reader, 1, &opt).unwrap();
    assert_eq!(changed.summary.series, 0);
    assert_eq!(before, disk_digest(&p));
    assert_ne!(s.override_revision, shadow(&p).0.override_revision);
    assert_eq!(
        f.connection
            .query_row("SELECT SUM(manual_type_override) FROM nodes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn overrides_cannot_weaken_manual_binding_fences() {
    let (mut f, p) = factory("override-fence", LibraryKind::Comic, "FOLDER");
    f.file_book("Series/第01卷.pdf").unwrap();
    f.file_book("Series/第02卷.pdf").unwrap();
    f.manual(f.nodes["Series"], "WORK").unwrap();
    let mut opt = ReadOptions::default();
    opt.overrides.push(ManualOverride {
        path: "Series".into(),
        role: Some(DirectoryRole::Series),
        series_directory: None,
        volume: None,
        chapter: None,
        no_merge: false,
    });
    let (_, o, _) = run(&mut ReadIndex::open(&p).unwrap(), 1, &opt).unwrap();
    assert_eq!(o.summary.series, 0);
}
#[test]
fn partial_offline_failed_and_cancelled_runs_retain_and_mark_review() {
    for (outcome, status) in [
        ("PARTIAL", "FAILED"),
        ("OFFLINE", "FAILED"),
        ("FAILED", "FAILED"),
        ("SUCCESS", "CANCELLED"),
        ("SUCCESS", "RUNNING"),
    ] {
        let (mut f, p) = factory(status, LibraryKind::Comic, "FOLDER");
        f.file_book("Series/第01卷.pdf").unwrap();
        f.file_book("Series/第02卷.pdf").unwrap();
        f.connection
            .execute("UPDATE library_scan_health SET outcome=?1", [outcome])
            .unwrap();
        f.connection
            .execute(
                "INSERT INTO scan_runs(id,root_id,status,started_at) VALUES('s',1,?1,'2026-02-01')",
                [status],
            )
            .unwrap();
        let (s, o) = shadow(&p);
        invariant(&s, &o);
        assert!(!s.scan_health.complete);
        assert_eq!(o.summary.review_groups, o.logical_proposal.groups.len());
        assert_eq!(o.summary.indexed_sources, 2);
    }
}
#[test]
fn explicit_cancellation_produces_no_report() {
    let (_f, p) = scenario(3);
    let opt = ReadOptions::default();
    opt.cancelled.store(true, Ordering::Relaxed);
    assert_eq!(
        run(&mut ReadIndex::open(&p).unwrap(), 1, &opt).unwrap_err(),
        "CANCELLED_NO_REPORT"
    );
}
#[test]
fn actual_sqlite_interrupt_rolls_back_and_reader_is_reusable() {
    let (_f, p) = scenario(3);
    let before = disk_digest(&p);
    let mut reader = ReadIndex::open(&p).unwrap();
    let opt = ReadOptions {
        sql_step_budget: Some(1000),
        ..Default::default()
    };
    assert_eq!(
        reader.read(1, &opt).unwrap_err(),
        "SQLITE_READ_FAILED_OR_INTERRUPTED"
    );
    assert!(reader.read(1, &ReadOptions::default()).is_ok());
    assert_eq!(before, disk_digest(&p));
}
#[test]
fn pinned_wal_read_is_consistent_across_concurrent_index_commit() {
    let (mut f, p) = factory("wal", LibraryKind::Ebook, "FOLDER");
    let b = f.file_book("Novel.epub").unwrap();
    f.connection
        .execute_batch("PRAGMA journal_mode=WAL;")
        .unwrap();
    let old = shadow(&p).0;
    let mut reader = ReadIndex::open(&p).unwrap();
    let snap = reader
        .read_with_pin_hook(1, &ReadOptions::default(), || {
            f.connection
                .execute(
                    "UPDATE comic_books SET revision='new-revision' WHERE id=?1",
                    [b],
                )
                .unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        snap.source_id_map[&b].revision,
        old.source_id_map[&b].revision
    );
    assert_eq!(snap.index_snapshot_version, old.index_snapshot_version);
    let new = reader.read(1, &ReadOptions::default()).unwrap();
    assert_ne!(new.index_snapshot_version, old.index_snapshot_version);
    assert_eq!(
        reader
            .read(
                1,
                &ReadOptions {
                    expected_version: Some(old.index_snapshot_version),
                    ..Default::default()
                }
            )
            .unwrap_err(),
        "STALE_INDEX_NO_REPORT"
    );
}
#[test]
fn rollback_injected_after_pin_has_no_half_state() {
    let (_f, p) = scenario(3);
    let before = disk_digest(&p);
    let mut r = ReadIndex::open(&p).unwrap();
    assert_eq!(
        r.read_with_pin_hook(
            1,
            &ReadOptions::default(),
            || Err("INJECTED_FAILURE".into())
        )
        .unwrap_err(),
        "INJECTED_FAILURE"
    );
    assert!(r.read(1, &ReadOptions::default()).is_ok());
    assert_eq!(before, disk_digest(&p));
}
#[test]
fn cross_root_sources_and_traversal_paths_fail_closed() {
    for invalid in [
        "S:\\Other\\Book.pdf",
        "R:\\SyntheticLibraryExtra\\Book.pdf",
        "R:\\SyntheticLibrary\\..\\Book.pdf",
        "R:\\SyntheticLibrary\\a.\\Book.pdf",
    ] {
        let (mut f, p) = factory("escape", LibraryKind::Ebook, "FOLDER");
        let b = f.file_book("Book.pdf").unwrap();
        f.connection
            .execute(
                "UPDATE comic_books SET source_path=?1 WHERE id=?2",
                params![invalid, b],
            )
            .unwrap();
        let before = disk_digest(&p);
        assert!(ReadIndex::open(&p)
            .unwrap()
            .read(1, &ReadOptions::default())
            .is_err());
        assert_eq!(before, disk_digest(&p));
    }
}
#[test]
fn windows_verbatim_unc_and_literal_unicode_paths_are_lexically_scoped() {
    assert_eq!(
        relative_index_path("\\\\?\\R:\\Root", "r:\\Root\\中 [文]\\a.pdf").unwrap(),
        "中 [文]/a.pdf"
    );
    assert_eq!(
        relative_index_path("\\\\?\\UNC\\Server\\Share", "\\\\server\\Share\\x.pdf").unwrap(),
        "x.pdf"
    );
    assert!(relative_index_path("R:\\Root", "R:\\Root\\a:b.pdf").is_err());
}
#[test]
fn missing_or_future_schema_fails_closed() {
    for version in [0, if cfg!(feature = "production") { 27 } else { 25 }] {
        let (f, p) = factory("schema", LibraryKind::Comic, "FOLDER");
        if version == 0 {
            f.connection
                .execute(
                    "DELETE FROM mediashelf_schema_migrations WHERE version=3",
                    [],
                )
                .unwrap();
        } else {
            f.connection
                .execute(
                    "INSERT INTO mediashelf_schema_migrations(version) VALUES(?1)",
                    [version],
                )
                .unwrap();
        }
        assert_eq!(
            ReadIndex::open(&p)
                .unwrap()
                .read(1, &ReadOptions::default())
                .unwrap_err(),
            "UNSUPPORTED_SCHEMA_VERSION"
        );
    }
}
#[test]
fn source_format_disagreement_and_missing_revision_are_not_verified() {
    let (mut f, p) = factory("format", LibraryKind::Ebook, "FOLDER");
    let a = f.file_book("A.pdf").unwrap();
    let b = f.file_book("B.mobi").unwrap();
    f.connection
        .execute(
            "UPDATE comic_books SET reader_format='EPUB' WHERE id=?1",
            [a],
        )
        .unwrap();
    f.connection
        .execute("UPDATE comic_books SET revision='' WHERE id=?1", [b])
        .unwrap();
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert_eq!(o.fallback_book_ids.len(), 2);
    assert_eq!(o.summary.works, 0);
}
#[test]
fn noncontiguous_page_index_is_preserved_not_renumbered() {
    let (mut f, p) = factory("page-index", LibraryKind::Comic, "FOLDER");
    let b = f.image_book("Images", &["001.PNG", "002.PNG"]).unwrap();
    f.connection
        .execute(
            "UPDATE comic_pages SET page_index=7 WHERE comic_book_id=?1 AND page_index=1",
            [b],
        )
        .unwrap();
    let (s, o) = shadow(&p);
    assert_eq!(s.source_id_map[&b].pages[1].page_index, 7);
    assert_eq!(s.source_id_map[&b].status, "INVALID_BOOK_INDEX_RETAINED");
    assert_eq!(o.fallback_book_ids, vec![b]);
}
#[test]
fn page_locator_escape_and_cross_root_resource_fk_are_rejected() {
    let (mut f, p) = factory("page-escape", LibraryKind::Comic, "FOLDER");
    f.image_book("Images", &["1.png", "2.png"]).unwrap();
    f.connection
        .execute(
            r"UPDATE comic_pages SET source_locator='S:\Other\1.png' WHERE page_index=1",
            [],
        )
        .unwrap();
    assert!(ReadIndex::open(&p)
        .unwrap()
        .read(1, &ReadOptions::default())
        .is_err());
    let (mut g, q) = factory("resource-fk", LibraryKind::Ebook, "FOLDER");
    let b = g.file_book("B.pdf").unwrap();
    g.connection.execute_batch(r"INSERT INTO library_roots(id,path,display_name) VALUES(2,'S:\Other','Other');INSERT INTO nodes(id,library_root_id,absolute_path,folder_name,display_name) VALUES(999,2,'S:\Other','Other','Other');INSERT INTO resource_files(id,node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(999,999,'S:\Other\B.pdf','B.pdf','pdf',1,'x');").unwrap();
    g.connection
        .execute(
            "UPDATE comic_books SET source_resource_id=999 WHERE id=?1",
            [b],
        )
        .unwrap();
    assert_eq!(
        ReadIndex::open(&q)
            .unwrap()
            .read(1, &ReadOptions::default())
            .unwrap_err(),
        "CROSS_ROOT_OR_MISSING_RESOURCE"
    );
}
#[test]
fn oversized_index_text_is_rejected_before_allocating_a_string() {
    let (mut f, p) = factory("text-limit", LibraryKind::Ebook, "FOLDER");
    let b = f.file_book("A.pdf").unwrap();
    f.connection
        .execute(
            "UPDATE comic_books SET revision=?1 WHERE id=?2",
            params!["x".repeat(32768), b],
        )
        .unwrap();
    assert_eq!(
        ReadIndex::open(&p)
            .unwrap()
            .read(1, &ReadOptions::default())
            .unwrap_err(),
        "INDEX_TEXT_LIMIT"
    );
}
#[test]
fn readonly_sqlite_connection_rejects_writes_and_preserves_wal_bytes() {
    let (mut f, p) = factory("readonly", LibraryKind::Ebook, "FOLDER");
    f.file_book("A.pdf").unwrap();
    f.connection
        .execute_batch("PRAGMA journal_mode=WAL;UPDATE library_scan_health SET error_count=0;")
        .unwrap();
    let before = logical_digest(&f.connection);
    let disk = disk_digest(&p);
    let wal = p.with_extension("sqlite-wal");
    let wal_before = disk_digest(&wal);
    let c = rusqlite::Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    assert!(c.execute("DELETE FROM comic_books", []).is_err());
    drop(c);
    shadow(&p);
    assert_eq!(before, logical_digest(&f.connection));
    assert_eq!(disk, disk_digest(&p));
    assert_eq!(wal_before, disk_digest(&wal));
}

#[test]
fn single_image_attachment_keeps_file_identity_without_fake_folder() {
    let (mut f, p) = factory("single-image-attachment", LibraryKind::Artbook, "FOLDER");
    let b = f.image_book("image.png", &["unused.png"]).unwrap();
    let r = f.resource("image.png").unwrap();
    // Attachment books belong to the ResourceFile's directory, unlike a core image collection.
    f.connection
        .execute(
            "UPDATE comic_books SET node_id=?1 WHERE id=?2",
            params![f.nodes[""], b],
        )
        .unwrap();
    f.connection
        .execute("DELETE FROM nodes WHERE id=?1", [f.nodes["image.png"]])
        .unwrap();
    f.connection.execute("UPDATE comic_books SET source_resource_id=?1,source_resource_stamp='2026-01-01' WHERE id=?2",params![r,b]).unwrap();
    f.connection
        .execute(
            "UPDATE comic_pages SET source_locator=?1 WHERE comic_book_id=?2",
            params![Factory::absolute("image.png"), b],
        )
        .unwrap();
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert_eq!(o.fallback_book_ids, vec![b]);
    assert_eq!(s.source_id_map[&b].resource_id, Some(r));
    assert!(s
        .recognition_input
        .entries
        .iter()
        .any(|e| e.path == "image.png" && e.kind == EntryKind::File));
}
#[test]
fn orphaned_node_is_an_error_not_zero_orphans_success() {
    let (mut f, p) = factory("orphan", LibraryKind::Comic, "FOLDER");
    f.file_book("Child/Book.pdf").unwrap();
    f.connection
        .execute(
            "UPDATE nodes SET parent_node_id=NULL WHERE id=?1",
            [f.nodes["Child"]],
        )
        .unwrap();
    assert_eq!(
        ReadIndex::open(&p)
            .unwrap()
            .read(1, &ReadOptions::default())
            .unwrap_err(),
        "ORPHAN_NODE_NO_REPORT"
    );
}
#[test]
fn page_limit_rejects_a_large_book_without_partial_report() {
    let (mut f, p) = factory("page-budget", LibraryKind::Comic, "FOLDER");
    let b = f.image_book("Images", &["0.png"]).unwrap();
    f.connection.execute("WITH RECURSIVE seq(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM seq WHERE i<10000) INSERT INTO comic_pages(comic_book_id,page_index,page_name,source_locator,file_size,modified_at) SELECT ?1,i,'indexed.png','R:\\SyntheticLibrary\\Images\\indexed.png',1,'x' FROM seq",[b]).unwrap();
    assert_eq!(
        ReadIndex::open(&p)
            .unwrap()
            .read(1, &ReadOptions::default())
            .unwrap_err(),
        "PAGE_LIMIT"
    );
}
#[test]
fn drive_root_and_unicode_paths_remain_literal() {
    assert_eq!(
        relative_index_path("R:\\", "R:\\中\\a.pdf").unwrap(),
        "中/a.pdf"
    );
}

#[test]
fn legacy_txt_text_encoding_fallback_matches_production_book_select() {
    let (mut f, p) = factory("legacy-txt", LibraryKind::Ebook, "FOLDER");
    let b = f.file_book("Old.txt").unwrap();
    f.connection.execute("UPDATE comic_books SET reader_format=NULL,document_format=NULL,text_encoding='UTF-8' WHERE id=?1",[b]).unwrap();
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert_eq!(o.summary.proposed_reading_units, 1);
    assert_eq!(s.source_id_map[&b].text_encoding.as_deref(), Some("UTF-8"));
    assert_eq!(
        o.logical_proposal.reading_units[0].format,
        Some(Format::Txt)
    );
}
#[test]
fn exact_current_production_owned_sql_and_browse_predicate_match_shadow_views() {
    let (mut f, p) = factory("production-query-parity", LibraryKind::Comic, "FOLDER");
    f.file_book("Series/第01卷.pdf").unwrap();
    f.file_book("Series/Bound/第02卷.pdf").unwrap();
    f.file_book("Series/Manual/第03卷.pdf").unwrap();
    f.file_book("Series/Ignored/第04卷.pdf").unwrap();
    f.bind(f.nodes["Series"], 1).unwrap();
    f.bind(f.nodes["Series/Bound"], 2).unwrap();
    f.manual(f.nodes["Series/Manual"], "WORK").unwrap();
    f.manual(f.nodes["Series/Ignored"], "IGNORED").unwrap();
    let (s, _) = shadow(&p);
    // Execute the literal SQL from the current production source, not another authored expected query.
    let source = include_str!("../../../src-tauri/src/comics.rs");
    let tail = source.split("fn owned_nodes(").nth(1).unwrap();
    let sql = tail
        .split("prepare(\"")
        .nth(1)
        .unwrap()
        .split("\").map_err")
        .next()
        .unwrap();
    for view in &s.current_details {
        let owned = f
            .connection
            .prepare(sql)
            .unwrap()
            .query_map([view.anchor_node_id], |r| r.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<BTreeSet<_>, _>>()
            .unwrap();
        let expected = s
            .source_id_map
            .values()
            .filter(|b| owned.contains(&b.node_id) && !b.path.ends_with(".zip"))
            .map(|b| b.book_id)
            .collect::<Vec<_>>();
        assert_eq!(view.book_ids, expected);
    }
    let db = include_str!("../../../src-tauri/src/db.rs");
    let tail = db
        .split("pub(crate) fn list_children_conn(")
        .nth(1)
        .unwrap();
    let pred = tail
        .split("\"{} ")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let old = f
        .connection
        .prepare(&format!("SELECT n.id FROM nodes n {pred} ORDER BY n.id"))
        .unwrap()
        .query_map([f.nodes[""]], |r| r.get::<_, i64>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(old, s.current_browse_node_ids);
}

#[test]
fn detail_sampling_keeps_hidden_root_even_when_its_id_is_later() {
    let (mut f, p) = factory("late-root", LibraryKind::Comic, "FOLDER");
    f.connection
        .execute("UPDATE nodes SET id=1000 WHERE id=1", [])
        .unwrap();
    f.nodes.insert(String::new(), 1000);
    // Insert lower ids explicitly: old indexes can acquire a hidden Root after children.
    for id in 2..=67 {
        f.connection.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type) VALUES(?1,1,1000,?2,?3,?3,'CONTAINER')", rusqlite::params![id, Factory::absolute(&format!("Folder-{id}")),format!("Folder-{id}")]).unwrap();
    }
    let (s, o) = shadow(&p);
    invariant(&s, &o);
    assert_eq!(s.current_details.len(), 65);
    assert!(s.current_details.iter().any(|d| d.anchor_node_id == 1000));
    assert_eq!(s.diagnostics["DETAIL_COMPARISON_SAMPLED_65_ANCHORS"], 1);
}

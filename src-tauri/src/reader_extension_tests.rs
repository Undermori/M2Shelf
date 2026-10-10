//! Integration regressions for additive library policy and attachment identities.
use crate::{db::Database, models::LibraryMediaKind};
use rusqlite::params;

#[test]
fn migration_22_preserves_existing_kinds_bindings_covers_progress_and_settings() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::new(temp.path().join("old.sqlite"));
    let c = db.connect().unwrap();
    c.execute_batch("CREATE TABLE mediashelf_schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)").unwrap();
    let mut migrations =
        std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
    migrations.sort();
    for path in migrations {
        let version: i64 = path.file_name().unwrap().to_string_lossy()[..4]
            .parse()
            .unwrap();
        if version > 22 {
            continue;
        }
        c.execute_batch(&std::fs::read_to_string(path).unwrap())
            .unwrap();
        c.execute(
            "INSERT INTO mediashelf_schema_migrations(version) VALUES(?1)",
            [version],
        )
        .unwrap();
    }
    for (i, (kind, scope, book, doujin)) in [
        ("VIDEO", "ANIMATION", "COMIC", 0),
        ("VIDEO", "LIVE_ACTION", "COMIC", 0),
        ("COMIC", "MIXED", "COMIC", 0),
        ("COMIC", "MIXED", "EBOOK", 0),
        ("COMIC", "MIXED", "COMIC", 1),
    ]
    .into_iter()
    .enumerate()
    {
        let id = i as i64 + 1;
        c.execute("INSERT INTO library_roots(id,path,display_name,media_kind,video_subject_scope,book_library_kind,doujin_library) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,temp.path().join(format!("Root{id}")).to_string_lossy(),format!("Root{id}"),kind,scope,book,doujin]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,absolute_path,folder_name,display_name,node_type,cover_source,cover_cache_path) VALUES(?1,?1,?2,'Work','Manual display','WORK','MANUAL','cache/manual.png')",params![id,temp.path().join(format!("Root{id}/Work")).to_string_lossy()]).unwrap();
    }
    c.execute_batch("INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_title,provider_subject_type) VALUES(3,'BANGUMI',777,'Bound title',1); INSERT INTO comic_books(id,node_id,source_path,source_kind,display_name,modified_at,page_count) VALUES(1,3,'book.pdf','ZIP_ARCHIVE','Book','stamp',1); INSERT INTO comic_pages(comic_book_id,page_index,page_name,source_locator,file_size,modified_at) VALUES(1,0,'Page','0',1,'stamp'); INSERT INTO comic_reading_progress(comic_book_id,last_page_index) VALUES(1,0); INSERT INTO comic_bookmarks(comic_book_id,page_index) VALUES(1,0); INSERT INTO settings(key,value) VALUES('language','ja-JP');").unwrap();
    db.migrate().unwrap();
    db.migrate().unwrap();
    let roots = db.list_roots().unwrap();
    assert_eq!(
        roots.iter().map(|r| r.media_kind).collect::<Vec<_>>(),
        [
            LibraryMediaKind::Animation,
            LibraryMediaKind::LiveAction,
            LibraryMediaKind::Comic,
            LibraryMediaKind::Ebook,
            LibraryMediaKind::Doujin
        ]
    );
    assert_eq!(
        roots.iter().map(|r| r.auto_bangumi).collect::<Vec<_>>(),
        [true, true, true, true, false]
    );
    assert_eq!(db.get_binding(3).unwrap().unwrap().provider_subject_id, 777);
    assert_eq!(
        c.query_row(
            "SELECT display_name||':'||cover_source||':'||cover_cache_path FROM nodes WHERE id=3",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "Manual display:MANUAL:cache/manual.png"
    );
    for table in [
        "comic_reading_progress",
        "comic_bookmarks",
        "comic_pages",
        "comic_books",
    ] {
        assert_eq!(
            c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    assert_eq!(
        c.query_row("SELECT value FROM settings WHERE key='language'", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "ja-JP"
    );
    assert_eq!(
        c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

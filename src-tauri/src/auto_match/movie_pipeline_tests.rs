use super::*;
use crate::models::{
    LibraryMediaKind, LibraryRecognitionMode, ScanPhase, ScanProgress, ScanStatus,
};
use std::{
    fs,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

#[derive(Default)]
struct HttpMovies {
    requests: Vec<Vec<(String, String)>>,
    result: Vec<crate::tmdb::Movie>,
}
impl crate::tmdb::AutomaticProvider for HttpMovies {
    fn available(&mut self) -> bool {
        true
    }
    fn search(
        &mut self,
        _: &Database,
        _: i64,
        query: &str,
        year: Option<u16>,
        _: &str,
    ) -> crate::db::AppResult<Vec<crate::tmdb::Movie>> {
        let (movies, args) = crate::tmdb::fixture_movie_http(query, year);
        self.requests.push(args);
        self.result = movies.clone();
        Ok(movies)
    }
    fn detail(&mut self, _: i64, _: &str) -> crate::db::AppResult<crate::tmdb::Movie> {
        Ok(self.result[0].clone())
    }
    fn cover(
        &mut self,
        root: &Path,
        movie: &crate::tmdb::Movie,
    ) -> crate::db::AppResult<Option<PathBuf>> {
        crate::tmdb::fixture_movie_cover(root, movie).map(Some)
    }
}
fn control(root: &crate::models::LibraryRoot) -> crate::scanner::ScanControl {
    crate::scanner::ScanControl {
        unchanged_directories: Default::default(),
        scan_id: "movie-http-fixture".into(),
        cancel: Arc::new(AtomicBool::new(false)),
        progress: Arc::new(Mutex::new(ScanProgress {
            background: false,
            library_changed: None,
            scan_id: "movie-http-fixture".into(),
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
fn movie_only_cache() -> MatchRunCache {
    // Fail before any Bangumi request if import/rescan/rematch enters the wrong provider route.
    MatchRunCache {
        forbid_bangumi: true,
        ..Default::default()
    }
}

#[test]
fn movie_production_import_rescan_rematch_final_http_arguments_and_persistent_cover() {
    let _serial = crate::tmdb::MOVIE_TEST_GATE
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let cases = [
        (
            "Superbad.2007.1080p.BluRay.x264-GROUP",
            "Superbad.2007.1080p.BluRay.x264-GROUP.mkv",
            "Superbad",
            Some(2007),
        ),
        (
            "Ernest.And.Celestine.2012.DUBBED.1080p.BluRay.H264.AAC-RARBG",
            "Ernest.And.Celestine.2012.DUBBED.1080p.BluRay.H264.AAC-RARBG.mkv",
            "Ernest And Celestine",
            Some(2012),
        ),
        (
            "1917.2019.1080p.WEB-DL",
            "1917.2019.1080p.WEB-DL.mkv",
            "1917",
            Some(2019),
        ),
        (
            "Se7en.1995.BluRay",
            "Se7en.1995.BluRay.mkv",
            "Se7en",
            Some(1995),
        ),
        (
            "2001.A.Space.Odyssey.1968.1080p",
            "2001.A.Space.Odyssey.1968.1080p.mkv",
            "2001 A Space Odyssey",
            Some(1968),
        ),
        (
            "Big.Hero.6.2014",
            "Big.Hero.6.2014.mkv",
            "Big Hero 6",
            Some(2014),
        ),
        (
            "Movies",
            "Ernest.And.Celestine.2012.1080p.mkv",
            "Ernest And Celestine",
            Some(2012),
        ),
        (
            "Ernest.And.Celestine.2012",
            "film.mkv",
            "Ernest And Celestine",
            Some(2012),
        ),
        (
            "[RARBG] 霸王别姬.1993.1080p",
            "霸王别姬.1993.1080p.mkv",
            "霸王别姬",
            Some(1993),
        ),
        (
            "千と千尋の神隠し.2001.1080p",
            "千と千尋の神隠し.2001.1080p.mkv",
            "千と千尋の神隠し",
            Some(2001),
        ),
        ("Inception", "Inception.mkv", "Inception", None),
        ("2046.2004.1080p", "2046.2004.1080p.mkv", "2046", Some(2004)),
        ("Example.Feature.2009.2160p.UHD.BluRay.x265.HDR.DTS-HD.MA.5.1-GROUP", "Example.Feature.2009.2160p.UHD.BluRay.x265.HDR.DTS-HD.MA.5.1-GROUP.mkv", "Example Feature", Some(2009)),
        ("Example Feature (2011) (1080p BluRay x265 r00t)", "Example Feature (2011) (1080p BluRay x265 r00t).mkv", "Example Feature", Some(2011)),
        ("Example.Feature.2011.Extended.Cut.Bluray.1080p.GROUP@SITE.COM", "Example.Feature.2011.Extended.Cut.Bluray.1080p.GROUP@SITE.COM.mkv", "Example Feature", Some(2011)),
        ("Example.Feature.2014.JAPANESE.1080p.BluRay.H264.AAC-GROUP", "Example.Feature.2014.JAPANESE.1080p.BluRay.H264.AAC-GROUP.mp4", "Example Feature", Some(2014)),
        ("【发布站www.example.com】示例片名.The.Example.2013.2160p.iTunes.WEB-DL.DD5.1.H265-GROUP", "The.Example.2013.2160p.iTunes.WEB-DL.DD5.1.H265-GROUP.mkv", "示例片名", Some(2013)),
        ("示例片名.Example.Feature.2014.BD1080P.X264.AAC.English.CHS-ENG.Mp4Ba", "示例片名.Example.Feature.2014.BD1080P.X264.AAC.English.CHS-ENG.Mp4Ba.mp4", "示例片名", Some(2014)),
        ("Blade.Runner.2049.2017.1080p.BluRay", "Blade.Runner.2049.2017.1080p.BluRay.mkv", "Blade Runner 2049", Some(2017)),
    ];
    for mode in [
        LibraryRecognitionMode::Folder,
        LibraryRecognitionMode::VideoFile,
    ] {
        for (folder, file, folder_title, year) in cases {
            let file_title = if folder_title == "示例片名" && file.starts_with("The.Example") {
                "The Example"
            } else {
                folder_title
            };
            let title = if matches!(mode, LibraryRecognitionMode::VideoFile) {
                file_title
            } else {
                folder_title
            };
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("library");
            let dir = source.join(folder);
            fs::create_dir_all(&dir).unwrap();
            let video = dir.join(file);
            fs::write(&video, b"synthetic").unwrap();
            let cache_root = temp.path().join("covers");
            fs::create_dir_all(&cache_root).unwrap();
            crate::cache::ensure_directories(&cache_root).unwrap();
            let db = Database::new(temp.path().join("app.db"));
            db.migrate().unwrap();
            let root = db
                .add_root_with_kind(&source, None, LibraryMediaKind::LiveAction, mode.clone())
                .unwrap();
            let target = ScanTarget {
                root: root.clone(),
                path: source,
                parent_node_id: None,
            };
            let mut provider = HttpMovies::default();
            for phase in ["import", "rescan", "explicit-rematch"] {
                if phase == "explicit-rematch" {
                    let nodes = candidates_in_targets(&db, std::slice::from_ref(&target)).unwrap();
                    let cache = movie_only_cache();
                    let report = run_match_nodes_with_tmdb(
                        &db,
                        &nodes,
                        Ok(&cache_root),
                        MatchWriteMode::ExplicitRematch,
                        |_, _, _, _| {},
                        (|| false, &mut provider),
                        cache,
                    );
                    assert_eq!(
                        report.matched,
                        usize::from(year.is_some()),
                        "{folder}/{phase}"
                    );
                } else {
                    let scan = control(&root);
                    crate::scanner::run_scan_with_matcher(
                        None,
                        &db,
                        vec![target.clone()],
                        &scan,
                        &crate::db::default_video_extensions(),
                        Some(Ok(cache_root.clone())),
                        |targets, cache_root| {
                            let cache = movie_only_cache();
                            run_auto_match_using(
                                &db,
                                targets,
                                &HashSet::new(),
                                cache_root,
                                |_, _, _, _| {},
                                (|| false, &mut provider, cache),
                            )
                        },
                    );
                    assert!(matches!(scan.progress().status, ScanStatus::Completed));
                    assert_eq!(
                        scan.progress().auto_match_matched,
                        u64::from(year.is_some()),
                        "{folder}/{phase}"
                    );
                }
                let args = provider
                    .requests
                    .last()
                    .unwrap_or_else(|| panic!("no TMDb HTTP request: {folder}/{phase}/{mode:?}"));
                assert_eq!(
                    args.iter().find(|(key, _)| key == "query").unwrap().1,
                    title,
                    "{mode:?}/{phase}/{folder}"
                );
                assert_eq!(
                    args.iter()
                        .find(|(key, _)| key == "primary_release_year")
                        .map(|v| v.1.clone()),
                    year.map(|v| v.to_string())
                );
                assert_eq!(
                    provider.requests.len(),
                    if phase == "import" {
                        1
                    } else if phase == "rescan" {
                        2
                    } else {
                        3
                    }
                );
                let reopened = Database::new(temp.path().join("app.db"));
                let diagnostics = crate::tmdb::diagnostics(&reopened).unwrap();
                assert_eq!(diagnostics[0].query, title);
                assert_eq!(diagnostics[0].year, year);
                assert_eq!(
                    diagnostics[0].outcome,
                    if year.is_some() {
                        "matched"
                    } else {
                        "year-uncertain"
                    }
                );
                if year.is_some() {
                    let c = reopened.connect().unwrap();
                    let cover: String = c
                        .query_row(
                            "SELECT cover_cache_path FROM tmdb_movie_bindings",
                            [],
                            |r| r.get(0),
                        )
                        .unwrap();
                    assert!(crate::cache::cached_cover_is_valid(Path::new(&cover)));
                    assert!(Path::new(&cover).starts_with(&cache_root));
                    // Removing only this synthetic binding models a previously unbound item.
                    if phase != "explicit-rematch" {
                        c.execute("DELETE FROM tmdb_movie_bindings", []).unwrap();
                    } else {
                        let nodes = candidates_in_targets(&reopened, std::slice::from_ref(&target))
                            .unwrap();
                        run_match_nodes_with_tmdb(
                            &reopened,
                            &nodes,
                            Ok(&cache_root),
                            MatchWriteMode::IfAbsent,
                            |_, _, _, _| {},
                            (|| false, &mut provider),
                            movie_only_cache(),
                        );
                        assert_eq!(provider.requests.len(), 3);
                    }
                }
                assert_eq!(fs::read(&video).unwrap(), b"synthetic");
            }
        }
    }
}

#[test]
fn movie_title_priority_aliases_and_bangumi_animation_regression() {
    use LibraryRecognitionMode::{Folder, VideoFile};
    let folder = title_extractor::build_movie_match_evidence(
        "Specific Movie 2007",
        "Specific Movie 2007",
        &["Different Movie 2008.mkv".into()],
        Folder,
    );
    let file = title_extractor::build_movie_match_evidence(
        "Specific Movie 2007",
        "Specific Movie 2007",
        &["Different Movie 2008.mkv".into()],
        VideoFile,
    );
    assert_eq!(folder.primary_title, "Specific Movie");
    assert_eq!(file.primary_title, "Different Movie");
    assert!(!folder.year_is_strong);
    assert!(!file.year_is_strong);
    let alias = title_extractor::build_movie_match_evidence(
        "盗梦空间 (Inception) 2010",
        "盗梦空间 (Inception) 2010",
        &["Inception.2010.mkv".into()],
        Folder,
    );
    assert!(alias.alternate_titles.iter().any(|v| v == "Inception"));
    // The original animation/TV builder and recall retain their structured season evidence.
    let old = title_extractor::build_match_evidence(
        "[SubsPlease] Frieren S02 1080p",
        "[SubsPlease] Frieren S02 1080p",
        None,
        &[],
    );
    assert_eq!(old.season_number, Some(2));
    assert!(match_queries(&old).iter().any(|v| v.contains("Frieren")));
}

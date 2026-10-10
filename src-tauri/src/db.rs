use std::{
    cmp::Ordering,
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};

use crate::models::{
    AllBookLibrary, AllResourcesResult, AppSettings, BatchMutationResult, BreadcrumbItem,
    CollectionSort, CollectionSortPreferences, CollectionSortScope, CoverSource, FavoriteFolder,
    LibraryMediaKind, LibraryRecognitionMode, LibraryRoot, MediaFile, MediaNode, MetadataBinding,
    NodeType, RecentlyWatchedEntry, ResourceFile, ResourceType, SearchHit, SearchHitKind, UserTag,
    UserTagMembership, ViewMode, WindowSize,
};

pub type AppResult<T> = Result<T, String>;

#[derive(Debug, PartialEq, Eq)]
pub enum ConditionalBindingSave {
    Applied(Option<PathBuf>),
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfirmedTitleAliasMatch {
    pub subject_id: i64,
    pub subject_type: i64,
    pub matched_alias: String,
}

const MAX_BATCH_NODE_IDS: usize = 500;
const NODE_METADATA_CHUNK_SIZE: usize = 500;
const MAX_SEARCH_QUERY_CHARS: usize = 500;
const MAX_DISPLAY_NAME_CHARS: usize = 240;
const MAX_CONFIRMED_TITLE_ALIASES: usize = 32;
const MAX_CONFIRMED_TITLE_ALIAS_CHARS: usize = 200;
const MAX_VIDEO_EXTENSIONS: usize = 64;
const WINDOW_SIZE_SETTING_KEY: &str = "main_window_size";

#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
}

/// Holds SQLite's single-writer reservation for the final Portable update window.
///
/// The guard is intentionally kept alive until the current process exits. This prevents a
/// command that was queued after the rollback snapshot from committing data which would then be
/// lost if the newly installed version fails its health check and restores that snapshot.
pub struct DatabaseUpdateBarrier {
    _connection: Connection,
}

impl Database {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn connect(&self) -> AppResult<Connection> {
        let connection = Connection::open(&self.path).map_err(db_error)?;
        connection
            .busy_timeout(Duration::from_secs(15))
            .map_err(db_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(db_error)?;
        Ok(connection)
    }

    pub(crate) fn read_snapshot<T>(
        &self,
        read: impl FnOnce(&Connection) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut connection = self.connect()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(db_error)?;
        let result = read(&tx)?;
        tx.commit().map_err(db_error)?;
        Ok(result)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Creates a transactionally consistent SQLite snapshot and retains SQLite's single-writer
    /// reservation. The caller must keep the returned guard alive until the old process exits.
    /// The destination must be application-owned and outside every Library Root.
    pub fn backup_for_portable_update(
        &self,
        destination: &Path,
    ) -> AppResult<DatabaseUpdateBarrier> {
        if destination.exists() {
            return Err("数据库备份目标已存在。".into());
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let barrier = self.connect()?;
        // BEGIN IMMEDIATE waits for any writer which was already in flight, then prevents every
        // other connection from committing a later write. Readers remain available in WAL mode.
        barrier
            .execute_batch("BEGIN IMMEDIATE;")
            .map_err(db_error)?;
        // SQLite's online-backup API cannot make progress when its source connection owns the
        // write reservation. Use a separate read connection while `barrier` blocks later writers.
        let source = Connection::open(&self.path).map_err(db_error)?;
        source
            .busy_timeout(Duration::from_secs(15))
            .map_err(db_error)?;
        let mut target = Connection::open(destination).map_err(db_error)?;
        let backup = rusqlite::backup::Backup::new(&source, &mut target).map_err(db_error)?;
        backup
            .run_to_completion(64, Duration::from_millis(10), None)
            .map_err(db_error)?;
        drop(backup);
        target
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(db_error)?;
        Ok(DatabaseUpdateBarrier {
            _connection: barrier,
        })
    }

    pub fn migrate(&self) -> AppResult<()> {
        let connection = self.connect()?;
        let existing: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='mediashelf_schema_migrations')", [], |row| row.get(0)).map_err(db_error)?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS mediashelf_schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );",
            )
            .map_err(db_error)?;

        let migrations = [
            (1_i64, include_str!("../migrations/0001_initial.sql")),
            (2_i64, include_str!("../migrations/0002_mvp.sql")),
            (
                3_i64,
                include_str!("../migrations/0003_resources_and_cover_status.sql"),
            ),
            (
                4_i64,
                include_str!("../migrations/0004_multilingual_metadata.sql"),
            ),
            (5_i64, include_str!("../migrations/0005_user_tags.sql")),
            (6_i64, include_str!("../migrations/0006_watch_history.sql")),
            (
                7_i64,
                include_str!("../migrations/0007_favorite_folders.sql"),
            ),
            (
                8_i64,
                include_str!("../migrations/0008_bangumi_subject_type.sql"),
            ),
            (
                9_i64,
                include_str!("../migrations/0009_library_recognition_mode.sql"),
            ),
            (
                10_i64,
                include_str!("../migrations/0010_confirmed_title_aliases.sql"),
            ),
            (
                11_i64,
                include_str!("../migrations/0011_incremental_scan.sql"),
            ),
            (
                12_i64,
                include_str!("../migrations/0012_provider_aliases.sql"),
            ),
            (13_i64, include_str!("../migrations/0013_alias_sync.sql")),
            (14_i64, include_str!("../migrations/0014_scan_health.sql")),
            (
                15_i64,
                include_str!("../migrations/0015_comic_library_kind.sql"),
            ),
            (16_i64, include_str!("../migrations/0016_comics.sql")),
            (
                17_i64,
                include_str!("../migrations/0017_comic_binding_types.sql"),
            ),
            (
                18_i64,
                include_str!("../migrations/0018_document_books.sql"),
            ),
            (19_i64, include_str!("../migrations/0019_ebook_library.sql")),
            (
                20_i64,
                include_str!("../migrations/0020_book_file_recognition.sql"),
            ),
            (
                21_i64,
                include_str!("../migrations/0021_doujin_and_text_books.sql"),
            ),
            (
                22_i64,
                include_str!("../migrations/0022_poster_cache_failures.sql"),
            ),
            (
                23_i64,
                include_str!("../migrations/0023_readable_resources.sql"),
            ),
            (
                24_i64,
                include_str!("../migrations/0024_artbook_matching_policy.sql"),
            ),
            (25_i64, include_str!("../migrations/0025_smart_mixed.sql")),
            (
                26_i64,
                include_str!("../migrations/0026_text_reader_positions.sql"),
            ),
        ];
        let reclassification_needed: bool = connection
            .query_row(
                "SELECT NOT EXISTS(SELECT 1 FROM mediashelf_schema_migrations WHERE version=19)",
                [],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        for (version, sql) in migrations {
            let applied = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM mediashelf_schema_migrations WHERE version = ?1)",
                    [version],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(db_error)?;
            if !applied {
                connection
                    .execute_batch("BEGIN IMMEDIATE;")
                    .map_err(db_error)?;
                let result: AppResult<()> = (|| {
                    connection.execute_batch(sql).map_err(db_error)?;
                    // Reclassification uses the current DTO projection. Run only once all
                    // additive columns exist, including when upgrading a pre-14 database.
                    if version == 24 && reclassification_needed {
                        crate::logical_works::LogicalWorkIndex::reclassify(&connection, None)?;
                    }
                    connection
                        .execute(
                            "INSERT INTO mediashelf_schema_migrations(version) VALUES (?1)",
                            [version],
                        )
                        .map_err(db_error)?;
                    connection.execute_batch("COMMIT;").map_err(db_error)
                })();
                if let Err(error) = result {
                    let _ = connection.execute_batch("ROLLBACK;");
                    return Err(format!("数据库迁移 {version} 失败：{error}"));
                }
            }
        }
        connection
            .execute(
                "INSERT OR IGNORE INTO settings(key,value) VALUES('auto_scan_on_startup',?1)",
                [if existing { "false" } else { "true" }],
            )
            .map_err(db_error)?;
        Ok(())
    }

    pub fn list_roots(&self) -> AppResult<Vec<LibraryRoot>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT r.id, r.path, r.display_name, r.created_at, r.last_scan_at, r.recognition_mode,
                    (SELECT COUNT(*) FROM nodes n
                     WHERE n.library_root_id = r.id AND n.node_type <> 'IGNORED'
                       AND (n.total_video_count > 0 OR n.total_comic_book_count > 0)
                       AND EXISTS (
                           SELECT 1 FROM nodes hidden
                           WHERE hidden.id=n.parent_node_id
                             AND hidden.library_root_id=r.id
                             AND hidden.parent_node_id IS NULL
                       )),
                    CASE WHEN r.media_kind='COMIC' THEN
                      (SELECT COUNT(*) FROM comic_books b JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=r.id AND b.source_resource_id IS NULL)
                    ELSE (SELECT COUNT(*) FROM media_files f JOIN nodes n ON n.id=f.node_id WHERE n.library_root_id=r.id) END,
                    CASE WHEN r.artbook_library=1 THEN 'ARTBOOK' WHEN r.doujin_library=1 THEN 'DOUJIN' WHEN r.media_kind='COMIC' THEN r.book_library_kind WHEN r.video_subject_scope<>'MIXED' THEN r.video_subject_scope ELSE r.media_kind END, r.auto_bangumi, r.book_organization_strategy
                 FROM library_roots r ORDER BY r.display_name COLLATE NOCASE, r.path COLLATE NOCASE",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(LibraryRoot {
                    media_kind: LibraryMediaKind::from_db(&row.get::<_, String>(8)?),
                    auto_bangumi: row.get(9)?,
                    book_organization_strategy: row.get(10)?,
                    scan_health: None,
                    id: row.get(0)?,
                    path: row.get(1)?,
                    display_name: row.get(2)?,
                    created_at: row.get(3)?,
                    last_scan_at: row.get(4)?,
                    recognition_mode: LibraryRecognitionMode::from_db(&row.get::<_, String>(5)?),
                    node_count: Some(row.get(6)?),
                    media_count: Some(row.get(7)?),
                })
            })
            .map_err(db_error)?;
        let mut roots = rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
        for root in &mut roots {
            root.scan_health = read_scan_health_conn(&connection, root.id)?;
        }
        Ok(roots)
    }

    pub fn get_root(&self, root_id: i64) -> AppResult<LibraryRoot> {
        let connection = self.connect()?;
        let mut root = get_root_conn(&connection, root_id)?;
        root.scan_health = read_scan_health_conn(&connection, root.id)?;
        Ok(root)
    }

    pub fn set_root_auto_bangumi(&self, root_id: i64, enabled: bool) -> AppResult<LibraryRoot> {
        let mut connection = self.connect()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        if !get_root_conn(&tx, root_id)?.media_kind.is_book() {
            return Err("BANGUMI_MEDIA_KIND_CONFLICT".into());
        }
        tx.execute(
            "UPDATE library_roots SET auto_bangumi=?1 WHERE id=?2",
            params![enabled, root_id],
        )
        .map_err(db_error)?;
        let root = get_root_conn(&tx, root_id)?;
        tx.commit().map_err(db_error)?;
        Ok(root)
    }

    pub fn update_root_display_name(
        &self,
        root_id: i64,
        display_name: &str,
    ) -> AppResult<LibraryRoot> {
        let display_name = display_name.trim();
        if display_name.is_empty() {
            return Err("资源库显示名称不能为空。".into());
        }
        if display_name.chars().count() > 120 {
            return Err("资源库显示名称不能超过 120 个字符。".into());
        }
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE library_roots SET display_name=?1 WHERE id=?2",
                params![display_name, root_id],
            )
            .map_err(db_error)?;
        if changed == 0 {
            return Err("资源库不存在。".into());
        }
        drop(connection);
        self.get_root(root_id)
    }

    #[cfg(test)]
    pub fn add_root(&self, path: &Path, display_name: Option<String>) -> AppResult<LibraryRoot> {
        self.add_root_with_mode(path, display_name, LibraryRecognitionMode::Folder)
    }

    #[cfg(test)]
    pub fn add_root_with_mode(
        &self,
        path: &Path,
        display_name: Option<String>,
        recognition_mode: LibraryRecognitionMode,
    ) -> AppResult<LibraryRoot> {
        self.add_root_with_kind(
            path,
            display_name,
            LibraryMediaKind::Video,
            recognition_mode,
        )
    }

    #[cfg(test)]
    pub fn add_root_with_kind(
        &self,
        path: &Path,
        display_name: Option<String>,
        media_kind: LibraryMediaKind,
        recognition_mode: LibraryRecognitionMode,
    ) -> AppResult<LibraryRoot> {
        self.add_root_with_policy(
            path,
            display_name,
            media_kind,
            recognition_mode,
            !media_kind.is_book(),
        )
    }

    #[cfg(test)]
    pub fn add_root_with_policy(
        &self,
        path: &Path,
        display_name: Option<String>,
        media_kind: LibraryMediaKind,
        recognition_mode: LibraryRecognitionMode,
        auto_bangumi: bool,
    ) -> AppResult<LibraryRoot> {
        self.add_root_with_strategy(
            path,
            display_name,
            media_kind,
            recognition_mode,
            auto_bangumi,
            "LEGACY",
        )
    }

    pub fn add_root_with_strategy(
        &self,
        path: &Path,
        display_name: Option<String>,
        media_kind: LibraryMediaKind,
        recognition_mode: LibraryRecognitionMode,
        auto_bangumi: bool,
        strategy: &str,
    ) -> AppResult<LibraryRoot> {
        if !matches!(strategy, "LEGACY" | "SMART_MIXED")
            || strategy == "SMART_MIXED"
                && (!media_kind.is_book()
                    || !matches!(recognition_mode, LibraryRecognitionMode::Folder))
        {
            return Err("INVALID_BOOK_ORGANIZATION_STRATEGY".into());
        }
        let canonical = canonical_library_root(path)?;
        let normalized = display_path(&canonical);
        let derived_name = Path::new(&normalized)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&normalized)
            .to_string();
        let name = display_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(derived_name);
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        ensure_root_does_not_overlap_conn(&transaction, &canonical, None)?;
        transaction
            .execute(
                "INSERT INTO library_roots(path, display_name, recognition_mode, media_kind, book_library_kind, video_subject_scope, doujin_library, artbook_library, auto_bangumi, book_organization_strategy) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![normalized, name, recognition_mode.as_db(), if media_kind.is_book() { "COMIC" } else { "VIDEO" }, if media_kind == LibraryMediaKind::Ebook { "EBOOK" } else { "COMIC" }, match media_kind { LibraryMediaKind::Animation => "ANIMATION", LibraryMediaKind::LiveAction => "LIVE_ACTION", _ => "MIXED" }, media_kind == LibraryMediaKind::Doujin,media_kind == LibraryMediaKind::Artbook, auto_bangumi, strategy],
            )
            .map_err(|error| {
                if error.to_string().contains("UNIQUE") {
                    "该资源目录已经添加。".to_string()
                } else {
                    db_error(error)
                }
            })?;
        let root_id = transaction.last_insert_rowid();
        transaction.commit().map_err(db_error)?;
        get_root_conn(&connection, root_id)
    }

    /// Resolves a proposed Library Root and rejects any path that is equal to, contains, or is
    /// contained by a registered root. `add_root` repeats this inside an immediate transaction;
    /// this public check lets the Tauri command fail before doing other setup work.
    pub fn validate_new_root_path(&self, path: &Path) -> AppResult<PathBuf> {
        let canonical = canonical_library_root(path)?;
        let connection = self.connect()?;
        ensure_root_does_not_overlap_conn(&connection, &canonical, None)?;
        Ok(canonical)
    }

    /// Defense for legacy databases and forged scan targets. A scan may proceed only when its
    /// root still resolves to the registered directory and does not overlap another root.
    pub fn validate_scan_root(&self, root: &LibraryRoot) -> AppResult<PathBuf> {
        let supplied = canonical_library_root(Path::new(&root.path))?;
        let connection = self.connect()?;
        let registered = connection
            .query_row(
                "SELECT path FROM library_roots WHERE id=?1",
                [root.id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| "资源库不存在。".to_string())?;
        let registered = canonical_library_root(Path::new(&registered))?;
        if !paths_equal_for_library_roots(&supplied, &registered) {
            return Err("扫描目标与已登记的资源库路径不一致，已拒绝扫描。".into());
        }
        ensure_root_does_not_overlap_conn(&connection, &registered, Some(root.id))?;
        Ok(registered)
    }

    /// Presentation preference only; retain the real scan outcome and successful baseline.
    pub fn set_scan_warnings_ignored(&self, root_id: i64, ignored: bool) -> AppResult<LibraryRoot> {
        let mut connection = self.connect()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        get_root_conn(&tx, root_id)?;
        tx.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![format!("library_scan_warnings_ignored:{root_id}"), if ignored { "true" } else { "false" }]).map_err(db_error)?;
        let mut root = get_root_conn(&tx, root_id)?;
        root.scan_health = read_scan_health_conn(&tx, root_id)?;
        tx.commit().map_err(db_error)?;
        Ok(root)
    }

    pub fn remove_root(&self, root_id: i64) -> AppResult<()> {
        let mut connection = self.connect()?;
        let tx = connection.transaction().map_err(db_error)?;
        let changed = tx
            .execute("DELETE FROM library_roots WHERE id=?1", [root_id])
            .map_err(db_error)?;
        if changed == 0 {
            return Err("资源库不存在。".into());
        }
        tx.execute(
            "DELETE FROM settings WHERE key=?1",
            [format!("library_scan_warnings_ignored:{root_id}")],
        )
        .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        Ok(())
    }

    pub fn hidden_root_node_id(&self, root: &LibraryRoot) -> AppResult<Option<i64>> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT id FROM nodes WHERE library_root_id=?1 AND absolute_path=?2 COLLATE NOCASE",
                params![root.id, root.path],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)
    }

    pub fn get_node(&self, node_id: i64) -> AppResult<MediaNode> {
        let connection = self.connect()?;
        get_node_conn(&connection, node_id)
    }

    /// Explicitly ignored entries at every depth, including independently ignored children.
    /// Read the index even when the source drive is disconnected.
    pub fn list_hidden_nodes(&self) -> AppResult<Vec<MediaNode>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(&format!("{} WHERE n.node_type = 'IGNORED'", node_select()))
            .map_err(db_error)?;
        let mut nodes = statement
            .query_map([], node_from_row)
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        hydrate_nodes_metadata_conn(&connection, &mut nodes)?;
        nodes.sort_by(|left, right| {
            natural_cmp(&left.display_name, &right.display_name)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(nodes)
    }

    #[cfg(test)]
    pub fn list_children(&self, parent_id: i64) -> AppResult<Vec<MediaNode>> {
        let connection = self.connect()?;
        list_children_conn(&connection, parent_id)
    }

    pub fn list_media(&self, node_id: i64) -> AppResult<Vec<MediaFile>> {
        let connection = self.connect()?;
        list_media_conn(&connection, node_id)
    }

    #[cfg(test)]
    pub fn list_resources(&self, node_id: i64) -> AppResult<Vec<ResourceFile>> {
        let connection = self.connect()?;
        list_resources_conn(&connection, node_id)
    }

    /// Returns the application-owned cover path and Library Root paths needed by the cover
    /// transport command using one lightweight connection.
    ///
    /// Card lists already contain the rest of the Node DTO. Re-hydrating the complete Node here
    /// and then calling the aggregate `list_roots` query used to execute several unnecessary
    /// queries for every visible poster and made startup cost grow with cached-cover count.
    pub fn cover_read_context(&self, node_id: i64) -> AppResult<(Option<PathBuf>, Vec<PathBuf>)> {
        let connection = self.connect()?;
        let cover_path = connection
            .query_row(
                "SELECT CASE WHEN n.cover_source='MANUAL' THEN n.cover_cache_path ELSE COALESCE((SELECT t.cover_cache_path FROM tmdb_movie_bindings t WHERE t.node_id=n.id AND t.active=1),n.cover_cache_path) END FROM nodes n WHERE n.id=?1",
                [node_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(db_error)?
            .map(|path| path.map(PathBuf::from))
            .ok_or_else(|| "目录节点不存在。".to_string())?;
        let mut statement = connection
            .prepare("SELECT path FROM library_roots ORDER BY id")
            .map_err(db_error)?;
        let roots = statement
            .query_map([], |row| row.get::<_, String>(0).map(PathBuf::from))
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        Ok((cover_path, roots))
    }

    /// Bounded background thumbnail backfill; no media enumeration or full Node hydration.
    pub(crate) fn poster_node_count(&self) -> AppResult<u64> {
        self.connect()?
            .query_row(
                &format!(
                    "{} SELECT COUNT(*) FROM nodes n WHERE {}",
                    crate::comics::PRESENTATION_CTE,
                    crate::comics::POSTER_ELIGIBILITY
                ),
                [],
                |row| row.get(0),
            )
            .map_err(db_error)
    }

    pub(crate) fn poster_nodes_after(&self, after: i64) -> AppResult<Vec<i64>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(&format!(
                "{} SELECT n.id FROM nodes n WHERE n.id>?1 AND {} ORDER BY n.id LIMIT 128",
                crate::comics::PRESENTATION_CTE,
                crate::comics::POSTER_ELIGIBILITY
            ))
            .map_err(db_error)?;
        let rows = statement
            .query_map([after], |row| row.get(0))
            .map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
    }

    pub(crate) fn poster_cache_root(&self, default: &Path) -> AppResult<PathBuf> {
        let connection = self.connect()?;
        let configured: Option<String> = connection
            .query_row(
                "SELECT value FROM settings WHERE key='cover_cache_directory'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_error)?;
        Ok(configured
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| default.into()))
    }

    pub(crate) fn library_paths(&self) -> AppResult<Vec<PathBuf>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare("SELECT path FROM library_roots")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0).map(PathBuf::from))
            .map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
    }

    /// Returns the union of every library's visible top-level cards. Descendants remain
    /// reachable through their parent and are deliberately not flattened into this collection.
    pub fn list_all_resources(&self) -> AppResult<AllResourcesResult> {
        self.read_snapshot(|connection| {
            let mut statement = connection
                .prepare(&format!(
                    "{} WHERE n.node_type <> 'IGNORED' AND (n.total_video_count > 0 OR n.total_comic_book_count > 0)
                 AND (n.parent_node_id IN (
                     SELECT hidden.id FROM nodes hidden
                     WHERE hidden.parent_node_id IS NULL
                 ))",
                    node_select()
                ))
                .map_err(db_error)?;
            let mut nodes = statement
                .query_map([], node_from_row)
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?;
            nodes.retain(|n| ensure_node_visible_conn(connection, n.id).is_ok());
            hydrate_nodes_metadata_conn(connection, &mut nodes)?;
            nodes.sort_by(|left, right| {
                natural_cmp(&left.display_name, &right.display_name)
                    .then_with(|| left.library_root_id.cmp(&right.library_root_id))
                    .then_with(|| left.id.cmp(&right.id))
            });
            let mut book_libraries = Vec::new();
            let mut roots_statement = connection.prepare("SELECT id FROM library_roots WHERE media_kind='COMIC'").map_err(db_error)?;
            let root_ids = roots_statement.query_map([], |r| r.get::<_, i64>(0)).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            for root_id in root_ids {
                let root = get_root_conn(connection, root_id)?;
                let catalogue = if root.book_organization_strategy == "SMART_MIXED" {
                    nodes.retain(|n| n.library_root_id != root_id);
                    crate::smart_mixed::catalogue_conn(connection, &root)?
                } else {
                    // A hidden Root owns loose FOLDER-mode books; it is never a Work card.
                    let mut books = connection.prepare(&format!("{} JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=?1 AND n.parent_node_id IS NULL AND n.node_type<>'IGNORED' AND b.source_resource_id IS NULL", crate::comics::book_select())).map_err(db_error)?;
                    let direct = books.query_map([root_id], crate::comics::book_from_row).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
                    if direct.is_empty() { continue; }
                    crate::smart_mixed::Catalogue { status: "PHYSICAL".into(), revision: 0, groups: Vec::new(), directories: Vec::new(), fallback_books: direct, directory_nodes: Vec::new() }
                };
                book_libraries.push(AllBookLibrary { root, catalogue });
            }
            let index = crate::logical_works::LogicalWorkIndex::load(connection)?;
            let mut comic_statement = connection.prepare(&format!("{} WHERE n.parent_node_id IS NOT NULL AND n.direct_comic_book_count>0 AND n.node_type IN ('WORK','AUTO_WORK','MIXED') AND n.library_root_id NOT IN (SELECT id FROM library_roots WHERE book_organization_strategy='SMART_MIXED')", node_select())).map_err(db_error)?;
            let mut comic_nodes = comic_statement.query_map([], node_from_row).map_err(db_error)?.collect::<Result<Vec<_>,_>>().map_err(db_error)?;
            comic_nodes.retain(|n| ensure_node_visible_conn(connection,n.id).is_ok());
            hydrate_nodes_metadata_conn(connection, &mut comic_nodes)?;
            Ok(AllResourcesResult {
                total_count: nodes.len() as i64 + book_libraries.iter().map(|b| b.catalogue.top_level_count(&b.root) as i64).sum::<i64>(),
                book_libraries,
                comic_nodes,
                recognition_warnings: index
                    .warnings
                    .iter()
                    .filter_map(|id| index.nodes.get(id))
                    .cloned()
                    .collect(),
                nodes,
                works: crate::works::groups_from_index(connection, &index)?,
            })
        })
    }

    #[cfg(test)]
    pub fn list_work_sources(&self) -> AppResult<Vec<MediaNode>> {
        self.read_snapshot(|connection| {
            let index = crate::logical_works::LogicalWorkIndex::load(connection)?;
            index.sources(connection)
        })
    }

    /// Records a successfully launched video against its owning Node. Repeated playback keeps
    /// one history row and advances both its timestamp and count atomically.
    pub fn record_node_watched(&self, node_id: i64) -> AppResult<()> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO watch_history(node_id,last_watched_at,watch_count)
                 VALUES(?1,strftime('%Y-%m-%dT%H:%M:%fZ','now'),1)
                 ON CONFLICT(node_id) DO UPDATE SET
                    last_watched_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                    watch_count=watch_history.watch_count+1",
                [node_id],
            )
            .map_err(db_error)?;
        Ok(())
    }

    /// Returns video Nodes and individual books across every Library Root, newest first. History for deleted Nodes
    /// is removed by the migration's foreign key; ignored Nodes remain stored but are hidden so
    /// restoring their type can make the prior local history visible again.
    pub fn list_recently_watched(&self) -> AppResult<Vec<RecentlyWatchedEntry>> {
        self.read_snapshot(|connection| {
        let records = {
            let mut statement = connection
                .prepare(
                    "WITH activity(node_id,stamp,book_id) AS (
                       SELECT node_id,last_watched_at,NULL FROM watch_history
                       UNION ALL SELECT b.node_id,p.last_read_at,b.id FROM comic_reading_progress p JOIN comic_books b ON b.id=p.comic_book_id
                     ), ranked AS (SELECT *,ROW_NUMBER() OVER(PARTITION BY node_id,book_id ORDER BY stamp DESC) AS rank FROM activity)
                     SELECT a.node_id,a.stamp,a.book_id FROM ranked a JOIN nodes n ON n.id=a.node_id
                     WHERE a.rank=1 AND n.node_type<>'IGNORED' ORDER BY a.stamp DESC,a.node_id DESC,a.book_id DESC",
                )
                .map_err(db_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                })
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?;
            rows
        };

        records
            .into_iter()
            .filter(|(node_id, _, _)| ensure_node_visible_conn(connection, *node_id).is_ok())
            .map(|(node_id, watched_at, comic_book_id)| {
                Ok(RecentlyWatchedEntry {
                    comic_book: comic_book_id.map(|id| connection.query_row(&format!("{} WHERE b.id=?1", crate::comics::book_select()), [id], crate::comics::book_from_row).map_err(db_error)).transpose()?,
                    comic_book_id,
                    node: get_node_conn(connection, node_id)?,
                    watched_at,
                })
            })
            .collect()
        })
    }

    pub fn search(&self, query: &str, root_id: Option<i64>) -> AppResult<Vec<SearchHit>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        if query.chars().count() > MAX_SEARCH_QUERY_CHARS {
            return Err(format!(
                "搜索内容不能超过 {MAX_SEARCH_QUERY_CHARS} 个字符。"
            ));
        }
        let mut connection = self.connect()?;
        // Keep the base search rows and their hydrated metadata on one SQLite snapshot. WAL mode
        // still permits concurrent writers while preventing a binding/tag change between the
        // result query and the bounded batch hydration from producing a torn DTO.
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(db_error)?;
        let pattern = format!("%{}%", escape_like(query));
        let root_filter = root_id.unwrap_or(-1);
        let mut hits = Vec::new();

        let mut nodes = {
            let mut node_statement = transaction
                .prepare(&format!(
                    "{} WHERE n.node_type <> 'IGNORED' AND NOT EXISTS (SELECT 1 FROM library_roots hidden_root WHERE hidden_root.id=n.library_root_id AND n.parent_node_id IS NULL AND n.absolute_path=hidden_root.path COLLATE NOCASE)
                     AND (?2 < 0 OR n.library_root_id=?2)
                     AND (n.display_name LIKE ?1 ESCAPE '\\' OR n.folder_name LIKE ?1 ESCAPE '\\'
                          OR EXISTS (
                              SELECT 1 FROM node_tags nt JOIN tags t ON t.id=nt.tag_id
                              WHERE nt.node_id=n.id AND t.name LIKE ?1 ESCAPE '\\'
                          )
                          OR EXISTS (
                              SELECT 1 FROM metadata_bindings b
                              WHERE b.node_id=n.id AND b.provider='BANGUMI'
                                AND (b.provider_title LIKE ?1 ESCAPE '\\'
                                     OR b.provider_title_cn LIKE ?1 ESCAPE '\\'
                                     OR b.provider_title_en LIKE ?1 ESCAPE '\\'
                                     OR b.provider_title_ja LIKE ?1 ESCAPE '\\'
                                     OR b.provider_title_ko LIKE ?1 ESCAPE '\\'
                                     OR EXISTS (SELECT 1 FROM json_each(b.provider_aliases_json) alias WHERE alias.value LIKE ?1 ESCAPE '\\'))
                          ))
                     LIMIT 200",
                    node_select()
                ))
                .map_err(db_error)?;
            let rows = node_statement
                .query_map(params![pattern, root_filter], node_from_row)
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?;
            rows
        };
        let node_hit_ids = nodes.iter().map(|node| node.id).collect::<Vec<_>>();
        let mut book_statement=transaction.prepare(&format!("{} JOIN nodes n ON n.id=b.node_id WHERE n.node_type<>'IGNORED' AND (?2<0 OR n.library_root_id=?2) AND b.display_name LIKE ?1 ESCAPE '\\' LIMIT 200",crate::comics::book_select())).map_err(db_error)?;
        let books = book_statement
            .query_map(params![pattern, root_filter], crate::comics::book_from_row)
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;

        let files = {
            let mut file_statement = transaction
                .prepare(&format!(
                    "SELECT {} FROM media_files f
                     JOIN nodes n ON n.id=f.node_id
                     WHERE n.node_type <> 'IGNORED' AND (?2 < 0 OR n.library_root_id=?2)
                     AND f.file_name LIKE ?1 ESCAPE '\\' LIMIT 200",
                    media_columns("f")
                ))
                .map_err(db_error)?;
            let rows = file_statement
                .query_map(params![pattern, root_filter], media_from_row)
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?;
            rows
        };

        // File hits can repeat the same owning Node many times. Load each missing Node row once,
        // then hydrate the union of direct Node hits and file owners in bounded batches.
        let direct_node_ids = nodes
            .iter()
            .map(|node| (node.id, ()))
            .collect::<HashMap<_, _>>();
        let mut missing_file_node_ids = files
            .iter()
            .map(|file| file.node_id)
            .chain(books.iter().map(|b| b.node_id))
            .filter(|node_id| !direct_node_ids.contains_key(node_id))
            .collect::<Vec<_>>();
        missing_file_node_ids.sort_unstable();
        missing_file_node_ids.dedup();
        nodes.extend(load_node_rows_by_ids_conn(
            &transaction,
            &missing_file_node_ids,
        )?);
        hydrate_nodes_metadata_conn(&transaction, &mut nodes)?;
        let hydrated_nodes = nodes
            .into_iter()
            .map(|node| (node.id, node))
            .collect::<HashMap<_, _>>();

        for node_id in node_hit_ids {
            let node = hydrated_nodes
                .get(&node_id)
                .cloned()
                .ok_or_else(|| db_error(rusqlite::Error::QueryReturnedNoRows))?;
            if books.iter().any(|b| b.node_id == node.id) && node.total_comic_book_count == 1 {
                continue;
            }
            hits.push(SearchHit {
                comic_book: None,
                kind: SearchHitKind::Node,
                node,
                media_file: None,
            });
        }
        for media_file in files {
            let node = hydrated_nodes
                .get(&media_file.node_id)
                .cloned()
                .ok_or_else(|| db_error(rusqlite::Error::QueryReturnedNoRows))?;
            hits.push(SearchHit {
                comic_book: None,
                kind: SearchHitKind::MediaFile,
                node,
                media_file: Some(media_file),
            });
        }
        for book in books {
            if ensure_node_visible_conn(&transaction, book.node_id).is_err() {
                continue;
            }
            let node = hydrated_nodes
                .get(&book.node_id)
                .cloned()
                .ok_or_else(|| db_error(rusqlite::Error::QueryReturnedNoRows))?;
            hits.push(SearchHit {
                kind: SearchHitKind::ComicBook,
                node,
                media_file: None,
                comic_book: Some(book),
            });
        }
        hits.sort_by(|left, right| {
            natural_cmp(
                left.comic_book
                    .as_ref()
                    .map_or(&left.node.display_name, |b| &b.display_name),
                right
                    .comic_book
                    .as_ref()
                    .map_or(&right.node.display_name, |b| &b.display_name),
            )
            .then_with(|| {
                let left_file = left
                    .media_file
                    .as_ref()
                    .map(|f| f.file_name.as_str())
                    .unwrap_or("");
                let right_file = right
                    .media_file
                    .as_ref()
                    .map(|f| f.file_name.as_str())
                    .unwrap_or("");
                natural_cmp(left_file, right_file)
            })
        });
        hits.truncate(300);
        drop(book_statement);
        transaction.commit().map_err(db_error)?;
        Ok(hits)
    }

    pub fn list_unbound_bangumi_candidates(&self, root_id: i64) -> AppResult<Vec<MediaNode>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(&format!(
                "{} WHERE n.library_root_id=?1 AND (EXISTS(SELECT 1 FROM library_roots r WHERE r.id=n.library_root_id AND r.auto_bangumi=1) OR EXISTS(SELECT 1 FROM metadata_bindings b WHERE b.node_id=n.id))
                 AND (
                     EXISTS (SELECT 1 FROM tmdb_movie_bindings t WHERE t.node_id=n.id AND t.active=1)
                     OR
                     NOT EXISTS (
                         SELECT 1 FROM metadata_bindings b
                         WHERE b.node_id=n.id AND b.provider='BANGUMI'
                     )
                     OR (
                         n.cover_source <> 'MANUAL'
                         AND EXISTS (
                             SELECT 1 FROM metadata_bindings b
                             WHERE b.node_id=n.id AND b.provider='BANGUMI'
                               AND b.provider_image_url IS NOT NULL
                               AND trim(b.provider_image_url) <> ''
                         )
                     )
                 )
                 AND NOT EXISTS (
                     WITH RECURSIVE ancestors(id,parent_node_id,node_type) AS (
                         SELECT p.id,p.parent_node_id,p.node_type
                         FROM nodes p WHERE p.id=n.parent_node_id
                         UNION ALL
                         SELECT p.id,p.parent_node_id,p.node_type
                         FROM nodes p JOIN ancestors a ON p.id=a.parent_node_id
                     )
                     SELECT 1 FROM ancestors WHERE node_type='IGNORED'
                 )
                 AND (
                     n.node_type IN ('AUTO_WORK','WORK')
                     OR (n.node_type='CONTAINER' AND n.parent_node_id IS NOT NULL
                         AND EXISTS (SELECT 1 FROM library_roots r WHERE r.id=n.library_root_id AND r.media_kind='COMIC'))
                 )
                 ORDER BY n.id",
                node_select()
            ))
            .map_err(db_error)?;
        let nodes = statement
            .query_map([root_id], node_from_row)
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        let nodes = filter_missing_bound_cover_candidates(&connection, nodes)?;
        filter_structural_supplementary_match_candidates(&connection, nodes)
    }

    /// Lists indexed Nodes that are eligible for an explicit Bangumi matching pass.
    ///
    /// `None` selects the full database. A supplied ID list is normalized and bounded, then acts
    /// only as a scope filter: existing but ineligible/ignored Nodes are deliberately omitted.
    /// This query never mutates bindings. Callers decide whether the returned existing bindings
    /// may be replaced, and all actual writes remain subject-conditional/transactional elsewhere.
    pub fn list_bangumi_match_candidates(
        &self,
        node_ids: Option<&[i64]>,
        include_bound: bool,
    ) -> AppResult<Vec<MediaNode>> {
        let normalized_ids = node_ids.map(normalize_batch_node_ids).transpose()?;
        let id_filter = normalized_ids
            .as_ref()
            .map(|ids| format!(" AND n.id IN ({})", sql_placeholders(ids.len())))
            .unwrap_or_default();
        let policy_filter = if include_bound {
            ""
        } else {
            " AND (EXISTS(SELECT 1 FROM library_roots r WHERE r.id=n.library_root_id AND r.auto_bangumi=1) OR EXISTS(SELECT 1 FROM metadata_bindings b WHERE b.node_id=n.id))"
        };
        let binding_filter = if include_bound {
            ""
        } else {
            " AND (
                 EXISTS (SELECT 1 FROM tmdb_movie_bindings t WHERE t.node_id=n.id AND t.active=1)
                 OR
                 NOT EXISTS (
                     SELECT 1 FROM metadata_bindings b
                     WHERE b.node_id=n.id AND b.provider='BANGUMI'
                 )
                 OR (
                     n.cover_source <> 'MANUAL'
                     AND EXISTS (
                         SELECT 1 FROM metadata_bindings b
                         WHERE b.node_id=n.id AND b.provider='BANGUMI'
                           AND b.provider_image_url IS NOT NULL
                           AND trim(b.provider_image_url) <> ''
                     )
                 )
              )"
        };
        let sql = format!(
            "{} WHERE (
                 n.node_type IN ('AUTO_WORK','WORK')
                 OR (n.node_type='CONTAINER' AND n.parent_node_id IS NOT NULL
                     AND EXISTS (SELECT 1 FROM library_roots r WHERE r.id=n.library_root_id AND r.media_kind='COMIC'))
             )
             AND NOT EXISTS (
                 WITH RECURSIVE ancestors(id,parent_node_id,node_type) AS (
                     SELECT p.id,p.parent_node_id,p.node_type
                     FROM nodes p WHERE p.id=n.parent_node_id
                     UNION ALL
                     SELECT p.id,p.parent_node_id,p.node_type
                     FROM nodes p JOIN ancestors a ON p.id=a.parent_node_id
                 )
                 SELECT 1 FROM ancestors WHERE node_type='IGNORED'
             )
             {binding_filter}{id_filter}{policy_filter}
             ORDER BY n.id",
            node_select()
        );

        let connection = self.connect()?;
        let mut statement = connection.prepare(&sql).map_err(db_error)?;
        let nodes = if let Some(ids) = normalized_ids.as_ref() {
            statement
                .query_map(rusqlite::params_from_iter(ids.iter()), node_from_row)
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?
        } else {
            statement
                .query_map([], node_from_row)
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?
        };
        let nodes = if include_bound {
            nodes
        } else {
            filter_missing_bound_cover_candidates(&connection, nodes)?
        };
        let mut nodes = filter_structural_supplementary_match_candidates(&connection, nodes)?;
        if include_bound {
            for node in &mut nodes {
                node.binding = get_binding_conn(&connection, node.id)?;
            }
        }
        Ok(nodes)
    }

    pub fn set_node_type(&self, node_id: i64, node_type: NodeType) -> AppResult<MediaNode> {
        if !matches!(
            node_type,
            NodeType::Work | NodeType::Container | NodeType::Mixed | NodeType::Ignored
        ) {
            return Err("人工类型只能设置为作品、系列、其他资源或忽略。".into());
        }
        let mut writer = self.connect()?;
        let connection = writer
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let changed = connection
            .execute(
                "UPDATE nodes SET node_type=?1, manual_type_override=1, updated_at=CURRENT_TIMESTAMP WHERE id=?2",
                params![node_type.as_db(), node_id],
            )
            .map_err(db_error)?;
        if changed == 0 {
            return Err("目录节点不存在。".into());
        }
        crate::logical_works::LogicalWorkIndex::reclassify_related(&connection, &[node_id], true)?;
        crate::comics::refresh_related(&connection, &[node_id])?;
        let result = get_node_conn(&connection, node_id)?;
        connection.commit().map_err(db_error)?;
        Ok(result)
    }

    pub fn batch_set_node_type(
        &self,
        node_ids: &[i64],
        node_type: NodeType,
    ) -> AppResult<BatchMutationResult> {
        if !matches!(
            node_type,
            NodeType::Work | NodeType::Container | NodeType::Mixed | NodeType::Ignored
        ) {
            return Err("人工类型只能设置为作品、系列、其他资源或忽略。".into());
        }
        let node_ids = normalize_batch_node_ids(node_ids)?;
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        ensure_batch_nodes_exist_conn(&transaction, &node_ids)?;
        let mut statement = transaction
            .prepare(
                "UPDATE nodes SET node_type=?1,manual_type_override=1,
                 updated_at=CURRENT_TIMESTAMP WHERE id=?2",
            )
            .map_err(db_error)?;
        let mut updated = 0_usize;
        for node_id in &node_ids {
            updated += statement
                .execute(params![node_type.as_db(), node_id])
                .map_err(db_error)?;
        }
        drop(statement);
        crate::logical_works::LogicalWorkIndex::reclassify_related(&transaction, &node_ids, true)?;
        crate::comics::refresh_related(&transaction, &node_ids)?;
        transaction.commit().map_err(db_error)?;
        Ok(BatchMutationResult {
            requested: node_ids.len() as u64,
            updated: updated as u64,
            skipped: node_ids.len() as u64 - updated as u64,
        })
    }

    pub fn reset_node_type(&self, node_id: i64) -> AppResult<MediaNode> {
        let mut writer = self.connect()?;
        let connection = writer
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let node = get_node_conn(&connection, node_id)?;
        let root = get_root_conn(&connection, node.library_root_id)?;
        if root.media_kind.is_book() {
            connection
                .execute(
                    "UPDATE nodes SET manual_type_override=0 WHERE id=?1",
                    [node_id],
                )
                .map_err(db_error)?;
            crate::comics::refresh_related(&connection, &[node_id])?;
            let result = get_node_conn(&connection, node_id)?;
            connection.commit().map_err(db_error)?;
            return Ok(result);
        }
        let has_bdmv =
            crate::scanner::has_typical_bdmv(Path::new(&node.absolute_path), Path::new(&root.path));
        let direct_video_count = connection
            .query_row(
                "SELECT COUNT(*) FROM media_files WHERE node_id=?1",
                [node_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(db_error)?;
        let child_summary = crate::scanner::indexed_child_media_summary(&connection, node_id)?;
        let child_media_branch_count = child_summary.branch_count;
        let total_video_count = direct_video_count + child_summary.total_videos;
        let node_type = crate::scanner::classify_directory(
            direct_video_count,
            child_media_branch_count,
            child_summary.supplementary_branch_count,
            has_bdmv,
            false,
        );
        connection
            .execute(
                "UPDATE nodes SET node_type=?1,manual_type_override=0,direct_video_count=?2,
                 child_media_branch_count=?3,total_video_count=?4,updated_at=CURRENT_TIMESTAMP WHERE id=?5",
                params![
                    node_type.as_db(),
                    direct_video_count,
                    child_media_branch_count,
                    total_video_count,
                    node_id
                ],
            )
            .map_err(db_error)?;
        crate::logical_works::LogicalWorkIndex::reclassify_related(&connection, &[node_id], true)?;
        let result = get_node_conn(&connection, node_id)?;
        connection.commit().map_err(db_error)?;
        Ok(result)
    }

    pub fn batch_reset_node_type(&self, node_ids: &[i64]) -> AppResult<BatchMutationResult> {
        let node_ids = normalize_batch_node_ids(node_ids)?;
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        ensure_batch_nodes_exist_conn(&transaction, &node_ids)?;
        let mut updated = 0_usize;
        for node_id in &node_ids {
            if get_node_conn(&transaction, *node_id)?.media_kind.is_book() {
                transaction
                    .execute(
                        "UPDATE nodes SET manual_type_override=0 WHERE id=?1",
                        [node_id],
                    )
                    .map_err(db_error)?;
                crate::comics::refresh_related(&transaction, &[*node_id])?;
                updated += 1;
                continue;
            }
            let (absolute_path, root_path) = transaction
                .query_row(
                    "SELECT n.absolute_path,r.path
                     FROM nodes n
                     JOIN library_roots r ON r.id=n.library_root_id
                     WHERE n.id=?1",
                    [node_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .map_err(db_error)?;
            let has_bdmv =
                crate::scanner::has_typical_bdmv(Path::new(&absolute_path), Path::new(&root_path));
            let direct_video_count = transaction
                .query_row(
                    "SELECT COUNT(*) FROM media_files WHERE node_id=?1",
                    [node_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(db_error)?;
            let child_summary =
                crate::scanner::indexed_child_media_summary(&transaction, *node_id)?;
            let child_media_branch_count = child_summary.branch_count;
            let total_video_count = direct_video_count + child_summary.total_videos;
            let node_type = crate::scanner::classify_directory(
                direct_video_count,
                child_media_branch_count,
                child_summary.supplementary_branch_count,
                has_bdmv,
                false,
            );
            updated += transaction
                .execute(
                    "UPDATE nodes SET node_type=?1,manual_type_override=0,
                     direct_video_count=?2,child_media_branch_count=?3,total_video_count=?4,
                     updated_at=CURRENT_TIMESTAMP WHERE id=?5",
                    params![
                        node_type.as_db(),
                        direct_video_count,
                        child_media_branch_count,
                        total_video_count,
                        node_id
                    ],
                )
                .map_err(db_error)?;
        }
        crate::logical_works::LogicalWorkIndex::reclassify_related(&transaction, &node_ids, true)?;
        transaction.commit().map_err(db_error)?;
        Ok(BatchMutationResult {
            requested: node_ids.len() as u64,
            updated: updated as u64,
            skipped: node_ids.len() as u64 - updated as u64,
        })
    }

    pub fn set_display_name(
        &self,
        node_id: i64,
        display_name: Option<String>,
    ) -> AppResult<MediaNode> {
        let node = self.get_node(node_id)?;
        let value = display_name
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or(node.folder_name);
        if value.chars().count() > MAX_DISPLAY_NAME_CHARS {
            return Err(format!(
                "作品显示名称不能超过 {MAX_DISPLAY_NAME_CHARS} 个字符。"
            ));
        }
        let connection = self.connect()?;
        connection
            .execute(
                "UPDATE nodes SET display_name=?1, updated_at=CURRENT_TIMESTAMP WHERE id=?2",
                params![value, node_id],
            )
            .map_err(db_error)?;
        drop(connection);
        self.get_node(node_id)
    }

    pub fn list_user_tags(&self, node_id: i64) -> AppResult<Vec<UserTagMembership>> {
        let connection = self.connect()?;
        ensure_node_exists_conn(&connection, node_id)?;
        let mut statement = connection
            .prepare(
                "SELECT t.id,t.name,t.created_at,t.updated_at,
                        EXISTS(SELECT 1 FROM node_tags nt WHERE nt.node_id=?1 AND nt.tag_id=t.id)
                 FROM tags t ORDER BY t.name COLLATE NOCASE,t.id",
            )
            .map_err(db_error)?;
        let mut tags = statement
            .query_map([node_id], user_tag_membership_from_row)
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        tags.sort_by(|left, right| natural_cmp(&left.name, &right.name));
        Ok(tags)
    }

    pub fn create_or_assign_user_tag(&self, node_id: i64, name: &str) -> AppResult<UserTag> {
        let (name, normalized_name) = normalize_tag_name(name)?;
        let mut connection = self.connect()?;
        ensure_node_exists_conn(&connection, node_id)?;
        let transaction = connection.transaction().map_err(db_error)?;
        transaction
            .execute(
                "INSERT INTO tags(name,normalized_name) VALUES(?1,?2)
                 ON CONFLICT(normalized_name) DO NOTHING",
                params![name, normalized_name],
            )
            .map_err(db_error)?;
        let tag = transaction
            .query_row(
                "SELECT id,name,created_at,updated_at FROM tags WHERE normalized_name=?1",
                [normalized_name],
                user_tag_from_row,
            )
            .map_err(db_error)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO node_tags(node_id,tag_id) VALUES(?1,?2)",
                params![node_id, tag.id],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)?;
        Ok(tag)
    }

    pub fn assign_user_tag(&self, node_id: i64, tag_id: i64) -> AppResult<UserTag> {
        let connection = self.connect()?;
        ensure_node_exists_conn(&connection, node_id)?;
        let tag = get_user_tag_conn(&connection, tag_id)?;
        connection
            .execute(
                "INSERT OR IGNORE INTO node_tags(node_id,tag_id) VALUES(?1,?2)",
                params![node_id, tag_id],
            )
            .map_err(db_error)?;
        Ok(tag)
    }

    pub fn batch_assign_tag(
        &self,
        node_ids: &[i64],
        tag_id: i64,
    ) -> AppResult<BatchMutationResult> {
        let node_ids = normalize_batch_node_ids(node_ids)?;
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        ensure_batch_nodes_exist_conn(&transaction, &node_ids)?;
        get_user_tag_conn(&transaction, tag_id)?;
        let mut statement = transaction
            .prepare("INSERT OR IGNORE INTO node_tags(node_id,tag_id) VALUES(?1,?2)")
            .map_err(db_error)?;
        let mut updated = 0_usize;
        for node_id in &node_ids {
            updated += statement
                .execute(params![node_id, tag_id])
                .map_err(db_error)?;
        }
        drop(statement);
        transaction.commit().map_err(db_error)?;
        Ok(BatchMutationResult {
            requested: node_ids.len() as u64,
            updated: updated as u64,
            skipped: node_ids.len() as u64 - updated as u64,
        })
    }

    pub fn batch_create_and_assign_tag(
        &self,
        node_ids: &[i64],
        name: &str,
    ) -> AppResult<BatchMutationResult> {
        let node_ids = normalize_batch_node_ids(node_ids)?;
        let (name, normalized_name) = normalize_tag_name(name)?;
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        ensure_batch_nodes_exist_conn(&transaction, &node_ids)?;
        transaction
            .execute(
                "INSERT INTO tags(name,normalized_name) VALUES(?1,?2)
                 ON CONFLICT(normalized_name) DO NOTHING",
                params![name, normalized_name],
            )
            .map_err(db_error)?;
        let tag_id = transaction
            .query_row(
                "SELECT id FROM tags WHERE normalized_name=?1",
                [normalized_name],
                |row| row.get::<_, i64>(0),
            )
            .map_err(db_error)?;
        let mut statement = transaction
            .prepare("INSERT OR IGNORE INTO node_tags(node_id,tag_id) VALUES(?1,?2)")
            .map_err(db_error)?;
        let mut updated = 0_usize;
        for node_id in &node_ids {
            updated += statement
                .execute(params![node_id, tag_id])
                .map_err(db_error)?;
        }
        drop(statement);
        transaction.commit().map_err(db_error)?;
        Ok(BatchMutationResult {
            requested: node_ids.len() as u64,
            updated: updated as u64,
            skipped: node_ids.len() as u64 - updated as u64,
        })
    }

    pub fn rename_user_tag(&self, tag_id: i64, name: &str) -> AppResult<UserTag> {
        let (name, normalized_name) = normalize_tag_name(name)?;
        let connection = self.connect()?;
        get_user_tag_conn(&connection, tag_id)?;
        connection
            .execute(
                "UPDATE tags SET name=?1,normalized_name=?2,updated_at=CURRENT_TIMESTAMP WHERE id=?3",
                params![name, normalized_name, tag_id],
            )
            .map_err(|error| {
                if error.to_string().contains("UNIQUE") {
                    "已存在同名标签。".to_string()
                } else {
                    db_error(error)
                }
            })?;
        get_user_tag_conn(&connection, tag_id)
    }

    pub fn unassign_user_tag(&self, node_id: i64, tag_id: i64) -> AppResult<()> {
        let connection = self.connect()?;
        ensure_node_exists_conn(&connection, node_id)?;
        get_user_tag_conn(&connection, tag_id)?;
        connection
            .execute(
                "DELETE FROM node_tags WHERE node_id=?1 AND tag_id=?2",
                params![node_id, tag_id],
            )
            .map_err(db_error)?;
        Ok(())
    }

    pub fn delete_user_tag(&self, tag_id: i64) -> AppResult<()> {
        let connection = self.connect()?;
        let changed = connection
            .execute("DELETE FROM tags WHERE id=?1", [tag_id])
            .map_err(db_error)?;
        if changed == 0 {
            return Err("标签不存在。".into());
        }
        Ok(())
    }

    pub fn list_favorite_folders(&self) -> AppResult<Vec<FavoriteFolder>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT f.id,f.name,COUNT(nff.node_id),f.created_at,f.updated_at
                 FROM favorite_folders f
                 LEFT JOIN node_favorite_folders nff ON nff.folder_id=f.id
                 GROUP BY f.id,f.name,f.created_at,f.updated_at
                 ORDER BY f.name COLLATE NOCASE,f.id",
            )
            .map_err(db_error)?;
        let mut folders = statement
            .query_map([], favorite_folder_from_row)
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        folders.sort_by(|left, right| {
            natural_cmp(&left.name, &right.name).then_with(|| left.id.cmp(&right.id))
        });
        Ok(folders)
    }

    pub fn create_favorite_folder(&self, name: &str) -> AppResult<FavoriteFolder> {
        let (name, normalized_name) = normalize_favorite_folder_name(name)?;
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO favorite_folders(name,normalized_name) VALUES(?1,?2)",
                params![name, normalized_name],
            )
            .map_err(favorite_folder_write_error)?;
        get_favorite_folder_conn(&connection, connection.last_insert_rowid())
    }

    pub fn rename_favorite_folder(&self, folder_id: i64, name: &str) -> AppResult<FavoriteFolder> {
        let (name, normalized_name) = normalize_favorite_folder_name(name)?;
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE favorite_folders
                 SET name=?1,normalized_name=?2,
                     updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
                 WHERE id=?3",
                params![name, normalized_name, folder_id],
            )
            .map_err(favorite_folder_write_error)?;
        if changed == 0 {
            return Err("收藏夹不存在。".into());
        }
        get_favorite_folder_conn(&connection, folder_id)
    }

    pub fn delete_favorite_folder(&self, folder_id: i64) -> AppResult<()> {
        let connection = self.connect()?;
        let changed = connection
            .execute("DELETE FROM favorite_folders WHERE id=?1", [folder_id])
            .map_err(db_error)?;
        if changed == 0 {
            return Err("收藏夹不存在。".into());
        }
        Ok(())
    }

    pub fn list_favorite_folder_nodes(&self, folder_id: i64) -> AppResult<Vec<MediaNode>> {
        let connection = self.connect()?;
        get_favorite_folder_conn(&connection, folder_id)?;
        let node_ids = {
            let mut statement = connection
                .prepare(
                    "SELECT node_id FROM node_favorite_folders
                     WHERE folder_id=?1
                     ORDER BY added_at DESC,node_id DESC",
                )
                .map_err(db_error)?;
            let rows = statement
                .query_map([folder_id], |row| row.get::<_, i64>(0))
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?;
            rows
        };
        node_ids
            .into_iter()
            .map(|node_id| get_node_conn(&connection, node_id))
            .collect()
    }

    pub fn batch_add_nodes_to_favorite(
        &self,
        folder_id: i64,
        node_ids: &[i64],
    ) -> AppResult<BatchMutationResult> {
        let node_ids = normalize_batch_node_ids(node_ids)?;
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        get_favorite_folder_conn(&transaction, folder_id)?;
        ensure_batch_nodes_exist_conn(&transaction, &node_ids)?;
        let mut statement = transaction
            .prepare(
                "INSERT OR IGNORE INTO node_favorite_folders(folder_id,node_id)
                 VALUES(?1,?2)",
            )
            .map_err(db_error)?;
        let mut updated = 0_usize;
        for node_id in &node_ids {
            updated += statement
                .execute(params![folder_id, node_id])
                .map_err(db_error)?;
        }
        drop(statement);
        if updated > 0 {
            transaction
                .execute(
                    "UPDATE favorite_folders
                     SET updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
                    [folder_id],
                )
                .map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)?;
        Ok(BatchMutationResult {
            requested: node_ids.len() as u64,
            updated: updated as u64,
            skipped: node_ids.len() as u64 - updated as u64,
        })
    }

    pub fn batch_remove_nodes_from_favorite(
        &self,
        folder_id: i64,
        node_ids: &[i64],
    ) -> AppResult<BatchMutationResult> {
        let node_ids = normalize_batch_node_ids(node_ids)?;
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        get_favorite_folder_conn(&transaction, folder_id)?;
        ensure_batch_nodes_exist_conn(&transaction, &node_ids)?;
        let mut statement = transaction
            .prepare("DELETE FROM node_favorite_folders WHERE folder_id=?1 AND node_id=?2")
            .map_err(db_error)?;
        let mut updated = 0_usize;
        for node_id in &node_ids {
            updated += statement
                .execute(params![folder_id, node_id])
                .map_err(db_error)?;
        }
        drop(statement);
        if updated > 0 {
            transaction
                .execute(
                    "UPDATE favorite_folders
                     SET updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
                    [folder_id],
                )
                .map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)?;
        Ok(BatchMutationResult {
            requested: node_ids.len() as u64,
            updated: updated as u64,
            skipped: node_ids.len() as u64 - updated as u64,
        })
    }

    pub fn get_binding(&self, node_id: i64) -> AppResult<Option<MetadataBinding>> {
        let connection = self.connect()?;
        get_binding_conn(&connection, node_id)
    }

    pub fn pending_alias_subjects(&self) -> AppResult<Vec<crate::models::BangumiSubject>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare("SELECT MIN(b.node_id) FROM metadata_bindings b
            WHERE b.provider='BANGUMI' AND b.provider_subject_type IN (1,2,6)
            AND NOT EXISTS(SELECT 1 FROM provider_alias_sync s WHERE s.subject_id=b.provider_subject_id AND s.subject_type=b.provider_subject_type)
            GROUP BY b.provider_subject_id,b.provider_subject_type ORDER BY MIN(b.node_id)").map_err(db_error)?;
        let ids = statement
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        ids.into_iter()
            .filter_map(|id| get_binding_conn(&connection, id).transpose())
            .map(|binding| {
                let b = binding?;
                Ok(crate::models::BangumiSubject {
                    subject_id: b.provider_subject_id,
                    subject_type: b.provider_subject_type,
                    title: b.provider_title,
                    title_cn: b.provider_title_cn,
                    title_en: b.provider_title_en,
                    title_ja: b.provider_title_ja,
                    title_ko: b.provider_title_ko,
                    match_aliases: Vec::new(),
                    date: b.provider_date,
                    image_url: b.provider_image_url,
                    summary: None,
                })
            })
            .collect()
    }

    /// Persist completion with the aliases so empty results are not repeatedly fetched.
    pub fn complete_provider_alias_sync(
        &self,
        subject: &crate::models::BangumiSubject,
    ) -> AppResult<bool> {
        let mut connection = self.connect()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let changed = tx.execute(
            "UPDATE metadata_bindings SET provider_aliases_json=?1 WHERE provider='BANGUMI' AND provider_subject_id=?2 AND provider_subject_type=?3 AND provider_aliases_json<>?1",
            params![provider_aliases_json(subject),subject.subject_id,subject.subject_type],
        ).map_err(db_error)?;
        tx.execute("INSERT INTO provider_alias_sync(subject_id,subject_type,aliases_json) VALUES(?1,?2,?3) ON CONFLICT(subject_id,subject_type) DO UPDATE SET aliases_json=excluded.aliases_json", params![subject.subject_id,subject.subject_type,provider_aliases_json(subject)]).map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        Ok(changed > 0)
    }

    /// A late detail response cannot overwrite a newer Subject choice or recreate a cleared binding.
    pub fn update_binding_if_subject(
        &self,
        node_id: i64,
        subject: &crate::models::BangumiSubject,
    ) -> AppResult<bool> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE metadata_bindings SET
                    provider_title=?1,
                    provider_title_cn=?2,
                    provider_title_en=?3,
                    provider_title_ja=?4,
                    provider_title_ko=?5,
                    provider_date=?6,
                    provider_image_url=?7,
                    provider_subject_type=?8,
                    provider_aliases_json=?11,
                    updated_at=CURRENT_TIMESTAMP
                 WHERE node_id=?9 AND provider='BANGUMI' AND provider_subject_id=?10",
                params![
                    subject.title,
                    subject.title_cn,
                    subject.title_en,
                    subject.title_ja,
                    subject.title_ko,
                    subject.date,
                    subject.image_url,
                    subject.subject_type,
                    node_id,
                    subject.subject_id,
                    provider_aliases_json(subject),
                ],
            )
            .map_err(db_error)?;
        Ok(changed > 0)
    }

    /// Persists an automatic binding only while the Node is still unbound.
    ///
    /// `ON CONFLICT DO NOTHING` makes the final check and write atomic, so a manual binding that
    /// races with an in-flight network request can never be overwritten by the automatic result.
    pub fn save_binding_if_absent(
        &self,
        node_id: i64,
        subject: &crate::models::BangumiSubject,
    ) -> AppResult<bool> {
        let node = self.get_node(node_id)?;
        if !node.media_kind.accepts_subject(subject.subject_type) || subject.subject_id <= 0 {
            return Err("BANGUMI_MEDIA_KIND_CONFLICT".into());
        }
        if !node.can_bind_bangumi() {
            return Err("只有作品或包含视频的系列可以绑定 Bangumi。".into());
        }
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "INSERT INTO metadata_bindings(
                    node_id, provider, provider_subject_id, provider_subject_type,
                    provider_title, provider_title_cn,
                    provider_title_en, provider_title_ja, provider_title_ko, provider_date,
                    provider_image_url, bound_at, updated_at, cover_download_error, provider_aliases_json
                 ) SELECT ?1, 'BANGUMI', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                    CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, NULL, COALESCE((SELECT aliases_json FROM provider_alias_sync WHERE subject_id=?2 AND subject_type=?3),?11)
                    WHERE EXISTS(SELECT 1 FROM nodes n JOIN library_roots r ON r.id=n.library_root_id WHERE n.id=?1 AND r.auto_bangumi=1)
                    AND NOT EXISTS(SELECT 1 FROM tmdb_movie_bindings t WHERE t.node_id=?1)
                 ON CONFLICT(node_id, provider) DO NOTHING",
                params![
                    node_id,
                    subject.subject_id,
                    subject.subject_type,
                    subject.title,
                    subject.title_cn,
                    subject.title_en,
                    subject.title_ja,
                    subject.title_ko,
                    subject.date,
                    subject.image_url,
                    provider_aliases_json(subject)
                ],
            )
            .map_err(db_error)?;
        Ok(changed > 0)
    }

    /// Replaces a user-confirmed binding and clears an incompatible Bangumi cover in the same
    /// immediate transaction. This prevents an automatic cover write from landing between a
    /// stale cover snapshot and the confirmed Subject update.
    #[allow(dead_code)] // Retained as the compatibility API for callers without alias evidence.
    pub fn save_confirmed_binding(
        &self,
        node_id: i64,
        subject: &crate::models::BangumiSubject,
    ) -> AppResult<Option<PathBuf>> {
        match self.save_binding_transaction(node_id, subject, None, None)? {
            ConditionalBindingSave::Applied(path) => Ok(path),
            ConditionalBindingSave::Stale => Err("无条件人工绑定不应产生并发冲突。".to_string()),
        }
    }

    /// Replaces a user-confirmed binding and the local title observations which led to that
    /// choice in one immediate transaction. Only cleaned title strings are stored; media paths
    /// never enter the alias table.
    pub fn save_confirmed_binding_with_aliases(
        &self,
        node_id: i64,
        subject: &crate::models::BangumiSubject,
        aliases: &[String],
    ) -> AppResult<Option<PathBuf>> {
        let aliases = sanitize_confirmed_title_aliases(aliases);
        if aliases.is_empty() {
            return self.save_confirmed_binding(node_id, subject);
        }
        match self.save_binding_transaction(node_id, subject, None, Some(&aliases))? {
            ConditionalBindingSave::Applied(path) => Ok(path),
            ConditionalBindingSave::Stale => Err("无条件人工绑定不应产生并发冲突。".to_string()),
        }
    }

    /// Resolves the strongest locally learned title observation whose own history agrees on one
    /// Bangumi Subject. An ambiguous higher-priority alias deliberately falls back to ordinary
    /// provider search instead of guessing from weaker observations.
    pub(crate) fn resolve_confirmed_title_alias(
        &self,
        aliases: &[String],
    ) -> AppResult<Option<ConfirmedTitleAliasMatch>> {
        let aliases = sanitize_confirmed_title_aliases(aliases);
        if aliases.is_empty() {
            return Ok(None);
        }

        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT subject_id,subject_type
                 FROM confirmed_title_aliases WHERE normalized_alias=?1",
            )
            .map_err(db_error)?;
        for (alias, normalized) in aliases {
            let observations = statement
                .query_map([normalized], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(db_error)?
                .collect::<Result<Vec<(i64, i64)>, _>>()
                .map_err(db_error)?;
            if observations.is_empty() {
                continue;
            }
            let subject = observations[0];
            if observations
                .iter()
                .any(|observation| *observation != subject)
            {
                return Ok(None);
            }
            return Ok(Some(ConfirmedTitleAliasMatch {
                subject_id: subject.0,
                subject_type: subject.1,
                matched_alias: alias,
            }));
        }
        Ok(None)
    }

    /// Replaces a binding only if it still equals the Subject observed before an explicit
    /// rematch started. The comparison and replacement share one immediate transaction, so a
    /// newer manual bind can never be overwritten in the check/write gap.
    pub fn save_rematched_binding_if_unchanged(
        &self,
        node_id: i64,
        expected_subject: Option<i64>,
        subject: &crate::models::BangumiSubject,
    ) -> AppResult<ConditionalBindingSave> {
        self.save_binding_transaction(node_id, subject, Some(expected_subject), None)
    }

    fn save_binding_transaction(
        &self,
        node_id: i64,
        subject: &crate::models::BangumiSubject,
        expected_subject: Option<Option<i64>>,
        replacement_aliases: Option<&[(String, String)]>,
    ) -> AppResult<ConditionalBindingSave> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let result = save_binding_conn(
            &transaction,
            node_id,
            subject,
            expected_subject,
            replacement_aliases,
        )?;
        if matches!(result, ConditionalBindingSave::Applied(_)) {
            crate::logical_works::LogicalWorkIndex::reclassify_related(
                &transaction,
                &[node_id],
                false,
            )?;
        }
        transaction.commit().map_err(db_error)?;
        Ok(result)
    }

    /// Updates cover status only if the binding still points at the automatic Subject and the
    /// user has not selected a manual cover while the download was running.
    pub fn set_binding_cover_error_if_subject(
        &self,
        node_id: i64,
        subject_id: i64,
        error: Option<&str>,
    ) -> AppResult<bool> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE metadata_bindings
                 SET cover_download_error=?1,updated_at=CURRENT_TIMESTAMP
                 WHERE node_id=?2 AND provider='BANGUMI' AND provider_subject_id=?3
                   AND EXISTS(
                     SELECT 1 FROM nodes n
                     WHERE n.id=metadata_bindings.node_id AND n.cover_source <> 'MANUAL'
                   )",
                params![error, node_id, subject_id],
            )
            .map_err(db_error)?;
        Ok(changed > 0)
    }

    pub fn set_node_cover(
        &self,
        node_id: i64,
        source: CoverSource,
        path: Option<&Path>,
    ) -> AppResult<()> {
        let connection = self.connect()?;
        connection
            .execute(
                "UPDATE nodes SET cover_source=?1, cover_cache_path=?2, updated_at=CURRENT_TIMESTAMP WHERE id=?3",
                params![source.as_db(), path.map(|p| p.to_string_lossy().into_owned()), node_id],
            )
            .map_err(db_error)?;
        Ok(())
    }

    /// Applies a downloaded automatic cover only while both the matching Subject binding and the
    /// absence of a manual cover are still true.
    pub fn set_bangumi_cover_for_subject_unless_manual(
        &self,
        node_id: i64,
        subject_id: i64,
        path: &Path,
    ) -> AppResult<bool> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE nodes SET cover_source='BANGUMI',cover_cache_path=?1,
                 updated_at=CURRENT_TIMESTAMP
                 WHERE id=?2 AND cover_source <> 'MANUAL'
                   AND EXISTS(
                     SELECT 1 FROM metadata_bindings b
                     WHERE b.node_id=nodes.id AND b.provider='BANGUMI'
                       AND b.provider_subject_id=?3
                   )",
                params![path.to_string_lossy(), node_id, subject_id],
            )
            .map_err(db_error)?;
        Ok(changed > 0)
    }

    pub fn cover_path_reference_count(&self, path: &Path) -> AppResult<i64> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM nodes WHERE cover_cache_path=?1 COLLATE NOCASE)+(SELECT COUNT(*) FROM tmdb_movie_bindings WHERE cover_cache_path=?1 COLLATE NOCASE)",
                [path.to_string_lossy().as_ref()],
                |row| row.get(0),
            )
            .map_err(db_error)
    }

    pub fn clear_binding(&self, node_id: i64) -> AppResult<Option<PathBuf>> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let path = clear_binding_conn(&transaction, node_id)?;
        crate::logical_works::LogicalWorkIndex::reclassify_related(
            &transaction,
            &[node_id],
            false,
        )?;
        transaction.commit().map_err(db_error)?;
        Ok(path)
    }

    pub fn validate_work_target(
        &self,
        target: &crate::models::WorkTarget,
    ) -> AppResult<Vec<MediaNode>> {
        self.read_snapshot(|connection| validate_work_target_conn(connection, target))
    }

    pub fn change_work_binding(
        &self,
        target: &crate::models::WorkTarget,
        subject: Option<&crate::models::BangumiSubject>,
    ) -> AppResult<WorkBindingChange> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let sources = validate_work_target_conn(&transaction, target)?;
        let mut paths = Vec::new();
        let index = crate::logical_works::LogicalWorkIndex::load(&transaction)?;
        for node in sources {
            let path = if let Some(subject) = subject {
                let names = index
                    .owned_nodes(node.id)
                    .into_iter()
                    .flat_map(|id| {
                        index
                            .videos
                            .get(&id)
                            .into_iter()
                            .flatten()
                            .map(|file| file.file_name.clone())
                    })
                    .collect::<Vec<_>>();
                let evidence = crate::title_extractor::build_match_evidence(
                    &node.folder_name,
                    &node.display_name,
                    None,
                    &names,
                );
                let aliases = sanitize_confirmed_title_aliases(
                    &crate::title_extractor::confirmed_alias_candidates(&evidence),
                );
                match save_binding_conn(&transaction, node.id, subject, None, Some(&aliases))? {
                    ConditionalBindingSave::Applied(path) => path,
                    ConditionalBindingSave::Stale => return Err("WORK_TARGET_STALE".into()),
                }
            } else {
                clear_binding_conn(&transaction, node.id)?
            };
            if let Some(path) = path {
                paths.push(path);
            }
        }
        crate::logical_works::LogicalWorkIndex::reclassify_related(
            &transaction,
            &target.source_node_ids,
            false,
        )?;
        let cover_target = capture_work_sources_conn(&transaction, &target.source_node_ids)?;
        transaction.commit().map_err(db_error)?;
        Ok(WorkBindingChange {
            previous_paths: paths,
            cover_target,
        })
    }

    #[cfg(test)]
    pub fn capture_work_sources(&self, ids: &[i64]) -> AppResult<crate::models::WorkTarget> {
        self.read_snapshot(|connection| capture_work_sources_conn(connection, ids))
    }

    #[cfg(test)]
    pub fn apply_work_cover(
        &self,
        target: &crate::models::WorkTarget,
        subject: &crate::models::BangumiSubject,
        path: Option<&Path>,
        error: Option<&str>,
        retry_only: bool,
    ) -> AppResult<Vec<PathBuf>> {
        self.apply_work_cover_with_failures(target, subject, path, error, retry_only, &[])
    }

    pub fn apply_work_cover_with_failures(
        &self,
        target: &crate::models::WorkTarget,
        subject: &crate::models::BangumiSubject,
        path: Option<&Path>,
        error: Option<&str>,
        retry_only: bool,
        failed_source_ids: &[i64],
    ) -> AppResult<Vec<PathBuf>> {
        let mut connection = self.connect()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        // Binding may itself merge ownership boundaries. Cover writes still use exactly the
        // original selected Nodes; retries must validate the complete current logical group.
        let sources = if retry_only {
            validate_work_target_conn(&tx, target)?
        } else {
            normalize_work_source_ids(&target.source_node_ids)?
                .into_iter()
                .map(|id| get_node_conn(&tx, id).map_err(|_| "WORK_TARGET_STALE".to_string()))
                .collect::<AppResult<Vec<_>>>()?
        };
        let index = crate::logical_works::LogicalWorkIndex::load(&tx)?;
        if crate::works::target_for_index(&sources, &index) != *target
            || failed_source_ids
                .iter()
                .any(|id| !target.source_node_ids.contains(id))
            || sources.iter().any(|node| {
                node.binding.as_ref().is_none_or(|binding| {
                    (binding.provider_subject_type, binding.provider_subject_id)
                        != (subject.subject_type, subject.subject_id)
                })
            })
        {
            return Err("WORK_TARGET_STALE".into());
        }
        let mut old_paths = Vec::new();
        for node in sources.into_iter().filter(|node| {
            node.cover_source != CoverSource::Manual
                && (!retry_only
                    || failed_source_ids.contains(&node.id)
                    || node
                        .cover_cache_path
                        .as_ref()
                        .is_none_or(|path| !crate::cache::cached_cover_is_valid(Path::new(path)))
                    || node
                        .binding
                        .as_ref()
                        .is_some_and(|binding| binding.cover_download_error.is_some()))
        }) {
            if let Some(path) = path {
                tx.execute("UPDATE nodes SET cover_source='BANGUMI',cover_cache_path=?1,updated_at=CURRENT_TIMESTAMP WHERE id=?2",params![path.to_string_lossy(),node.id]).map_err(db_error)?;
                if let Some(old) = node.cover_cache_path {
                    if Path::new(&old) != path {
                        old_paths.push(PathBuf::from(old));
                    }
                }
            }
            tx.execute("UPDATE metadata_bindings SET cover_download_error=?1,updated_at=CURRENT_TIMESTAMP WHERE node_id=?2 AND provider='BANGUMI'",params![error,node.id]).map_err(db_error)?;
        }
        tx.commit().map_err(db_error)?;
        Ok(old_paths)
    }

    pub fn list_cover_records(&self) -> AppResult<Vec<(i64, CoverSource, PathBuf)>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id,cover_source,cover_cache_path FROM nodes
                 WHERE cover_cache_path IS NOT NULL",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    CoverSource::from_db(&row.get::<_, String>(1)?),
                    PathBuf::from(row.get::<_, String>(2)?),
                ))
            })
            .map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
    }

    pub fn clear_cover_paths_for_nodes(
        &self,
        nodes: &[(i64, CoverSource, PathBuf)],
    ) -> AppResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction().map_err(db_error)?;
        for (node_id, source, expected_path) in nodes {
            let changed = transaction
                .execute(
                    "UPDATE nodes SET cover_source='PLACEHOLDER',cover_cache_path=NULL,
                     updated_at=CURRENT_TIMESTAMP WHERE id=?1 AND cover_cache_path=?2",
                    params![node_id, expected_path.to_string_lossy()],
                )
                .map_err(db_error)?;
            if changed > 0 && *source == CoverSource::Bangumi {
                transaction
                    .execute(
                        "UPDATE metadata_bindings SET
                         cover_download_error='封面缓存已清理，请重新获取。',
                         updated_at=CURRENT_TIMESTAMP
                         WHERE node_id=?1 AND provider='BANGUMI'",
                        [node_id],
                    )
                    .map_err(db_error)?;
            }
        }
        transaction.commit().map_err(db_error)
    }

    pub fn get_media_file(&self, media_file_id: i64) -> AppResult<MediaFile> {
        let connection = self.connect()?;
        connection
            .query_row(
                &format!(
                    "SELECT {} FROM media_files f WHERE f.id=?1",
                    media_columns("f")
                ),
                [media_file_id],
                media_from_row,
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| "视频索引不存在。".to_string())
    }

    pub fn get_resource_file(&self, resource_file_id: i64) -> AppResult<ResourceFile> {
        let connection = self.connect()?;
        connection
            .query_row(
                &format!(
                    "SELECT {} FROM resource_files f WHERE f.id=?1",
                    resource_columns("f")
                ),
                [resource_file_id],
                resource_from_row,
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| "附属资源索引不存在。".to_string())
    }

    pub fn get_settings(&self, default_cache_directory: &Path) -> AppResult<AppSettings> {
        let connection = self.connect()?;
        let mut values = HashMap::new();
        let mut statement = connection
            .prepare("SELECT key, value FROM settings")
            .map_err(db_error)?;
        for row in statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(db_error)?
        {
            let (key, value) = row.map_err(db_error)?;
            values.insert(key, value);
        }
        let video_extensions = values
            .get("video_extensions")
            .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
            .unwrap_or_else(default_video_extensions);
        Ok(AppSettings {
            comic_reader: values
                .get("comic_reader")
                .and_then(|v| serde_json::from_str(v).ok())
                .unwrap_or_default(),
            mpv_path: values.get("mpv_path").filter(|p| !p.is_empty()).cloned(),
            default_view_mode: match values.get("default_view_mode").map(String::as_str) {
                Some("LIST") => ViewMode::List,
                _ => ViewMode::Grid,
            },
            video_extensions,
            bangumi_search_enabled: values
                .get("bangumi_search_enabled")
                .map(|value| value != "false")
                .unwrap_or(true),
            cover_cache_directory: values
                .get("cover_cache_directory")
                .filter(|path| !path.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| default_cache_directory.to_string_lossy().into_owned()),
            language: match values.get("language").map(String::as_str) {
                Some(value @ ("zh-CN" | "en-US" | "ja-JP" | "ko-KR")) => value.to_string(),
                _ => "zh-CN".into(),
            },
            theme: match values.get("theme").map(String::as_str) {
                Some(value @ ("system" | "light" | "dark")) => value.to_string(),
                _ => "system".into(),
            },
            auto_check_updates: values
                .get("auto_check_updates")
                .map(|value| value != "false")
                .unwrap_or(true),
            auto_scan_on_startup: values
                .get("auto_scan_on_startup")
                .map(|value| value != "false")
                .unwrap_or(true),
            all_resources_flattened: values
                .get("all_resources_flattened")
                .is_some_and(|value| value == "true"),
        })
    }

    /// Native window lifecycle state is deliberately stored under its own settings key instead
    /// of being part of the frontend's complete `AppSettings` snapshots. Invalid legacy/corrupt
    /// values are ignored so they can never prevent the application from opening.
    pub fn get_window_size(&self) -> AppResult<Option<WindowSize>> {
        let connection = self.connect()?;
        let raw = connection
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                [WINDOW_SIZE_SETTING_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(db_error)?;
        Ok(raw
            .and_then(|value| serde_json::from_str::<WindowSize>(&value).ok())
            .and_then(crate::window_state::validate_persisted_size))
    }

    pub fn save_window_size(&self, size: WindowSize) -> AppResult<()> {
        let size = crate::window_state::validate_persisted_size(size)
            .ok_or_else(|| "窗口大小超出可保存范围。".to_string())?;
        let value = serde_json::to_string(&size).map_err(|error| error.to_string())?;
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO settings(key,value) VALUES(?1,?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![WINDOW_SIZE_SETTING_KEY, value],
            )
            .map_err(db_error)?;
        Ok(())
    }

    pub fn get_collection_sort_preferences(&self) -> AppResult<CollectionSortPreferences> {
        let connection = self.connect()?;
        let read = |key: &str| -> AppResult<CollectionSort> {
            let value = connection
                .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
                    row.get::<_, String>(0)
                })
                .optional()
                .map_err(db_error)?;
            Ok(value
                .as_deref()
                .map(CollectionSort::from_db)
                .unwrap_or_default())
        };
        Ok(CollectionSortPreferences {
            all: read(CollectionSortScope::All.setting_key())?,
            browse: read(CollectionSortScope::Browse.setting_key())?,
            favorites: read(CollectionSortScope::Favorites.setting_key())?,
        })
    }

    pub fn update_collection_sort_preference(
        &self,
        scope: CollectionSortScope,
        sort: CollectionSort,
    ) -> AppResult<CollectionSort> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO settings(key,value) VALUES(?1,?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![scope.setting_key(), sort.as_db()],
            )
            .map_err(db_error)?;
        Ok(sort)
    }

    pub fn update_settings(
        &self,
        settings: &AppSettings,
        default_cache_directory: &Path,
    ) -> AppResult<AppSettings> {
        settings.validate_path_lengths()?;
        if !matches!(
            settings.language.as_str(),
            "zh-CN" | "en-US" | "ja-JP" | "ko-KR"
        ) {
            return Err("界面语言仅支持 zh-CN、en-US、ja-JP 或 ko-KR。".into());
        }
        if !matches!(settings.theme.as_str(), "system" | "light" | "dark") {
            return Err("主题仅支持 system、light 或 dark。".into());
        }
        if settings.cover_cache_directory.trim().is_empty() {
            return Err("封面缓存目录不能为空。".into());
        }
        if settings.video_extensions.len() > MAX_VIDEO_EXTENSIONS {
            return Err(format!("视频扩展名最多支持 {MAX_VIDEO_EXTENSIONS} 项。"));
        }
        let mut extensions = settings
            .video_extensions
            .iter()
            .map(|extension| {
                extension
                    .trim()
                    .trim_start_matches('.')
                    .to_ascii_lowercase()
            })
            .filter(|extension| {
                !extension.is_empty()
                    && extension.len() <= 12
                    && extension.chars().all(|c| c.is_ascii_alphanumeric())
            })
            .collect::<Vec<_>>();
        extensions.sort();
        extensions.dedup();
        if extensions.is_empty() {
            return Err("至少需要保留一个视频扩展名。".into());
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction().map_err(db_error)?;
        let values = [
            (
                "comic_reader",
                serde_json::to_string(&settings.comic_reader).map_err(|e| e.to_string())?,
            ),
            ("mpv_path", settings.mpv_path.clone().unwrap_or_default()),
            (
                "default_view_mode",
                settings.default_view_mode.as_db().to_string(),
            ),
            (
                "video_extensions",
                serde_json::to_string(&extensions).map_err(|e| e.to_string())?,
            ),
            (
                "bangumi_search_enabled",
                settings.bangumi_search_enabled.to_string(),
            ),
            (
                "cover_cache_directory",
                settings.cover_cache_directory.clone(),
            ),
            ("language", settings.language.clone()),
            ("theme", settings.theme.clone()),
            (
                "auto_check_updates",
                settings.auto_check_updates.to_string(),
            ),
            (
                "auto_scan_on_startup",
                settings.auto_scan_on_startup.to_string(),
            ),
            (
                "all_resources_flattened",
                settings.all_resources_flattened.to_string(),
            ),
        ];
        for (key, value) in values {
            transaction
                .execute(
                    "INSERT INTO settings(key,value) VALUES(?1,?2)
                     ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                    params![key, value],
                )
                .map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)?;
        self.get_settings(default_cache_directory)
    }

    #[cfg(test)]
    pub fn record_auto_scan_health(
        &self,
        root_id: i64,
        outcome: &str,
        errors: u64,
        detail: Option<&str>,
    ) -> AppResult<()> {
        self.record_scan_health(root_id, outcome, errors, detail, true)
    }

    pub fn record_scan_health(
        &self,
        root_id: i64,
        outcome: &str,
        errors: u64,
        detail: Option<&str>,
        automatic: bool,
    ) -> AppResult<()> {
        let connection = self.connect()?;
        connection.execute("INSERT INTO library_scan_health(library_root_id,last_auto_attempt_at,last_success_at,outcome,error_count,detail) VALUES(?1,CASE WHEN ?5 THEN strftime('%Y-%m-%dT%H:%M:%fZ','now') ELSE NULL END,CASE WHEN ?2='SUCCESS' THEN strftime('%Y-%m-%dT%H:%M:%fZ','now') ELSE NULL END,?2,?3,?4) ON CONFLICT(library_root_id) DO UPDATE SET last_auto_attempt_at=COALESCE(excluded.last_auto_attempt_at,library_scan_health.last_auto_attempt_at),last_success_at=COALESCE(excluded.last_success_at,library_scan_health.last_success_at),outcome=excluded.outcome,error_count=excluded.error_count,detail=excluded.detail",params![root_id,outcome,errors,detail,automatic]).map_err(db_error)?;
        Ok(())
    }

    pub fn start_scan_run(&self, scan_id: &str, root_id: i64) -> AppResult<()> {
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO scan_runs(id,root_id,status) VALUES (?1,?2,'RUNNING')",
                params![scan_id, root_id],
            )
            .map_err(db_error)?;
        Ok(())
    }

    pub fn finish_scan_run(&self, progress: &crate::models::ScanProgress) -> AppResult<()> {
        let connection = self.connect()?;
        connection
            .execute(
                "UPDATE scan_runs SET finished_at=CURRENT_TIMESTAMP,status=?1,folders_scanned=?2,
                 files_scanned=?3,errors=?4,message=?5 WHERE id=?6 AND root_id=?7",
                params![
                    progress.status.as_db(),
                    progress.folders_scanned as i64,
                    progress.videos_found as i64,
                    progress.errors as i64,
                    progress.message,
                    progress.scan_id,
                    progress.root_id
                ],
            )
            .map_err(db_error)?;
        if matches!(progress.status, crate::models::ScanStatus::Completed) && progress.errors == 0 {
            connection
                .execute(
                    "UPDATE library_roots SET last_scan_at=CURRENT_TIMESTAMP WHERE id=?1",
                    [progress.root_id],
                )
                .map_err(db_error)?;
        }
        Ok(())
    }

    pub fn delete_scan_runs(&self, scan_id: &str) -> AppResult<()> {
        let connection = self.connect()?;
        connection
            .execute("DELETE FROM scan_runs WHERE id=?1", [scan_id])
            .map_err(db_error)?;
        Ok(())
    }
}

pub fn default_video_extensions() -> Vec<String> {
    ["mkv", "mp4", "m4v", "avi", "mov", "webm", "ts", "m2ts"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn save_binding_conn(
    transaction: &Connection,
    node_id: i64,
    subject: &crate::models::BangumiSubject,
    expected_subject: Option<Option<i64>>,
    replacement_aliases: Option<&[(String, String)]>,
) -> AppResult<ConditionalBindingSave> {
    let node = get_node_conn(transaction, node_id)?;
    if !node.media_kind.accepts_subject(subject.subject_type) || subject.subject_id <= 0 {
        return Err("BANGUMI_MEDIA_KIND_CONFLICT".into());
    }
    let (node_type, total_video_count, cover_source, cover_path): (
        String,
        i64,
        String,
        Option<String>,
    ) = transaction
        .query_row(
            "SELECT node_type,total_video_count,cover_source,cover_cache_path
                 FROM nodes WHERE id=?1",
            [node_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(db_error)?
        .ok_or_else(|| "目录节点不存在。".to_string())?;
    let node_type = NodeType::from_db(&node_type);
    if !(node_type.is_work()
        || (node_type == NodeType::Container
            && (total_video_count > 0 || node.total_comic_book_count > 0)))
    {
        return Err("只有作品或包含视频的系列可以绑定 Bangumi。".into());
    }
    let previous_subject = transaction
        .query_row(
            "SELECT provider_subject_id FROM metadata_bindings
                 WHERE node_id=?1 AND provider='BANGUMI'",
            [node_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(db_error)?;
    if expected_subject
        .is_some_and(|expected| previous_subject != expected || node.tmdb_binding.is_some())
    {
        return Ok(ConditionalBindingSave::Stale);
    }
    let cleared_path = if previous_subject != Some(subject.subject_id)
        && CoverSource::from_db(&cover_source) == CoverSource::Bangumi
    {
        transaction
            .execute(
                "UPDATE nodes SET cover_source='PLACEHOLDER',cover_cache_path=NULL,
                     updated_at=CURRENT_TIMESTAMP WHERE id=?1",
                [node_id],
            )
            .map_err(db_error)?;
        cover_path.map(PathBuf::from)
    } else {
        None
    };
    transaction
            .execute(
                "INSERT INTO metadata_bindings(
                    node_id, provider, provider_subject_id, provider_subject_type,
                    provider_title, provider_title_cn,
                    provider_title_en, provider_title_ja, provider_title_ko, provider_date,
                    provider_image_url, bound_at, updated_at, cover_download_error, provider_aliases_json
                 ) VALUES (?1, 'BANGUMI', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                    CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, NULL, COALESCE((SELECT aliases_json FROM provider_alias_sync WHERE subject_id=?2 AND subject_type=?3),?11))
                 ON CONFLICT(node_id, provider) DO UPDATE SET
                    provider_subject_id=excluded.provider_subject_id,
                    provider_subject_type=excluded.provider_subject_type,
                    provider_title=excluded.provider_title,
                    provider_title_cn=excluded.provider_title_cn,
                    provider_title_en=excluded.provider_title_en,
                    provider_title_ja=excluded.provider_title_ja,
                    provider_title_ko=excluded.provider_title_ko,
                    provider_date=excluded.provider_date,
                    provider_image_url=excluded.provider_image_url,
                    provider_aliases_json=excluded.provider_aliases_json,
                    cover_download_error=NULL,
                    updated_at=CURRENT_TIMESTAMP",
                params![
                    node_id,
                    subject.subject_id,
                    subject.subject_type,
                    subject.title,
                    subject.title_cn,
                    subject.title_en,
                    subject.title_ja,
                    subject.title_ko,
                    subject.date,
                    subject.image_url,
                    provider_aliases_json(subject)
                ],
            )
            .map_err(db_error)?;
    if previous_subject != Some(subject.subject_id) || replacement_aliases.is_some() {
        transaction
            .execute(
                "DELETE FROM confirmed_title_aliases WHERE source_node_id=?1",
                [node_id],
            )
            .map_err(db_error)?;
    }
    if let Some(aliases) = replacement_aliases {
        {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO confirmed_title_aliases(
                            normalized_alias,original_alias,subject_id,subject_type,source_node_id
                         ) VALUES(?1,?2,?3,?4,?5)",
                )
                .map_err(db_error)?;
            for (original_alias, normalized_alias) in aliases {
                statement
                    .execute(params![
                        normalized_alias,
                        original_alias,
                        subject.subject_id,
                        subject.subject_type,
                        node_id,
                    ])
                    .map_err(db_error)?;
            }
        }
    }
    Ok(ConditionalBindingSave::Applied(cleared_path))
}

fn clear_binding_conn(transaction: &Connection, node_id: i64) -> AppResult<Option<PathBuf>> {
    let (cover_source, cover_path): (String, Option<String>) = transaction
        .query_row(
            "SELECT cover_source,cover_cache_path FROM nodes WHERE id=?1",
            [node_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(db_error)?
        .ok_or_else(|| "目录节点不存在。".to_string())?;
    transaction
        .execute(
            "DELETE FROM metadata_bindings WHERE node_id=?1 AND provider='BANGUMI'",
            [node_id],
        )
        .map_err(db_error)?;
    transaction
        .execute(
            "DELETE FROM confirmed_title_aliases WHERE source_node_id=?1",
            [node_id],
        )
        .map_err(db_error)?;
    let path = if CoverSource::from_db(&cover_source) == CoverSource::Bangumi {
        transaction
                .execute(
                    "UPDATE nodes SET cover_source='PLACEHOLDER', cover_cache_path=NULL, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
                    [node_id],
                )
                .map_err(db_error)?;
        cover_path.map(PathBuf::from)
    } else {
        None
    };
    Ok(path)
}

#[derive(Debug)]
pub struct WorkBindingChange {
    pub previous_paths: Vec<PathBuf>,
    pub cover_target: crate::models::WorkTarget,
}

fn normalize_work_source_ids(ids: &[i64]) -> AppResult<Vec<i64>> {
    if ids.is_empty()
        || ids.iter().any(|id| *id <= 0)
        || ids.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err("WORK_TARGET_STALE".into());
    }
    Ok(ids.to_vec())
}

fn capture_work_sources_conn(
    connection: &Connection,
    ids: &[i64],
) -> AppResult<crate::models::WorkTarget> {
    let ids = normalize_work_source_ids(ids)?;
    let index = crate::logical_works::LogicalWorkIndex::load(connection)?;
    if ids.len() > index.nodes.len() {
        return Err("WORK_TARGET_STALE".into());
    }
    let sources = ids
        .into_iter()
        .map(|id| {
            index
                .nodes
                .get(&id)
                .cloned()
                .ok_or_else(|| "WORK_TARGET_STALE".to_string())
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(crate::works::target_for_index(&sources, &index))
}

fn validate_work_target_conn(
    connection: &Connection,
    target: &crate::models::WorkTarget,
) -> AppResult<Vec<MediaNode>> {
    let ids = normalize_work_source_ids(&target.source_node_ids)?;
    if ids != target.source_node_ids {
        return Err("WORK_TARGET_STALE".into());
    }
    let index = crate::logical_works::LogicalWorkIndex::load(connection)?;
    crate::works::groups_from_index(connection, &index)?
        .into_iter()
        .find(|group| group.target == *target)
        .map(|group| group.sources)
        .ok_or_else(|| "WORK_TARGET_STALE".into())
}

fn read_scan_health_conn(
    connection: &Connection,
    root_id: i64,
) -> AppResult<Option<crate::models::ScanHealth>> {
    connection.query_row("SELECT last_auto_attempt_at,last_success_at,outcome,error_count,detail FROM library_scan_health WHERE library_root_id=?1",[root_id],|row|Ok(crate::models::ScanHealth {
        warnings_ignored: connection.query_row("SELECT value='true' FROM settings WHERE key=?1", [format!("library_scan_warnings_ignored:{root_id}")], |r| r.get(0)).optional()?.unwrap_or(false),
        last_auto_attempt_at:row.get(0)?,last_success_at:row.get(1)?,outcome:row.get(2)?,error_count:row.get(3)?,detail:row.get(4)?,
    })).optional().map_err(db_error)
}

fn get_root_conn(connection: &Connection, root_id: i64) -> AppResult<LibraryRoot> {
    connection
        .query_row(
            "SELECT r.id,r.path,r.display_name,r.created_at,r.last_scan_at,r.recognition_mode,
             (SELECT COUNT(*) FROM nodes n
              WHERE n.library_root_id=r.id AND n.node_type <> 'IGNORED'
                AND (n.total_video_count > 0 OR n.total_comic_book_count > 0)
                AND EXISTS (
                    SELECT 1 FROM nodes hidden
                    WHERE hidden.id=n.parent_node_id
                      AND hidden.library_root_id=r.id
                      AND hidden.parent_node_id IS NULL
                )),
             CASE WHEN r.media_kind='COMIC' THEN
                (SELECT COUNT(*) FROM comic_books b JOIN nodes n ON n.id=b.node_id WHERE n.library_root_id=r.id AND b.source_resource_id IS NULL)
             ELSE (SELECT COUNT(*) FROM media_files f JOIN nodes n ON n.id=f.node_id WHERE n.library_root_id=r.id) END,
             CASE WHEN r.artbook_library=1 THEN 'ARTBOOK' WHEN r.doujin_library=1 THEN 'DOUJIN' WHEN r.media_kind='COMIC' THEN r.book_library_kind WHEN r.video_subject_scope<>'MIXED' THEN r.video_subject_scope ELSE r.media_kind END, r.auto_bangumi, r.book_organization_strategy
             FROM library_roots r WHERE r.id=?1",
            [root_id],
            |row| {
                Ok(LibraryRoot {
                    media_kind: LibraryMediaKind::from_db(&row.get::<_, String>(8)?),
                    auto_bangumi: row.get(9)?,
                    book_organization_strategy: row.get(10)?,
                    scan_health: None,
                    id: row.get(0)?,
                    path: row.get(1)?,
                    display_name: row.get(2)?,
                    created_at: row.get(3)?,
                    last_scan_at: row.get(4)?,
                    recognition_mode: LibraryRecognitionMode::from_db(&row.get::<_, String>(5)?),
                    node_count: Some(row.get(6)?),
                    media_count: Some(row.get(7)?),
                })
            },
        )
        .optional()
        .map_err(db_error)?
        .ok_or_else(|| "资源库不存在。".to_string())
}

pub(crate) fn breadcrumbs_conn(
    connection: &Connection,
    node_id: i64,
    include_self: bool,
) -> AppResult<Vec<BreadcrumbItem>> {
    let mut items = Vec::new();
    let mut current = if include_self {
        Some(node_id)
    } else {
        connection
            .query_row(
                "SELECT parent_node_id FROM nodes WHERE id=?1",
                [node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?
            .flatten()
    };
    while let Some(id) = current {
        let value = connection
            .query_row(
                "SELECT parent_node_id, display_name FROM nodes WHERE id=?1",
                [id],
                |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(db_error)?;
        let Some((parent_id, display_name)) = value else {
            break;
        };
        // The node with no parent mirrors LibraryRoot and is intentionally hidden.
        if parent_id.is_some() {
            items.push(BreadcrumbItem { id, display_name });
        }
        current = parent_id;
    }
    items.reverse();
    Ok(items)
}

pub(crate) fn ensure_node_visible_conn(connection: &Connection, node_id: i64) -> AppResult<()> {
    let (exists, hidden): (bool, bool) = connection
        .query_row(
            "WITH RECURSIVE ancestors(id,parent_node_id,node_type) AS (
                 SELECT id,parent_node_id,node_type FROM nodes WHERE id=?1
                 UNION
                 SELECT n.id,n.parent_node_id,n.node_type FROM nodes n
                 JOIN ancestors a ON n.id=a.parent_node_id
             ) SELECT EXISTS(SELECT 1 FROM ancestors),
                      EXISTS(SELECT 1 FROM ancestors WHERE node_type='IGNORED')",
            [node_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(db_error)?;
    if !exists || hidden {
        return Err("NODE_NOT_VISIBLE：目录已隐藏或已移除，请返回资源库。".into());
    }
    Ok(())
}

pub(crate) fn get_node_conn(connection: &Connection, node_id: i64) -> AppResult<MediaNode> {
    let mut node = connection
        .query_row(
            &format!("{} WHERE n.id=?1", node_select()),
            [node_id],
            node_from_row,
        )
        .optional()
        .map_err(db_error)?
        .ok_or_else(|| "目录节点不存在。".to_string())?;
    node.binding = get_binding_conn(connection, node.id)?;
    crate::tmdb::hydrate(connection, std::slice::from_mut(&mut node))?;
    node.user_tags = list_node_tags_conn(connection, node.id)?;
    hydrate_file_modified_times_conn(connection, std::slice::from_mut(&mut node))?;
    Ok(node)
}

pub(crate) fn node_select() -> &'static str {
    "SELECT n.id,n.library_root_id,n.parent_node_id,n.absolute_path,n.folder_name,n.display_name,
     n.node_type,n.manual_type_override,n.cover_source,n.cover_cache_path,n.direct_video_count,
     n.child_media_branch_count,n.total_video_count,n.created_at,n.updated_at,n.last_seen_at,
     (SELECT CASE WHEN r.artbook_library=1 THEN 'ARTBOOK' WHEN r.doujin_library=1 THEN 'DOUJIN' WHEN r.media_kind='COMIC' THEN r.book_library_kind WHEN r.video_subject_scope<>'MIXED' THEN r.video_subject_scope ELSE r.media_kind END FROM library_roots r WHERE r.id=n.library_root_id),
     n.direct_comic_book_count,n.child_comic_branch_count,n.total_comic_book_count FROM nodes n"
}

/// Automatic matching treats a non-manually-classified SP/OVA/Extras child as part of a parent
/// that already has direct episodes. Keep that structural exclusion in this shared DB boundary so
/// scan-triggered and explicit existing-index passes cannot diverge. Manual Bangumi binding does
/// not use this candidate query and remains available for intentional exceptions.
fn filter_structural_supplementary_match_candidates(
    connection: &Connection,
    nodes: Vec<MediaNode>,
) -> AppResult<Vec<MediaNode>> {
    let mut statement = connection
        .prepare(
            "WITH RECURSIVE readable(id,parent_node_id) AS (
            SELECT n.id,n.parent_node_id FROM nodes n JOIN comic_books b ON b.node_id=n.id
            WHERE b.index_error IS NULL AND b.page_count>0 AND n.node_type<>'IGNORED'
            UNION SELECT n.id,n.parent_node_id FROM nodes n JOIN readable r ON r.parent_node_id=n.id
            WHERE n.node_type<>'IGNORED'
         ) SELECT DISTINCT id FROM readable",
        )
        .map_err(db_error)?;
    let readable = statement
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(db_error)?
        .collect::<Result<std::collections::HashSet<_>, _>>()
        .map_err(db_error)?;
    let detail_only = connection
        .prepare(&format!(
            "{} SELECT id FROM comic_presentation WHERE anchor IS NOT NULL AND id<>anchor",
            crate::comics::PRESENTATION_CTE
        ))
        .map_err(db_error)?
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(db_error)?
        .collect::<Result<std::collections::HashSet<_>, _>>()
        .map_err(db_error)?;
    let index = crate::logical_works::LogicalWorkIndex::load(connection)?;
    let mut sources = index
        .sources(connection)?
        .into_iter()
        .map(|node| (node.id, node))
        .collect::<HashMap<_, _>>();
    Ok(nodes
        .into_iter()
        .filter_map(|node| {
            if node.media_kind.is_book() {
                (readable.contains(&node.id) && !detail_only.contains(&node.id)).then_some(node)
            } else {
                sources.remove(&node.id)
            }
        })
        .collect())
}

/// SQL selects every bound automatic cover with a retained provider URL so this final filesystem
/// check can recover both a cleared NULL path and a stale path whose file disappeared. Valid
/// automatic covers are discarded before the matcher sees them; unbound/manual-cover Nodes remain
/// eligible for metadata binding through the separate unbound SQL branch.
fn filter_missing_bound_cover_candidates(
    connection: &Connection,
    mut nodes: Vec<MediaNode>,
) -> AppResult<Vec<MediaNode>> {
    // TMDb may coexist with a retained Bangumi cover. Hydrate before filtering so a ready
    // historical Bangumi image cannot hide an old/failed provider cover from either entry point.
    crate::tmdb::hydrate(connection, &mut nodes)?;
    Ok(nodes
        .into_iter()
        .filter(|node| {
            node.tmdb_binding
                .as_ref()
                .is_some_and(|bound| crate::tmdb::needs_cover_refresh(node, bound))
                || node.cover_source != CoverSource::Bangumi
                || node
                    .cover_cache_path
                    .as_deref()
                    .is_none_or(|path| !Path::new(path).is_file())
        })
        .collect())
}

pub(crate) fn node_from_row(row: &Row<'_>) -> rusqlite::Result<MediaNode> {
    Ok(MediaNode {
        tmdb_binding: None,
        media_kind: LibraryMediaKind::from_db(&row.get::<_, String>(16)?),
        direct_comic_book_count: row.get(17)?,
        child_comic_branch_count: row.get(18)?,
        total_comic_book_count: row.get(19)?,
        latest_file_modified_at: None,
        last_watched_at: None,
        id: row.get(0)?,
        library_root_id: row.get(1)?,
        parent_node_id: row.get(2)?,
        absolute_path: row.get(3)?,
        folder_name: row.get(4)?,
        display_name: row.get(5)?,
        node_type: NodeType::from_db(&row.get::<_, String>(6)?),
        manual_type_override: row.get(7)?,
        cover_source: CoverSource::from_db(&row.get::<_, String>(8)?),
        cover_cache_path: row.get(9)?,
        direct_video_count: row.get(10)?,
        child_media_branch_count: row.get(11)?,
        total_video_count: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
        last_seen_at: row.get(15)?,
        binding: None,
        user_tags: Vec::new(),
    })
}

/// Use indexed source timestamps, never Node.updated_at (which also changes on metadata edits).
/// Traverse relationships in bounded batches; hidden descendants do not affect visible parents.
fn hydrate_file_modified_times_conn(
    connection: &Connection,
    nodes: &mut [MediaNode],
) -> AppResult<()> {
    for chunk in nodes.chunks_mut(NODE_METADATA_CHUNK_SIZE) {
        let sql = format!(
            "WITH RECURSIVE subtree(owner,id) AS (
                SELECT id,id FROM nodes WHERE id IN ({})
                UNION
                SELECT s.owner,n.id FROM subtree s JOIN nodes n ON n.parent_node_id=s.id
                WHERE n.node_type<>'IGNORED'
             ), times(owner,modified,watched) AS (
                SELECT s.owner,julianday(f.modified_at),NULL FROM subtree s JOIN media_files f ON f.node_id=s.id
                UNION ALL
                SELECT s.owner,julianday(f.modified_at),NULL FROM subtree s JOIN resource_files f ON f.node_id=s.id
                UNION ALL
                SELECT s.owner,NULL,julianday(w.last_watched_at) FROM subtree s JOIN watch_history w ON w.node_id=s.id
                UNION ALL SELECT s.owner,julianday(b.modified_at),NULL FROM subtree s JOIN comic_books b ON b.node_id=s.id
                UNION ALL SELECT s.owner,NULL,julianday(p.last_read_at) FROM subtree s JOIN comic_books b ON b.node_id=s.id JOIN comic_reading_progress p ON p.comic_book_id=b.id
             ) SELECT owner,strftime('%Y-%m-%dT%H:%M:%fZ',MAX(modified)),strftime('%Y-%m-%dT%H:%M:%fZ',MAX(watched)) FROM times GROUP BY owner",
            sql_placeholders(chunk.len())
        );
        let mut statement = connection.prepare(&sql).map_err(db_error)?;
        let times = statement
            .query_map(
                rusqlite::params_from_iter(chunk.iter().map(|node| node.id)),
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        (
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ),
                    ))
                },
            )
            .map_err(db_error)?
            .collect::<Result<HashMap<_, _>, _>>()
            .map_err(db_error)?;
        for node in chunk {
            node.last_watched_at = times.get(&node.id).and_then(|time| time.1.clone());
            node.latest_file_modified_at = times.get(&node.id).and_then(|time| time.0.clone());
        }
    }
    Ok(())
}

pub(crate) fn list_children_conn(
    connection: &Connection,
    parent_id: i64,
) -> AppResult<Vec<MediaNode>> {
    list_nodes_conn(
        connection,
        &format!(
            "{} WHERE n.parent_node_id=?1 AND n.node_type <> 'IGNORED'",
            node_select()
        ),
        parent_id,
    )
}

/// Book detail needs only folders not already expanded into the core table.
/// Filter those rows before hydrating metadata, rather than hydrating every volume.
pub(crate) fn list_remaining_children_conn(
    connection: &Connection,
    parent_ids: &[i64],
) -> AppResult<Vec<MediaNode>> {
    let expanded = parent_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let mut nodes = Vec::new();
    for chunk in parent_ids.chunks(NODE_METADATA_CHUNK_SIZE) {
        let sql = format!(
            "{} WHERE n.parent_node_id IN ({}) AND n.node_type<>'IGNORED'",
            node_select(),
            sql_placeholders(chunk.len())
        );
        let mut statement = connection.prepare(&sql).map_err(db_error)?;
        nodes.extend(
            statement
                .query_map(rusqlite::params_from_iter(chunk), node_from_row)
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?
                .into_iter()
                .filter(|node| !expanded.contains(&node.id)),
        );
    }
    hydrate_nodes_metadata_conn(connection, &mut nodes)?;
    Ok(nodes)
}

fn list_nodes_conn(
    connection: &Connection,
    sql: &str,
    parent_id: i64,
) -> AppResult<Vec<MediaNode>> {
    let mut statement = connection.prepare(sql).map_err(db_error)?;
    let mut nodes = statement
        .query_map([parent_id], node_from_row)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    hydrate_nodes_metadata_conn(connection, &mut nodes)?;
    nodes.sort_by(|a, b| natural_cmp(&a.display_name, &b.display_name));
    Ok(nodes)
}

fn load_node_rows_by_ids_conn(
    connection: &Connection,
    node_ids: &[i64],
) -> AppResult<Vec<MediaNode>> {
    let mut nodes = Vec::with_capacity(node_ids.len());
    for chunk in node_ids.chunks(NODE_METADATA_CHUNK_SIZE) {
        if chunk.is_empty() {
            continue;
        }
        let sql = format!(
            "{} WHERE n.id IN ({})",
            node_select(),
            sql_placeholders(chunk.len())
        );
        let mut statement = connection.prepare(&sql).map_err(db_error)?;
        nodes.extend(
            statement
                .query_map(rusqlite::params_from_iter(chunk.iter()), node_from_row)
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?,
        );
    }
    Ok(nodes)
}

/// Hydrate card-list metadata in bounded batches. The previous per-Node binding and tag lookups
/// made All Resources and ordinary directory browsing execute 1 + 2N SQL statements.
pub(crate) fn hydrate_nodes_metadata_conn(
    connection: &Connection,
    nodes: &mut [MediaNode],
) -> AppResult<()> {
    if nodes.is_empty() {
        return Ok(());
    }
    hydrate_file_modified_times_conn(connection, nodes)?;
    for node in nodes.iter_mut() {
        node.binding = None;
        node.user_tags.clear();
    }
    let positions = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id, index))
        .collect::<HashMap<_, _>>();
    let node_ids = nodes.iter().map(|node| node.id).collect::<Vec<_>>();

    for chunk in node_ids.chunks(NODE_METADATA_CHUNK_SIZE) {
        let placeholders = sql_placeholders(chunk.len());
        let binding_sql = format!(
            "SELECT b.id,b.node_id,b.provider,b.provider_subject_id,b.provider_subject_type,
             b.provider_title,b.provider_title_cn,
             b.provider_title_en,b.provider_title_ja,b.provider_title_ko,
             b.provider_date,b.provider_image_url,b.bound_at,b.updated_at,
             CASE WHEN n.cover_source='BANGUMI' THEN n.cover_cache_path ELSE NULL END,
             b.cover_download_error,b.provider_aliases_json
             FROM metadata_bindings b JOIN nodes n ON n.id=b.node_id
             WHERE b.provider='BANGUMI' AND b.node_id IN ({placeholders})"
        );
        let mut binding_statement = connection.prepare(&binding_sql).map_err(db_error)?;
        let bindings = binding_statement
            .query_map(rusqlite::params_from_iter(chunk.iter()), binding_from_row)
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        for binding in bindings {
            if let Some(index) = positions.get(&binding.node_id) {
                nodes[*index].binding = Some(binding);
            }
        }

        let tag_sql = format!(
            "SELECT nt.node_id,t.id,t.name,t.created_at,t.updated_at
             FROM node_tags nt JOIN tags t ON t.id=nt.tag_id
             WHERE nt.node_id IN ({placeholders})
             ORDER BY nt.node_id,t.name COLLATE NOCASE,t.id"
        );
        let mut tag_statement = connection.prepare(&tag_sql).map_err(db_error)?;
        let tags = tag_statement
            .query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    UserTag {
                        id: row.get(1)?,
                        name: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    },
                ))
            })
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        for (node_id, tag) in tags {
            if let Some(index) = positions.get(&node_id) {
                nodes[*index].user_tags.push(tag);
            }
        }
    }
    crate::tmdb::hydrate(connection, nodes)?;
    for node in nodes {
        node.user_tags
            .sort_by(|left, right| natural_cmp(&left.name, &right.name));
    }
    Ok(())
}

fn user_tag_from_row(row: &Row<'_>) -> rusqlite::Result<UserTag> {
    Ok(UserTag {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn user_tag_membership_from_row(row: &Row<'_>) -> rusqlite::Result<UserTagMembership> {
    Ok(UserTagMembership {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        assigned: row.get(4)?,
    })
}

fn list_node_tags_conn(connection: &Connection, node_id: i64) -> AppResult<Vec<UserTag>> {
    let mut statement = connection
        .prepare(
            "SELECT t.id,t.name,t.created_at,t.updated_at
             FROM tags t JOIN node_tags nt ON nt.tag_id=t.id
             WHERE nt.node_id=?1 ORDER BY t.name COLLATE NOCASE,t.id",
        )
        .map_err(db_error)?;
    let mut tags = statement
        .query_map([node_id], user_tag_from_row)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    tags.sort_by(|left, right| natural_cmp(&left.name, &right.name));
    Ok(tags)
}

fn get_user_tag_conn(connection: &Connection, tag_id: i64) -> AppResult<UserTag> {
    connection
        .query_row(
            "SELECT id,name,created_at,updated_at FROM tags WHERE id=?1",
            [tag_id],
            user_tag_from_row,
        )
        .optional()
        .map_err(db_error)?
        .ok_or_else(|| "标签不存在。".to_string())
}

fn favorite_folder_from_row(row: &Row<'_>) -> rusqlite::Result<FavoriteFolder> {
    Ok(FavoriteFolder {
        id: row.get(0)?,
        name: row.get(1)?,
        item_count: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn get_favorite_folder_conn(connection: &Connection, folder_id: i64) -> AppResult<FavoriteFolder> {
    connection
        .query_row(
            "SELECT f.id,f.name,COUNT(nff.node_id),f.created_at,f.updated_at
             FROM favorite_folders f
             LEFT JOIN node_favorite_folders nff ON nff.folder_id=f.id
             WHERE f.id=?1
             GROUP BY f.id,f.name,f.created_at,f.updated_at",
            [folder_id],
            favorite_folder_from_row,
        )
        .optional()
        .map_err(db_error)?
        .ok_or_else(|| "收藏夹不存在。".to_string())
}

fn ensure_node_exists_conn(connection: &Connection, node_id: i64) -> AppResult<()> {
    let exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM nodes WHERE id=?1)",
            [node_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(db_error)?;
    if !exists {
        return Err("目录节点不存在。".into());
    }
    Ok(())
}

fn normalize_batch_node_ids(node_ids: &[i64]) -> AppResult<Vec<i64>> {
    let mut normalized = node_ids.to_vec();
    normalized.sort_unstable();
    normalized.dedup();
    if normalized.is_empty() {
        return Err("请至少选择一个资源。".into());
    }
    if normalized.len() > MAX_BATCH_NODE_IDS {
        return Err(format!(
            "单次批量操作最多支持 {MAX_BATCH_NODE_IDS} 个资源。"
        ));
    }
    if normalized.iter().any(|node_id| *node_id <= 0) {
        return Err("资源 ID 无效。".into());
    }
    Ok(normalized)
}

fn ensure_batch_nodes_exist_conn(connection: &Connection, node_ids: &[i64]) -> AppResult<()> {
    let sql = format!(
        "SELECT COUNT(*) FROM nodes WHERE id IN ({})",
        sql_placeholders(node_ids.len())
    );
    let count = connection
        .query_row(&sql, rusqlite::params_from_iter(node_ids.iter()), |row| {
            row.get::<_, i64>(0)
        })
        .map_err(db_error)?;
    if count != node_ids.len() as i64 {
        return Err("部分所选资源已不存在，批量操作未执行。".into());
    }
    Ok(())
}

fn sql_placeholders(count: usize) -> String {
    vec!["?"; count].join(",")
}

fn normalize_tag_name(value: &str) -> AppResult<(String, String)> {
    let name = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err("标签名称不能为空。".into());
    }
    if name.chars().count() > 40 {
        return Err("标签名称不能超过 40 个字符。".into());
    }
    let normalized_name = name.to_lowercase();
    Ok((name, normalized_name))
}

fn normalize_favorite_folder_name(value: &str) -> AppResult<(String, String)> {
    let name = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err("收藏夹名称不能为空。".into());
    }
    if name.chars().count() > 80 {
        return Err("收藏夹名称不能超过 80 个字符。".into());
    }
    let normalized_name = name.to_lowercase();
    Ok((name, normalized_name))
}

fn favorite_folder_write_error(error: rusqlite::Error) -> String {
    if error.to_string().contains("UNIQUE") {
        "已存在同名收藏夹。".to_string()
    } else {
        db_error(error)
    }
}

pub(crate) fn media_columns(alias: &str) -> String {
    format!(
        "{alias}.id,{alias}.node_id,{alias}.absolute_path,{alias}.file_name,{alias}.extension,{alias}.file_size,
         {alias}.modified_at,{alias}.duration_ms,{alias}.width,{alias}.height,{alias}.codec,{alias}.last_seen_at"
    )
}

pub(crate) fn media_from_row(row: &Row<'_>) -> rusqlite::Result<MediaFile> {
    Ok(MediaFile {
        id: row.get(0)?,
        node_id: row.get(1)?,
        absolute_path: row.get(2)?,
        file_name: row.get(3)?,
        extension: row.get(4)?,
        file_size: row.get(5)?,
        modified_at: row.get(6)?,
        duration_ms: row.get(7)?,
        width: row.get(8)?,
        height: row.get(9)?,
        codec: row.get(10)?,
        last_seen_at: row.get(11)?,
    })
}

pub(crate) fn list_media_conn(connection: &Connection, node_id: i64) -> AppResult<Vec<MediaFile>> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {} FROM media_files f WHERE f.node_id=?1",
            media_columns("f")
        ))
        .map_err(db_error)?;
    let mut files = statement
        .query_map([node_id], media_from_row)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    files.sort_by(|a, b| natural_cmp(&a.file_name, &b.file_name));
    Ok(files)
}

fn resource_columns(alias: &str) -> String {
    format!(
        "{alias}.id,{alias}.node_id,{alias}.absolute_path,{alias}.file_name,{alias}.extension,{alias}.file_size,
         {alias}.modified_at,{alias}.resource_type,{alias}.last_seen_at"
    )
}

fn resource_from_row(row: &Row<'_>) -> rusqlite::Result<ResourceFile> {
    Ok(ResourceFile {
        id: row.get(0)?,
        node_id: row.get(1)?,
        absolute_path: row.get(2)?,
        file_name: row.get(3)?,
        extension: row.get(4)?,
        file_size: row.get(5)?,
        modified_at: row.get(6)?,
        resource_type: ResourceType::from_db(&row.get::<_, String>(7)?),
        last_seen_at: row.get(8)?,
    })
}

pub(crate) fn list_resources_conn(
    connection: &Connection,
    node_id: i64,
) -> AppResult<Vec<ResourceFile>> {
    let mut files = list_resources_for_nodes_conn(connection, &[node_id])?;
    files.sort_by(|left, right| natural_cmp(&left.file_name, &right.file_name));
    Ok(files)
}

pub(crate) fn list_resources_for_nodes_conn(
    connection: &Connection,
    node_ids: &[i64],
) -> AppResult<Vec<ResourceFile>> {
    let mut files = Vec::new();
    for chunk in node_ids.chunks(NODE_METADATA_CHUNK_SIZE) {
        let sql = format!("SELECT {} FROM resource_files f WHERE f.node_id IN ({}) AND NOT EXISTS (SELECT 1 FROM comic_books b WHERE b.source_resource_id IS NULL AND b.node_id=f.node_id AND b.source_path=f.absolute_path COLLATE NOCASE AND lower(b.source_path) NOT LIKE '%.zip') AND NOT EXISTS (SELECT 1 FROM comic_books b JOIN comic_pages p ON p.comic_book_id=b.id WHERE b.source_resource_id IS NULL AND b.node_id=f.node_id AND b.source_kind='IMAGE_FOLDER' AND p.source_locator=f.absolute_path COLLATE NOCASE)", resource_columns("f"), sql_placeholders(chunk.len()));
        let mut statement = connection.prepare(&sql).map_err(db_error)?;
        files.extend(
            statement
                .query_map(rusqlite::params_from_iter(chunk), resource_from_row)
                .map_err(db_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_error)?,
        );
    }
    Ok(files)
}

fn get_binding_conn(connection: &Connection, node_id: i64) -> AppResult<Option<MetadataBinding>> {
    connection
        .query_row(
            "SELECT b.id,b.node_id,b.provider,b.provider_subject_id,b.provider_subject_type,
             b.provider_title,b.provider_title_cn,
             b.provider_title_en,b.provider_title_ja,b.provider_title_ko,
             b.provider_date,b.provider_image_url,b.bound_at,b.updated_at,
             CASE WHEN n.cover_source='BANGUMI' THEN n.cover_cache_path ELSE NULL END
             ,b.cover_download_error,b.provider_aliases_json
             FROM metadata_bindings b JOIN nodes n ON n.id=b.node_id
             WHERE b.node_id=?1 AND b.provider='BANGUMI'",
            [node_id],
            binding_from_row,
        )
        .optional()
        .map_err(db_error)
}

fn binding_from_row(row: &Row<'_>) -> rusqlite::Result<MetadataBinding> {
    Ok(MetadataBinding {
        id: row.get(0)?,
        node_id: row.get(1)?,
        provider: row.get(2)?,
        provider_subject_id: row.get(3)?,
        provider_subject_type: row.get(4)?,
        provider_title: row.get(5)?,
        provider_title_cn: row.get(6)?,
        provider_title_en: row.get(7)?,
        provider_title_ja: row.get(8)?,
        provider_title_ko: row.get(9)?,
        provider_date: row.get(10)?,
        provider_image_url: row.get(11)?,
        bound_at: row.get(12)?,
        updated_at: row.get(13)?,
        cover_cache_path: row.get(14)?,
        cover_download_error: row.get(15)?,
        provider_aliases: serde_json::from_str(&row.get::<_, String>(16)?).unwrap_or_default(),
    })
}

// Keep provider text bounded even when a Subject arrives through a manual IPC call.
fn provider_aliases_json(subject: &crate::models::BangumiSubject) -> String {
    let mut aliases = Vec::new();
    for alias in subject.match_aliases.iter().take(32) {
        let clean: String = alias
            .chars()
            .filter(|c| !c.is_control())
            .take(256)
            .collect();
        let clean = clean.trim().to_string();
        if !clean.is_empty() && !aliases.contains(&clean) {
            aliases.push(clean);
        }
    }
    serde_json::to_string(&aliases).unwrap_or_else(|_| "[]".into())
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn canonical_library_root(path: &Path) -> AppResult<PathBuf> {
    if !path.is_absolute() {
        return Err("资源库目录必须是绝对路径。".into());
    }
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("无法确认所选资源目录的实际位置：{error}"))?;
    if !canonical.is_dir() {
        return Err("所选资源目录不存在或不是文件夹。".into());
    }
    Ok(canonical)
}

fn ensure_root_does_not_overlap_conn(
    connection: &Connection,
    candidate: &Path,
    ignored_root_id: Option<i64>,
) -> AppResult<()> {
    let mut statement = connection
        .prepare("SELECT id,path FROM library_roots ORDER BY id")
        .map_err(db_error)?;
    let roots = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    for (root_id, path) in roots {
        if ignored_root_id == Some(root_id) {
            continue;
        }
        let existing_path = Path::new(&path);
        let existing = existing_path
            .canonicalize()
            .unwrap_or_else(|_| normalize_library_root_lexically(existing_path));
        if paths_equal_for_library_roots(candidate, &existing) {
            return Err("该资源目录已经添加。".into());
        }
        if library_root_paths_overlap(candidate, &existing) {
            return Err("资源库目录不能与已有资源库重叠；请勿添加其上级或下级目录。".into());
        }
    }
    Ok(())
}

fn library_root_paths_overlap(left: &Path, right: &Path) -> bool {
    library_root_path_starts_with(left, right) || library_root_path_starts_with(right, left)
}

fn paths_equal_for_library_roots(left: &Path, right: &Path) -> bool {
    library_root_path_starts_with(left, right) && library_root_path_starts_with(right, left)
}

/// `Path::starts_with` is component-aware but follows the host's `OsStr` equality. M²Shelf is a
/// Windows product, so comparison on Windows must additionally be case-insensitive. This avoids
/// treating `D:\Anime` and `d:\ANIME\Season` as unrelated while still keeping textual siblings
/// such as `Anime-Backup` separate.
fn library_root_path_starts_with(candidate: &Path, root: &Path) -> bool {
    let mut candidate_components = candidate.components();
    for expected in root.components() {
        let Some(actual) = candidate_components.next() else {
            return false;
        };
        if !library_root_component_eq(actual, expected) {
            return false;
        }
    }
    true
}

fn library_root_component_eq(left: Component<'_>, right: Component<'_>) -> bool {
    #[cfg(target_os = "windows")]
    {
        // Canonical paths normalize drive/UNC prefixes. Unicode lowercase supplies the remaining
        // case-insensitive component comparison without ever converting paths into shell text.
        left.as_os_str().to_string_lossy().to_lowercase()
            == right.as_os_str().to_string_lossy().to_lowercase()
    }
    #[cfg(not(target_os = "windows"))]
    {
        left == right
    }
}

fn normalize_library_root_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    #[cfg(target_os = "windows")]
    {
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    value.into_owned()
}

// Windows CI may expose TEMP through an 8.3 alias. Synthetic scan fixtures must use
// the same long, non-verbatim path spelling as registered Library Roots.
#[cfg(test)]
pub(crate) fn test_temp_dir() -> tempfile::TempDir {
    let parent = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    tempfile::TempDir::new_in(display_path(&parent)).unwrap()
}

pub fn natural_cmp(left: &str, right: &str) -> Ordering {
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let (mut li, mut ri) = (0, 0);
    while li < left_bytes.len() && ri < right_bytes.len() {
        if left_bytes[li].is_ascii_digit() && right_bytes[ri].is_ascii_digit() {
            let (ls, rs) = (li, ri);
            while li < left_bytes.len() && left_bytes[li].is_ascii_digit() {
                li += 1;
            }
            while ri < right_bytes.len() && right_bytes[ri].is_ascii_digit() {
                ri += 1;
            }
            let left_number = left[ls..li].trim_start_matches('0');
            let right_number = right[rs..ri].trim_start_matches('0');
            let left_significant = if left_number.is_empty() {
                "0"
            } else {
                left_number
            };
            let right_significant = if right_number.is_empty() {
                "0"
            } else {
                right_number
            };
            let ordering = left_significant
                .len()
                .cmp(&right_significant.len())
                .then_with(|| left_significant.cmp(right_significant))
                .then_with(|| (li - ls).cmp(&(ri - rs)));
            if ordering != Ordering::Equal {
                return ordering;
            }
        } else {
            let ordering = left_bytes[li].cmp(&right_bytes[ri]);
            if ordering != Ordering::Equal {
                return ordering;
            }
            li += 1;
            ri += 1;
        }
    }
    left_bytes.len().cmp(&right_bytes.len())
}

fn db_error(error: rusqlite::Error) -> String {
    format!("数据库操作失败：{error}")
}

fn sanitize_confirmed_title_aliases(aliases: &[String]) -> Vec<(String, String)> {
    let mut sanitized = Vec::new();
    for raw_alias in aliases {
        if sanitized.len() >= MAX_CONFIRMED_TITLE_ALIASES {
            break;
        }
        let trimmed = raw_alias.trim();
        if trimmed.is_empty()
            || Path::new(trimmed).is_absolute()
            || trimmed.starts_with(r"\\")
            || trimmed.contains('\0')
        {
            continue;
        }
        let bounded = trimmed
            .chars()
            .take(MAX_CONFIRMED_TITLE_ALIAS_CHARS)
            .collect::<String>();
        // Candidates already come from the structured title extractor. A second generic search
        // cleanup would erase meaningful season/year qualifiers and collide seasons or remakes.
        let original_alias = bounded
            .trim()
            .chars()
            .take(MAX_CONFIRMED_TITLE_ALIAS_CHARS)
            .collect::<String>();
        if !(crate::title_extractor::is_safe_match_query(&original_alias)
            || crate::title_extractor::is_four_digit_numeric_title(&original_alias))
        {
            continue;
        }
        let normalized_alias = crate::title_extractor::normalize_title_for_match(&original_alias)
            .chars()
            .take(MAX_CONFIRMED_TITLE_ALIAS_CHARS)
            .collect::<String>();
        if normalized_alias.is_empty()
            || sanitized
                .iter()
                .any(|(_, existing)| existing == &normalized_alias)
        {
            continue;
        }
        sanitized.push((original_alias, normalized_alias));
    }
    sanitized
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_bangumi_subject(
        subject_id: i64,
        subject_type: i64,
        title: &str,
    ) -> crate::models::BangumiSubject {
        crate::models::BangumiSubject {
            subject_id,
            title: title.into(),
            title_cn: None,
            title_en: None,
            title_ja: None,
            title_ko: None,
            match_aliases: Vec::new(),
            date: None,
            image_url: None,
            summary: None,
            subject_type,
        }
    }

    fn legacy_search_reference(
        database: &Database,
        query: &str,
        root_id: Option<i64>,
    ) -> Vec<SearchHit> {
        let connection = database.connect().unwrap();
        let pattern = format!("%{}%", escape_like(query.trim()));
        let root_filter = root_id.unwrap_or(-1);
        let mut hits = Vec::new();

        let mut node_statement = connection
            .prepare(&format!(
                "{} WHERE n.node_type <> 'IGNORED'
                 AND (?2 < 0 OR n.library_root_id=?2)
                 AND (n.display_name LIKE ?1 ESCAPE '\\' OR n.folder_name LIKE ?1 ESCAPE '\\'
                      OR EXISTS (
                          SELECT 1 FROM node_tags nt JOIN tags t ON t.id=nt.tag_id
                          WHERE nt.node_id=n.id AND t.name LIKE ?1 ESCAPE '\\'
                      )
                      OR EXISTS (
                          SELECT 1 FROM metadata_bindings b
                          WHERE b.node_id=n.id AND b.provider='BANGUMI'
                            AND (b.provider_title LIKE ?1 ESCAPE '\\'
                                 OR b.provider_title_cn LIKE ?1 ESCAPE '\\'
                                 OR b.provider_title_en LIKE ?1 ESCAPE '\\'
                                 OR b.provider_title_ja LIKE ?1 ESCAPE '\\'
                                 OR b.provider_title_ko LIKE ?1 ESCAPE '\\')
                      ))
                 LIMIT 200",
                node_select()
            ))
            .unwrap();
        let nodes = node_statement
            .query_map(params![&pattern, root_filter], node_from_row)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for mut node in nodes {
            node.binding = get_binding_conn(&connection, node.id).unwrap();
            node.user_tags = list_node_tags_conn(&connection, node.id).unwrap();
            hits.push(SearchHit {
                comic_book: None,
                kind: SearchHitKind::Node,
                node,
                media_file: None,
            });
        }

        let mut file_statement = connection
            .prepare(&format!(
                "SELECT {} FROM media_files f
                 JOIN nodes n ON n.id=f.node_id
                 WHERE n.node_type <> 'IGNORED' AND (?2 < 0 OR n.library_root_id=?2)
                 AND f.file_name LIKE ?1 ESCAPE '\\' LIMIT 200",
                media_columns("f")
            ))
            .unwrap();
        let files = file_statement
            .query_map(params![&pattern, root_filter], media_from_row)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for media_file in files {
            let mut node = connection
                .query_row(
                    &format!("{} WHERE n.id=?1", node_select()),
                    [media_file.node_id],
                    node_from_row,
                )
                .unwrap();
            node.binding = get_binding_conn(&connection, node.id).unwrap();
            node.user_tags = list_node_tags_conn(&connection, node.id).unwrap();
            hits.push(SearchHit {
                comic_book: None,
                kind: SearchHitKind::MediaFile,
                node,
                media_file: Some(media_file),
            });
        }
        hits.sort_by(|left, right| {
            natural_cmp(&left.node.display_name, &right.node.display_name).then_with(|| {
                let left_file = left
                    .media_file
                    .as_ref()
                    .map(|file| file.file_name.as_str())
                    .unwrap_or("");
                let right_file = right
                    .media_file
                    .as_ref()
                    .map(|file| file.file_name.as_str())
                    .unwrap_or("");
                natural_cmp(left_file, right_file)
            })
        });
        hits.truncate(300);
        hits
    }

    #[test]
    fn search_rejects_oversized_queries_before_opening_the_database() {
        let database = Database::new(PathBuf::from("unused-for-validation.sqlite"));
        let error = database
            .search(&"x".repeat(MAX_SEARCH_QUERY_CHARS + 1), None)
            .unwrap_err();
        assert!(error.contains("不能超过"));
    }

    #[test]
    fn batched_search_hydration_matches_legacy_results_for_many_unicode_hits() {
        let temp = TempDir::new().unwrap();
        let first_root_path = temp.path().join("媒体资料库甲");
        let second_root_path = temp.path().join("媒体资料库乙");
        fs::create_dir_all(&first_root_path).unwrap();
        fs::create_dir_all(&second_root_path).unwrap();
        let database = Database::new(temp.path().join("search-batch.db"));
        database.migrate().unwrap();
        let first_root = database.add_root(&first_root_path, None).unwrap();
        let second_root = database.add_root(&second_root_path, None).unwrap();

        let mut connection = database.connect().unwrap();
        let query = "检索_%";
        for (name, normalized_name) in [
            (format!("{query} 标签"), format!("{query} 标签")),
            ("标签 10".to_string(), "标签 10".to_string()),
            ("标签 2".to_string(), "标签 2".to_string()),
        ] {
            connection
                .execute(
                    "INSERT INTO tags(name,normalized_name) VALUES(?1,?2)",
                    params![name, normalized_name],
                )
                .unwrap();
        }
        let query_tag_id = 1_i64;
        let tag_ten_id = 2_i64;
        let tag_two_id = 3_i64;

        let transaction = connection.transaction().unwrap();
        for index in 0..280_usize {
            let (root_id, root_path) = if index < 260 {
                (first_root.id, &first_root_path)
            } else {
                (second_root.id, &second_root_path)
            };
            let match_source = index % 4;
            let folder_name = format!("原始目录 [{index:03}]");
            let display_name = if match_source == 0 {
                format!("作品 {index} {query}")
            } else {
                format!("作品 {index}")
            };
            let node_type = if index % 47 == 0 { "IGNORED" } else { "WORK" };
            let node_path = root_path.join(&folder_name);
            let cover_path = temp.path().join("covers").join(format!("{index}.jpg"));
            transaction
                .execute(
                    "INSERT INTO nodes(
                        library_root_id,absolute_path,folder_name,display_name,node_type,
                        cover_source,cover_cache_path,total_video_count
                     ) VALUES(?1,?2,?3,?4,?5,'BANGUMI',?6,1)",
                    params![
                        root_id,
                        display_path(&node_path),
                        folder_name,
                        display_name,
                        node_type,
                        display_path(&cover_path)
                    ],
                )
                .unwrap();
            let node_id = transaction.last_insert_rowid();
            let provider_title_ja = if match_source == 2 {
                format!("{query} バインド {index}")
            } else {
                format!("バインド {index}")
            };
            transaction
                .execute(
                    "INSERT INTO metadata_bindings(
                        node_id,provider,provider_subject_id,provider_subject_type,
                        provider_title,provider_title_cn,provider_title_en,provider_title_ja,
                        provider_title_ko,provider_date,provider_image_url,cover_download_error
                     ) VALUES(?1,'BANGUMI',?2,?3,?4,?5,?6,?7,?8,'2026-08-24',?9,NULL)",
                    params![
                        node_id,
                        10_000_i64 + index as i64,
                        if index % 2 == 0 { 2_i64 } else { 6_i64 },
                        format!("Bound title {index}"),
                        format!("绑定标题 {index}"),
                        format!("Bound title EN {index}"),
                        provider_title_ja,
                        format!("바인딩 제목 {index}"),
                        format!("https://lain.bgm.tv/pic/cover/l/{index}.jpg")
                    ],
                )
                .unwrap();
            for tag_id in [tag_ten_id, tag_two_id] {
                transaction
                    .execute(
                        "INSERT INTO node_tags(node_id,tag_id) VALUES(?1,?2)",
                        params![node_id, tag_id],
                    )
                    .unwrap();
            }
            if match_source == 1 {
                transaction
                    .execute(
                        "INSERT INTO node_tags(node_id,tag_id) VALUES(?1,?2)",
                        params![node_id, query_tag_id],
                    )
                    .unwrap();
            }
            let file_name = format!("[字幕组] {query} 第{index:03}话.mkv");
            transaction
                .execute(
                    "INSERT INTO media_files(
                        node_id,absolute_path,file_name,extension,file_size,modified_at
                     ) VALUES(?1,?2,?3,'mkv',?4,'2026-08-24T00:00:00Z')",
                    params![
                        node_id,
                        display_path(&node_path.join(&file_name)),
                        file_name,
                        index as i64 + 1
                    ],
                )
                .unwrap();
        }
        transaction.commit().unwrap();
        drop(connection);

        for root_filter in [None, Some(first_root.id), Some(second_root.id)] {
            let mut expected = legacy_search_reference(&database, query, root_filter);
            // Every fixture Node owns a video with this timestamp. The legacy reader predates
            // this additive DTO field; retain comparison of every old field and the new value.
            for hit in &mut expected {
                hit.node.latest_file_modified_at = Some("2026-08-24T00:00:00.000Z".into());
            }
            let actual = database.search(query, root_filter).unwrap();
            assert_eq!(
                serde_json::to_value(&actual).unwrap(),
                serde_json::to_value(&expected).unwrap(),
                "batch hydration must preserve complete search DTOs for root {root_filter:?}"
            );
        }

        assert_eq!(database.search(query, None).unwrap().len(), 300);
        assert!(database
            .search(query, Some(second_root.id))
            .unwrap()
            .iter()
            .all(|hit| hit.node.library_root_id == second_root.id));
    }

    #[test]
    fn natural_sort_orders_episode_numbers() {
        let mut names = vec!["EP10.mkv", "EP2.mkv", "EP01.mkv", "EP1.mkv"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, vec!["EP1.mkv", "EP01.mkv", "EP2.mkv", "EP10.mkv"]);
    }

    #[test]
    fn default_extensions_cover_mvp_formats() {
        let extensions = default_video_extensions();
        for expected in ["mkv", "mp4", "m2ts", "webm", "ts"] {
            assert!(extensions.iter().any(|value| value == expected));
        }
    }

    #[test]
    fn portable_update_snapshot_holds_the_sqlite_writer_barrier_until_dropped() {
        let temp = TempDir::new().unwrap();
        let database = Database::new(temp.path().join("live.db"));
        database.migrate().unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO settings(key,value) VALUES('before-update','kept')",
                [],
            )
            .unwrap();
        drop(connection);

        let snapshot = temp.path().join("snapshot.db");
        let barrier = database.backup_for_portable_update(&snapshot).unwrap();
        let writer = Connection::open(database.path()).unwrap();
        writer.busy_timeout(Duration::ZERO).unwrap();
        let error = writer
            .execute(
                "INSERT INTO settings(key,value) VALUES('after-snapshot','must-block')",
                [],
            )
            .unwrap_err();
        assert!(matches!(
            error,
            rusqlite::Error::SqliteFailure(ref code, _)
                if code.code == rusqlite::ErrorCode::DatabaseBusy
                    || code.code == rusqlite::ErrorCode::DatabaseLocked
        ));

        let backup = Connection::open(snapshot).unwrap();
        assert_eq!(
            backup
                .query_row(
                    "SELECT value FROM settings WHERE key='before-update'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "kept"
        );
        assert!(backup
            .query_row(
                "SELECT value FROM settings WHERE key='after-snapshot'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .unwrap()
            .is_none());
        drop(backup);
        drop(barrier);

        writer
            .execute(
                "INSERT INTO settings(key,value) VALUES('after-snapshot','now-allowed')",
                [],
            )
            .unwrap();
    }

    #[test]
    fn bulk_card_metadata_hydration_preserves_bindings_and_natural_tag_order_across_chunks() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        fs::create_dir_all(&root_path).unwrap();
        let database = Database::new(temp.path().join("bulk-hydration.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let mut connection = database.connect().unwrap();
        let mut empty_nodes = Vec::new();
        hydrate_nodes_metadata_conn(&connection, &mut empty_nodes).unwrap();
        assert!(empty_nodes.is_empty());
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,absolute_path,folder_name,display_name,node_type,total_video_count
                 ) VALUES(?1,?2,'library','library','CONTAINER',?3)",
                params![
                    root.id,
                    display_path(&root_path),
                    (NODE_METADATA_CHUNK_SIZE + 1) as i64
                ],
            )
            .unwrap();
        let hidden_root_id = connection.last_insert_rowid();
        let transaction = connection.transaction().unwrap();
        let mut node_ids = Vec::with_capacity(NODE_METADATA_CHUNK_SIZE + 1);
        for index in 0..=NODE_METADATA_CHUNK_SIZE {
            let name = format!("Work {index}");
            transaction
                .execute(
                    "INSERT INTO nodes(
                        library_root_id,parent_node_id,absolute_path,folder_name,display_name,
                        node_type,total_video_count
                     ) VALUES(?1,?2,?3,?4,?4,'WORK',1)",
                    params![
                        root.id,
                        hidden_root_id,
                        display_path(&root_path.join(&name)),
                        name
                    ],
                )
                .unwrap();
            node_ids.push(transaction.last_insert_rowid());
        }
        transaction.commit().unwrap();
        drop(connection);

        let last_node_id = *node_ids.last().unwrap();
        database
            .save_confirmed_binding(
                last_node_id,
                &crate::models::BangumiSubject {
                    subject_id: 42,
                    title: "Bulk Hydration Work".into(),
                    title_cn: Some("批量装载作品".into()),
                    title_en: None,
                    title_ja: None,
                    title_ko: None,
                    match_aliases: Vec::new(),
                    date: None,
                    image_url: None,
                    summary: None,
                    subject_type: 2,
                },
            )
            .unwrap();
        database
            .create_or_assign_user_tag(last_node_id, "Tag 10")
            .unwrap();
        database
            .create_or_assign_user_tag(last_node_id, "Tag 2")
            .unwrap();

        let resources = database.list_all_resources().unwrap();
        assert_eq!(
            resources.nodes.len(),
            NODE_METADATA_CHUNK_SIZE + 1,
            "all visible card rows must survive metadata chunking"
        );
        let hydrated = resources
            .nodes
            .iter()
            .find(|node| node.id == last_node_id)
            .unwrap();
        assert_eq!(hydrated.binding.as_ref().unwrap().provider_subject_id, 42);
        assert_eq!(
            hydrated
                .user_tags
                .iter()
                .map(|tag| tag.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Tag 2", "Tag 10"]
        );
    }

    #[test]
    fn provider_aliases_persist_and_search_across_roots_without_rebinding() {
        let temp = TempDir::new().unwrap();
        let database = Database::new(temp.path().join("aliases.db"));
        database.migrate().unwrap();
        let mut roots = Vec::new();
        for name in ["Anime", "Watching"] {
            let path = temp.path().join(name);
            fs::create_dir(&path).unwrap();
            roots.push(database.add_root(&path, None).unwrap());
        }
        let connection = database.connect().unwrap();
        for (id, root) in [(1, &roots[0]), (2, &roots[1]), (3, &roots[1])] {
            connection.execute("INSERT INTO nodes(id,library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(?1,?2,?3,'Folder','Folder','WORK')", params![id,root.id,format!("fixture-{id}")]).unwrap();
        }
        let mut subject = test_bangumi_subject(174584, 2, "フリップフラッパーズ");
        subject.title_cn = Some("轻拍翻转小魔女".into());
        subject.match_aliases = vec![
            "Flip Flappers".into(),
            "Flip Flappers".into(),
            "  フリフラ  ".into(),
            "100%_literal".into(),
        ];
        database.save_confirmed_binding(2, &subject).unwrap();
        database.save_binding_if_absent(3, &subject).unwrap();
        for query in ["轻拍", "flip", "フリフラ", "100%_"] {
            let hits = database.search(query, None).unwrap();
            assert_eq!(
                hits.iter().map(|h| h.node.id).collect::<Vec<_>>(),
                vec![2, 3]
            );
            assert!(database
                .search(query, Some(roots[0].id))
                .unwrap()
                .is_empty());
            assert_eq!(database.search(query, Some(roots[1].id)).unwrap().len(), 2);
        }
        assert!(database.search("100%X", None).unwrap().is_empty());
        assert_eq!(
            database
                .get_node(2)
                .unwrap()
                .binding
                .unwrap()
                .provider_aliases
                .len(),
            3
        );
        // Sync updates every still-matching source, preserving a different binding chosen meanwhile.
        database
            .save_confirmed_binding(3, &test_bangumi_subject(99, 2, "Other"))
            .unwrap();
        subject.match_aliases = vec!["New official alias".into()];
        database.complete_provider_alias_sync(&subject).unwrap();
        assert_eq!(database.search("New official", None).unwrap().len(), 1);
        assert_eq!(
            database
                .get_binding(3)
                .unwrap()
                .unwrap()
                .provider_subject_id,
            99
        );
        assert!(database.search("flip", None).unwrap().is_empty());
        subject.match_aliases = vec!["Updated detail alias".into()];
        database.update_binding_if_subject(2, &subject).unwrap();
        assert_eq!(database.search("Updated detail", None).unwrap().len(), 1);
        database.migrate().unwrap();
        assert_eq!(
            database.get_binding(2).unwrap().unwrap().provider_aliases,
            subject.match_aliases
        );
    }

    #[test]
    fn file_update_times_aggregate_visible_sources_and_ignore_index_edits() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        fs::create_dir(&root_path).unwrap();
        let database = Database::new(temp.path().join("times.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        for (id, parent, name, kind) in [
            (1, None, "root", "CONTAINER"),
            (2, Some(1), "Show - 01.mkv", "WORK"),
            (3, Some(1), "Show - 02.mkv", "WORK"),
            (4, Some(2), "hidden", "IGNORED"),
            (5, Some(1), "empty", "MIXED"),
        ] {
            connection.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type) VALUES(?1,?2,?3,?4,?5,?5,?6)",
                params![id,root.id,parent,root_path.join(name).to_string_lossy(),name.trim_end_matches(".mkv"),kind]).unwrap();
        }
        for (node_id, date) in [
            (2, "2026-09-01T10:00:00+08:00"),
            (3, "2026-09-02T01:00:00Z"),
            (4, "2099-01-01T00:00:00Z"),
        ] {
            connection.execute("INSERT INTO media_files(node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(?1,?2,'episode.mkv','mkv',1,?3)",
                params![node_id,format!("fixture-{node_id}.mkv"),date]).unwrap();
        }
        connection.execute("INSERT INTO resource_files(node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(2,'fixture.ass','subtitles.ass','ass',1,'2026-09-03T00:00:00Z')",[]).unwrap();
        assert_eq!(
            database
                .get_node(1)
                .unwrap()
                .latest_file_modified_at
                .as_deref(),
            Some("2026-09-03T00:00:00.000Z")
        );
        assert_eq!(database.get_node(5).unwrap().latest_file_modified_at, None);
        assert_eq!(database.get_node(5).unwrap().last_watched_at, None);
        for (id, time) in [
            (2, "2026-09-01T12:00:00+08:00"),
            (3, "2026-09-01T05:00:00Z"),
            (4, "2099-01-01T00:00:00Z"),
        ] {
            connection.execute("INSERT INTO watch_history(node_id,last_watched_at,watch_count) VALUES(?1,?2,1)", params![id,time]).unwrap();
        }
        assert_eq!(
            database.get_node(1).unwrap().last_watched_at.as_deref(),
            Some("2026-09-01T05:00:00.000Z")
        );
        let nodes = database.list_children(1).unwrap();
        assert_eq!(
            nodes
                .iter()
                .find(|n| n.id == 2)
                .unwrap()
                .latest_file_modified_at
                .as_deref(),
            Some("2026-09-03T00:00:00.000Z")
        );
        let groups = crate::works::group_works(
            nodes
                .into_iter()
                .filter(|n| n.node_type.is_work())
                .collect(),
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].node.last_watched_at.as_deref(),
            Some("2026-09-01T05:00:00.000Z")
        );
        assert_eq!(
            groups[0].node.latest_file_modified_at.as_deref(),
            Some("2026-09-03T00:00:00.000Z")
        );
        connection
            .execute(
                "UPDATE nodes SET display_name='edited title',updated_at='2100-01-01' WHERE id=2",
                [],
            )
            .unwrap();
        assert_eq!(
            database
                .get_node(2)
                .unwrap()
                .latest_file_modified_at
                .as_deref(),
            Some("2026-09-03T00:00:00.000Z")
        );
        connection
            .execute("DELETE FROM resource_files WHERE node_id=2", [])
            .unwrap();
        assert_eq!(
            database
                .get_node(2)
                .unwrap()
                .latest_file_modified_at
                .as_deref(),
            Some("2026-09-01T02:00:00.000Z")
        );
        assert_eq!(
            database
                .get_node(1)
                .unwrap()
                .latest_file_modified_at
                .as_deref(),
            Some("2026-09-02T01:00:00.000Z")
        );
        for (scope, sort) in [
            (CollectionSortScope::All, CollectionSort::ModifiedDesc),
            (CollectionSortScope::Browse, CollectionSort::ModifiedAsc),
            (CollectionSortScope::Favorites, CollectionSort::ModifiedDesc),
        ] {
            database
                .update_collection_sort_preference(scope, sort)
                .unwrap();
        }
        let preferences = database.get_collection_sort_preferences().unwrap();
        assert_eq!(preferences.all, CollectionSort::ModifiedDesc);
        assert_eq!(preferences.browse, CollectionSort::ModifiedAsc);
        assert_eq!(preferences.favorites, CollectionSort::ModifiedDesc);
        database
            .update_collection_sort_preference(
                CollectionSortScope::All,
                CollectionSort::WatchedDesc,
            )
            .unwrap();
        database
            .update_collection_sort_preference(
                CollectionSortScope::Browse,
                CollectionSort::WatchedAsc,
            )
            .unwrap();
        let preferences = database.get_collection_sort_preferences().unwrap();
        assert_eq!(preferences.all, CollectionSort::WatchedDesc);
        assert_eq!(preferences.browse, CollectionSort::WatchedAsc);
    }

    #[test]
    fn library_roots_reject_equal_ancestor_and_descendant_paths() {
        let temp = TempDir::new().unwrap();
        let registered = temp.path().join("Library");
        let child = registered.join("Season 1");
        let prefix_sibling = temp.path().join("Library-Archive");
        fs::create_dir_all(&child).unwrap();
        fs::create_dir_all(&prefix_sibling).unwrap();
        let sentinel = child.join("01.mkv");
        fs::write(&sentinel, b"read only source").unwrap();

        let database = Database::new(temp.path().join("roots.db"));
        database.migrate().unwrap();
        database.add_root(&registered, None).unwrap();

        let equal = database.add_root(&registered.join("."), None).unwrap_err();
        assert!(equal.contains("已经添加"), "{equal}");
        let descendant = database.add_root(&child, None).unwrap_err();
        assert!(descendant.contains("上级或下级"), "{descendant}");
        let ancestor = database.add_root(temp.path(), None).unwrap_err();
        assert!(ancestor.contains("上级或下级"), "{ancestor}");
        database.add_root(&prefix_sibling, None).unwrap();

        assert_eq!(database.list_roots().unwrap().len(), 2);
        assert_eq!(fs::read(sentinel).unwrap(), b"read only source");
    }

    #[test]
    fn scan_guard_rejects_legacy_overlapping_root_rows() {
        let temp = TempDir::new().unwrap();
        let parent = temp.path().join("Library");
        let child = parent.join("Nested");
        fs::create_dir_all(&child).unwrap();

        let database = Database::new(temp.path().join("legacy-overlap.db"));
        database.migrate().unwrap();
        let parent_root = database.add_root(&parent, None).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO library_roots(path,display_name) VALUES(?1,'Nested')",
                [display_path(&child)],
            )
            .unwrap();
        let child_id = connection.last_insert_rowid();
        drop(connection);
        let child_root = database.get_root(child_id).unwrap();

        let parent_error = database.validate_scan_root(&parent_root).unwrap_err();
        assert!(parent_error.contains("上级或下级"), "{parent_error}");
        let child_error = database.validate_scan_root(&child_root).unwrap_err();
        assert!(child_error.contains("上级或下级"), "{child_error}");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_library_root_comparison_is_component_and_case_aware() {
        assert!(library_root_paths_overlap(
            Path::new(r"C:\Media"),
            Path::new(r"c:\MEDIA\Season 2")
        ));
        assert!(!library_root_paths_overlap(
            Path::new(r"C:\Media"),
            Path::new(r"C:\Media-Backup")
        ));
    }

    #[test]
    fn migrations_upgrade_a_real_version_two_shape_idempotently() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("old.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE mediashelf_schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );",
            )
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/0001_initial.sql"))
            .unwrap();
        connection
            .execute(
                "INSERT INTO mediashelf_schema_migrations(version) VALUES(1)",
                [],
            )
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/0002_mvp.sql"))
            .unwrap();
        connection
            .execute(
                "INSERT INTO mediashelf_schema_migrations(version) VALUES(2)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO library_roots(path,display_name) VALUES('D:\\Media','Media')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,absolute_path,folder_name,display_name,node_type,total_video_count
                 ) VALUES(1,'D:\\Media\\Show','Show','Show','WORK',1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO metadata_bindings(
                    node_id,provider,provider_subject_id,provider_title,provider_title_cn
                 ) VALUES(1,'BANGUMI',42,'Original','中文')",
                [],
            )
            .unwrap();
        drop(connection);

        let database = Database::new(path);
        database.migrate().unwrap();
        database
            .connect()
            .unwrap()
            .execute(
                "INSERT INTO confirmed_title_aliases(
                    normalized_alias,original_alias,subject_id,subject_type,source_node_id
                 ) VALUES('legacy observation','Legacy Observation',42,2,1)",
                [],
            )
            .unwrap();
        database.migrate().unwrap();
        let connection = database.connect().unwrap();
        let versions: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM mediashelf_schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(versions, 26);
        connection
            .prepare("SELECT library_root_id,snapshot_json FROM library_scan_snapshots")
            .unwrap();
        let recognition_mode: String = connection
            .query_row(
                "SELECT recognition_mode FROM library_roots WHERE id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(recognition_mode, "FOLDER");
        connection
            .prepare(
                "SELECT cover_download_error,provider_title_en,provider_title_ja,provider_title_ko,
                        provider_subject_type
                 FROM metadata_bindings",
            )
            .unwrap();
        connection
            .prepare("SELECT resource_type,last_seen_at FROM resource_files")
            .unwrap();
        connection
            .prepare("SELECT name,normalized_name,created_at,updated_at FROM tags")
            .unwrap();
        connection
            .prepare("SELECT node_id,tag_id,created_at FROM node_tags")
            .unwrap();
        connection
            .prepare("SELECT node_id,last_watched_at,watch_count FROM watch_history")
            .unwrap();
        connection
            .prepare("SELECT id,name,normalized_name,created_at,updated_at FROM favorite_folders")
            .unwrap();
        connection
            .prepare("SELECT folder_id,node_id,added_at FROM node_favorite_folders")
            .unwrap();
        connection
            .prepare(
                "SELECT normalized_alias,original_alias,subject_id,subject_type,source_node_id,
                        confirmed_at
                 FROM confirmed_title_aliases",
            )
            .unwrap();
        let alias_columns = connection
            .prepare("PRAGMA table_info(confirmed_title_aliases)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            alias_columns,
            vec![
                "normalized_alias",
                "original_alias",
                "subject_id",
                "subject_type",
                "source_node_id",
                "confirmed_at",
            ]
        );
        let preserved_alias: (String, String, i64, i64, i64) = connection
            .query_row(
                "SELECT normalized_alias,original_alias,subject_id,subject_type,source_node_id
                 FROM confirmed_title_aliases",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            preserved_alias,
            (
                "legacy observation".into(),
                "Legacy Observation".into(),
                42,
                2,
                1,
            )
        );
        let preserved: (
            i64,
            i64,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = connection
            .query_row(
                "SELECT provider_subject_id,provider_subject_type,provider_title,provider_title_en,
                        provider_title_ja,provider_title_ko
                 FROM metadata_bindings WHERE node_id=1",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(preserved, (42, 2, "Original".into(), None, None, None));
    }

    #[test]
    fn confirmed_title_aliases_resolve_only_one_observed_subject_and_are_bounded() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        std::fs::create_dir_all(&root_path).unwrap();
        let database = Database::new(temp.path().join("confirmed-aliases.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        let insert_node = |name: &str| {
            connection
                .execute(
                    "INSERT INTO nodes(
                        library_root_id,absolute_path,folder_name,display_name,node_type,
                        total_video_count
                     ) VALUES(?1,?2,?3,?3,'WORK',1)",
                    params![root.id, root_path.join(name).to_string_lossy(), name],
                )
                .unwrap();
            connection.last_insert_rowid()
        };
        let first_node_id = insert_node("First");
        let agreeing_node_id = insert_node("Second");
        let conflicting_node_id = insert_node("Third");
        drop(connection);

        let subject = test_bangumi_subject(100, 2, "Official title");
        let aliases = vec![
            "The Fan Translation".to_string(),
            "THE FAN TRANSLATION".to_string(),
            "Season 2".to_string(),
            r"C:\Private\Anime\The Fan Translation".to_string(),
            "x".repeat(MAX_CONFIRMED_TITLE_ALIAS_CHARS + 50),
        ];
        database
            .save_confirmed_binding_with_aliases(first_node_id, &subject, &aliases)
            .unwrap();
        database
            .save_confirmed_binding_with_aliases(
                agreeing_node_id,
                &subject,
                &["The Fan Translation".into()],
            )
            .unwrap();

        let resolved = database
            .resolve_confirmed_title_alias(&["Unknown title".into(), "THE FAN TRANSLATION".into()])
            .unwrap()
            .unwrap();
        assert_eq!(
            resolved,
            ConfirmedTitleAliasMatch {
                subject_id: subject.subject_id,
                subject_type: subject.subject_type,
                matched_alias: "THE FAN TRANSLATION".into(),
            }
        );

        let connection = database.connect().unwrap();
        let stored = connection
            .prepare(
                "SELECT original_alias,normalized_alias
                 FROM confirmed_title_aliases WHERE source_node_id=?1 ORDER BY original_alias",
            )
            .unwrap()
            .query_map([first_node_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            stored.len(),
            2,
            "duplicates, generic labels, and paths are rejected"
        );
        assert!(stored.iter().all(|(original, normalized)| {
            original.chars().count() <= MAX_CONFIRMED_TITLE_ALIAS_CHARS
                && normalized.chars().count() <= MAX_CONFIRMED_TITLE_ALIAS_CHARS
                && !original.contains("Private")
        }));
        drop(connection);

        let conflicting_subject = test_bangumi_subject(200, 6, "Different official title");
        database
            .save_confirmed_binding_with_aliases(
                conflicting_node_id,
                &conflicting_subject,
                &["Different unique translation".into()],
            )
            .unwrap();
        let priority = database
            .resolve_confirmed_title_alias(&[
                "The Fan Translation".into(),
                "Different unique translation".into(),
            ])
            .unwrap()
            .unwrap();
        assert_eq!(priority.subject_id, subject.subject_id);
        assert_eq!(priority.matched_alias, "The Fan Translation");

        database
            .save_confirmed_binding_with_aliases(
                conflicting_node_id,
                &conflicting_subject,
                &["The Fan Translation".into()],
            )
            .unwrap();
        assert!(database
            .resolve_confirmed_title_alias(&["The Fan Translation".into()])
            .unwrap()
            .is_none());

        let many_aliases = (0..40)
            .map(|index| {
                format!(
                    "Confirmed Alternate {}",
                    char::from_u32(0x4e00 + index).unwrap()
                )
            })
            .collect::<Vec<_>>();
        database
            .save_confirmed_binding_with_aliases(first_node_id, &subject, &many_aliases)
            .unwrap();
        let count: i64 = database
            .connect()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM confirmed_title_aliases WHERE source_node_id=?1",
                [first_node_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, MAX_CONFIRMED_TITLE_ALIASES as i64);
    }

    #[test]
    fn confirmed_title_alias_sanitizing_preserves_season_and_year_qualifiers() {
        let aliases =
            sanitize_confirmed_title_aliases(&["Example Show S2".into(), "Dune 2021".into()]);
        assert_eq!(aliases.len(), 2);
        assert!(aliases.iter().any(
            |(original, normalized)| original == "Example Show S2" && normalized.contains('2')
        ));
        assert!(aliases
            .iter()
            .any(|(original, normalized)| original == "Dune 2021" && normalized.ends_with("2021")));
    }

    #[test]
    fn confirmed_title_aliases_follow_rebind_clear_and_node_cleanup() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        std::fs::create_dir_all(&root_path).unwrap();
        let database = Database::new(temp.path().join("confirmed-alias-cleanup.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,absolute_path,folder_name,display_name,node_type,
                    total_video_count
                 ) VALUES(?1,?2,'Work','Work','WORK',1)",
                params![root.id, root_path.join("Work").to_string_lossy()],
            )
            .unwrap();
        let node_id = connection.last_insert_rowid();
        drop(connection);

        let original = test_bangumi_subject(300, 2, "Original official title");
        database
            .save_confirmed_binding_with_aliases(
                node_id,
                &original,
                &["Remembered fan title".into()],
            )
            .unwrap();
        database.save_confirmed_binding(node_id, &original).unwrap();
        assert!(database
            .resolve_confirmed_title_alias(&["Remembered fan title".into()])
            .unwrap()
            .is_some());

        let replacement = test_bangumi_subject(301, 2, "Replacement official title");
        assert_eq!(
            database
                .save_rematched_binding_if_unchanged(
                    node_id,
                    Some(original.subject_id),
                    &replacement,
                )
                .unwrap(),
            ConditionalBindingSave::Applied(None)
        );
        assert!(database
            .resolve_confirmed_title_alias(&["Remembered fan title".into()])
            .unwrap()
            .is_none());

        database
            .save_confirmed_binding_with_aliases(
                node_id,
                &replacement,
                &["Replacement fan title".into()],
            )
            .unwrap();
        let invalid = test_bangumi_subject(302, 99, "Invalid type");
        assert!(database
            .save_confirmed_binding_with_aliases(node_id, &invalid, &["Must not commit".into()],)
            .is_err());
        assert_eq!(
            database
                .get_binding(node_id)
                .unwrap()
                .unwrap()
                .provider_subject_id,
            replacement.subject_id
        );
        assert!(database
            .resolve_confirmed_title_alias(&["Replacement fan title".into()])
            .unwrap()
            .is_some());

        database.clear_binding(node_id).unwrap();
        assert!(database
            .resolve_confirmed_title_alias(&["Replacement fan title".into()])
            .unwrap()
            .is_none());

        database
            .save_confirmed_binding_with_aliases(
                node_id,
                &replacement,
                &["Cascade fan title".into()],
            )
            .unwrap();
        database
            .connect()
            .unwrap()
            .execute("DELETE FROM nodes WHERE id=?1", [node_id])
            .unwrap();
        assert!(database
            .resolve_confirmed_title_alias(&["Cascade fan title".into()])
            .unwrap()
            .is_none());
    }

    #[test]
    fn recently_watched_is_upserted_sorted_filtered_and_cascaded() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        std::fs::create_dir_all(&root_path).unwrap();
        let database_path = temp.path().join("history.db");
        let database = Database::new(database_path.clone());
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        let insert_node = |name: &str| {
            connection
                .execute(
                    "INSERT INTO nodes(
                        library_root_id,absolute_path,folder_name,display_name,node_type,
                        total_video_count
                     ) VALUES(?1,?2,?3,?3,'WORK',1)",
                    params![root.id, root_path.join(name).to_string_lossy(), name],
                )
                .unwrap();
            connection.last_insert_rowid()
        };
        let older_id = insert_node("Older");
        let newer_id = insert_node("Newer");
        let ignored_id = insert_node("Ignored");
        drop(connection);

        database.record_node_watched(older_id).unwrap();
        database.record_node_watched(older_id).unwrap();
        database.record_node_watched(newer_id).unwrap();
        database.record_node_watched(ignored_id).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "UPDATE watch_history SET last_watched_at='2026-08-22T10:00:00.000Z'
                 WHERE node_id=?1",
                [older_id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE watch_history SET last_watched_at='2026-08-22T11:00:00.000Z'
                 WHERE node_id=?1",
                [newer_id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE watch_history SET last_watched_at='2026-08-22T12:00:00.000Z'
                 WHERE node_id=?1",
                [ignored_id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE nodes SET node_type='IGNORED' WHERE id=?1",
                [ignored_id],
            )
            .unwrap();
        let older_count: i64 = connection
            .query_row(
                "SELECT watch_count FROM watch_history WHERE node_id=?1",
                [older_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(older_count, 2);
        drop(connection);

        let reopened = Database::new(database_path);
        let entries = reopened.list_recently_watched().unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.node.id)
                .collect::<Vec<_>>(),
            vec![newer_id, older_id]
        );
        assert_eq!(entries[0].watched_at, "2026-08-22T11:00:00.000Z");

        let connection = reopened.connect().unwrap();
        connection
            .execute("DELETE FROM nodes WHERE id=?1", [newer_id])
            .unwrap();
        let remaining: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM watch_history WHERE node_id=?1",
                [newer_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);
        drop(connection);
        assert_eq!(
            reopened
                .list_recently_watched()
                .unwrap()
                .into_iter()
                .map(|entry| entry.node.id)
                .collect::<Vec<_>>(),
            vec![older_id]
        );
    }

    #[test]
    fn favorite_folders_are_many_to_many_persistent_ordered_and_cascaded() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        std::fs::create_dir_all(&root_path).unwrap();
        let database_path = temp.path().join("favorites.db");
        let database = Database::new(database_path.clone());
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        let insert_node = |name: &str| {
            connection
                .execute(
                    "INSERT INTO nodes(
                        library_root_id,absolute_path,folder_name,display_name,node_type,
                        total_video_count
                     ) VALUES(?1,?2,?3,?3,'WORK',1)",
                    params![root.id, root_path.join(name).to_string_lossy(), name],
                )
                .unwrap();
            connection.last_insert_rowid()
        };
        let first_id = insert_node("First");
        let second_id = insert_node("Second");
        let third_id = insert_node("Third");
        drop(connection);

        let watching = database.create_favorite_folder("  正在   看 ").unwrap();
        assert_eq!(watching.name, "正在 看");
        let archive = database.create_favorite_folder("Archive 2").unwrap();
        let archive_ten = database.create_favorite_folder("Archive 10").unwrap();
        assert!(database
            .create_favorite_folder("ARCHIVE 2")
            .unwrap_err()
            .contains("同名"));

        let added = database
            .batch_add_nodes_to_favorite(watching.id, &[first_id, second_id, first_id])
            .unwrap();
        assert_eq!(
            added,
            BatchMutationResult {
                requested: 2,
                updated: 2,
                skipped: 0,
            }
        );
        assert_eq!(
            database
                .batch_add_nodes_to_favorite(watching.id, &[first_id, second_id])
                .unwrap(),
            BatchMutationResult {
                requested: 2,
                updated: 0,
                skipped: 2,
            }
        );
        database
            .batch_add_nodes_to_favorite(archive.id, &[first_id, third_id])
            .unwrap();

        let connection = database.connect().unwrap();
        connection
            .execute(
                "UPDATE node_favorite_folders SET added_at='2026-08-22T10:00:00.000Z'
                 WHERE folder_id=?1 AND node_id=?2",
                params![watching.id, first_id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE node_favorite_folders SET added_at='2026-08-22T11:00:00.000Z'
                 WHERE folder_id=?1 AND node_id=?2",
                params![watching.id, second_id],
            )
            .unwrap();
        drop(connection);

        assert_eq!(
            database
                .list_favorite_folder_nodes(watching.id)
                .unwrap()
                .into_iter()
                .map(|node| node.id)
                .collect::<Vec<_>>(),
            vec![second_id, first_id]
        );
        let folders = database.list_favorite_folders().unwrap();
        assert_eq!(
            folders
                .iter()
                .map(|folder| folder.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Archive 2", "Archive 10", "正在 看"]
        );
        assert_eq!(
            folders
                .iter()
                .find(|folder| folder.id == watching.id)
                .unwrap()
                .item_count,
            2
        );

        let reopened = Database::new(database_path);
        assert_eq!(
            reopened
                .list_favorite_folder_nodes(archive.id)
                .unwrap()
                .len(),
            2,
            "favorite membership must persist independently of scans and process lifetime"
        );
        let renamed = reopened
            .rename_favorite_folder(archive_ten.id, "  已   归档 ")
            .unwrap();
        assert_eq!(renamed.name, "已 归档");
        assert!(reopened
            .rename_favorite_folder(archive_ten.id, "archive 2")
            .unwrap_err()
            .contains("同名"));

        assert!(reopened
            .batch_add_nodes_to_favorite(watching.id, &[third_id, 999_999])
            .unwrap_err()
            .contains("未执行"));
        assert!(!reopened
            .list_favorite_folder_nodes(watching.id)
            .unwrap()
            .iter()
            .any(|node| node.id == third_id));
        assert!(reopened
            .batch_remove_nodes_from_favorite(999_999, &[first_id])
            .unwrap_err()
            .contains("不存在"));

        let removed = reopened
            .batch_remove_nodes_from_favorite(watching.id, &[first_id, second_id, first_id])
            .unwrap();
        assert_eq!(removed.requested, 2);
        assert_eq!(removed.updated, 2);
        assert_eq!(removed.skipped, 0);
        assert!(reopened
            .list_favorite_folder_nodes(watching.id)
            .unwrap()
            .is_empty());

        let connection = reopened.connect().unwrap();
        connection
            .execute("DELETE FROM nodes WHERE id=?1", [first_id])
            .unwrap();
        drop(connection);
        assert_eq!(
            reopened
                .list_favorite_folder_nodes(archive.id)
                .unwrap()
                .into_iter()
                .map(|node| node.id)
                .collect::<Vec<_>>(),
            vec![third_id]
        );
        reopened.delete_favorite_folder(archive.id).unwrap();
        assert!(reopened.list_favorite_folder_nodes(archive.id).is_err());
        let connection = reopened.connect().unwrap();
        let dangling_memberships: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM node_favorite_folders WHERE folder_id=?1",
                [archive.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(dangling_memberships, 0);
    }

    #[test]
    fn favorite_folder_names_and_batches_are_bounded() {
        assert!(normalize_favorite_folder_name(" \t\n ").is_err());
        assert!(normalize_favorite_folder_name(&"a".repeat(81)).is_err());
        assert_eq!(
            normalize_favorite_folder_name("  My   Favorites ").unwrap(),
            ("My Favorites".into(), "my favorites".into())
        );

        let temp = TempDir::new().unwrap();
        let database = Database::new(temp.path().join("favorites-bounds.db"));
        database.migrate().unwrap();
        let folder = database.create_favorite_folder("Favorites").unwrap();
        assert!(database
            .batch_add_nodes_to_favorite(folder.id, &[])
            .is_err());
        assert!(database
            .batch_remove_nodes_from_favorite(
                folder.id,
                &(1..=MAX_BATCH_NODE_IDS as i64 + 1).collect::<Vec<_>>()
            )
            .unwrap_err()
            .contains("最多"));
    }

    #[test]
    fn user_tags_are_normalized_persistent_searchable_and_global() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("媒体 资料库");
        std::fs::create_dir_all(&root_path).unwrap();
        let database_path = temp.path().join("tags.db");
        let database = Database::new(database_path.clone());
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,absolute_path,folder_name,display_name,node_type,total_video_count
                 ) VALUES(?1,?2,'作品一','作品一','WORK',1)",
                params![root.id, root_path.join("作品一").to_string_lossy()],
            )
            .unwrap();
        let first_node_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,absolute_path,folder_name,display_name,node_type,total_video_count
                 ) VALUES(?1,?2,'作品二','作品二','WORK',1)",
                params![root.id, root_path.join("作品二").to_string_lossy()],
            )
            .unwrap();
        let second_node_id = connection.last_insert_rowid();
        drop(connection);

        let watch = database
            .create_or_assign_user_tag(first_node_id, "  待   看  ")
            .unwrap();
        assert_eq!(watch.name, "待 看");
        let same_watch = database
            .create_or_assign_user_tag(first_node_id, "待 看")
            .unwrap();
        assert_eq!(same_watch.id, watch.id);

        let favorite = database
            .create_or_assign_user_tag(first_node_id, "Favorites")
            .unwrap();
        let same_favorite = database
            .create_or_assign_user_tag(second_node_id, "favorites")
            .unwrap();
        assert_eq!(same_favorite.id, favorite.id);
        database.assign_user_tag(second_node_id, watch.id).unwrap();

        let first = database.get_node(first_node_id).unwrap();
        assert_eq!(first.user_tags.len(), 2);
        let memberships = database.list_user_tags(first_node_id).unwrap();
        assert_eq!(memberships.len(), 2);
        assert!(memberships.iter().all(|tag| tag.assigned));

        let renamed = database.rename_user_tag(watch.id, "  稍后 看 ").unwrap();
        assert_eq!(renamed.name, "稍后 看");
        assert!(database
            .rename_user_tag(renamed.id, "FAVORITES")
            .unwrap_err()
            .contains("同名"));
        let reopened = Database::new(database_path);
        assert!(reopened
            .get_node(second_node_id)
            .unwrap()
            .user_tags
            .iter()
            .any(|tag| tag.name == "稍后 看"));
        let hits = reopened.search("稍后", None).unwrap();
        assert_eq!(hits.len(), 2);

        reopened
            .unassign_user_tag(first_node_id, renamed.id)
            .unwrap();
        assert!(
            !reopened
                .list_user_tags(first_node_id)
                .unwrap()
                .into_iter()
                .find(|tag| tag.id == renamed.id)
                .unwrap()
                .assigned
        );
        assert!(reopened
            .get_node(second_node_id)
            .unwrap()
            .user_tags
            .iter()
            .any(|tag| tag.id == renamed.id));

        reopened.delete_user_tag(renamed.id).unwrap();
        assert!(reopened
            .get_node(second_node_id)
            .unwrap()
            .user_tags
            .iter()
            .all(|tag| tag.id != renamed.id));
        assert!(reopened.delete_user_tag(renamed.id).is_err());
    }

    #[test]
    fn hidden_entries_include_nested_and_resource_only_nodes_and_restore_preserves_metadata() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        let work_path = root_path.join("作品");
        fs::create_dir_all(&work_path).unwrap();
        let sentinel = work_path.join("01.mkv");
        fs::write(&sentinel, b"source remains read only").unwrap();
        let database = Database::new(temp.path().join("hidden.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        let insert = |parent: Option<i64>, path: &Path, name: &str| {
            connection.execute(
                "INSERT INTO nodes(library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type)
                 VALUES(?1,?2,?3,?4,?4,'CONTAINER')",
                params![root.id, parent, path.to_string_lossy(), name],
            ).unwrap();
            connection.last_insert_rowid()
        };
        let root_id = insert(None, &root_path, "library");
        let work_id = insert(Some(root_id), &work_path, "自定义名称");
        let child_id = insert(Some(work_id), &work_path.join("Extras"), "Hidden resources");
        let visible_id = insert(Some(work_id), &work_path.join("Other"), "Visible child");
        connection.execute(
            "INSERT INTO media_files(node_id,absolute_path,file_name,extension,file_size,modified_at)
             VALUES(?1,?2,'01.mkv','mkv',24,'2026-01-01')",
            params![work_id, sentinel.to_string_lossy()],
        ).unwrap();
        drop(connection);
        database.reset_node_type(work_id).unwrap();
        database
            .save_confirmed_binding(work_id, &test_bangumi_subject(42, 2, "Bound Work"))
            .unwrap();
        let tag = database
            .create_or_assign_user_tag(work_id, "Keep tag")
            .unwrap();
        let favorite = database.create_favorite_folder("Keep favorite").unwrap();
        database
            .batch_add_nodes_to_favorite(favorite.id, &[work_id])
            .unwrap();
        database.set_node_type(work_id, NodeType::Ignored).unwrap();
        database.set_node_type(child_id, NodeType::Ignored).unwrap();
        let hidden = database.list_hidden_nodes().unwrap();
        assert_eq!(hidden.len(), 2);
        assert!(hidden
            .iter()
            .any(|node| node.id == child_id && node.total_video_count == 0));
        assert!(!hidden
            .iter()
            .any(|node| node.id == root_id || node.id == visible_id));
        let before = hidden.iter().find(|node| node.id == work_id).unwrap();
        assert_eq!(before.binding.as_ref().unwrap().provider_subject_id, 42);
        assert_eq!(before.user_tags[0].id, tag.id);
        let restored = database.reset_node_type(work_id).unwrap();
        assert_eq!(restored.node_type, NodeType::AutoWork);
        assert!(!restored.manual_type_override);
        assert_eq!(restored.display_name, before.display_name);
        assert_eq!(restored.cover_cache_path, before.cover_cache_path);
        assert_eq!(restored.binding.as_ref().unwrap().provider_subject_id, 42);
        assert_eq!(restored.user_tags[0].id, tag.id);
        assert_eq!(
            database.list_favorite_folder_nodes(favorite.id).unwrap()[0].id,
            work_id
        );
        assert_eq!(database.list_children(root_id).unwrap()[0].id, work_id);
        assert_eq!(database.list_hidden_nodes().unwrap()[0].id, child_id);
        assert_eq!(fs::read(&sentinel).unwrap(), b"source remains read only");

        // The application index remains usable when a drive disappears.
        fs::rename(&root_path, temp.path().join("offline-library")).unwrap();
        assert_eq!(database.list_hidden_nodes().unwrap()[0].id, child_id);
        database.reset_node_type(child_id).unwrap();
        assert!(database.list_hidden_nodes().unwrap().is_empty());
    }

    #[test]
    fn user_tag_names_reject_empty_and_overlong_values() {
        assert!(normalize_tag_name(" \t\n ").is_err());
        assert!(normalize_tag_name(&"a".repeat(41)).is_err());
        assert_eq!(
            normalize_tag_name("  My   Tag ").unwrap(),
            ("My Tag".into(), "my tag".into())
        );
    }

    #[test]
    fn batch_mutations_are_deduplicated_transactional_and_preserve_single_item_semantics() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        let first_path = root_path.join("First");
        let second_path = root_path.join("Second");
        std::fs::create_dir_all(&first_path).unwrap();
        std::fs::create_dir_all(&second_path).unwrap();
        let database = Database::new(temp.path().join("batch.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        let insert_node = |path: &Path, name: &str| {
            connection
                .execute(
                    "INSERT INTO nodes(
                        library_root_id,absolute_path,folder_name,display_name,node_type
                     ) VALUES(?1,?2,?3,?3,'CONTAINER')",
                    params![root.id, path.to_string_lossy(), name],
                )
                .unwrap();
            connection.last_insert_rowid()
        };
        let first_id = insert_node(&first_path, "First");
        let second_id = insert_node(&second_path, "Second");
        connection
            .execute(
                "INSERT INTO media_files(
                    node_id,absolute_path,file_name,extension,file_size,modified_at
                 ) VALUES(?1,?2,'01.mkv','mkv',1,'2026-08-22T00:00:00Z')",
                params![first_id, first_path.join("01.mkv").to_string_lossy()],
            )
            .unwrap();
        drop(connection);

        let set_result = database
            .batch_set_node_type(&[second_id, first_id, first_id], NodeType::Mixed)
            .unwrap();
        assert_eq!(
            set_result,
            BatchMutationResult {
                requested: 2,
                updated: 2,
                skipped: 0,
            }
        );
        for node_id in [first_id, second_id] {
            let node = database.get_node(node_id).unwrap();
            assert_eq!(node.node_type, NodeType::Mixed);
            assert!(node.manual_type_override);
        }

        assert!(database
            .batch_set_node_type(&[first_id, 999_999], NodeType::Ignored)
            .unwrap_err()
            .contains("未执行"));
        assert_eq!(
            database.get_node(first_id).unwrap().node_type,
            NodeType::Mixed,
            "a missing Node must roll back the entire batch"
        );

        let tag_result = database
            .batch_create_and_assign_tag(&[first_id, second_id, first_id], "  批量   标签 ")
            .unwrap();
        assert_eq!(tag_result.requested, 2);
        assert_eq!(tag_result.updated, 2);
        assert_eq!(tag_result.skipped, 0);
        let tag_id = database
            .list_user_tags(first_id)
            .unwrap()
            .into_iter()
            .find(|tag| tag.name == "批量 标签")
            .unwrap()
            .id;
        let duplicate_assign = database
            .batch_assign_tag(&[first_id, second_id], tag_id)
            .unwrap();
        assert_eq!(duplicate_assign.updated, 0);
        assert_eq!(duplicate_assign.skipped, 2);
        assert!(database
            .batch_assign_tag(&[first_id, second_id], 999_999)
            .is_err());

        let reset_result = database
            .batch_reset_node_type(&[second_id, first_id, first_id])
            .unwrap();
        assert_eq!(reset_result.requested, 2);
        assert_eq!(reset_result.updated, 2);
        assert_eq!(reset_result.skipped, 0);
        assert!(!database.get_node(first_id).unwrap().manual_type_override);
        assert!(!database.get_node(second_id).unwrap().manual_type_override);
        assert_eq!(
            database.get_node(first_id).unwrap().node_type,
            NodeType::AutoWork
        );

        assert!(database.batch_reset_node_type(&[]).is_err());
        assert!(database
            .batch_reset_node_type(&(1..=MAX_BATCH_NODE_IDS as i64 + 1).collect::<Vec<_>>())
            .unwrap_err()
            .contains("最多"));
    }

    #[test]
    fn local_search_matches_all_bound_title_languages() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        std::fs::create_dir_all(&root_path).unwrap();
        let database = Database::new(temp.path().join("search.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,absolute_path,folder_name,display_name,node_type,total_video_count
                 ) VALUES(?1,?2,'release-folder','release-folder','WORK',1)",
                params![root.id, root_path.join("release-folder").to_string_lossy()],
            )
            .unwrap();
        let node_id = connection.last_insert_rowid();
        drop(connection);

        let subject = crate::models::BangumiSubject {
            subject_id: 1096,
            title: "Code Geass: Hangyaku no Lelouch".into(),
            title_cn: Some("反叛的鲁路修".into()),
            title_en: Some("Code Geass: Lelouch of the Rebellion".into()),
            title_ja: Some("コードギアス 反逆のルルーシュ".into()),
            title_ko: Some("코드 기아스 반역의 를르슈".into()),
            match_aliases: Vec::new(),
            date: None,
            image_url: None,
            summary: None,
            subject_type: 2,
        };
        database.save_confirmed_binding(node_id, &subject).unwrap();

        for query in ["Hangyaku", "反叛", "Lelouch", "ルルーシュ", "를르슈"] {
            let hits = database.search(query, None).unwrap();
            assert_eq!(hits.len(), 1, "query {query:?} should find the bound Node");
            assert_eq!(hits[0].node.id, node_id);
        }
    }

    #[test]
    fn auto_bind_candidates_are_unbound_bindable_visible_nodes_and_mixed_is_manual() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        std::fs::create_dir_all(&root_path).unwrap();
        let database = Database::new(temp.path().join("candidates.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,absolute_path,folder_name,display_name,node_type,total_video_count
                 ) VALUES(?1,?2,'library','library','CONTAINER',10)",
                params![root.id, root_path.to_string_lossy()],
            )
            .unwrap();
        let hidden_root_id = connection.last_insert_rowid();
        let insert_node = |name: &str, node_type: &str, total_video_count: i64| {
            connection
                .execute(
                    "INSERT INTO nodes(
                        library_root_id,parent_node_id,absolute_path,folder_name,display_name,
                        node_type,total_video_count,direct_video_count
                     ) VALUES(?1,?2,?3,?4,?4,?5,?6,CASE WHEN ?5 IN ('WORK','AUTO_WORK') THEN ?6 ELSE 0 END)",
                    params![
                        root.id,
                        hidden_root_id,
                        root_path.join(name).to_string_lossy(),
                        name,
                        node_type,
                        total_video_count
                    ],
                )
                .unwrap();
            connection.last_insert_rowid()
        };
        let auto_work_id = insert_node("Auto", "AUTO_WORK", 1);
        let bound_work_id = insert_node("Bound", "WORK", 1);
        let mixed_id = insert_node("Mixed", "MIXED", 2);
        let _video_container_id = insert_node("Series", "CONTAINER", 4);
        let _resource_container_id = insert_node("Resources", "CONTAINER", 0);
        let ignored_parent_id = insert_node("Ignored branch", "IGNORED", 1);
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,parent_node_id,absolute_path,folder_name,display_name,
                    node_type,total_video_count
                 ) VALUES(?1,?2,?3,'Hidden work','Hidden work','WORK',1)",
                params![
                    root.id,
                    ignored_parent_id,
                    root_path
                        .join("Ignored branch/Hidden work")
                        .to_string_lossy()
                ],
            )
            .unwrap();
        drop(connection);

        let subject = crate::models::BangumiSubject {
            subject_id: 1,
            title: "Bound".into(),
            title_cn: None,
            title_en: None,
            title_ja: None,
            title_ko: None,
            match_aliases: Vec::new(),
            date: None,
            image_url: None,
            summary: None,
            subject_type: 2,
        };
        database
            .save_confirmed_binding(bound_work_id, &subject)
            .unwrap();

        let candidate_ids = database
            .list_unbound_bangumi_candidates(root.id)
            .unwrap()
            .into_iter()
            .map(|node| node.id)
            .collect::<Vec<_>>();
        assert_eq!(candidate_ids, vec![auto_work_id]);

        let all_unbound_ids = database
            .list_bangumi_match_candidates(None, false)
            .unwrap()
            .into_iter()
            .map(|node| node.id)
            .collect::<Vec<_>>();
        assert_eq!(all_unbound_ids, vec![auto_work_id]);
        let all_including_bound_ids = database
            .list_bangumi_match_candidates(None, true)
            .unwrap()
            .into_iter()
            .map(|node| node.id)
            .collect::<Vec<_>>();
        assert_eq!(all_including_bound_ids, vec![auto_work_id, bound_work_id]);
        let selected_ids = database
            .list_bangumi_match_candidates(Some(&[mixed_id, bound_work_id, bound_work_id]), true)
            .unwrap();
        assert_eq!(selected_ids.len(), 1);
        assert_eq!(selected_ids[0].id, bound_work_id);
        assert_eq!(
            selected_ids[0]
                .binding
                .as_ref()
                .map(|binding| binding.provider_subject_id),
            Some(subject.subject_id)
        );
        assert!(database
            .list_bangumi_match_candidates(Some(&[]), false)
            .is_err());

        let competing_subject = crate::models::BangumiSubject {
            subject_id: 2,
            title: "Manual choice".into(),
            title_cn: None,
            title_en: None,
            title_ja: None,
            title_ko: None,
            match_aliases: Vec::new(),
            date: None,
            image_url: None,
            summary: None,
            subject_type: 2,
        };
        assert!(!database
            .save_binding_if_absent(bound_work_id, &competing_subject)
            .unwrap());
        assert_eq!(
            database
                .get_binding(bound_work_id)
                .unwrap()
                .unwrap()
                .provider_subject_id,
            subject.subject_id
        );

        let automatic_subject = crate::models::BangumiSubject {
            subject_id: 3,
            title: "Auto".into(),
            title_cn: None,
            title_en: None,
            title_ja: None,
            title_ko: None,
            match_aliases: Vec::new(),
            date: None,
            image_url: None,
            summary: None,
            subject_type: 2,
        };
        assert!(database
            .save_binding_if_absent(auto_work_id, &automatic_subject)
            .unwrap());
        assert!(database
            .set_binding_cover_error_if_subject(
                auto_work_id,
                automatic_subject.subject_id,
                Some("automatic cover failed"),
            )
            .unwrap());
        let automatic_cover = temp.path().join("automatic-subject-cover.jpg");
        assert!(database
            .set_bangumi_cover_for_subject_unless_manual(
                auto_work_id,
                automatic_subject.subject_id,
                &automatic_cover,
            )
            .unwrap());
        let replaced_path = database
            .save_confirmed_binding(auto_work_id, &competing_subject)
            .unwrap();
        assert_eq!(replaced_path.as_deref(), Some(automatic_cover.as_path()));
        assert_eq!(
            database.get_node(auto_work_id).unwrap().cover_source,
            CoverSource::Placeholder
        );
        assert!(!database
            .set_bangumi_cover_for_subject_unless_manual(
                auto_work_id,
                automatic_subject.subject_id,
                &temp.path().join("stale-automatic-cover.jpg"),
            )
            .unwrap());
        assert!(!database
            .set_binding_cover_error_if_subject(
                auto_work_id,
                automatic_subject.subject_id,
                Some("stale automatic failure"),
            )
            .unwrap());
        let confirmed_cover = temp.path().join("confirmed-subject-cover.jpg");
        assert!(database
            .set_bangumi_cover_for_subject_unless_manual(
                auto_work_id,
                competing_subject.subject_id,
                &confirmed_cover,
            )
            .unwrap());
        assert_eq!(
            database.clear_binding(auto_work_id).unwrap().as_deref(),
            Some(confirmed_cover.as_path())
        );
        assert!(database.get_binding(auto_work_id).unwrap().is_none());
        assert_eq!(
            database.get_node(auto_work_id).unwrap().cover_source,
            CoverSource::Placeholder
        );
        database
            .save_confirmed_binding(auto_work_id, &competing_subject)
            .unwrap();

        assert_eq!(
            database
                .save_rematched_binding_if_unchanged(
                    auto_work_id,
                    Some(automatic_subject.subject_id),
                    &subject,
                )
                .unwrap(),
            ConditionalBindingSave::Stale
        );
        let mut refreshed_competing_subject = competing_subject.clone();
        refreshed_competing_subject.title = "Manual choice refreshed".into();
        assert_eq!(
            database
                .save_rematched_binding_if_unchanged(
                    auto_work_id,
                    Some(competing_subject.subject_id),
                    &refreshed_competing_subject,
                )
                .unwrap(),
            ConditionalBindingSave::Applied(None)
        );
        assert_eq!(
            database
                .get_binding(auto_work_id)
                .unwrap()
                .unwrap()
                .provider_title,
            "Manual choice refreshed"
        );

        let mixed = database
            .set_node_type(auto_work_id, NodeType::Mixed)
            .unwrap();
        assert_eq!(mixed.node_type, NodeType::Mixed);
        assert!(mixed.manual_type_override);
        assert!(database
            .set_node_type(mixed_id, NodeType::AutoWork)
            .unwrap_err()
            .contains("其他资源"));

        let manual_path = temp.path().join("manual-cover.png");
        database
            .set_node_cover(auto_work_id, CoverSource::Manual, Some(&manual_path))
            .unwrap();
        assert!(!database
            .set_bangumi_cover_for_subject_unless_manual(
                auto_work_id,
                competing_subject.subject_id,
                &temp.path().join("automatic-cover.jpg"),
            )
            .unwrap());
        let after_cover_race = database.get_node(auto_work_id).unwrap();
        assert_eq!(after_cover_race.cover_source, CoverSource::Manual);
        assert_eq!(
            after_cover_race.cover_cache_path.as_deref(),
            manual_path.to_str()
        );
    }

    #[test]
    fn cleared_bangumi_cover_becomes_a_recovery_candidate_without_losing_binding() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        std::fs::create_dir_all(root_path.join("Movie")).unwrap();
        let database = Database::new(temp.path().join("cover-recovery.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,
                    total_video_count
                 ) VALUES(?1,NULL,?2,'library','library','CONTAINER',1)",
                params![root.id, root_path.to_string_lossy()],
            )
            .unwrap();
        let root_node_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,
                    direct_video_count,total_video_count
                 ) VALUES(?1,?2,?3,'Movie','Movie','WORK',1,1)",
                params![
                    root.id,
                    root_node_id,
                    root_path.join("Movie").to_string_lossy()
                ],
            )
            .unwrap();
        let node_id = connection.last_insert_rowid();
        drop(connection);

        let subject = crate::models::BangumiSubject {
            subject_id: 325392,
            title: "The Sword of Doom".into(),
            title_cn: None,
            title_en: Some("The Sword of Doom".into()),
            title_ja: None,
            title_ko: None,
            match_aliases: Vec::new(),
            date: Some("1966-02-25".into()),
            image_url: Some("https://lain.bgm.tv/pic/cover/l/test.jpg".into()),
            summary: None,
            subject_type: crate::bangumi::SUBJECT_TYPE_LIVE_ACTION,
        };
        assert!(database.save_binding_if_absent(node_id, &subject).unwrap());
        let old_cover = temp.path().join("cache/bangumi/325392.jpg");
        std::fs::create_dir_all(old_cover.parent().unwrap()).unwrap();
        std::fs::write(&old_cover, b"cached").unwrap();
        assert!(database
            .set_bangumi_cover_for_subject_unless_manual(node_id, subject.subject_id, &old_cover)
            .unwrap());
        assert!(database
            .list_unbound_bangumi_candidates(root.id)
            .unwrap()
            .iter()
            .all(|node| node.id != node_id));
        std::fs::remove_file(&old_cover).unwrap();
        assert!(database
            .list_unbound_bangumi_candidates(root.id)
            .unwrap()
            .iter()
            .any(|node| node.id == node_id));

        database
            .clear_cover_paths_for_nodes(&[(node_id, CoverSource::Bangumi, old_cover)])
            .unwrap();

        assert!(database
            .list_unbound_bangumi_candidates(root.id)
            .unwrap()
            .iter()
            .any(|node| node.id == node_id));
        assert!(database
            .list_bangumi_match_candidates(Some(&[node_id]), false)
            .unwrap()
            .iter()
            .any(|node| node.id == node_id));
        let binding = database.get_binding(node_id).unwrap().unwrap();
        assert_eq!(binding.provider_subject_id, subject.subject_id);
        assert_eq!(
            binding.provider_subject_type,
            crate::bangumi::SUBJECT_TYPE_LIVE_ACTION
        );
        assert_eq!(binding.provider_image_url, subject.image_url);
        assert!(binding.cover_cache_path.is_none());
        assert!(binding
            .cover_download_error
            .as_deref()
            .is_some_and(|error| error.contains("重新获取")));
    }

    #[test]
    fn automatic_matching_uses_owned_work_sources_and_explicit_boundaries() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        std::fs::create_dir_all(&root_path).unwrap();
        let database = Database::new(temp.path().join("supplement-candidates.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,absolute_path,folder_name,display_name,node_type,total_video_count
                 ) VALUES(?1,?2,'library','library','CONTAINER',20)",
                params![root.id, root_path.to_string_lossy()],
            )
            .unwrap();
        let hidden_root_id = connection.last_insert_rowid();
        let insert_node = |parent_id: i64,
                           name: &str,
                           node_type: &str,
                           manually_classified: bool,
                           direct_video_count: i64,
                           total_video_count: i64| {
            connection
                .execute(
                    "INSERT INTO nodes(
                        library_root_id,parent_node_id,absolute_path,folder_name,display_name,
                        node_type,manual_type_override,direct_video_count,total_video_count
                     ) VALUES(?1,?2,?3,?4,?4,?5,?6,?7,?8)",
                    params![
                        root.id,
                        parent_id,
                        root_path.join(name).to_string_lossy(),
                        name,
                        node_type,
                        manually_classified,
                        direct_video_count,
                        total_video_count
                    ],
                )
                .unwrap();
            connection.last_insert_rowid()
        };

        let direct_video_parent_id =
            insert_node(hidden_root_id, "Main work", "AUTO_WORK", false, 12, 14);
        let automatic_sp_id =
            insert_node(direct_video_parent_id, "SP 01", "AUTO_WORK", false, 1, 1);
        let manual_ova_id = insert_node(direct_video_parent_id, "OVA", "WORK", true, 1, 1);
        let no_direct_video_parent_id =
            insert_node(hidden_root_id, "Series", "CONTAINER", false, 0, 1);
        let standalone_specials_id = insert_node(
            no_direct_video_parent_id,
            "Specials",
            "AUTO_WORK",
            false,
            1,
            1,
        );
        drop(connection);

        let scan_candidate_ids = database
            .list_unbound_bangumi_candidates(root.id)
            .unwrap()
            .into_iter()
            .map(|node| node.id)
            .collect::<Vec<_>>();
        assert!(!scan_candidate_ids.contains(&automatic_sp_id));
        assert!(scan_candidate_ids.contains(&direct_video_parent_id));
        assert!(scan_candidate_ids.contains(&manual_ova_id));
        assert!(!scan_candidate_ids.contains(&standalone_specials_id));

        let existing_candidate_ids = database
            .list_bangumi_match_candidates(
                Some(&[automatic_sp_id, manual_ova_id, standalone_specials_id]),
                false,
            )
            .unwrap()
            .into_iter()
            .map(|node| node.id)
            .collect::<Vec<_>>();
        assert_eq!(existing_candidate_ids, vec![manual_ova_id]);

        // Structural automatic exclusion must not remove the existing manual search/bind path.
        let automatic_sp = database.get_node(automatic_sp_id).unwrap();
        assert!(automatic_sp.can_bind_bangumi());
        let manually_confirmed_subject = crate::models::BangumiSubject {
            subject_id: 42,
            title: "Manually confirmed SP".into(),
            title_cn: None,
            title_en: None,
            title_ja: None,
            title_ko: None,
            match_aliases: Vec::new(),
            date: None,
            image_url: None,
            summary: None,
            subject_type: 2,
        };
        database
            .save_confirmed_binding(automatic_sp_id, &manually_confirmed_subject)
            .unwrap();
        assert_eq!(
            database
                .get_binding(automatic_sp_id)
                .unwrap()
                .unwrap()
                .provider_subject_id,
            manually_confirmed_subject.subject_id
        );
        assert_eq!(
            database
                .list_bangumi_match_candidates(Some(&[automatic_sp_id]), true)
                .unwrap()
                .iter()
                .map(|node| node.id)
                .collect::<Vec<_>>(),
            vec![automatic_sp_id],
            "an explicit standalone binding establishes an independent source boundary"
        );
    }

    #[test]
    fn container_binding_and_cover_error_survive_database_reload() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        std::fs::create_dir_all(&root_path).unwrap();
        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,
                    total_video_count
                 ) VALUES(?1,NULL,?2,'系列','系列','CONTAINER',1)",
                params![root.id, root_path.join("系列").to_string_lossy()],
            )
            .unwrap();
        let node_id = connection.last_insert_rowid();
        drop(connection);

        let subject = crate::models::BangumiSubject {
            subject_id: 123,
            title: "Series".into(),
            title_cn: Some("系列".into()),
            title_en: Some("Series EN".into()),
            title_ja: Some("シリーズ".into()),
            title_ko: Some("시리즈".into()),
            match_aliases: Vec::new(),
            date: None,
            image_url: Some("https://lain.bgm.tv/pic/cover/l/test.jpg".into()),
            summary: None,
            subject_type: 2,
        };
        database.save_confirmed_binding(node_id, &subject).unwrap();
        assert!(database
            .set_binding_cover_error_if_subject(
                node_id,
                subject.subject_id,
                Some("network unavailable"),
            )
            .unwrap());
        drop(database.connect().unwrap());
        let binding = database.get_binding(node_id).unwrap().unwrap();
        assert_eq!(binding.provider_subject_id, 123);
        assert_eq!(binding.provider_title_en.as_deref(), Some("Series EN"));
        assert_eq!(binding.provider_title_ja.as_deref(), Some("シリーズ"));
        assert_eq!(binding.provider_title_ko.as_deref(), Some("시리즈"));
        assert_eq!(
            binding.cover_download_error.as_deref(),
            Some("network unavailable")
        );

        let connection = database.connect().unwrap();
        connection
            .execute("UPDATE nodes SET node_type='MIXED' WHERE id=?1", [node_id])
            .unwrap();
        drop(connection);
        assert!(database.save_confirmed_binding(node_id, &subject).is_err());

        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,
                    total_video_count
                 ) VALUES(?1,NULL,?2,'纯资源','纯资源','CONTAINER',0)",
                params![root.id, root_path.join("纯资源").to_string_lossy()],
            )
            .unwrap();
        let resource_only_container_id = connection.last_insert_rowid();
        drop(connection);
        let error = database
            .save_confirmed_binding(resource_only_container_id, &subject)
            .unwrap_err();
        assert!(error.contains("包含视频的系列"));
        assert!(database
            .get_binding(resource_only_container_id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn live_action_subject_type_survives_automatic_and_manual_binding_paths() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("movies");
        std::fs::create_dir_all(&root_path).unwrap();
        let database = Database::new(temp.path().join("live-action.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,
                    total_video_count
                 ) VALUES(?1,NULL,?2,'Oppenheimer (2023)','Oppenheimer (2023)','WORK',1)",
                params![
                    root.id,
                    root_path.join("Oppenheimer (2023)").to_string_lossy()
                ],
            )
            .unwrap();
        let node_id = connection.last_insert_rowid();
        drop(connection);

        let automatic = crate::models::BangumiSubject {
            subject_id: 451975,
            title: "Oppenheimer".into(),
            title_cn: Some("奥本海默".into()),
            title_en: Some("Oppenheimer".into()),
            title_ja: None,
            title_ko: None,
            match_aliases: Vec::new(),
            date: Some("2023-07-21".into()),
            image_url: Some("https://lain.bgm.tv/pic/cover/l/test.jpg".into()),
            summary: None,
            subject_type: crate::bangumi::SUBJECT_TYPE_LIVE_ACTION,
        };
        assert!(database
            .save_binding_if_absent(node_id, &automatic)
            .unwrap());
        assert_eq!(
            database
                .get_binding(node_id)
                .unwrap()
                .unwrap()
                .provider_subject_type,
            crate::bangumi::SUBJECT_TYPE_LIVE_ACTION
        );

        let mut manual_replacement = automatic.clone();
        manual_replacement.subject_id += 1;
        manual_replacement.title = "Manually corrected live-action film".into();
        database
            .save_confirmed_binding(node_id, &manual_replacement)
            .unwrap();
        let binding = database.get_binding(node_id).unwrap().unwrap();
        assert_eq!(binding.provider_subject_id, manual_replacement.subject_id);
        assert_eq!(
            binding.provider_subject_type,
            crate::bangumi::SUBJECT_TYPE_LIVE_ACTION
        );
    }

    #[test]
    fn settings_persist_language_theme_and_custom_cache_with_whitelists() {
        let temp = TempDir::new().unwrap();
        let database_path = temp.path().join("settings.db");
        let default_cache = temp.path().join("default-cache");
        let custom_cache = temp.path().join("custom-cache");
        let database = Database::new(database_path.clone());
        database.migrate().unwrap();

        let defaults = database.get_settings(&default_cache).unwrap();
        assert_eq!(defaults.language, "zh-CN");
        assert_eq!(defaults.theme, "system");
        assert!(defaults.auto_scan_on_startup);
        assert!(!defaults.all_resources_flattened);
        assert_eq!(
            defaults.cover_cache_directory,
            default_cache.to_string_lossy()
        );

        let updated = AppSettings {
            comic_reader: Default::default(),
            mpv_path: None,
            default_view_mode: ViewMode::List,
            video_extensions: vec!["MKV".into(), ".mp4".into()],
            bangumi_search_enabled: true,
            cover_cache_directory: custom_cache.to_string_lossy().into_owned(),
            language: "ja-JP".into(),
            theme: "dark".into(),
            auto_check_updates: false,
            auto_scan_on_startup: false,
            all_resources_flattened: true,
        };
        database.update_settings(&updated, &default_cache).unwrap();
        let reopened = Database::new(database_path)
            .get_settings(&default_cache)
            .unwrap();
        assert_eq!(reopened.language, "ja-JP");
        assert_eq!(reopened.theme, "dark");
        assert!(!reopened.auto_check_updates);
        assert!(!reopened.auto_scan_on_startup);
        assert!(reopened.all_resources_flattened);
        let mut legacy_json = serde_json::to_value(&reopened).unwrap();
        legacy_json
            .as_object_mut()
            .unwrap()
            .remove("allResourcesFlattened");
        let legacy: AppSettings = serde_json::from_value(legacy_json).unwrap();
        assert!(!legacy.all_resources_flattened);
        database.update_settings(&legacy, &default_cache).unwrap();
        assert!(
            !database
                .get_settings(&default_cache)
                .unwrap()
                .all_resources_flattened
        );
        assert_eq!(
            reopened.cover_cache_directory,
            custom_cache.to_string_lossy()
        );

        let invalid = AppSettings {
            language: "xx-XX".into(),
            ..updated
        };
        assert!(database.update_settings(&invalid, &default_cache).is_err());
    }

    #[test]
    fn window_size_uses_an_independent_validated_settings_key() {
        let temp = TempDir::new().unwrap();
        let database = Database::new(temp.path().join("window-size.db"));
        let default_cache = temp.path().join("default-cache");
        database.migrate().unwrap();
        assert_eq!(database.get_window_size().unwrap(), None);

        let expected = WindowSize {
            width: 1_440,
            height: 900,
        };
        database.save_window_size(expected).unwrap();
        assert_eq!(database.get_window_size().unwrap(), Some(expected));

        // A complete frontend settings save must preserve native window lifecycle state.
        let mut settings = database.get_settings(&default_cache).unwrap();
        settings.theme = "dark".into();
        database.update_settings(&settings, &default_cache).unwrap();
        assert_eq!(database.get_window_size().unwrap(), Some(expected));

        let connection = database.connect().unwrap();
        connection
            .execute(
                "UPDATE settings SET value=?1 WHERE key=?2",
                params![r#"{"width":1,"height":1}"#, WINDOW_SIZE_SETTING_KEY],
            )
            .unwrap();
        drop(connection);
        assert_eq!(database.get_window_size().unwrap(), None);
        assert!(database
            .save_window_size(WindowSize {
                width: 1,
                height: 1,
            })
            .is_err());
    }

    #[test]
    fn collection_sort_preferences_are_independent_and_survive_settings_saves() {
        let temp = TempDir::new().unwrap();
        let database_path = temp.path().join("collection-sort.db");
        let default_cache = temp.path().join("default-cache");
        let database = Database::new(database_path.clone());
        database.migrate().unwrap();

        assert_eq!(
            database.get_collection_sort_preferences().unwrap(),
            CollectionSortPreferences::default()
        );
        database
            .update_collection_sort_preference(CollectionSortScope::All, CollectionSort::TitleDesc)
            .unwrap();
        database
            .update_collection_sort_preference(
                CollectionSortScope::Browse,
                CollectionSort::AddedDesc,
            )
            .unwrap();
        database
            .update_collection_sort_preference(
                CollectionSortScope::Favorites,
                CollectionSort::AddedAsc,
            )
            .unwrap();

        let mut settings = database.get_settings(&default_cache).unwrap();
        settings.theme = "dark".into();
        database.update_settings(&settings, &default_cache).unwrap();

        let reopened = Database::new(database_path);
        assert_eq!(
            reopened.get_collection_sort_preferences().unwrap(),
            CollectionSortPreferences {
                all: CollectionSort::TitleDesc,
                browse: CollectionSort::AddedDesc,
                favorites: CollectionSort::AddedAsc,
            }
        );

        let connection = reopened.connect().unwrap();
        connection
            .execute(
                "UPDATE settings SET value='damaged' WHERE key=?1",
                [CollectionSortScope::Browse.setting_key()],
            )
            .unwrap();
        drop(connection);
        let repaired = reopened.get_collection_sort_preferences().unwrap();
        assert_eq!(repaired.all, CollectionSort::TitleDesc);
        assert_eq!(repaired.browse, CollectionSort::TitleAsc);
        assert_eq!(repaired.favorites, CollectionSort::AddedAsc);
    }

    #[test]
    fn clearing_active_cache_preserves_old_cache_paths_and_all_bindings() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        let active_cache = temp.path().join("active-cache");
        let old_cache = temp.path().join("old-cache");
        std::fs::create_dir_all(&root_path).unwrap();
        crate::cache::ensure_directories(&active_cache).unwrap();
        crate::cache::ensure_directories(&old_cache).unwrap();
        let database = Database::new(temp.path().join("covers.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,absolute_path,folder_name,display_name,node_type,
                    total_video_count
                 ) VALUES(?1,?2,'Active','Active','WORK',1)",
                params![root.id, root_path.join("Active").to_string_lossy()],
            )
            .unwrap();
        let active_node_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,absolute_path,folder_name,display_name,node_type,
                    total_video_count
                 ) VALUES(?1,?2,'Old','Old','WORK',1)",
                params![root.id, root_path.join("Old").to_string_lossy()],
            )
            .unwrap();
        let old_node_id = connection.last_insert_rowid();
        drop(connection);
        let subject = crate::models::BangumiSubject {
            subject_id: 101,
            title: "Title".into(),
            title_cn: None,
            title_en: None,
            title_ja: Some("Title".into()),
            title_ko: None,
            match_aliases: Vec::new(),
            date: None,
            image_url: None,
            summary: None,
            subject_type: 2,
        };
        database
            .save_confirmed_binding(active_node_id, &subject)
            .unwrap();
        database
            .save_confirmed_binding(old_node_id, &subject)
            .unwrap();
        let active_path = active_cache.join("bangumi/101.jpg");
        let old_path = old_cache.join("bangumi/101.jpg");
        std::fs::write(&active_path, b"active").unwrap();
        std::fs::write(
            &old_path,
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x02X\x00\x00\x03\x84",
        )
        .unwrap();
        database
            .set_node_cover(active_node_id, CoverSource::Bangumi, Some(&active_path))
            .unwrap();
        database
            .set_node_cover(old_node_id, CoverSource::Bangumi, Some(&old_path))
            .unwrap();

        let affected = database
            .list_cover_records()
            .unwrap()
            .into_iter()
            .filter(|(_, _, path)| crate::cache::is_equal_or_within(path, &active_cache))
            .collect::<Vec<_>>();
        let cache_clear = crate::cache::begin_cover_cache_clear();
        crate::cache::clear_cover_cache(&cache_clear, &active_cache).unwrap();
        database.clear_cover_paths_for_nodes(&affected).unwrap();
        drop(cache_clear);

        let active_after = database.get_node(active_node_id).unwrap();
        let old_after = database.get_node(old_node_id).unwrap();
        assert_eq!(active_after.cover_source, CoverSource::Placeholder);
        assert!(active_after.cover_cache_path.is_none());
        assert_eq!(old_after.cover_source, CoverSource::Bangumi);
        assert_eq!(old_after.cover_cache_path.as_deref(), old_path.to_str());
        assert!(database.get_binding(active_node_id).unwrap().is_some());
        assert!(database.get_binding(old_node_id).unwrap().is_some());
        assert!(!active_path.exists());
        assert!(old_path.exists());
        let cache_operation = crate::cache::begin_cover_cache_operation();
        assert!(crate::cache::cover_data_url(&cache_operation, &old_path)
            .unwrap()
            .starts_with("data:image/png;base64,"));
    }

    #[test]
    fn cover_read_context_returns_only_cover_and_root_paths() {
        let temp = test_temp_dir();
        let root_path = temp.path().join("library");
        let cache_path = temp.path().join("cache/manual/node.png");
        std::fs::create_dir_all(&root_path).unwrap();
        let database = Database::new(temp.path().join("cover-context.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,absolute_path,folder_name,display_name,node_type,
                    cover_source,cover_cache_path,total_video_count
                 ) VALUES(?1,?2,'Work','Work','WORK','MANUAL',?3,1)",
                params![
                    root.id,
                    root_path.join("Work").to_string_lossy(),
                    cache_path.to_string_lossy()
                ],
            )
            .unwrap();
        let node_id = connection.last_insert_rowid();
        drop(connection);

        let (cover, roots) = database.cover_read_context(node_id).unwrap();
        assert_eq!(cover.as_deref(), Some(cache_path.as_path()));
        assert_eq!(roots, vec![root_path]);
        assert!(database.cover_read_context(i64::MAX).is_err());
    }
}

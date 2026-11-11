use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::{
    auto_match, bangumi, cache,
    db::{AppResult, Database},
    models::{
        AllResourcesResult, AppBootstrap, AppSettings, BangumiSearchPrefill, BangumiSubject,
        BatchMutationResult, BrowseResult, CacheStats, CollectionSort, CollectionSortPreferences,
        CollectionSortScope, CoverSource, FavoriteFolder, LibraryMediaKind, LibraryRecognitionMode,
        LibraryRoot, MediaFile, MediaNode, MetadataBinding, NodeType, PlayerTestResult,
        RebuildResult, RecentlyWatchedEntry, ScanPhase, ScanProgress, ScanStarted, ScanStatus,
        SearchHit, UpdateCheckResult, UpdateDistribution, UpdateDownloadStatus, UserTag,
        UserTagMembership, WorkTarget,
    },
    player,
    scanner::{self, ScanControl, ScanTarget},
    title_extractor, AppState, DATABASE_URL,
};

const BILIBILI_URL: &str = "https://space.bilibili.com/2903441";

#[tauri::command]
pub async fn get_book_catalogue(
    root_id: i64,
    state: State<'_, AppState>,
) -> AppResult<crate::smart_mixed::Catalogue> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || crate::smart_mixed::catalogue(&database, root_id))
        .await
        .map_err(|_| "SMART_READ_FAILED")?
}
#[tauri::command]
pub async fn correct_book_organization(
    root_id: i64,
    relative_path: String,
    action: crate::smart_mixed::Correction,
    target: Option<String>,
    expected_revision: i64,
    app: AppHandle,
) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _lifecycle = lock_scan_lifecycle(&state);
        ensure_no_active_scan(&state)?;
        crate::smart_mixed::correct(
            &state.database,
            root_id,
            &relative_path,
            action,
            target.as_deref(),
            expected_revision,
        )
    })
    .await
    .map_err(|_| "SMART_CORRECTION_FAILED")?
}
const X_URL: &str = "https://x.com/f_undermori";

#[tauri::command]
pub async fn tmdb_retry_cover(
    node_id: i64,
    state: State<'_, AppState>,
) -> AppResult<crate::tmdb::Binding> {
    let db = state.database.clone();
    let root = active_cover_cache_directory(&state)?;
    tauri::async_runtime::spawn_blocking(move || crate::tmdb::retry_cover(&db, node_id, &root))
        .await
        .map_err(|_| "TMDB_BIND_FAILED")?
}
#[tauri::command]
pub fn tmdb_cancel() {
    crate::tmdb::cancel_mutation();
}
#[tauri::command]
pub fn tmdb_status() -> bool {
    crate::tmdb_credentials::selected().is_ok()
}
#[tauri::command]
pub fn tmdb_credential_status() -> crate::tmdb_credentials::CredentialStatus {
    crate::tmdb_credentials::status()
}
#[tauri::command]
pub fn tmdb_select_source(
    source: String,
    state: State<'_, AppState>,
) -> AppResult<crate::tmdb_credentials::CredentialStatus> {
    crate::tmdb_credentials::select(&state.database, &source)?;
    Ok(crate::tmdb_credentials::status())
}
#[tauri::command]
pub async fn tmdb_verify() -> AppResult<crate::tmdb_credentials::CredentialStatus> {
    tauri::async_runtime::spawn_blocking(crate::tmdb_credentials::verify)
        .await
        .map_err(|_| "TMDB_NETWORK_FAILED")?
}
#[tauri::command]
pub async fn tmdb_delete_personal(
    title: String,
    message: String,
    app: AppHandle,
) -> AppResult<bool> {
    if title.chars().count() > 200 || message.chars().count() > 1000 {
        return Err("TMDB_QUERY_INVALID".into());
    }
    #[cfg(windows)]
    let parent = app
        .get_webview_window("main")
        .and_then(|w| w.hwnd().ok())
        .map_or(0, |h| h.0 as usize);
    #[cfg(not(windows))]
    let parent = 0;
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::tmdb_credentials::delete(parent, &title, &message)
    })
    .await
    .map_err(|_| "TMDB_CREDENTIAL_DIALOG_FAILED")??;
    if result {
        crate::tmdb_credentials::select(&app.state::<AppState>().database, "DEFAULT")?;
    }
    Ok(result)
}
#[tauri::command]
pub fn tmdb_match_diagnostics(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::tmdb::MatchDiagnostic>> {
    crate::tmdb::diagnostics(&state.database)
}
#[tauri::command]
pub fn tmdb_open_page(node_id: i64, state: State<'_, AppState>) -> AppResult<()> {
    let binding = crate::tmdb::binding(&state.database.connect()?, node_id)?
        .filter(|binding| binding.active)
        .ok_or("TMDB_BINDING_REQUIRED")?;
    player::open_external_url(&format!(
        "https://www.themoviedb.org/{}/{}",
        binding.movie.media_type.endpoint(),
        binding.movie.id
    ))
}
#[cfg(test)]
fn tmdb_movie_url(id: i64) -> AppResult<String> {
    if id <= 0 {
        return Err("TMDB_INVALID_ID".into());
    }
    Ok(format!("https://www.themoviedb.org/movie/{id}"))
}
#[tauri::command]
pub async fn tmdb_configure(title: String, message: String, app: AppHandle) -> AppResult<bool> {
    if title.chars().count() > 200 || message.chars().count() > 1000 {
        return Err("TMDB_QUERY_INVALID".into());
    }
    #[cfg(windows)]
    let parent = app
        .get_webview_window("main")
        .and_then(|w| w.hwnd().ok())
        .map_or(0, |h| h.0 as usize);
    #[cfg(not(windows))]
    let parent = 0;
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::tmdb_credentials::configure(parent, &title, &message)
    })
    .await
    .map_err(|_| "TMDB_CREDENTIAL_DIALOG_FAILED")??;
    if result {
        crate::tmdb_credentials::select(&app.state::<AppState>().database, "PERSONAL")?;
    }
    Ok(result)
}
#[tauri::command]
pub async fn tmdb_search(
    node_id: i64,
    query: String,
    year: Option<u16>,
    language: String,
    media_type: Option<crate::tmdb::MediaType>,
    state: State<'_, AppState>,
) -> AppResult<crate::tmdb::Search> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::tmdb::search(&db, node_id, &query, year, &language, media_type)
    })
    .await
    .map_err(|_| "TMDB_SEARCH_FAILED")?
}
#[tauri::command]
pub async fn tmdb_detail(
    movie_id: i64,
    language: String,
    media_type: Option<crate::tmdb::MediaType>,
) -> AppResult<crate::tmdb::Movie> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::tmdb::detail_for(media_type.unwrap_or_default(), movie_id, &language)
    })
    .await
    .map_err(|_| "TMDB_DETAIL_FAILED")?
}
#[tauri::command]
pub async fn tmdb_bind(
    node_id: i64,
    movie_id: i64,
    language: String,
    expected_snapshot: String,
    media_type: Option<crate::tmdb::MediaType>,
    state: State<'_, AppState>,
) -> AppResult<crate::tmdb::Binding> {
    let db = state.database.clone();
    let root = active_cover_cache_directory(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::tmdb::bind_for(
            &db,
            node_id,
            movie_id,
            &language,
            &expected_snapshot,
            &root,
            media_type.unwrap_or_default(),
        )
    })
    .await
    .map_err(|_| "TMDB_BIND_FAILED")?
}
#[tauri::command]
pub fn tmdb_clear(node_id: i64, state: State<'_, AppState>) -> AppResult<()> {
    crate::tmdb::deactivate(&state.database, node_id)
}
const MAX_BINDING_TITLE_CHARS: usize = 500;

#[tauri::command]
pub fn set_library_scan_warnings_ignored(
    root_id: i64,
    ignored: bool,
    state: State<'_, AppState>,
) -> AppResult<LibraryRoot> {
    state.database.set_scan_warnings_ignored(root_id, ignored)
}
const MAX_BINDING_SUMMARY_CHARS: usize = 50_000;
const MAX_BINDING_URL_CHARS: usize = 2_048;

fn validate_bangumi_subject_payload(subject: &BangumiSubject) -> AppResult<()> {
    if subject.title.trim().is_empty() || subject.title.chars().count() > MAX_BINDING_TITLE_CHARS {
        return Err("Bangumi 标题为空或过长。".into());
    }
    if [
        &subject.title_cn,
        &subject.title_en,
        &subject.title_ja,
        &subject.title_ko,
    ]
    .into_iter()
    .flatten()
    .any(|title| title.chars().count() > MAX_BINDING_TITLE_CHARS)
    {
        return Err("Bangumi 多语言标题过长。".into());
    }
    if subject
        .summary
        .as_deref()
        .is_some_and(|summary| summary.chars().count() > MAX_BINDING_SUMMARY_CHARS)
    {
        return Err("Bangumi 简介过长。".into());
    }
    if subject
        .image_url
        .as_deref()
        .is_some_and(|url| url.len() > MAX_BINDING_URL_CHARS)
    {
        return Err("Bangumi 封面地址过长。".into());
    }
    if subject.match_aliases.len() > 32
        || subject
            .match_aliases
            .iter()
            .any(|alias| alias.chars().count() > 200)
    {
        return Err("Bangumi 别名数据过多或过长。".into());
    }
    Ok(())
}

fn validate_bindable_bangumi_subject(subject: &BangumiSubject) -> AppResult<()> {
    if subject.subject_id <= 0 || !bangumi::is_supported_subject_type(subject.subject_type) {
        return Err("只能绑定有效的 Bangumi 动画或真人影视条目。".into());
    }
    validate_bangumi_subject_payload(subject)
}

fn library_root_paths(state: &AppState) -> AppResult<Vec<PathBuf>> {
    state.database.list_roots().map(|roots| {
        roots
            .into_iter()
            .map(|root| PathBuf::from(root.path))
            .collect()
    })
}

fn same_path(left: &Path, right: &Path) -> bool {
    cache::is_equal_or_within(left, right) && cache::is_equal_or_within(right, left)
}

fn active_cover_cache_directory(state: &AppState) -> AppResult<PathBuf> {
    active_cover_cache_directory_for(&state.database, &state.default_cover_cache_dir)
}

fn active_cover_cache_directory_for(database: &Database, default: &Path) -> AppResult<PathBuf> {
    let configured = database.poster_cache_root(default)?;
    let validated = cache::validate_cache_location(&configured, database.library_paths()?)?;
    if same_path(&validated, default) {
        cache::ensure_directories(&validated)?;
    } else {
        cache::initialize_custom_cache(&validated)?;
    }
    Ok(validated)
}

fn prepare_cover_cache_directory(state: &AppState, path: &str) -> AppResult<PathBuf> {
    let path = path.trim();
    if path.is_empty() {
        return Err("封面缓存目录不能为空。".into());
    }
    let validated = cache::validate_cache_location(Path::new(path), library_root_paths(state)?)?;
    if same_path(&validated, &state.default_cover_cache_dir) {
        cache::ensure_directories(&validated)?;
    } else {
        cache::initialize_custom_cache(&validated)?;
    }
    Ok(validated)
}

fn remove_cached_file_if_unreferenced(
    cache_operation: &cache::CoverCacheOperationGuard,
    database: &crate::db::Database,
    path: &Path,
    cache_root: &Path,
) {
    if cache::is_equal_or_within(path, cache_root)
        && database.cover_path_reference_count(path).ok() == Some(0)
    {
        let _ = cache::remove_cached_file(cache_operation, path, cache_root);
    }
}

#[tauri::command]
pub fn get_app_bootstrap(state: State<'_, AppState>) -> AppBootstrap {
    let update_recovery_notice = state
        .update_recovery_notice
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    AppBootstrap {
        name: "M²Shelf",
        version: env!("CARGO_PKG_VERSION"),
        database_url: DATABASE_URL,
        build_date: env!("M2SHELF_BUILD_DATE"),
        architecture: display_architecture(),
        website_url: BILIBILI_URL,
        x_url: X_URL,
        update_recovery_notice,
    }
}

#[tauri::command]
pub fn acknowledge_update_recovery_notice(
    notice: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    ensure_update_cache_outside_library_roots(&state)?;
    if notice != "ROLLED_BACK" && notice != "RECOVERY_REQUIRED" {
        return Err("更新恢复通知类型无效。".into());
    }
    let mut pending = state
        .update_recovery_notice
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if pending.as_deref() != Some(notice.as_str()) {
        return Err("更新恢复通知已发生变化，请重新启动应用后确认。".into());
    }
    // Clear the durable marker first. If that fails, keep the in-memory copy so the warning stays
    // visible and can be acknowledged again instead of being silently lost.
    crate::portable_update::clear_rollback_notice(state.update_manager.cache_dir())?;
    *pending = None;
    Ok(())
}

/// The main window is created hidden so restoring its persisted size and preparing the themed
/// React shell never expose the configured fallback frame. The frontend calls this once after its
/// startup settings have been applied.
#[tauri::command]
pub fn show_main_window(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "找不到主窗口。".to_owned())?;
    if let Err(error) = crate::portable_update::mark_running_update_healthy(
        state.update_manager.cache_dir(),
        env!("CARGO_PKG_VERSION"),
    ) {
        // During a Portable update the helper must be able to trust this receipt. Keep the new
        // process hidden and terminate it immediately on failure so rollback completes before a
        // user can change application-owned metadata through the uncommitted version.
        app.exit(1);
        return Err(error);
    }
    if let Err(error) = window.show() {
        // A Portable helper may already have observed the health marker above. Exiting inside its
        // survival grace makes the transaction fail closed and preserves rollback material.
        app.exit(1);
        return Err(format!("无法显示主窗口：{error}"));
    }
    // Match normal desktop startup behavior without making focus a prerequisite for visibility.
    let _ = window.set_focus();
    // Cache recovery is optional background work; it must not delay the prepared first frame.
    let database = state.database.clone();
    let default_cache = state.default_cover_cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(cache_root) = database.poster_cache_root(&default_cache) {
            crate::poster_cache::resume_warmup(&database, &cache_root);
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn check_for_update(state: State<'_, AppState>) -> AppResult<UpdateCheckResult> {
    ensure_update_cache_outside_library_roots(&state)?;
    let manager = state.update_manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let distribution = crate::portable_update::detect_distribution()?;
        manager.check(distribution)
    })
    .await
    .map_err(|error| format!("更新检查任务失败：{error}"))?
}

#[tauri::command]
pub async fn download_update(
    version: String,
    state: State<'_, AppState>,
) -> AppResult<UpdateDownloadStatus> {
    let library_roots = ensure_update_cache_outside_library_roots(&state)?;
    let manager = state.update_manager.clone();
    tauri::async_runtime::spawn_blocking(move || manager.download(&version, &library_roots))
        .await
        .map_err(|error| format!("更新下载任务失败：{error}"))?
}

#[tauri::command]
pub fn get_update_download_status(state: State<'_, AppState>) -> UpdateDownloadStatus {
    state.update_manager.status()
}

#[tauri::command]
pub fn install_downloaded_update(
    version: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let library_roots = ensure_update_cache_outside_library_roots(&state)?;
    let _update_operation = state.update_manager.begin_operation()?;
    let _scan_lifecycle = lock_scan_lifecycle(&state);
    ensure_no_active_scan(&state)?;
    crate::update::ensure_newer_version(env!("CARGO_PKG_VERSION"), &version)?;
    let downloaded = state.update_manager.downloaded(&version)?;
    crate::update::validate_existing_update_subdirectory(
        state.update_manager.cache_dir(),
        &[version.as_str()],
        &library_roots,
    )?;
    crate::update::ensure_safe_update_file(
        &downloaded.path,
        state.update_manager.cache_dir(),
        &library_roots,
        "更新包",
    )?;
    let _verified_artifact = match crate::update::lock_and_verify_file_against_manifest(
        &downloaded.path,
        &downloaded.checked.version,
        downloaded.checked.platform,
        downloaded.checked.asset.size,
        &downloaded.checked.asset.sha256,
        &downloaded.checked.asset.signature,
    ) {
        Ok(guard) => guard,
        Err(error) => {
            state.update_manager.fail(Some(&version), &error);
            return Err(error);
        }
    };
    if let Some(size) = state.window_size.get() {
        state.database.save_window_size(size)?;
    }
    state.update_manager.set_applying(&version);

    match downloaded.checked.distribution {
        UpdateDistribution::Portable => {
            state
                .update_exit_in_progress
                .store(true, std::sync::atomic::Ordering::Release);
            let prepared = crate::portable_update::prepare_portable_update(
                &downloaded,
                &state.database,
                state.update_manager.cache_dir(),
            )
            .and_then(|mut prepared| {
                crate::portable_update::launch_prepared_helper(&mut prepared)?;
                Ok(prepared)
            });
            let prepared = match prepared {
                Ok(prepared) => prepared,
                Err(error) => {
                    state
                        .update_exit_in_progress
                        .store(false, std::sync::atomic::Ordering::Release);
                    state.update_manager.fail(Some(&version), &error);
                    return Err(error);
                }
            };
            app.exit(0);
            // `app.exit` is asynchronous. Leak this one-shot guard so SQLite's write reservation
            // survives command return and is released only when Windows tears down the process.
            std::mem::forget(prepared);
            Ok(())
        }
        UpdateDistribution::Nsis => {
            state
                .update_exit_in_progress
                .store(true, std::sync::atomic::Ordering::Release);
            if let Err(error) = std::process::Command::new(&downloaded.path)
                .args(["/P", "/UPDATE", "/R", "/ARGS"])
                .spawn()
                .map(|_| ())
                .map_err(|error| format!("无法启动 NSIS 更新安装包：{error}"))
            {
                state
                    .update_exit_in_progress
                    .store(false, std::sync::atomic::Ordering::Release);
                state.update_manager.fail(Some(&version), &error);
                return Err(error);
            }
            app.exit(0);
            Ok(())
        }
    }
}

fn display_architecture() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "ARM64",
        "x86" => "x86",
        other => other,
    }
}

#[tauri::command]
pub async fn list_library_roots(state: State<'_, AppState>) -> AppResult<Vec<LibraryRoot>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || database.list_roots())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn get_next_volume(
    comic_book_id: i64,
    expected_revision: String,
    state: State<'_, AppState>,
) -> AppResult<crate::next_volume::NextVolumeResult> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::next_volume::lookup(&database, comic_book_id, Some(&expected_revision))
    })
    .await
    .map_err(|_| "NEXT_VOLUME_FAILED".to_string())?
}
#[tauri::command]
pub async fn confirm_next_volume(
    comic_book_id: i64,
    expected_revision: String,
    next_book_id: i64,
    snapshot: String,
    state: State<'_, AppState>,
) -> AppResult<crate::next_volume::NextVolumeResult> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::next_volume::confirm(
            &database,
            comic_book_id,
            &expected_revision,
            next_book_id,
            &snapshot,
        )
    })
    .await
    .map_err(|_| "NEXT_VOLUME_FAILED".to_string())?
}
#[tauri::command]
pub async fn episode_watch_snapshot(
    node_id: i64,
    work_view: bool,
    state: State<'_, AppState>,
) -> AppResult<crate::episode_watch::EpisodeWatchSnapshot> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        database.episode_watch_snapshot(node_id, work_view)
    })
    .await
    .map_err(|_| "EPISODE_WATCH_FAILED".to_string())?
}
#[tauri::command]
pub async fn set_episode_watched(
    node_id: i64,
    work_view: bool,
    file_id: i64,
    watched: bool,
    snapshot: String,
    state: State<'_, AppState>,
) -> AppResult<crate::episode_watch::EpisodeWatchSnapshot> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        database.set_episode_watched(node_id, work_view, file_id, watched, &snapshot)
    })
    .await
    .map_err(|_| "EPISODE_WATCH_FAILED".to_string())?
}
#[tauri::command]
pub async fn clear_episode_watch(
    node_id: i64,
    work_view: bool,
    snapshot: String,
    state: State<'_, AppState>,
) -> AppResult<crate::episode_watch::EpisodeWatchSnapshot> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        database.clear_episode_watch(node_id, work_view, &snapshot)
    })
    .await
    .map_err(|_| "EPISODE_WATCH_FAILED".to_string())?
}
#[tauri::command]
pub fn get_comic_detail(
    node_id: i64,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::comics::ComicBook>> {
    crate::comic_reader::detail(&state.database, node_id)
}
#[tauri::command]
pub fn open_comic_in_explorer(comic_book_id: i64, state: State<'_, AppState>) -> AppResult<()> {
    crate::comic_reader::reveal(&state.database, comic_book_id)
}
#[tauri::command]
pub async fn open_comic_book(
    comic_book_id: i64,
    state: State<'_, AppState>,
) -> AppResult<crate::comics::ComicOpenResult> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::comic_reader::open(&database, comic_book_id)
    })
    .await
    .map_err(|_| "COMIC_READ_FAILED".to_string())?
}
#[tauri::command]
pub async fn read_comic_page(
    comic_book_id: i64,
    page_index: i64,
    expected_revision: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<tauri::ipc::Response> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::comic_reader::read_page_at_revision(
            &database,
            comic_book_id,
            page_index,
            expected_revision.as_deref(),
        )
        .map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|_| "COMIC_READ_FAILED".to_string())?
}
#[tauri::command]
pub async fn read_pdf_range(
    comic_book_id: i64,
    begin: u64,
    end: u64,
    expected_revision: String,
    state: State<'_, AppState>,
) -> AppResult<tauri::ipc::Response> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::comic_reader::read_pdf_range(
            &database,
            comic_book_id,
            begin,
            end,
            &expected_revision,
        )
        .map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|_| "COMIC_READ_FAILED".to_string())?
}
#[tauri::command]
pub async fn read_book_document(
    comic_book_id: i64,
    page_index: i64,
    expected_revision: String,
    state: State<'_, AppState>,
) -> AppResult<tauri::ipc::Response> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::comic_reader::read_document(&database, comic_book_id, page_index, &expected_revision)
            .map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|_| "COMIC_READ_FAILED".to_string())?
}

#[tauri::command]
pub async fn read_epub_illustration(
    comic_book_id: i64,
    page_index: i64,
    expected_revision: String,
    block_index: usize,
    state: State<'_, AppState>,
) -> AppResult<tauri::ipc::Response> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::comic_reader::read_epub_image(
            &database,
            comic_book_id,
            page_index,
            &expected_revision,
            block_index,
        )
        .map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|_| "COMIC_READ_FAILED")?
}

#[tauri::command]
pub async fn update_comic_progress(
    comic_book_id: i64,
    page_index: i64,
    expected_revision: Option<String>,
    text_position: Option<crate::comics::TextPosition>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::comic_reader::progress_with_position_at_revision(
            &database,
            comic_book_id,
            page_index,
            expected_revision.as_deref(),
            text_position,
        )
    })
    .await
    .map_err(|_| "COMIC_PROGRESS_FAILED".to_string())?
}
#[tauri::command]
pub fn list_comic_bookmarks(comic_book_id: i64, state: State<'_, AppState>) -> AppResult<Vec<i64>> {
    crate::comic_reader::bookmarks(&state.database, comic_book_id)
}
#[tauri::command]
pub async fn add_comic_bookmark(
    comic_book_id: i64,
    page_index: i64,
    expected_revision: Option<String>,
    text_position: Option<crate::comics::TextPosition>,
    state: State<'_, AppState>,
) -> AppResult<Vec<i64>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::comic_reader::bookmark_with_position_at_revision(
            &database,
            comic_book_id,
            page_index,
            true,
            expected_revision.as_deref(),
            text_position,
        )
    })
    .await
    .map_err(|_| "COMIC_BOOKMARK_FAILED".to_string())?
}
#[tauri::command]
pub async fn remove_comic_bookmark(
    comic_book_id: i64,
    page_index: i64,
    expected_revision: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<Vec<i64>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::comic_reader::bookmark_at_revision(
            &database,
            comic_book_id,
            page_index,
            false,
            expected_revision.as_deref(),
        )
    })
    .await
    .map_err(|_| "COMIC_BOOKMARK_FAILED".to_string())?
}

#[tauri::command]
pub fn add_library_root(
    path: String,
    display_name: Option<String>,
    media_kind: Option<LibraryMediaKind>,
    auto_bangumi: Option<bool>,
    recognition_mode: LibraryRecognitionMode,
    book_organization_strategy: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<LibraryRoot> {
    // Reject duplicate, ancestor and descendant roots before any other setup. `add_root` repeats
    // the same check transactionally so concurrent or non-command callers cannot bypass it.
    let canonical = state.database.validate_new_root_path(Path::new(&path))?;
    let active_cache = active_cover_cache_directory(&state)?;
    if cache::paths_overlap(&canonical, &active_cache) {
        return Err("媒体资源库不能等于、包含封面缓存目录，或位于封面缓存目录内。".into());
    }
    validate_new_library_root_update_boundary(
        &canonical,
        state.update_manager.cache_dir(),
        library_root_paths(&state)?,
    )?;
    let kind = media_kind.unwrap_or_default();
    let root = state.database.add_root_with_strategy(
        &canonical,
        display_name,
        kind,
        recognition_mode,
        auto_bangumi.unwrap_or(!kind.is_book()),
        book_organization_strategy.as_deref().unwrap_or("LEGACY"),
    )?;
    crate::poster_resource::clear_memory();
    Ok(root)
}

#[tauri::command]
pub fn remove_library_root(root_id: i64, state: State<'_, AppState>) -> AppResult<()> {
    let _scan_lifecycle = lock_scan_lifecycle(&state);
    ensure_no_active_scan(&state)?;
    state.database.remove_root(root_id)?;
    crate::poster_resource::clear_memory();
    Ok(())
}

#[tauri::command]
pub fn update_library_root_name(
    root_id: i64,
    display_name: String,
    state: State<'_, AppState>,
) -> AppResult<LibraryRoot> {
    state
        .database
        .update_root_display_name(root_id, &display_name)
}

#[tauri::command]
pub fn open_library_root_in_explorer(root_id: i64, state: State<'_, AppState>) -> AppResult<()> {
    let root = state.database.get_root(root_id)?;
    player::reveal(Path::new(&root.path))
}

#[tauri::command]
pub fn set_library_auto_bangumi(
    root_id: i64,
    enabled: bool,
    state: State<'_, AppState>,
) -> AppResult<LibraryRoot> {
    state.database.set_root_auto_bangumi(root_id, enabled)
}

#[tauri::command]
pub async fn set_library_hidden_from_all(
    root_id: i64,
    hidden: bool,
    state: State<'_, AppState>,
) -> AppResult<LibraryRoot> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || database.set_root_hidden_from_all(root_id, hidden))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn list_hidden_nodes(state: State<'_, AppState>) -> AppResult<Vec<MediaNode>> {
    state.database.list_hidden_nodes()
}

#[tauri::command]
pub async fn card_progress(
    requests: Vec<crate::card_progress::Request>,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::card_progress::Summary>> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || crate::card_progress::read(&db, &requests))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_all_resources(state: State<'_, AppState>) -> AppResult<AllResourcesResult> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || database.list_all_resources())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn list_recently_watched(
    state: State<'_, AppState>,
) -> AppResult<Vec<RecentlyWatchedEntry>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || database.list_recently_watched())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn list_favorite_folders(state: State<'_, AppState>) -> AppResult<Vec<FavoriteFolder>> {
    state.database.list_favorite_folders()
}

#[tauri::command]
pub fn create_favorite_folder(
    name: String,
    state: State<'_, AppState>,
) -> AppResult<FavoriteFolder> {
    state.database.create_favorite_folder(&name)
}

#[tauri::command]
pub fn rename_favorite_folder(
    folder_id: i64,
    name: String,
    state: State<'_, AppState>,
) -> AppResult<FavoriteFolder> {
    state.database.rename_favorite_folder(folder_id, &name)
}

#[tauri::command]
pub fn delete_favorite_folder(folder_id: i64, state: State<'_, AppState>) -> AppResult<()> {
    state.database.delete_favorite_folder(folder_id)
}

#[tauri::command]
pub async fn list_favorite_folder_nodes(
    folder_id: i64,
    state: State<'_, AppState>,
) -> AppResult<Vec<MediaNode>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || database.list_favorite_folder_nodes(folder_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn batch_add_nodes_to_favorite(
    folder_id: i64,
    node_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> AppResult<BatchMutationResult> {
    state
        .database
        .batch_add_nodes_to_favorite(folder_id, &node_ids)
}

#[tauri::command]
pub fn batch_remove_nodes_from_favorite(
    folder_id: i64,
    node_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> AppResult<BatchMutationResult> {
    state
        .database
        .batch_remove_nodes_from_favorite(folder_id, &node_ids)
}

#[tauri::command]
pub async fn browse_library(
    root_id: i64,
    parent_node_id: Option<i64>,
    state: State<'_, AppState>,
) -> AppResult<BrowseResult> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        browse_library_snapshot(&database, root_id, parent_node_id)
    })
    .await
    .map_err(|e| e.to_string())?
}
fn browse_library_snapshot(
    database: &Database,
    root_id: i64,
    parent_node_id: Option<i64>,
) -> AppResult<BrowseResult> {
    let root = database.get_root(root_id)?;
    let parent_id = match parent_node_id {
        Some(id) => id,
        None => match database.hidden_root_node_id(&root)? {
            Some(id) => id,
            None => {
                return Ok(BrowseResult {
                    comic_books: Vec::new(),
                    root,
                    breadcrumbs: Vec::new(),
                    nodes: Vec::new(),
                    media_files: Vec::new(),
                    resource_files: Vec::new(),
                })
            }
        },
    };
    database.read_snapshot(|connection| {
        crate::db::ensure_node_visible_conn(connection, parent_id)?;
        let parent_root: i64 = connection
            .query_row(
                "SELECT library_root_id FROM nodes WHERE id=?1",
                [parent_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if parent_root != root_id {
            return Err("目录节点不属于该资源库。".into());
        }
        let mut nodes = crate::db::list_children_conn(connection, parent_id)?;
        crate::card_progress::hydrate_library_nodes_conn(connection, root_id, &mut nodes)?;
        Ok(BrowseResult {
            comic_books: crate::comics::books(connection, parent_id)?,
            root,
            breadcrumbs: crate::db::breadcrumbs_conn(connection, parent_id, true)?,
            nodes,
            media_files: crate::db::list_media_conn(connection, parent_id)?,
            resource_files: crate::db::list_resources_conn(connection, parent_id)?,
        })
    })
}

#[cfg(test)]
mod browse_phase_profile {
    use super::*;
    #[test]
    fn library_browse_matches_uncached_snapshot_after_ownership_and_metadata_edits() {
        let temp = crate::db::test_temp_dir();
        let db = Database::new(temp.path().join("library-browse.db"));
        db.migrate().unwrap();
        let c = db.connect().unwrap();
        let root = temp.path().join("Synthetic");
        c.execute("INSERT INTO library_roots(id,path,display_name,media_kind,video_subject_scope) VALUES(1,?1,'Library','VIDEO','ANIMATION')",[root.to_string_lossy()]).unwrap();
        for (id, parent, name, kind, files, branches) in [
            (1, None, "Synthetic", "CONTAINER", 2, 1),
            (2, Some(1), "Series", "CONTAINER", 2, 2),
            (3, Some(2), "First Story", "WORK", 1, 0),
            (4, Some(2), "Second Story", "WORK", 1, 0),
        ] {
            let path = if id == 1 {
                root.clone()
            } else {
                root.join(name)
            };
            c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,manual_type_override,direct_video_count,total_video_count,child_media_branch_count) VALUES(?1,1,?2,?3,?4,?4,?5,1,?6,?7,?8)",rusqlite::params![id,parent,path.to_string_lossy(),name,kind,if id>=3{1}else{0},files,branches]).unwrap();
            if id >= 3 {
                c.execute("INSERT INTO media_files(id,node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(?1,?2,?3,'Story S01E01.mkv','mkv',123,'stamp')",rusqlite::params![id-2,id,path.join("Story S01E01.mkv").to_string_lossy()]).unwrap();
            }
        }
        let verify = || {
            let optimized = browse_library_snapshot(&db, 1, None).unwrap();
            let expected = db
                .read_snapshot(|c| {
                    let mut nodes = crate::db::list_children_conn(c, 1)?;
                    crate::card_progress::hydrate_nodes_conn(c, &mut nodes)?;
                    Ok(BrowseResult {
                        root: db.get_root(1)?,
                        breadcrumbs: crate::db::breadcrumbs_conn(c, 1, true)?,
                        nodes,
                        comic_books: crate::comics::books(c, 1)?,
                        media_files: crate::db::list_media_conn(c, 1)?,
                        resource_files: crate::db::list_resources_conn(c, 1)?,
                    })
                })
                .unwrap();
            assert_eq!(
                serde_json::to_value(&optimized).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
            optimized.nodes[0].card_progress.clone().unwrap()
        };
        assert_eq!(verify().total, 2);
        assert_eq!(verify().total, 2); // Revisit uses the same identities.
        let watch = db.episode_watch_snapshot(2, false).unwrap();
        db.set_episode_watched(2, false, 1, true, &watch.snapshot)
            .unwrap();
        assert_eq!(verify().watched, 1);
        for change in [
            "UPDATE nodes SET display_name='Renamed',manual_display_name=1 WHERE id=2",
            "UPDATE nodes SET node_type='IGNORED' WHERE id=4",
            "UPDATE nodes SET node_type='WORK' WHERE id=4",
            "INSERT INTO metadata_bindings(node_id,provider,provider_subject_id,provider_subject_type,provider_title) VALUES(3,'BANGUMI',9001,2,'One Story'),(4,'BANGUMI',9001,2,'One Story')",
            "UPDATE nodes SET parent_node_id=1 WHERE id=4",
        ] { c.execute(change,[]).unwrap(); verify(); verify(); }
        let error = browse_library_snapshot(&db, 999, Some(2));
        assert!(error.is_err());
        c.execute("UPDATE nodes SET node_type='IGNORED' WHERE id=2", [])
            .unwrap();
        assert!(browse_library_snapshot(&db, 1, Some(2)).is_err());
    }
    #[test]
    #[ignore = "Explicit read-only synthetic library-switch profiling"]
    fn synthetic_library_switch_phases() {
        let path = PathBuf::from(
            std::env::var("M2SHELF_SYNTHETIC_PROFILE_DB").expect("synthetic copy required"),
        );
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".tmp")
            .canonicalize()
            .unwrap();
        assert!(path.canonicalize().unwrap().starts_with(&base));
        let db = Database::new(path);
        for round in 0..2 {
            for title in ["Repair Audit", "Comic Atlas", "Folder Performance"] {
                db.read_snapshot(|c| {
                    let paths = c.prepare("SELECT path FROM library_roots").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
                    let prefix=base.to_string_lossy().replace("\\\\?\\", "").replace('\\',"/").to_lowercase()+"/";
                    assert!(paths.iter().all(|p|p.replace('\\',"/").to_lowercase().starts_with(&prefix)&&!p.replace('\\',"/").split('/').any(|s|s=="..")));
                    let (root_id,parent): (i64,i64) = c.query_row("SELECT r.id,n.id FROM library_roots r JOIN nodes n ON n.library_root_id=r.id AND n.parent_node_id IS NULL WHERE r.display_name=?1",[title],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
                    let mut times=Vec::new();
                    macro_rules! phase { ($name:expr,$body:expr) => {{let start=std::time::Instant::now();let v=$body;times.push(($name,start.elapsed().as_secs_f64()*1000.));v}}; }
                    phase!("visibility",crate::db::ensure_node_visible_conn(c,parent)?);
                    let node=phase!("parent_full_metadata",crate::db::get_node_conn(c,parent)?);
                    let mut nodes=phase!("children_full_metadata",crate::db::list_children_conn(c,parent)?);
                    let graph=phase!("ownership_graph",if node.media_kind.is_book(){None}else{Some(crate::logical_works::LogicalWorkIndex::load_for_root_detail(c,root_id)?)});
                    phase!("card_progress",crate::card_progress::Hydrator::new(c,graph).nodes(&mut nodes,false)?);
                    let books=phase!("books",crate::comics::books(c,parent)?);
                    phase!("breadcrumbs",crate::db::breadcrumbs_conn(c,parent,true)?);
                    let media=phase!("direct_media",crate::db::list_media_conn(c,parent)?);
                    let resources=phase!("direct_resources",crate::db::list_resources_conn(c,parent)?);
                    eprintln!("ROOT_PHASES {}",serde_json::json!({"round":round,"root":title,"nodes":nodes.len(),"books":books.len(),"files":media.len()+resources.len(),"ms":times}));
                    Ok(())
                }).unwrap();
                for cached in 0..2 {
                    let start = std::time::Instant::now();
                    let id = db
                        .list_roots()
                        .unwrap()
                        .iter()
                        .find(|r| r.display_name == title)
                        .unwrap()
                        .id;
                    let response = browse_library_snapshot(&db, id, None).unwrap();
                    eprintln!(
                        "ROOT_OPTIMIZED {}",
                        serde_json::json!({"round":round,"cached":cached,"root":title,"nodes":response.nodes.len(),"ms":start.elapsed().as_secs_f64()*1000.})
                    );
                }
            }
        }
    }
}

#[tauri::command]
pub async fn get_node_detail(
    node_id: i64,
    state: State<'_, AppState>,
) -> AppResult<crate::works::PresentationDetail> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::works::presentation_detail(&database, node_id, false)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn get_work_detail(
    node_id: i64,
    state: State<'_, AppState>,
) -> AppResult<crate::works::PresentationDetail> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::works::presentation_detail(&database, node_id, true)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn search_library(
    query: String,
    root_id: Option<i64>,
    filters: Option<crate::search_filters::SearchFilters>,
    state: State<'_, AppState>,
) -> AppResult<Vec<SearchHit>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || match filters {
        Some(filters) => database.search_filtered(&query, root_id, &filters),
        None => database.search(&query, root_id),
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn organize_issues(state: State<'_, AppState>) -> AppResult<Vec<crate::organize::Issue>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || crate::organize::list(&database))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn set_issue_reminder(
    node_id: i64,
    kind: String,
    ignored: bool,
    state: State<'_, AppState>,
) -> AppResult<()> {
    crate::organize::remind(&state.database, node_id, &kind, ignored)
}
#[tauri::command]
pub async fn search_extensions(
    root_id: Option<i64>,
    state: State<'_, AppState>,
) -> AppResult<Vec<String>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::search_filters::extensions(&database, root_id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn start_scan(
    app: AppHandle,
    root_id: Option<i64>,
    node_id: Option<i64>,
    background: Option<bool>,
    state: State<'_, AppState>,
) -> AppResult<ScanStarted> {
    start_scan_internal(app, &state, root_id, node_id, background.unwrap_or(false))
}

#[tauri::command]
pub fn match_existing_content(
    app: AppHandle,
    node_ids: Option<Vec<i64>>,
    rematch_existing: Option<bool>,
    state: State<'_, AppState>,
) -> AppResult<ScanStarted> {
    let _scan_lifecycle = lock_scan_lifecycle(&state);
    ensure_no_active_scan(&state)?;
    let rematch_existing = rematch_existing.unwrap_or(false);
    let selection_missing = match node_ids.as_ref() {
        Some(ids) => ids.is_empty(),
        None => true,
    };
    if rematch_existing && selection_missing {
        return Err("重新匹配已有绑定时必须明确选择至少一个项目。".into());
    }
    let settings = state
        .database
        .get_settings(&state.default_cover_cache_dir)?;
    if !settings.bangumi_search_enabled {
        return Err("Bangumi 搜索已在设置中关闭。".into());
    }
    let nodes = state
        .database
        .list_bangumi_match_candidates(node_ids.as_deref(), rematch_existing)?;
    let scan_id = Uuid::new_v4().to_string();
    let root_id = nodes.first().map_or(0, |node| node.library_root_id);
    let current_path = nodes
        .first()
        .map_or_else(String::new, |node| node.absolute_path.clone());
    let initial = ScanProgress {
        background: false,
        library_changed: None,
        scan_id: scan_id.clone(),
        root_id,
        current_path,
        folders_scanned: 0,
        videos_found: 0,
        comic_books_found: 0,
        status: ScanStatus::Running,
        errors: 0,
        message: Some("正在匹配现有资源的封面与标题…".into()),
        phase: ScanPhase::AutoMatching,
        auto_match_current: 0,
        auto_match_total: nodes.len() as u64,
        auto_match_matched: 0,
        auto_match_pending: 0,
        auto_match_unmatched: 0,
        auto_match_errors: 0,
    };
    let control = ScanControl {
        unchanged_directories: Default::default(),
        scan_id: scan_id.clone(),
        cancel: Arc::new(AtomicBool::new(false)),
        progress: Arc::new(Mutex::new(initial)),
    };
    *state
        .active_scan
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(control.clone());

    let database = state.database.clone();
    let cache_root = active_cover_cache_directory(&state);
    let write_mode = if rematch_existing {
        auto_match::MatchWriteMode::ExplicitRematch
    } else {
        auto_match::MatchWriteMode::IfAbsent
    };
    let worker_activity = ScanWorkerActivity::start(Arc::clone(&state.scan_worker_active));
    if let Err(error) = std::thread::Builder::new()
        .name("m2shelf-existing-match".into())
        .spawn(move || {
            let _worker_activity = worker_activity;
            let poster_root = cache_root.clone().ok();
            scanner::run_existing_content_match(
                Some(&app),
                &database,
                nodes,
                &control,
                cache_root,
                write_mode,
            );
            if !control.cancel.load(Ordering::Relaxed) {
                if let Some(root) = poster_root {
                    crate::poster_cache::schedule_warmup(&database, &root);
                }
            }
        })
    {
        *state
            .active_scan
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        return Err(format!("无法启动现有资源匹配线程：{error}"));
    }
    Ok(ScanStarted { scan_id })
}

#[tauri::command]
pub fn cancel_scan(scan_id: String, state: State<'_, AppState>) -> AppResult<bool> {
    let guard = state
        .active_scan
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(control) = guard.as_ref() else {
        return Ok(false);
    };
    if control.scan_id != scan_id || !control.progress().status.is_active() {
        return Ok(false);
    }
    control.cancel.store(true, Ordering::Relaxed);
    let mut progress = control
        .progress
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    progress.status = ScanStatus::Cancelling;
    progress.message = Some("正在停止扫描…".into());
    Ok(true)
}

#[tauri::command]
pub fn get_scan_status(state: State<'_, AppState>) -> Option<ScanProgress> {
    state
        .active_scan
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(ScanControl::progress)
}

#[tauri::command]
pub fn set_node_type(
    node_id: i64,
    node_type: NodeType,
    state: State<'_, AppState>,
) -> AppResult<MediaNode> {
    state.database.set_node_type(node_id, node_type)
}

#[tauri::command]
pub fn reset_node_type(node_id: i64, state: State<'_, AppState>) -> AppResult<MediaNode> {
    state.database.reset_node_type(node_id)
}

#[tauri::command]
pub fn batch_set_node_type(
    node_ids: Vec<i64>,
    node_type: NodeType,
    state: State<'_, AppState>,
) -> AppResult<BatchMutationResult> {
    state.database.batch_set_node_type(&node_ids, node_type)
}

#[tauri::command]
pub fn batch_reset_node_type(
    node_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> AppResult<BatchMutationResult> {
    state.database.batch_reset_node_type(&node_ids)
}

#[tauri::command]
pub fn set_node_display_name(
    node_id: i64,
    display_name: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<MediaNode> {
    state.database.set_display_name(node_id, display_name)
}

#[tauri::command]
pub fn list_user_tags(
    node_id: i64,
    state: State<'_, AppState>,
) -> AppResult<Vec<UserTagMembership>> {
    state.database.list_user_tags(node_id)
}

#[tauri::command]
pub fn create_or_assign_user_tag(
    node_id: i64,
    name: String,
    state: State<'_, AppState>,
) -> AppResult<UserTag> {
    state.database.create_or_assign_user_tag(node_id, &name)
}

#[tauri::command]
pub fn assign_user_tag(
    node_id: i64,
    tag_id: i64,
    state: State<'_, AppState>,
) -> AppResult<UserTag> {
    state.database.assign_user_tag(node_id, tag_id)
}

#[tauri::command]
pub fn batch_assign_tag(
    node_ids: Vec<i64>,
    tag_id: i64,
    state: State<'_, AppState>,
) -> AppResult<BatchMutationResult> {
    state.database.batch_assign_tag(&node_ids, tag_id)
}

#[tauri::command]
pub fn batch_create_and_assign_tag(
    node_ids: Vec<i64>,
    name: String,
    state: State<'_, AppState>,
) -> AppResult<BatchMutationResult> {
    state.database.batch_create_and_assign_tag(&node_ids, &name)
}

#[tauri::command]
pub fn rename_user_tag(
    tag_id: i64,
    name: String,
    state: State<'_, AppState>,
) -> AppResult<UserTag> {
    state.database.rename_user_tag(tag_id, &name)
}

#[tauri::command]
pub fn unassign_user_tag(node_id: i64, tag_id: i64, state: State<'_, AppState>) -> AppResult<()> {
    state.database.unassign_user_tag(node_id, tag_id)
}

#[tauri::command]
pub fn delete_user_tag(tag_id: i64, state: State<'_, AppState>) -> AppResult<()> {
    state.database.delete_user_tag(tag_id)
}

#[tauri::command]
pub fn get_bangumi_search_prefill(
    node_id: i64,
    state: State<'_, AppState>,
) -> AppResult<BangumiSearchPrefill> {
    let node = state.database.get_node(node_id)?;
    if !node.can_bind_bangumi() {
        return Err("只有作品或包含视频的系列可以搜索 Bangumi。".into());
    }
    let (_, evidence) = crate::media_scraper::indexed_evidence(&state.database, &node)?;
    Ok(BangumiSearchPrefill {
        original_name: node.folder_name,
        extracted_name: evidence.primary_title.clone(),
        candidates: crate::auto_match::match_queries(&evidence),
    })
}

#[tauri::command]
pub async fn search_bangumi(
    keyword: String,
    limit: Option<usize>,
    node_id: Option<i64>,
    state: State<'_, AppState>,
) -> AppResult<Vec<BangumiSubject>> {
    let settings = state
        .database
        .get_settings(&state.default_cover_cache_dir)?;
    if !settings.bangumi_search_enabled {
        return Err("Bangumi 搜索已在设置中关闭。".into());
    }
    let kind = node_id
        .map(|id| state.database.get_node(id).map(|n| n.media_kind))
        .transpose()?
        .unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        bangumi::search_for_kind(&keyword, limit.unwrap_or(20), kind)
    })
    .await
    .map_err(|error| format!("Bangumi 搜索任务失败：{error}"))?
}

#[tauri::command]
pub async fn bind_bangumi(
    node_id: i64,
    subject: BangumiSubject,
    state: State<'_, AppState>,
) -> AppResult<MetadataBinding> {
    validate_bangumi_subject_payload(&subject)?;
    let database = state.database.clone();
    let node = database.get_node(node_id)?;
    if !node.media_kind.accepts_subject(subject.subject_type) || subject.subject_id <= 0 {
        return Err("BANGUMI_MEDIA_KIND_CONFLICT".into());
    }
    if !node.can_bind_bangumi() {
        return Err("只有作品或包含视频的系列可以绑定 Bangumi。".into());
    }
    let (_, evidence) = crate::media_scraper::indexed_evidence(&database, &node)?;
    let confirmed_aliases = title_extractor::confirmed_alias_candidates(&evidence);
    // Detail enrichment improves multilingual metadata, but the confirmed search subject remains
    // authoritative when the optional detail request is unavailable.
    let fallback_subject = subject.clone();
    let subject_for_detail = subject;
    let mut detail_synced = false;
    let subject = match tauri::async_runtime::spawn_blocking(move || {
        bangumi::enrich_subject(&subject_for_detail)
    })
    .await
    {
        Ok(Ok(subject)) => {
            detail_synced = true;
            subject
        }
        Ok(Err(_)) | Err(_) => fallback_subject,
    };
    let active_cache = active_cover_cache_directory(&state);
    let previous_path =
        database.save_confirmed_binding_with_aliases(node_id, &subject, &confirmed_aliases)?;
    crate::tmdb::deactivate(&database, node_id)?;
    if detail_synced {
        database.complete_provider_alias_sync(&subject)?;
    }
    if let (Some(path), Ok(cache_root)) = (previous_path.as_deref(), active_cache.as_ref()) {
        let cache_operation = cache::begin_cover_cache_operation();
        remove_cached_file_if_unreferenced(&cache_operation, &database, path, cache_root);
    }
    let cache_root = match active_cache {
        Ok(cache_root) => cache_root,
        Err(error) => {
            database.set_binding_cover_error_if_subject(
                node_id,
                subject.subject_id,
                Some(&error),
            )?;
            return database
                .get_binding(node_id)?
                .ok_or_else(|| "保存 Bangumi 绑定失败。".to_string());
        }
    };
    let subject_for_download = subject.clone();
    let database_for_download = database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        refresh_bound_cover(
            &database_for_download,
            &cache_root,
            node_id,
            &subject_for_download,
        )
    })
    .await
    .map_err(|error| format!("封面下载任务失败：{error}"))?
}

#[tauri::command]
pub async fn bind_work_bangumi(
    target: WorkTarget,
    subject: BangumiSubject,
    state: State<'_, AppState>,
) -> AppResult<MetadataBinding> {
    validate_bindable_bangumi_subject(&subject)?;
    state.database.validate_work_target(&target)?;
    let fallback = subject.clone();
    let enriched = tauri::async_runtime::spawn_blocking(move || bangumi::enrich_subject(&subject))
        .await
        .ok()
        .and_then(Result::ok);
    let subject = enriched.clone().unwrap_or(fallback);
    let changed = state
        .database
        .change_work_binding(&target, Some(&subject))?;
    for id in &target.source_node_ids {
        crate::tmdb::deactivate(&state.database, *id)?;
    }
    if let Some(detail) = enriched {
        state.database.complete_provider_alias_sync(&detail)?;
    }
    let database = state.database.clone();
    let cache_root = active_cover_cache_directory(&state);
    tauri::async_runtime::spawn_blocking(move || {
        refresh_work_cover(
            &database,
            cache_root,
            changed.cover_target,
            &subject,
            changed.previous_paths,
            false,
            Vec::new(),
        )
    })
    .await
    .map_err(|error| format!("封面下载任务失败：{error}"))?
}

#[tauri::command]
pub async fn retry_work_bangumi_cover(
    target: WorkTarget,
    failed_source_node_ids: Option<Vec<i64>>,
    state: State<'_, AppState>,
) -> AppResult<MetadataBinding> {
    let sources = state.database.validate_work_target(&target)?;
    let binding = sources[0]
        .binding
        .clone()
        .ok_or_else(|| "作品尚未绑定 Bangumi。".to_string())?;
    let failed = failed_source_node_ids.unwrap_or_default();
    if failed.iter().any(|id| !target.source_node_ids.contains(id)) {
        return Err("WORK_TARGET_STALE".into());
    }
    // Capture the repair set before a download can replace the shared cache file.
    let repair_ids = sources
        .iter()
        .filter(|node| {
            node.cover_source != CoverSource::Manual
                && (failed.contains(&node.id)
                    || node
                        .cover_cache_path
                        .as_ref()
                        .is_none_or(|path| !cache::cached_cover_is_valid(Path::new(path)))
                    || node
                        .binding
                        .as_ref()
                        .is_some_and(|binding| binding.cover_download_error.is_some()))
        })
        .map(|node| node.id)
        .collect::<Vec<_>>();
    if repair_ids.is_empty() {
        return Ok(binding);
    }
    let subject = BangumiSubject {
        subject_id: binding.provider_subject_id,
        subject_type: binding.provider_subject_type,
        title: binding.provider_title,
        title_cn: binding.provider_title_cn,
        title_en: binding.provider_title_en,
        title_ja: binding.provider_title_ja,
        title_ko: binding.provider_title_ko,
        match_aliases: binding.provider_aliases,
        date: binding.provider_date,
        image_url: binding.provider_image_url,
        summary: None,
    };
    let fallback = subject.clone();
    let subject = tauri::async_runtime::spawn_blocking(move || bangumi::enrich_subject(&subject))
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or(fallback);
    state.database.validate_work_target(&target)?;
    let database = state.database.clone();
    let cache_root = active_cover_cache_directory(&state);
    tauri::async_runtime::spawn_blocking(move || {
        refresh_work_cover(
            &database,
            cache_root,
            target,
            &subject,
            Vec::new(),
            true,
            repair_ids,
        )
    })
    .await
    .map_err(|error| format!("封面下载任务失败：{error}"))?
}

#[tauri::command]
pub fn clear_work_bangumi_binding(target: WorkTarget, state: State<'_, AppState>) -> AppResult<()> {
    let operation = cache::begin_cover_cache_operation();
    let paths = state
        .database
        .change_work_binding(&target, None)?
        .previous_paths;
    if let Ok(cache_root) = active_cover_cache_directory(&state) {
        for path in paths {
            remove_cached_file_if_unreferenced(&operation, &state.database, &path, &cache_root);
        }
    }
    Ok(())
}

fn refresh_work_cover(
    database: &Database,
    cache_root: AppResult<PathBuf>,
    target: WorkTarget,
    subject: &BangumiSubject,
    previous_paths: Vec<PathBuf>,
    retry_only: bool,
    repair_ids: Vec<i64>,
) -> AppResult<MetadataBinding> {
    let operation = cache::begin_cover_cache_operation();
    let download = match &cache_root {
        Ok(root) => bangumi::download_cover(&operation, root, subject),
        Err(error) => Err(error.clone()),
    };
    let (path, error) = match download {
        Ok(Some(path)) => (Some(path), None),
        Ok(None) => (None, Some("该 Bangumi 条目没有可用封面。".to_string())),
        Err(error) => (None, Some(error)),
    };
    let result = database.apply_work_cover_with_failures(
        &target,
        subject,
        path.as_deref(),
        error.as_deref(),
        retry_only,
        &repair_ids,
    );
    if let Ok(root) = &cache_root {
        if result.is_err() {
            if let Some(path) = &path {
                remove_cached_file_if_unreferenced(&operation, database, path, root);
            }
        }
        for old in previous_paths
            .into_iter()
            .chain(result.as_ref().ok().into_iter().flatten().cloned())
        {
            remove_cached_file_if_unreferenced(&operation, database, &old, root);
        }
    }
    result?;
    if let Ok(root) = &cache_root {
        crate::poster_cache::schedule_warmup(database, root);
    }
    let mut binding = database
        .get_binding(target.source_node_ids[0])?
        .ok_or_else(|| "WORK_TARGET_STALE".to_string())?;
    // Report a failure from any automatic-cover source, even if the representative has manual art.
    if let Some(error) = error {
        binding.cover_download_error = Some(error);
    }
    Ok(binding)
}

#[tauri::command]
pub async fn sync_pending_bangumi_aliases(state: State<'_, AppState>) -> AppResult<bool> {
    if state.update_exit_in_progress.load(Ordering::Acquire)
        || !state
            .database
            .get_settings(&state.default_cover_cache_dir)?
            .bangumi_search_enabled
    {
        return Ok(false);
    }
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || crate::alias_sync::sync_pending(&database))
        .await
        .map_err(|error| format!("Bangumi 别称补全任务失败：{error}"))?
}

#[tauri::command]
pub async fn retry_bangumi_cover(
    node_id: i64,
    state: State<'_, AppState>,
) -> AppResult<MetadataBinding> {
    let node = state.database.get_node(node_id)?;
    if !node.can_bind_bangumi() {
        return Err("只有作品或包含视频的系列可以重试 Bangumi 封面。".into());
    }
    let binding = state
        .database
        .get_binding(node_id)?
        .ok_or_else(|| "该目录尚未绑定 Bangumi。".to_string())?;
    let subject = BangumiSubject {
        subject_id: binding.provider_subject_id,
        title: binding.provider_title,
        title_cn: binding.provider_title_cn,
        title_en: binding.provider_title_en,
        title_ja: binding.provider_title_ja,
        title_ko: binding.provider_title_ko,
        match_aliases: binding.provider_aliases,
        date: binding.provider_date,
        image_url: binding.provider_image_url,
        summary: None,
        subject_type: binding.provider_subject_type,
    };
    let database = state.database.clone();
    let fallback_subject = subject.clone();
    let subject_for_detail = subject;
    let subject = match tauri::async_runtime::spawn_blocking(move || {
        bangumi::enrich_subject(&subject_for_detail)
    })
    .await
    {
        Ok(Ok(subject)) => {
            database.update_binding_if_subject(node_id, &subject)?;
            subject
        }
        Ok(Err(_)) | Err(_) => fallback_subject,
    };
    let cache_root = match active_cover_cache_directory(&state) {
        Ok(cache_root) => cache_root,
        Err(error) => {
            database.set_binding_cover_error_if_subject(
                node_id,
                subject.subject_id,
                Some(&error),
            )?;
            return database
                .get_binding(node_id)?
                .ok_or_else(|| "该目录尚未绑定 Bangumi。".to_string());
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        refresh_bound_cover(&database, &cache_root, node_id, &subject)
    })
    .await
    .map_err(|error| format!("封面下载任务失败：{error}"))?
}

fn refresh_bound_cover(
    database: &crate::db::Database,
    cache_root: &Path,
    node_id: i64,
    subject: &BangumiSubject,
) -> AppResult<MetadataBinding> {
    let cache_operation = cache::begin_cover_cache_operation();
    let previous_node = database.get_node(node_id)?;
    match bangumi::download_cover(&cache_operation, cache_root, subject) {
        Ok(Some(path)) => {
            if database.set_bangumi_cover_for_subject_unless_manual(
                node_id,
                subject.subject_id,
                &path,
            )? {
                database.set_binding_cover_error_if_subject(node_id, subject.subject_id, None)?;
                if let Some(old_path) = previous_node.cover_cache_path.as_deref() {
                    let old_path = Path::new(old_path);
                    if old_path != path {
                        remove_cached_file_if_unreferenced(
                            &cache_operation,
                            database,
                            old_path,
                            cache_root,
                        );
                    }
                }
            } else {
                remove_cached_file_if_unreferenced(&cache_operation, database, &path, cache_root);
            }
        }
        Ok(None) => {
            database.set_binding_cover_error_if_subject(
                node_id,
                subject.subject_id,
                Some("该 Bangumi 条目没有可用封面。"),
            )?;
        }
        Err(error) => {
            database.set_binding_cover_error_if_subject(
                node_id,
                subject.subject_id,
                Some(&error),
            )?;
        }
    }
    crate::poster_cache::schedule_warmup(database, cache_root);
    database
        .get_binding(node_id)?
        .ok_or_else(|| "保存 Bangumi 绑定失败。".to_string())
}

#[tauri::command]
pub fn clear_bangumi_binding(node_id: i64, state: State<'_, AppState>) -> AppResult<MediaNode> {
    let cache_operation = cache::begin_cover_cache_operation();
    let active_cache = active_cover_cache_directory(&state).ok();
    if let Some(path) = state.database.clear_binding(node_id)? {
        if let Some(cache_root) = active_cache.as_deref() {
            remove_cached_file_if_unreferenced(
                &cache_operation,
                &state.database,
                &path,
                cache_root,
            );
        }
    }
    state.database.get_node(node_id)
}

#[tauri::command]
pub fn set_container_cover(
    node_id: i64,
    source_path: String,
    state: State<'_, AppState>,
) -> AppResult<MediaNode> {
    let cache_operation = cache::begin_cover_cache_operation();
    let node = state.database.get_node(node_id)?;
    if !matches!(node.node_type, NodeType::Container | NodeType::Mixed) {
        return Err("本地图片封面只用于系列或其他资源。".into());
    }
    let cache_root = active_cover_cache_directory(&state)?;
    let destination = cache::copy_manual_cover(
        &cache_operation,
        &cache_root,
        node_id,
        Path::new(&source_path),
    )?;
    state
        .database
        .set_node_cover(node_id, CoverSource::Manual, Some(&destination))?;
    crate::poster_cache::schedule_warmup(&state.database, &cache_root);
    if let Some(old_path) = node.cover_cache_path.as_deref() {
        let old_path = Path::new(old_path);
        if old_path != destination {
            remove_cached_file_if_unreferenced(
                &cache_operation,
                &state.database,
                old_path,
                &cache_root,
            );
        }
    }
    state.database.get_node(node_id)
}

#[tauri::command]
pub fn clear_node_cover(node_id: i64, state: State<'_, AppState>) -> AppResult<MediaNode> {
    let cache_operation = cache::begin_cover_cache_operation();
    let node = state.database.get_node(node_id)?;
    if node.cover_source == CoverSource::Bangumi && state.database.get_binding(node_id)?.is_some() {
        return Err("请使用“清除 Bangumi 绑定”；不能只移除仍被绑定使用的封面。".into());
    }
    let active_cache = active_cover_cache_directory(&state).ok();
    let previous_path = node.cover_cache_path.map(PathBuf::from);
    state
        .database
        .set_node_cover(node_id, CoverSource::Placeholder, None)?;
    if let (Some(path), Some(cache_root)) = (previous_path.as_deref(), active_cache.as_deref()) {
        remove_cached_file_if_unreferenced(&cache_operation, &state.database, path, cache_root);
    }
    state.database.get_node(node_id)
}

#[tauri::command]
pub async fn begin_pdf_cover(
    node_id: Option<i64>,
    comic_book_id: Option<i64>,
    expected_revision: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<Option<crate::pdf_cover::Request>> {
    let db = state.database.clone();
    let cache = db.poster_cache_root(&state.default_cover_cache_dir)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::pdf_cover::begin(
            &db,
            &cache,
            node_id,
            comic_book_id,
            expected_revision.as_deref(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn commit_pdf_cover(
    ticket: String,
    png_base64: String,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    let db = state.database.clone();
    let cache = db.poster_cache_root(&state.default_cover_cache_dir)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::pdf_cover::commit(&db, &cache, &ticket, &png_base64)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn cancel_pdf_cover(ticket: String) {
    crate::pdf_cover::cancel(&ticket);
}

#[tauri::command]
pub async fn get_cover_data_url(
    node_id: i64,
    width: Option<u32>,
    state: State<'_, AppState>,
) -> AppResult<Option<String>> {
    let database = state.database.clone();
    let cache_root = database.poster_cache_root(&state.default_cover_cache_dir)?;
    tauri::async_runtime::spawn_blocking(move || {
        let width = width.unwrap_or(512);
        if !crate::poster_cache::WIDTHS.contains(&width) {
            return Err("缩略图尺寸无效。".into());
        }
        match crate::poster_cache::cover_data_url(&database, &cache_root, node_id, width) {
            Ok(value) => Ok(value),
            Err(_) => {
                // Unwritable/disconnected custom caches must not hide a retained original cover.
                let operation = cache::begin_cover_cache_operation();
                let (path, roots) = database.cover_read_context(node_id)?;
                if let Some(path) = path {
                    for root in roots {
                        if cache::is_equal_or_within_checked(&path, &root)? {
                            return Err("拒绝从媒体资源库读取缓存封面。".into());
                        }
                    }
                    cache::cover_data_url(&operation, &path).map(Some)
                } else {
                    crate::comic_reader::first_image_cover(&database, node_id)
                }
            }
        }
    })
    .await
    .map_err(|e| format!("封面读取任务失败：{e}"))?
}

#[tauri::command]
pub async fn get_book_cover_data_url(
    comic_book_id: i64,
    expected_revision: String,
    width: u32,
    state: State<'_, AppState>,
) -> AppResult<Option<String>> {
    let database = state.database.clone();
    let cache_root = database.poster_cache_root(&state.default_cover_cache_dir)?;
    tauri::async_runtime::spawn_blocking(move || {
        if !crate::poster_cache::WIDTHS.contains(&width) {
            return Err("缩略图尺寸无效。".into());
        }
        match crate::poster_cache::book_cover_data_url(
            &database,
            &cache_root,
            comic_book_id,
            &expected_revision,
            width,
        ) {
            Ok(value) => Ok(value),
            Err(_) => {
                use base64::Engine;
                crate::cover_selection::book_manual(&database, comic_book_id)
                    .and_then(|path| {
                        if let Some(path) = path {
                            cache::read_cover_payload(&path).map(Some)
                        } else {
                            crate::comic_reader::read_book_cover(
                                &database,
                                comic_book_id,
                                &expected_revision,
                            )
                        }
                    })
                    .map(|bytes| {
                        bytes.map(|bytes| {
                            let mime = image::guess_format(&bytes)
                                .map(|format| format.to_mime_type())
                                .unwrap_or("image/jpeg");
                            format!(
                                "data:{mime};base64,{}",
                                base64::engine::general_purpose::STANDARD.encode(bytes)
                            )
                        })
                    })
            }
        }
    })
    .await
    .map_err(|_| "COMIC_READ_FAILED")?
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<AppSettings> {
    state.database.get_settings(&state.default_cover_cache_dir)
}

#[tauri::command]
pub fn get_collection_sort_preferences(
    state: State<'_, AppState>,
) -> AppResult<CollectionSortPreferences> {
    state.database.get_collection_sort_preferences()
}

#[tauri::command]
pub fn update_collection_sort_preference(
    scope: CollectionSortScope,
    sort: CollectionSort,
    state: State<'_, AppState>,
) -> AppResult<CollectionSort> {
    state
        .database
        .update_collection_sort_preference(scope, sort)
}

#[tauri::command]
pub fn update_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
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
    let cache_directory = prepare_cover_cache_directory(&state, &settings.cover_cache_directory)?;
    let previous_cache = state
        .database
        .poster_cache_root(&state.default_cover_cache_dir)?;
    let mut settings = settings;
    settings.cover_cache_directory = cache_directory.to_string_lossy().into_owned();
    let saved = state
        .database
        .update_settings(&settings, &state.default_cover_cache_dir)?;
    if !same_path(&previous_cache, &cache_directory) {
        crate::poster_resource::clear_memory();
        crate::pdf_cover::cancel_all();
        crate::poster_cache::cancel_warmup();
        crate::poster_cache::schedule_warmup(&state.database, &cache_directory);
    }
    Ok(saved)
}

#[tauri::command]
pub async fn test_mpv(
    path: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<PlayerTestResult> {
    let selected = match path {
        Some(path) if !path.trim().is_empty() => {
            if path.chars().count() > crate::models::MAX_SETTINGS_PATH_CHARS {
                return Err(format!(
                    "播放器路径不能超过 {} 个字符。",
                    crate::models::MAX_SETTINGS_PATH_CHARS
                ));
            }
            PathBuf::from(path)
        }
        _ => match player::resolve(
            &state
                .database
                .get_settings(&state.default_cover_cache_dir)?,
        )? {
            Some(path) => path,
            None => {
                return Ok(PlayerTestResult {
                    ok: true,
                    message: "SYSTEM".into(),
                    version: Some("SYSTEM".into()),
                })
            }
        },
    };
    tauri::async_runtime::spawn_blocking(move || player::test(&selected))
        .await
        .map_err(|error| format!("播放器测试任务失败：{error}"))?
}

#[tauri::command]
pub async fn play_media(
    media_file_id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let database = state.database.clone();
    let cache = state.default_cover_cache_dir.clone();
    let monitor = state.mpv_monitor.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let media = database.get_media_file(media_file_id)?;
        let settings = database.get_settings(&cache)?;
        let mpv = player::resolve(&settings)?;
        launch_media_and_record_watch(
            &database,
            &media,
            mpv.as_deref().unwrap_or(Path::new("")),
            |_, _| {
                monitor.launch(&database, mpv.as_deref(), &media, move |event| {
                    let _ = app.emit("mpv-playback", event);
                })
            },
        )
    })
    .await
    .map_err(|_| "PLAYER_LAUNCH_FAILED".to_string())?
}

fn launch_media_and_record_watch<F>(
    database: &Database,
    media: &MediaFile,
    executable: &Path,
    launch: F,
) -> AppResult<()>
where
    F: FnOnce(&Path, &Path) -> AppResult<()>,
{
    // The process is launched first. A validation or spawn failure must never create history.
    launch(executable, Path::new(&media.absolute_path))?;
    database.record_node_watched(media.node_id)
}

#[tauri::command]
pub fn open_node_in_explorer(node_id: i64, state: State<'_, AppState>) -> AppResult<()> {
    let node = state.database.get_node(node_id)?;
    player::reveal(Path::new(&node.absolute_path))
}

#[tauri::command]
pub fn open_media_in_explorer(media_file_id: i64, state: State<'_, AppState>) -> AppResult<()> {
    let media = state.database.get_media_file(media_file_id)?;
    player::reveal(Path::new(&media.absolute_path))
}

#[tauri::command]
pub async fn open_resource_file(
    resource_file_id: i64,
    state: State<'_, AppState>,
) -> AppResult<Option<crate::comics::ComicBook>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(book) = crate::comic_reader::open_resource(&database, resource_file_id)? {
            return Ok(Some(book));
        }
        let resource = database.get_resource_file(resource_file_id)?;
        database.read_snapshot(|c| crate::db::ensure_node_visible_conn(c, resource.node_id))?;
        player::open_with_default_application(Path::new(&resource.absolute_path))?;
        Ok(None)
    })
    .await
    .map_err(|_| "COMIC_READ_FAILED".to_string())?
}

#[tauri::command]
pub fn open_resource_in_explorer(
    resource_file_id: i64,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let resource = state.database.get_resource_file(resource_file_id)?;
    player::reveal(Path::new(&resource.absolute_path))
}

#[tauri::command]
pub fn open_cover_cache_directory(state: State<'_, AppState>) -> AppResult<()> {
    let cache_root = active_cover_cache_directory(&state)?;
    player::reveal(&cache_root)
}

#[tauri::command]
pub fn open_bangumi_subject(node_id: i64, state: State<'_, AppState>) -> AppResult<()> {
    let binding = state
        .database
        .get_binding(node_id)?
        .ok_or_else(|| "该目录尚未绑定 Bangumi。".to_string())?;
    player::open_external_url(&bangumi_subject_url(binding.provider_subject_id)?)
}

fn bangumi_subject_url(subject_id: i64) -> AppResult<String> {
    if subject_id <= 0 {
        return Err("Bangumi 条目 ID 无效。".into());
    }
    Ok(format!("https://bgm.tv/subject/{subject_id}"))
}

#[tauri::command]
pub fn open_external_url(url: String) -> AppResult<()> {
    // About links are intentionally allow-listed. This command is not a general URL launcher.
    let allowed_url =
        allowed_external_url(&url).ok_or_else(|| "拒绝打开未授权的外部链接。".to_string())?;
    player::open_external_url(allowed_url)
}

fn allowed_external_url(url: &str) -> Option<&'static str> {
    let url = url.trim();
    if url.trim_end_matches('/') == BILIBILI_URL {
        Some(BILIBILI_URL)
    } else if url == "https://www.themoviedb.org" {
        Some("https://www.themoviedb.org")
    } else if url == X_URL {
        Some(X_URL)
    } else {
        None
    }
}

#[tauri::command]
pub async fn get_cache_stats(state: State<'_, AppState>) -> AppResult<CacheStats> {
    let database = state.database.clone();
    let default = state.default_cover_cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _cache_operation = cache::begin_cover_cache_operation();
        cache::stats(&active_cover_cache_directory_for(&database, &default)?)
    })
    .await
    .map_err(|error| format!("缓存统计任务失败：{error}"))?
}

#[tauri::command]
pub fn get_poster_cache_status(state: State<'_, AppState>) -> crate::models::PosterCacheStatus {
    crate::poster_cache::status(&state.database)
}

#[tauri::command]
pub async fn get_poster_cache_failures(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::models::PosterCacheFailure>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || database.poster_failures())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn retry_poster_cache(state: State<'_, AppState>) -> AppResult<()> {
    let root = state
        .database
        .poster_cache_root(&state.default_cover_cache_dir)?;
    crate::poster_cache::retry_warmup(&state.database, &root);
    Ok(())
}

#[tauri::command]
pub fn clear_cover_cache(state: State<'_, AppState>) -> AppResult<CacheStats> {
    let _scan_lifecycle = lock_scan_lifecycle(&state);
    ensure_no_active_scan(&state)?;
    let cache_clear = cache::begin_cover_cache_clear();
    let cache_root = active_cover_cache_directory(&state)?;
    let affected = state
        .database
        .list_cover_records()?
        .into_iter()
        .filter(|(_, _, path)| cache::is_equal_or_within(path, &cache_root))
        .collect::<Vec<_>>();
    crate::pdf_cover::cancel_all();
    crate::poster_resource::clear_memory();
    cache::clear_cover_cache(&cache_clear, &cache_root)?;
    state.database.clear_cover_paths_for_nodes(&affected)?;
    state.database.clear_poster_checkpoint()?;
    crate::poster_cache::forget_status(&state.database);
    cache::stats(&cache_root)
}

#[tauri::command]
pub fn rebuild_index(app: AppHandle, state: State<'_, AppState>) -> AppResult<RebuildResult> {
    // Rescanning upserts paths and removes stale index entries, while preserving manual overrides,
    // display names, metadata bindings and source files.
    start_scan_internal(app, &state, None, None, false)
}

fn start_scan_internal(
    app: AppHandle,
    state: &AppState,
    root_id: Option<i64>,
    node_id: Option<i64>,
    background: bool,
) -> AppResult<ScanStarted> {
    let _scan_lifecycle = lock_scan_lifecycle(state);
    ensure_no_active_scan(state)?;
    if root_id.is_none() && node_id.is_some() {
        return Err("扫描指定目录时必须同时提供资源库 ID。".into());
    }
    if background && node_id.is_some() {
        return Err("后台增量检查须以资源库为单位。".into());
    }
    let mut roots = if let Some(root_id) = root_id {
        vec![state.database.get_root(root_id)?]
    } else {
        state.database.list_roots()?
    };
    if roots.is_empty() {
        return Err("请先添加至少一个资源库。".into());
    }
    // An unplugged disk must not prevent other libraries from refreshing, nor may it enter
    // stale-row cleanup. Explicit single-root scans still return the original validation error.
    let root_count = roots.len();
    if !background && root_id.is_none() {
        roots.retain(|root| Path::new(&root.path).is_dir());
    }
    let unavailable_roots = (root_count - roots.len()) as u64;
    if roots.is_empty() {
        return Err("所有资源库当前均不可访问；已有索引已保留。".into());
    }
    // Fail synchronously for stale, forged, or legacy-overlapping roots. The scanner repeats this
    // immediately before filesystem traversal to close the command-to-worker timing gap.
    if !background {
        for root in &roots {
            state.database.validate_scan_root(root)?;
        }
    }
    let mut targets = Vec::new();
    for root in roots {
        if let Some(node_id) = node_id {
            let node = state.database.get_node(node_id)?;
            if node.library_root_id != root.id {
                return Err("目录节点不属于该资源库。".into());
            }
            if matches!(root.recognition_mode, LibraryRecognitionMode::VideoFile) {
                targets.push(ScanTarget {
                    path: PathBuf::from(&root.path),
                    root,
                    parent_node_id: None,
                });
            } else {
                targets.push(ScanTarget {
                    root,
                    path: PathBuf::from(&node.absolute_path),
                    parent_node_id: node.parent_node_id,
                });
            }
        } else {
            targets.push(ScanTarget {
                path: PathBuf::from(&root.path),
                root,
                parent_node_id: None,
            });
        }
    }
    let scan_id = Uuid::new_v4().to_string();
    let initial = ScanProgress {
        background,
        library_changed: Some(false),
        scan_id: scan_id.clone(),
        root_id: targets[0].root.id,
        current_path: targets[0].path.to_string_lossy().into_owned(),
        folders_scanned: 0,
        videos_found: 0,
        comic_books_found: 0,
        status: ScanStatus::Running,
        errors: unavailable_roots,
        message: Some("正在扫描…".into()),
        phase: crate::models::ScanPhase::Scanning,
        auto_match_current: 0,
        auto_match_total: 0,
        auto_match_matched: 0,
        auto_match_pending: 0,
        auto_match_unmatched: 0,
        auto_match_errors: 0,
    };
    let control = ScanControl {
        unchanged_directories: Default::default(),
        scan_id: scan_id.clone(),
        cancel: Arc::new(AtomicBool::new(false)),
        progress: Arc::new(Mutex::new(initial)),
    };
    let settings = state
        .database
        .get_settings(&state.default_cover_cache_dir)?;
    let extensions = settings.video_extensions;
    let auto_match_cache_root = settings
        .bangumi_search_enabled
        .then(|| active_cover_cache_directory(state));
    let poster_cache_root = state
        .database
        .poster_cache_root(&state.default_cover_cache_dir)
        .ok();
    let mut inserted_any = false;
    for target in &targets {
        if let Err(error) = state.database.start_scan_run(&scan_id, target.root.id) {
            if inserted_any {
                let _ = state.database.delete_scan_runs(&scan_id);
            }
            return Err(error);
        }
        inserted_any = true;
    }
    *state
        .active_scan
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(control.clone());
    let database = state.database.clone();
    let worker_activity = ScanWorkerActivity::start(Arc::clone(&state.scan_worker_active));
    if let Err(error) = std::thread::Builder::new()
        .name("m2shelf-scan".into())
        .spawn(move || {
            let _worker_activity = worker_activity;
            scanner::run_scan_with_auto_match(
                Some(&app),
                &database,
                targets,
                &control,
                &extensions,
                auto_match_cache_root,
            );
            if !control.cancel.load(Ordering::Relaxed) {
                if let Some(root) = poster_cache_root {
                    let progress = control.progress();
                    if progress.background && progress.library_changed == Some(false) {
                        crate::poster_cache::resume_warmup(&database, &root);
                    } else {
                        crate::poster_cache::schedule_warmup(&database, &root);
                    }
                }
            }
        })
    {
        *state
            .active_scan
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        let _ = state.database.delete_scan_runs(&scan_id);
        return Err(format!("无法启动扫描线程：{error}"));
    }
    Ok(ScanStarted { scan_id })
}

fn ensure_no_active_scan(state: &AppState) -> AppResult<()> {
    if state.update_exit_in_progress.load(Ordering::Acquire) {
        return Err("应用正在退出以完成更新，不能开始新的资源操作。".into());
    }
    if state.scan_worker_active.load(Ordering::Acquire) {
        return Err("SCAN_WORKER_BUSY".into());
    }
    let guard = state
        .active_scan
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard
        .as_ref()
        .is_some_and(|control| control.progress().status.is_active())
    {
        Err("SCAN_WORKER_BUSY".into())
    } else {
        Ok(())
    }
}

fn lock_scan_lifecycle(state: &AppState) -> std::sync::MutexGuard<'_, ()> {
    state
        .scan_lifecycle
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct ScanWorkerActivity(Arc<AtomicBool>);

impl ScanWorkerActivity {
    fn start(active: Arc<AtomicBool>) -> Self {
        active.store(true, Ordering::Release);
        Self(active)
    }
}

impl Drop for ScanWorkerActivity {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn ensure_update_cache_outside_library_roots(state: &AppState) -> AppResult<Vec<PathBuf>> {
    let roots = library_root_paths(state)?;
    let update_cache = state.update_manager.cache_dir();
    crate::update::ensure_safe_update_cache(update_cache, &roots)?;
    Ok(roots)
}

fn validate_new_library_root_update_boundary(
    candidate: &Path,
    update_cache: &Path,
    mut existing_roots: Vec<PathBuf>,
) -> AppResult<()> {
    if cache::paths_overlap_checked(candidate, update_cache)? {
        return Err("媒体资源库不能等于、包含应用更新缓存，或位于更新缓存目录内。".into());
    }
    match std::fs::symlink_metadata(update_cache) {
        Ok(_) => {
            existing_roots.push(candidate.to_path_buf());
            crate::update::validate_existing_update_cache(update_cache, &existing_roots)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("无法检查应用更新缓存：{error}")),
    }
    Ok(())
}

pub fn cancel_scan_on_exit(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Some(control) = state
        .active_scan
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
    {
        control.cancel.store(true, Ordering::Relaxed);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmdb_bound_movie_link_uses_only_the_official_movie_path() {
        assert_eq!(
            tmdb_movie_url(27205).unwrap(),
            "https://www.themoviedb.org/movie/27205"
        );
        assert!(tmdb_movie_url(0).is_err());
        assert!(tmdb_movie_url(-1).is_err());
    }

    #[test]
    fn bound_subject_link_uses_only_the_official_subject_path() {
        assert_eq!(
            bangumi_subject_url(174584).unwrap(),
            "https://bgm.tv/subject/174584"
        );
        assert!(bangumi_subject_url(0).is_err());
        assert!(bangumi_subject_url(-1).is_err());
    }

    use rusqlite::params;
    use tempfile::TempDir;

    #[test]
    fn scan_worker_activity_stays_set_until_the_worker_scope_ends() {
        let active = Arc::new(AtomicBool::new(false));
        {
            let _worker = ScanWorkerActivity::start(Arc::clone(&active));
            assert!(active.load(Ordering::Acquire));
        }
        assert!(!active.load(Ordering::Acquire));
    }

    #[test]
    fn adding_a_root_that_contains_a_missing_update_cache_is_read_only() {
        let temp = TempDir::new().unwrap();
        let candidate = temp.path().join("library");
        std::fs::create_dir(&candidate).unwrap();
        let sentinel = candidate.join("sentinel.bin");
        std::fs::write(&sentinel, b"must remain unchanged").unwrap();
        let update_cache = candidate.join("application-data").join("updates");
        let entries_before = std::fs::read_dir(&candidate)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();

        assert!(
            validate_new_library_root_update_boundary(&candidate, &update_cache, Vec::new())
                .is_err()
        );

        let entries_after = std::fs::read_dir(&candidate)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries_after, entries_before);
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"must remain unchanged");
        assert!(!update_cache.exists());
        assert!(!candidate.join("application-data").exists());
    }

    #[test]
    fn about_link_allowlist_rejects_lookalikes_and_other_schemes() {
        assert_eq!(allowed_external_url(BILIBILI_URL), Some(BILIBILI_URL));
        assert_eq!(
            allowed_external_url(&format!("{BILIBILI_URL}/")),
            Some(BILIBILI_URL)
        );
        assert_eq!(allowed_external_url(X_URL), Some(X_URL));
        assert_eq!(allowed_external_url(&format!(" {X_URL} ")), Some(X_URL));
        assert_eq!(allowed_external_url(&format!("{X_URL}/")), None);
        assert_eq!(
            allowed_external_url("http://space.bilibili.com/2903441"),
            None
        );
        assert_eq!(
            allowed_external_url("https://space.bilibili.com/2903441?from=search"),
            None
        );
        assert_eq!(
            allowed_external_url("https://space.bilibili.com/2903441.evil.example"),
            None
        );
        assert_eq!(
            allowed_external_url("https://x.com.evil.example/f_undermori"),
            None
        );
        assert_eq!(
            allowed_external_url("https://x.com/f_undermori?redirect=evil"),
            None
        );
        assert_eq!(allowed_external_url("https://example.com"), None);
        assert_eq!(
            allowed_external_url("https://www.themoviedb.org"),
            Some("https://www.themoviedb.org")
        );
        assert_eq!(
            allowed_external_url("https://www.themoviedb.org.evil.example"),
            None
        );
    }

    #[test]
    fn bangumi_binding_payload_has_bounded_text_fields() {
        let mut subject = BangumiSubject {
            subject_id: 1,
            title: "Title".into(),
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
        assert!(validate_bangumi_subject_payload(&subject).is_ok());
        assert!(validate_bindable_bangumi_subject(&subject).is_ok());
        subject.subject_type = bangumi::SUBJECT_TYPE_LIVE_ACTION;
        assert!(validate_bindable_bangumi_subject(&subject).is_ok());
        subject.subject_type = 4;
        assert!(validate_bindable_bangumi_subject(&subject).is_err());
        subject.subject_type = bangumi::SUBJECT_TYPE_ANIME;
        subject.title = "x".repeat(MAX_BINDING_TITLE_CHARS + 1);
        assert!(validate_bangumi_subject_payload(&subject).is_err());
        subject.title = "Title".into();
        subject.image_url = Some("x".repeat(MAX_BINDING_URL_CHARS + 1));
        assert!(validate_bangumi_subject_payload(&subject).is_err());
    }

    #[test]
    fn settings_paths_are_bounded_before_filesystem_validation_or_persistence() {
        let mut settings = AppSettings {
            comic_reader: Default::default(),
            mpv_path: Some("C:\\Player\\player.exe".into()),
            player_mode: crate::models::PlayerMode::Custom,
            default_view_mode: crate::models::ViewMode::Grid,
            video_extensions: vec!["mkv".into()],
            bangumi_search_enabled: true,
            cover_cache_directory: "C:\\M2Shelf\\covers".into(),
            language: "zh-CN".into(),
            theme: "system".into(),
            auto_check_updates: true,
            auto_scan_on_startup: true,
            all_resources_flattened: false,
        };
        assert!(settings.validate_path_lengths().is_ok());
        settings.mpv_path = Some("x".repeat(crate::models::MAX_SETTINGS_PATH_CHARS + 1));
        assert!(settings.validate_path_lengths().is_err());
        settings.mpv_path = None;
        settings.cover_cache_directory = "x".repeat(crate::models::MAX_SETTINGS_PATH_CHARS + 1);
        assert!(settings.validate_path_lengths().is_err());
    }

    #[test]
    fn architecture_label_matches_current_compilation_target() {
        assert!(!display_architecture().is_empty());
        if cfg!(target_arch = "x86_64") {
            assert_eq!(display_architecture(), "x64");
        }
    }

    #[test]
    fn batch_mutation_result_has_a_stable_camel_case_ipc_shape() {
        let value = serde_json::to_value(BatchMutationResult {
            requested: 3,
            updated: 2,
            skipped: 1,
        })
        .unwrap();
        assert_eq!(value["requested"], 3);
        assert_eq!(value["updated"], 2);
        assert_eq!(value["skipped"], 1);
        assert_eq!(value.as_object().unwrap().len(), 3);
    }

    #[test]
    fn favorite_folder_has_a_stable_camel_case_ipc_shape() {
        let value = serde_json::to_value(FavoriteFolder {
            id: 7,
            name: "Favorites".into(),
            item_count: 3,
            created_at: "2026-08-22T10:00:00.000Z".into(),
            updated_at: "2026-08-22T11:00:00.000Z".into(),
        })
        .unwrap();
        assert_eq!(value["id"], 7);
        assert_eq!(value["name"], "Favorites");
        assert_eq!(value["itemCount"], 3);
        assert_eq!(value["createdAt"], "2026-08-22T10:00:00.000Z");
        assert_eq!(value["updatedAt"], "2026-08-22T11:00:00.000Z");
        assert_eq!(value.as_object().unwrap().len(), 5);
    }

    #[test]
    fn failed_player_launch_never_creates_watch_history() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        std::fs::create_dir_all(&root_path).unwrap();
        let database = Database::new(temp.path().join("play.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO nodes(
                    library_root_id,absolute_path,folder_name,display_name,node_type,total_video_count
                 ) VALUES(?1,?2,'Show','Show','WORK',1)",
                params![root.id, root_path.join("Show").to_string_lossy()],
            )
            .unwrap();
        let node_id = connection.last_insert_rowid();
        drop(connection);
        let media = MediaFile {
            id: 1,
            node_id,
            absolute_path: root_path
                .join("Show")
                .join("[01] episode.mkv")
                .to_string_lossy()
                .into_owned(),
            file_name: "[01] episode.mkv".into(),
            extension: "mkv".into(),
            file_size: 1,
            modified_at: "2026-08-22T00:00:00Z".into(),
            duration_ms: None,
            width: None,
            height: None,
            codec: None,
            last_seen_at: "2026-08-22T00:00:00Z".into(),
        };

        let error =
            launch_media_and_record_watch(&database, &media, Path::new("player.exe"), |_, _| {
                Err("spawn failed".into())
            })
            .unwrap_err();
        assert_eq!(error, "spawn failed");
        assert!(database.list_recently_watched().unwrap().is_empty());

        launch_media_and_record_watch(&database, &media, Path::new("player.exe"), |_, _| Ok(()))
            .unwrap();
        assert_eq!(
            database.list_recently_watched().unwrap()[0].node.id,
            node_id
        );
    }
}

#[tauri::command]
pub fn get_text_reader_settings(
    state: State<'_, AppState>,
) -> AppResult<crate::text_reader_settings::TextReaderSettings> {
    crate::text_reader_settings::get(&state.database)
}
#[tauri::command]
pub fn update_text_reader_settings(
    settings: crate::text_reader_settings::TextReaderSettings,
    state: State<'_, AppState>,
) -> AppResult<crate::text_reader_settings::TextReaderSettings> {
    crate::text_reader_settings::save(&state.database, settings)
}

#[tauri::command]
pub async fn remove_recent_activity(
    state: State<'_, AppState>,
    targets: Vec<crate::collection::Target>,
) -> AppResult<()> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || crate::collection::remove_recent(&db, &targets))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn clear_recent_activity(state: State<'_, AppState>) -> AppResult<()> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || crate::collection::clear_recent(&db))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn collection_statuses(
    state: State<'_, AppState>,
) -> AppResult<crate::collection::StatusSnapshot> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || crate::collection::statuses(&db))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn set_collection_status(
    state: State<'_, AppState>,
    target: crate::collection::Target,
    status: Option<crate::collection::Status>,
) -> AppResult<()> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || crate::collection::set_status(&db, target, status))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn export_collection_backup(app: AppHandle, destination: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _lifecycle = lock_scan_lifecycle(&state);
        ensure_no_active_scan(&state)?;
        if state.update_exit_in_progress.load(Ordering::SeqCst) {
            return Err("BACKUP_BUSY".into());
        }
        let data = app.path().app_data_dir().map_err(|_| "BACKUP_IO")?;
        crate::collection_backup::export(&state.database, &data, Path::new(&destination))
    })
    .await
    .map_err(|_| "BACKUP_IO")?
}
#[tauri::command]
pub async fn prepare_collection_restore(
    app: AppHandle,
    source: String,
) -> AppResult<crate::collection_backup::Preview> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _lifecycle = lock_scan_lifecycle(&state);
        ensure_no_active_scan(&state)?;
        if state.update_exit_in_progress.load(Ordering::SeqCst) {
            return Err("BACKUP_BUSY".into());
        }
        let data = app.path().app_data_dir().map_err(|_| "BACKUP_IO")?;
        crate::collection_backup::prepare(&data, Path::new(&source))
    })
    .await
    .map_err(|_| "BACKUP_IO")?
}
#[tauri::command]
pub async fn discard_collection_restore(app: AppHandle, token: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::collection_backup::discard(
            &app.path().app_data_dir().map_err(|_| "BACKUP_IO")?,
            &token,
        )
    })
    .await
    .map_err(|_| "BACKUP_IO")?
}
#[tauri::command]
pub async fn apply_collection_restore(app: AppHandle, token: String) -> AppResult<()> {
    let handle = app.clone();
    let restart_token = token.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let _lifecycle = lock_scan_lifecycle(&state);
        ensure_no_active_scan(&state)?;
        if state.update_exit_in_progress.load(Ordering::SeqCst) {
            return Err("BACKUP_BUSY".into());
        }
        crate::collection_backup::arm(
            &handle.path().app_data_dir().map_err(|_| "BACKUP_IO")?,
            &token,
            &state.database,
        )
    })
    .await
    .map_err(|_| "BACKUP_IO")??;
    if let Err(error) = crate::single_instance::spawn_collection_restart() {
        crate::collection_backup::disarm(
            &app.path().app_data_dir().map_err(|_| "BACKUP_IO")?,
            &restart_token,
        )?;
        return Err(error);
    }
    app.state::<AppState>()
        .update_exit_in_progress
        .store(true, Ordering::SeqCst);
    app.exit(0);
    Ok(())
}
#[tauri::command]
pub fn collection_restore_notice(state: State<'_, AppState>) -> Option<String> {
    state
        .collection_restore_notice
        .lock()
        .ok()
        .and_then(|mut notice| notice.take())
}

#[tauri::command]
pub async fn get_private_note(
    target: crate::collection::Target,
    state: State<'_, AppState>,
) -> AppResult<crate::notes::Note> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || crate::notes::get(&db, target))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn save_private_note(
    target: crate::collection::Target,
    body: String,
    revision: i64,
    state: State<'_, AppState>,
) -> AppResult<crate::notes::Note> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || crate::notes::save(&db, target, body, revision))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn validate_library_path(path: String, state: State<'_, AppState>) -> AppResult<String> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::library_paths::validate(&db, Path::new(&path))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn begin_cover_selection(
    target: crate::collection::Target,
    state: State<'_, AppState>,
) -> AppResult<String> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || crate::cover_selection::begin(&db, target))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn cover_selection_candidates(
    token: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::cover_selection::Candidate>> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || crate::cover_selection::online(&db, &token))
        .await
        .map_err(|e| e.to_string())?
}
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn apply_cover_selection(
    token: String,
    local: Option<String>,
    index: Option<usize>,
    page: Option<i64>,
    raster: Option<String>,
    clear: bool,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let db = state.database.clone();
    let root = active_cover_cache_directory(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::cover_selection::apply(
            &db,
            &root,
            &token,
            local.as_deref().map(Path::new),
            index,
            page,
            raster.as_deref(),
            clear,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn discard_cover_selection(token: String) {
    crate::cover_selection::discard(&token);
}

#[tauri::command]
pub async fn preview_library_relocation(
    root_id: i64,
    path: String,
    state: State<'_, AppState>,
) -> AppResult<crate::library_relocation::Preview> {
    ensure_no_active_scan(&state)?;
    let db = state.database.clone();
    let cache = active_cover_cache_directory(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        cache::validate_cache_location(&cache, [PathBuf::from(&path)])?;
        crate::library_relocation::preview(&db, root_id, Path::new(&path))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn apply_library_relocation(token: String, app: AppHandle) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _lifecycle = state.scan_lifecycle.lock().map_err(|_| "RELOCATE_BUSY")?;
        ensure_no_active_scan(&state)?;
        crate::poster_cache::cancel_warmup();
        let result = {
            let _covers = cache::begin_cover_cache_clear();
            crate::library_relocation::apply(&state.database, &token)
        };
        crate::poster_resource::clear_memory();
        if let Ok(root) = active_cover_cache_directory(&state) {
            crate::poster_cache::schedule_warmup(&state.database, &root);
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn discard_library_relocation(token: String) {
    crate::library_relocation::discard(&token);
}

#[tauri::command]
pub async fn preview_embedded_cover(
    token: String,
    state: State<'_, AppState>,
) -> AppResult<Option<String>> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::cover_selection::embedded_preview(&db, &token)
    })
    .await
    .map_err(|e| e.to_string())?
}

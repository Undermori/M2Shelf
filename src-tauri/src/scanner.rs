use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use tauri::{AppHandle, Emitter};

use crate::{
    auto_match,
    db::{AppResult, Database},
    models::{
        LibraryRecognitionMode, LibraryRoot, NodeType, ResourceType, ScanPhase, ScanProgress,
        ScanStatus,
    },
};

#[derive(Debug, Clone)]
pub struct ScanTarget {
    pub root: LibraryRoot,
    pub path: PathBuf,
    pub parent_node_id: Option<i64>,
}

#[derive(Clone)]
pub struct ScanControl {
    pub unchanged_directories: Arc<Mutex<HashSet<PathBuf>>>,
    pub scan_id: String,
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<Mutex<ScanProgress>>,
}

impl ScanControl {
    pub fn progress(&self) -> ScanProgress {
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ChildMediaSummary {
    pub branch_count: i64,
    pub supplementary_branch_count: i64,
    pub total_videos: i64,
}

#[derive(Debug)]
pub(crate) enum ScanAbort {
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScanEntryPath {
    /// Path kept in SQLite and shown to the user. This preserves the spelling of the configured
    /// Library Root instead of leaking Windows extended-length canonical path prefixes.
    logical: PathBuf,
    /// Path used for filesystem access. It is canonicalized and checked against the canonical
    /// Library Root immediately before a directory or file is read.
    filesystem: PathBuf,
}

impl From<String> for ScanAbort {
    fn from(value: String) -> Self {
        Self::Failed(value)
    }
}

#[cfg(test)]
pub fn run_scan(
    app: Option<&AppHandle>,
    database: &Database,
    targets: Vec<ScanTarget>,
    control: &ScanControl,
    extensions: &[String],
) {
    run_scan_with_auto_match(app, database, targets, control, extensions, None);
}

pub fn run_scan_with_auto_match(
    app: Option<&AppHandle>,
    database: &Database,
    targets: Vec<ScanTarget>,
    control: &ScanControl,
    extensions: &[String],
    auto_match_cache_root: Option<Result<PathBuf, String>>,
) {
    let extension_set = extensions
        .iter()
        .map(|value| value.trim_start_matches('.').to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let token = format!(
        "{}:{}",
        Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true),
        control.scan_id
    );
    let background = control.progress().background;
    let mut plan = None;
    let mut scan_targets = targets.clone();
    let mut root_results: HashMap<i64, (String, u64, Option<String>)> = HashMap::new();
    let mut completed_roots = HashSet::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !background {
            // A manual/rebuild scan can repair or partially replace the index. Its next startup
            // must establish a new baseline, including after interruption.
            let connection = database.connect()?;
            for target in &targets {
                connection
                    .execute(
                        "DELETE FROM library_scan_snapshots WHERE library_root_id=?1",
                        [target.root.id],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        if background {
            let next = crate::incremental::prepare(database, &targets, control, &extension_set)?;
            scan_targets = next.targets.clone();
            *control
                .unchanged_directories
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = next.unchanged.clone();
            control
                .progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .library_changed = Some(!scan_targets.is_empty());
            for (id, (partial, error)) in &next.failed_roots {
                root_results.insert(
                    *id,
                    (
                        if *partial { "PARTIAL" } else { "FAILED" }.into(),
                        1,
                        Some(error.clone()),
                    ),
                );
            }
            plan = Some(next);
        }
        let mut root_ids = targets
            .iter()
            .map(|target| target.root.id)
            .collect::<Vec<_>>();
        root_ids.sort_unstable();
        root_ids.dedup();
        for root_id in root_ids {
            if root_results.contains_key(&root_id) {
                completed_roots.insert(root_id);
                continue;
            }
            check_cancel(control)?;
            let scoped = scan_targets
                .iter()
                .filter(|target| target.root.id == root_id)
                .cloned()
                .collect::<Vec<_>>();
            let before = control.progress().errors;
            match run_scan_inner(app, database, &scoped, control, &extension_set, &token) {
                Ok(()) => {
                    let progress = control.progress();
                    let errors = progress.errors - before;
                    root_results.insert(
                        root_id,
                        (
                            if errors > 0 { "PARTIAL" } else { "SUCCESS" }.into(),
                            errors,
                            if errors > 0 { progress.message } else { None },
                        ),
                    );
                }
                Err(ScanAbort::Cancelled) => return Err(ScanAbort::Cancelled),
                Err(ScanAbort::Failed(error)) => {
                    let mut progress = control.progress.lock().unwrap_or_else(|e| e.into_inner());
                    progress.errors += 1;
                    progress.message = Some(error.clone());
                    root_results.insert(
                        root_id,
                        ("FAILED".into(), progress.errors - before, Some(error)),
                    );
                }
            }
            completed_roots.insert(root_id);
        }
        scan_targets.retain(|target| {
            root_results.get(&target.root.id).is_some_and(|result| {
                scan_outcome_allows_matching(&result.0, target.root.media_kind)
            })
        });
        Ok(())
    }))
    .unwrap_or_else(|_| Err(ScanAbort::Failed("扫描线程发生内部错误。".into())));

    let auto_match_report =
        if result.is_ok() && !scan_targets.is_empty() && !control.cancel.load(Ordering::Relaxed) {
            auto_match_cache_root.as_ref().map(|cache_root| {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    auto_match::run_auto_match(
                        database,
                        &scan_targets,
                        &control
                            .unchanged_directories
                            .lock()
                            .unwrap_or_else(|e| e.into_inner()),
                        cache_root
                            .as_ref()
                            .map(PathBuf::as_path)
                            .map_err(String::as_str),
                        |current, total, node, report| {
                            update_auto_match_progress(app, control, current, total, node, report)
                        },
                        || control.cancel.load(Ordering::Relaxed),
                    )
                }))
                .unwrap_or(auto_match::AutoMatchReport {
                    errors: 1,
                    ..auto_match::AutoMatchReport::default()
                })
            })
        } else {
            None
        };

    {
        if let Some(mut plan) = plan {
            let failed = targets
                .iter()
                .filter_map(|target| {
                    root_results
                        .get(&target.root.id)
                        .is_none_or(|result| result.0 != "SUCCESS")
                        .then_some(target.root.id)
                })
                .collect();
            plan.discard_failed_snapshots(&failed);
            if let Err(error) = plan.save(database) {
                let mut progress = control.progress.lock().unwrap_or_else(|e| e.into_inner());
                progress.errors += 1;
                progress.message = Some(error.clone());
                for result in root_results
                    .values_mut()
                    .filter(|result| result.0 == "SUCCESS")
                {
                    *result = ("FAILED".into(), 1, Some(error.clone()));
                }
            }
        }
    }
    let mut progress = control
        .progress
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(report) = auto_match_report {
        if report.examined > 0 || report.errors > 0 {
            progress.phase = ScanPhase::AutoMatching;
        }
        progress.auto_match_matched = report.matched as u64;
        progress.auto_match_pending = 0;
        progress.auto_match_unmatched = report.unmatched as u64;
        progress.auto_match_errors = report.errors as u64;
    }
    match result {
        Ok(()) if control.cancel.load(Ordering::Relaxed) => {
            progress.status = ScanStatus::Cancelled;
            progress.message = Some("扫描已停止；已完成的索引结果已保留。".into());
        }
        Ok(()) => {
            progress.status = if !root_results.is_empty()
                && root_results.values().all(|result| result.0 == "FAILED")
            {
                ScanStatus::Failed
            } else {
                ScanStatus::Completed
            };
            progress.message = if matches!(progress.status, ScanStatus::Failed) {
                root_results.values().find_map(|result| result.2.clone())
            } else {
                Some(match auto_match_report {
                    Some(report) if report.examined > 0 || report.errors > 0 => format!(
                        "扫描完成；自动匹配 {} 项，未匹配 {} 项，{} 项稍后重试。",
                        report.matched, report.unmatched, report.errors
                    ),
                    _ => "扫描完成。".into(),
                })
            };
        }
        Err(ScanAbort::Cancelled) => {
            progress.status = ScanStatus::Cancelled;
            progress.message = Some("扫描已停止；已完成的索引结果已保留。".into());
        }
        Err(ScanAbort::Failed(_)) if control.cancel.load(Ordering::Relaxed) => {
            progress.status = ScanStatus::Cancelled;
        }
        Err(ScanAbort::Failed(message)) => {
            progress.status = ScanStatus::Failed;
            progress.message = Some(message);
            progress.errors += 1;
        }
    }
    let final_progress = progress.clone();
    drop(progress);
    for target in &targets {
        let mut root_progress = final_progress.clone();
        root_progress.root_id = target.root.id;
        let result = root_results.get(&target.root.id);
        root_progress.errors = result.map_or(0, |result| result.1);
        if !completed_roots.contains(&target.root.id) {
            root_progress.status = final_progress.status;
        } else if result.is_some_and(|result| result.0 == "FAILED") {
            root_progress.status = ScanStatus::Failed;
        }
        let _ = database.finish_scan_run(&root_progress);
        if background
            || (target.parent_node_id.is_none() && target.path == Path::new(&target.root.path))
        {
            let cancelled = matches!(final_progress.status, ScanStatus::Cancelled)
                && !completed_roots.contains(&target.root.id);
            let outcome = if cancelled {
                "CANCELLED"
            } else {
                result.map_or("FAILED", |result| result.0.as_str())
            };
            let detail = result.and_then(|result| result.2.as_deref());
            let _ = database.record_scan_health(
                target.root.id,
                outcome,
                root_progress.errors,
                detail,
                background,
            );
        }
    }
    if let Some(app) = app {
        let _ = app.emit("scan-progress", &final_progress);
        let _ = app.emit("scan-completed", &final_progress);
    }
}

// Partial comic scans retain valid indexed books. An unrelated unreadable archive must not
// suppress their matching; failed/cancelled Roots and snapshot baselines remain protected.
fn scan_outcome_allows_matching(outcome: &str, kind: crate::models::LibraryMediaKind) -> bool {
    outcome == "SUCCESS" || (outcome == "PARTIAL" && kind.is_book())
}

/// Matches Nodes already indexed in SQLite without walking or mutating the media filesystem.
/// Reusing `ScanControl` keeps long online runs visible, cancellable, and compatible with the
/// existing progress/completion event flow.
pub fn run_existing_content_match(
    app: Option<&AppHandle>,
    database: &Database,
    nodes: Vec<crate::models::MediaNode>,
    control: &ScanControl,
    cache_root: Result<PathBuf, String>,
    write_mode: auto_match::MatchWriteMode,
) {
    let report = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        auto_match::run_match_nodes(
            database,
            &nodes,
            cache_root
                .as_ref()
                .map(PathBuf::as_path)
                .map_err(String::as_str),
            write_mode,
            |current, total, node, report| {
                update_auto_match_progress(app, control, current, total, node, report)
            },
            || control.cancel.load(Ordering::Relaxed),
        )
    }))
    .unwrap_or(auto_match::AutoMatchReport {
        errors: 1,
        ..auto_match::AutoMatchReport::default()
    });

    let mut progress = control
        .progress
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    progress.phase = ScanPhase::AutoMatching;
    progress.auto_match_current = report.examined as u64;
    progress.auto_match_total = nodes.len() as u64;
    progress.auto_match_matched = report.matched as u64;
    progress.auto_match_pending = 0;
    progress.auto_match_unmatched = report.unmatched as u64;
    progress.auto_match_errors = report.errors as u64;
    if control.cancel.load(Ordering::Relaxed) {
        progress.status = ScanStatus::Cancelled;
        progress.message = Some("现有资源匹配已停止；已完成的绑定保留。".into());
    } else {
        progress.status = ScanStatus::Completed;
        progress.message = Some(format!(
            "现有资源匹配完成；自动匹配 {} 项，未匹配 {} 项，{} 项稍后重试。",
            report.matched, report.unmatched, report.errors
        ));
    }
    let final_progress = progress.clone();
    drop(progress);
    if let Some(app) = app {
        let _ = app.emit("scan-progress", &final_progress);
        let _ = app.emit("scan-completed", &final_progress);
    }
}

fn update_auto_match_progress(
    app: Option<&AppHandle>,
    control: &ScanControl,
    current: usize,
    total: usize,
    node: &crate::models::MediaNode,
    report: auto_match::AutoMatchReport,
) {
    let snapshot = {
        let mut progress = control
            .progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        progress.current_path = node.absolute_path.clone();
        progress.phase = ScanPhase::AutoMatching;
        progress.auto_match_current = current as u64;
        progress.auto_match_total = total as u64;
        progress.auto_match_matched = report.matched as u64;
        progress.auto_match_pending = 0;
        progress.auto_match_unmatched = report.unmatched as u64;
        progress.auto_match_errors = report.errors as u64;
        progress.message = Some(format!("正在自动匹配封面与标题（{current}/{total}）…"));
        progress.clone()
    };
    if let Some(app) = app.filter(|_| !snapshot.background) {
        let _ = app.emit("scan-progress", snapshot);
    }
}

/// Resolves a path before it is read and applies a component-aware Library Root boundary.
/// `Path::starts_with` compares path components, so a sibling such as `Media-Backup` cannot be
/// mistaken for a child of `Media`. Canonicalization also resolves junctions and symlinks before
/// the boundary decision is made.
pub(crate) fn canonicalize_within_library_root(
    path: &Path,
    canonical_root: &Path,
) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        format!(
            "无法确认扫描路径 {} 的实际位置，已跳过：{error}",
            path.display()
        )
    })?;
    if !canonical.starts_with(canonical_root) {
        return Err(format!(
            "扫描路径超出资源库根目录，已跳过：{}",
            path.display()
        ));
    }
    Ok(canonical)
}

fn canonical_file_within_library_root(
    path: &Path,
    canonical_root: &Path,
) -> Result<PathBuf, String> {
    let canonical = canonicalize_within_library_root(path, canonical_root)?;
    if !canonical.is_file() {
        return Err(format!("扫描路径不是文件，已跳过：{}", path.display()));
    }
    Ok(canonical)
}

fn run_scan_inner(
    app: Option<&AppHandle>,
    database: &Database,
    targets: &[ScanTarget],
    control: &ScanControl,
    extensions: &HashSet<String>,
    token: &str,
) -> Result<(), ScanAbort> {
    let connection = database.connect()?;
    let mut visited = HashSet::new();
    for target in targets {
        check_cancel(control)?;
        let configured_root = Path::new(&target.root.path);
        // Revalidate the registered root immediately before each scan. Besides resolving links
        // and junctions, this blocks legacy overlapping-root rows from moving a Node between root
        // owners and cascading application metadata when either root is later removed.
        let canonical_root = database.validate_scan_root(&target.root).map_err(|error| {
            ScanAbort::Failed(format!(
                "资源库根目录校验失败 {}：{error}",
                configured_root.display()
            ))
        })?;
        let canonical_target = canonicalize_within_library_root(&target.path, &canonical_root)
            .map_err(|message| ScanAbort::Failed(format!("拒绝扫描目标：{message}")))?;
        if !canonical_target.is_dir() {
            return Err(ScanAbort::Failed(format!(
                "扫描目标不是目录，已拒绝：{}",
                target.path.display()
            )));
        }
        {
            let mut progress = control
                .progress
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            progress.root_id = target.root.id;
            progress.current_path = target.path.to_string_lossy().into_owned();
        }
        if target.root.media_kind.is_book() {
            crate::comics::scan_library(app, &connection, target, &canonical_root, control, token)?;
            continue;
        }
        if matches!(
            target.root.recognition_mode,
            LibraryRecognitionMode::VideoFile
        ) {
            scan_video_file_library(
                app,
                &connection,
                &target.root,
                &canonical_root,
                control,
                extensions,
                token,
            )?;
        } else {
            scan_directory(
                app,
                &connection,
                target.root.id,
                &target.path,
                &canonical_target,
                &canonical_root,
                target.parent_node_id,
                control,
                extensions,
                token,
                &mut visited,
            )?;
        }
        crate::logical_works::LogicalWorkIndex::reclassify(&connection, Some(target.root.id))?;
        if target.parent_node_id.is_some()
            && matches!(target.root.recognition_mode, LibraryRecognitionMode::Folder)
        {
            let scanned_node_id = connection
                .query_row(
                    "SELECT id FROM nodes WHERE library_root_id=?1 AND absolute_path=?2 COLLATE NOCASE",
                    params![target.root.id, target.path.to_string_lossy()],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|error| error.to_string())?;
            refresh_ancestors(&connection, Some(scanned_node_id), &canonical_root)?;
            crate::logical_works::LogicalWorkIndex::reclassify(&connection, Some(target.root.id))?;
        }
    }
    Ok(())
}

/// File recognition deliberately flattens videos beneath the hidden Library Root node. Each
/// source video therefore becomes one stable card and one independent metadata binding while the
/// source tree remains untouched. Folder recognition continues through `scan_directory` above.
fn scan_video_file_library(
    app: Option<&AppHandle>,
    connection: &Connection,
    root: &LibraryRoot,
    canonical_root: &Path,
    control: &ScanControl,
    extensions: &HashSet<String>,
    token: &str,
) -> Result<(), ScanAbort> {
    let logical_root = PathBuf::from(&root.path);
    let root_name = logical_root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(&root.display_name);
    let hidden_root_id = upsert_node(
        connection,
        root.id,
        None,
        &logical_root.to_string_lossy(),
        root_name,
        token,
    )?;

    let errors_before = control.progress().errors;
    let mut pending = vec![ScanEntryPath {
        logical: logical_root.clone(),
        filesystem: canonical_root.to_path_buf(),
    }];
    let mut visited = HashSet::new();
    let mut work_count = 0_i64;
    let mut video_count = 0_i64;
    let unchanged = control
        .unchanged_directories
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if !unchanged.is_empty() {
        // Flat Nodes share a parent instead of mirroring source directories. Mark the preserved
        // rows before root-scoped cleanup, without rereading their files or altering curation.
        let mut statement = connection
            .prepare("SELECT id,absolute_path,total_video_count FROM nodes WHERE parent_node_id=?1")
            .map_err(|e| e.to_string())?;
        let nodes = statement
            .query_map([hidden_root_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (id, path, count) in nodes {
            check_cancel(control)?;
            if Path::new(&path)
                .ancestors()
                .any(|directory| unchanged.contains(directory))
            {
                connection
                    .execute(
                        "UPDATE nodes SET last_seen_at=?1 WHERE id=?2",
                        params![token, id],
                    )
                    .map_err(|e| e.to_string())?;
                work_count += 1;
                video_count += count;
            }
        }
        let mut statement = connection
            .prepare("SELECT id,absolute_path FROM resource_files WHERE node_id=?1")
            .map_err(|e| e.to_string())?;
        let resources = statement
            .query_map([hidden_root_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (id, path) in resources {
            check_cancel(control)?;
            if Path::new(&path)
                .ancestors()
                .any(|directory| unchanged.contains(directory))
            {
                connection
                    .execute(
                        "UPDATE resource_files SET last_seen_at=?1 WHERE id=?2",
                        params![token, id],
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    while let Some(directory) = pending.pop() {
        check_cancel(control)?;
        let canonical_directory =
            match canonicalize_within_library_root(&directory.filesystem, canonical_root) {
                Ok(path) if path.is_dir() => path,
                Ok(_) => {
                    update_progress(app, control, &directory.logical, |progress| {
                        progress.errors += 1;
                        progress.message = Some(format!(
                            "扫描路径不是目录，已跳过：{}",
                            directory.logical.display()
                        ));
                    });
                    continue;
                }
                Err(message) => {
                    update_progress(app, control, &directory.logical, |progress| {
                        progress.errors += 1;
                        progress.message = Some(message);
                    });
                    continue;
                }
            };
        if !visited.insert(canonical_directory.clone()) {
            continue;
        }
        if unchanged.contains(&directory.logical) {
            continue;
        }
        update_progress(app, control, &directory.logical, |progress| {
            progress.folders_scanned += 1
        });
        let entries = match fs::read_dir(&canonical_directory) {
            Ok(entries) => entries,
            Err(error) => {
                update_progress(app, control, &directory.logical, |progress| {
                    progress.errors += 1;
                    progress.message =
                        Some(format!("无法读取 {}：{error}", directory.logical.display()));
                });
                continue;
            }
        };

        let mut bdmv = None;
        for entry_result in entries {
            check_cancel(control)?;
            let entry = match entry_result {
                Ok(entry) => entry,
                Err(error) => {
                    update_progress(app, control, &directory.logical, |progress| {
                        progress.errors += 1;
                        progress.message = Some(format!("读取目录项失败：{error}"));
                    });
                    continue;
                }
            };
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(error) => {
                    update_progress(app, control, &directory.logical, |progress| {
                        progress.errors += 1;
                        progress.message = Some(format!("读取文件类型失败：{error}"));
                    });
                    continue;
                }
            };
            if file_type.is_symlink() {
                continue;
            }
            let logical = directory.logical.join(entry.file_name());
            let filesystem = entry.path();
            if file_type.is_dir() {
                let child = ScanEntryPath {
                    logical,
                    filesystem,
                };
                if file_name_eq(&child.logical, "BDMV") {
                    bdmv = Some(child);
                } else {
                    pending.push(child);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let canonical_file =
                match canonical_file_within_library_root(&filesystem, canonical_root) {
                    Ok(path) => path,
                    Err(message) => {
                        update_progress(app, control, &logical, |progress| {
                            progress.errors += 1;
                            progress.message = Some(message);
                        });
                        continue;
                    }
                };
            if is_video(&logical, extensions) {
                let title = logical
                    .file_stem()
                    .map(|value| value.to_string_lossy().into_owned())
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| logical.to_string_lossy().into_owned());
                let node_id = upsert_node(
                    connection,
                    root.id,
                    Some(hidden_root_id),
                    &logical.to_string_lossy(),
                    &title,
                    token,
                )?;
                if let Err(error) =
                    index_media_file(connection, node_id, &logical, &canonical_file, token)
                {
                    update_progress(app, control, &logical, |progress| {
                        progress.errors += 1;
                        progress.message = Some(error);
                    });
                    continue;
                }
                connection
                    .execute(
                        "UPDATE nodes SET
                            node_type=CASE WHEN manual_type_override=1 THEN node_type ELSE 'AUTO_WORK' END,
                            direct_video_count=1,child_media_branch_count=0,total_video_count=1,
                            last_seen_at=?1,updated_at=CURRENT_TIMESTAMP
                         WHERE id=?2",
                        params![token, node_id],
                    )
                    .map_err(|error| error.to_string())?;
                work_count += 1;
                video_count += 1;
                update_progress(app, control, &logical, |progress| {
                    progress.videos_found += 1
                });
            } else if let Err(error) =
                index_resource_file(connection, hidden_root_id, &logical, &canonical_file, token)
            {
                update_progress(app, control, &logical, |progress| {
                    progress.errors += 1;
                    progress.message = Some(error);
                });
            }
        }

        if let Some(bdmv) = bdmv {
            if unchanged.contains(&bdmv.logical) {
                continue;
            }
            match index_flat_bdmv_work(
                app,
                connection,
                root.id,
                hidden_root_id,
                &directory.logical,
                &bdmv,
                canonical_root,
                control,
                extensions,
                token,
            )? {
                Some(indexed_videos) => {
                    work_count += 1;
                    video_count += indexed_videos;
                }
                None => pending.push(bdmv),
            }
        }
    }

    let complete = control.progress().errors == errors_before;
    if complete {
        connection
            .execute(
                "DELETE FROM media_files WHERE node_id=?1 AND last_seen_at<>?2",
                params![hidden_root_id, token],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "DELETE FROM resource_files WHERE node_id=?1 AND last_seen_at<>?2",
                params![hidden_root_id, token],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "DELETE FROM nodes WHERE parent_node_id=?1 AND last_seen_at<>?2",
                params![hidden_root_id, token],
            )
            .map_err(|error| error.to_string())?;
    }
    connection
        .execute(
            "UPDATE nodes SET
                node_type=CASE WHEN manual_type_override=1 THEN node_type ELSE 'CONTAINER' END,
                direct_video_count=0,child_media_branch_count=?1,total_video_count=?2,
                last_seen_at=?3,updated_at=CURRENT_TIMESTAMP
             WHERE id=?4",
            params![work_count, video_count, token, hidden_root_id],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn index_flat_bdmv_work(
    app: Option<&AppHandle>,
    connection: &Connection,
    root_id: i64,
    hidden_root_id: i64,
    containing_directory: &Path,
    bdmv: &ScanEntryPath,
    canonical_root: &Path,
    control: &ScanControl,
    extensions: &HashSet<String>,
    token: &str,
) -> Result<Option<i64>, ScanAbort> {
    let Some(streams) = bdmv_stream_entries(bdmv, canonical_root).map_err(ScanAbort::Failed)?
    else {
        return Ok(None);
    };
    let title = containing_directory
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| containing_directory.to_string_lossy().into_owned());
    let node_id = upsert_node(
        connection,
        root_id,
        Some(hidden_root_id),
        &bdmv.logical.to_string_lossy(),
        &title,
        token,
    )?;
    let mut indexed = 0_i64;
    for stream in streams {
        check_cancel(control)?;
        let canonical_file = canonical_file_within_library_root(&stream.filesystem, canonical_root)
            .map_err(ScanAbort::Failed)?;
        index_media_file(connection, node_id, &stream.logical, &canonical_file, token)?;
        indexed += 1;
        update_progress(app, control, &stream.logical, |progress| {
            progress.videos_found += 1
        });
    }
    let resources_complete = index_transparent_bdmv_resources(
        app,
        connection,
        node_id,
        bdmv,
        canonical_root,
        control,
        extensions,
        token,
    )?;
    if resources_complete {
        connection
            .execute(
                "DELETE FROM media_files WHERE node_id=?1 AND last_seen_at<>?2",
                params![node_id, token],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "DELETE FROM resource_files WHERE node_id=?1 AND last_seen_at<>?2",
                params![node_id, token],
            )
            .map_err(|error| error.to_string())?;
    }
    connection
        .execute(
            "UPDATE nodes SET
                node_type=CASE WHEN manual_type_override=1 THEN node_type ELSE 'AUTO_WORK' END,
                direct_video_count=?1,child_media_branch_count=0,total_video_count=?1,
                last_seen_at=?2,updated_at=CURRENT_TIMESTAMP
             WHERE id=?3",
            params![indexed, token, node_id],
        )
        .map_err(|error| error.to_string())?;
    Ok(Some(indexed))
}

fn bdmv_stream_entries(
    bdmv: &ScanEntryPath,
    canonical_root: &Path,
) -> Result<Option<Vec<ScanEntryPath>>, String> {
    let canonical_bdmv = canonicalize_within_library_root(&bdmv.filesystem, canonical_root)?;
    if !canonical_bdmv.is_dir() {
        return Err(format!(
            "BDMV 路径不是目录，已跳过：{}",
            bdmv.logical.display()
        ));
    }
    let bdmv_entries = fs::read_dir(&canonical_bdmv).map_err(|error| {
        format!(
            "无法读取 BDMV 目录 {}，已跳过：{error}",
            bdmv.logical.display()
        )
    })?;
    let mut stream = None;
    for entry_result in bdmv_entries {
        let entry = entry_result
            .map_err(|error| format!("读取 BDMV 目录项失败 {}：{error}", bdmv.logical.display()))?;
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "读取 BDMV 文件类型失败 {}：{error}",
                bdmv.logical.join(entry.file_name()).display()
            )
        })?;
        if file_type.is_symlink() {
            continue;
        }
        let entry_path = entry.path();
        if file_type.is_dir() && file_name_eq(&entry_path, "STREAM") {
            let logical = bdmv.logical.join(entry.file_name());
            let filesystem = canonicalize_within_library_root(&entry_path, canonical_root)?;
            if !filesystem.is_dir() {
                return Err(format!(
                    "BDMV STREAM 路径不是目录，已跳过：{}",
                    logical.display()
                ));
            }
            stream = Some(ScanEntryPath {
                logical,
                filesystem,
            });
            break;
        }
    }
    let Some(stream) = stream else {
        return Ok(None);
    };
    let entries = fs::read_dir(&stream.filesystem).map_err(|error| {
        format!(
            "无法读取 BDMV STREAM 目录 {}，已跳过：{error}",
            stream.logical.display()
        )
    })?;
    let mut videos = Vec::new();
    for entry_result in entries {
        let entry = entry_result.map_err(|error| {
            format!(
                "读取 BDMV STREAM 目录项失败 {}：{error}",
                stream.logical.display()
            )
        })?;
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "读取 BDMV STREAM 文件类型失败 {}：{error}",
                stream.logical.join(entry.file_name()).display()
            )
        })?;
        if file_type.is_symlink() || !file_type.is_file() {
            continue;
        }
        let filesystem = entry.path();
        if has_extension(&filesystem, "m2ts") {
            videos.push(ScanEntryPath {
                logical: stream.logical.join(entry.file_name()),
                filesystem,
            });
        }
    }
    Ok(Some(videos))
}

#[allow(clippy::too_many_arguments)]
fn scan_directory(
    app: Option<&AppHandle>,
    connection: &Connection,
    root_id: i64,
    path: &Path,
    filesystem_path: &Path,
    canonical_root: &Path,
    parent_node_id: Option<i64>,
    control: &ScanControl,
    extensions: &HashSet<String>,
    token: &str,
    visited: &mut HashSet<PathBuf>,
) -> Result<i64, ScanAbort> {
    check_cancel(control)?;
    let canonical = match canonicalize_within_library_root(filesystem_path, canonical_root) {
        Ok(canonical) if canonical.is_dir() => canonical,
        Ok(_) => {
            update_progress(app, control, path, |progress| {
                progress.errors += 1;
                progress.message = Some(format!("扫描路径不是目录，已跳过：{}", path.display()));
            });
            return Ok(0);
        }
        Err(message) => {
            update_progress(app, control, path, |progress| {
                progress.errors += 1;
                progress.message = Some(message);
            });
            return Ok(0);
        }
    };
    if !visited.insert(canonical.clone()) {
        return Ok(0);
    }

    if control
        .unchanged_directories
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(path)
    {
        let existing = connection.query_row(
            "SELECT id,node_type,total_video_count FROM nodes WHERE library_root_id=?1 AND absolute_path=?2",
            params![root_id, path.to_string_lossy()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)),
        ).optional().map_err(|e| e.to_string())?;
        if let Some((id, node_type, count)) = existing {
            // Retain only this branch marker for the parent's stale-row cleanup. Its children,
            // files, classification, timestamps and metadata are not reindexed.
            connection
                .execute(
                    "UPDATE nodes SET last_seen_at=?1 WHERE id=?2",
                    params![token, id],
                )
                .map_err(|e| e.to_string())?;
            return Ok(if node_type == "IGNORED" { 0 } else { count });
        }
    }

    update_progress(app, control, path, |progress| progress.folders_scanned += 1);
    let folder_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_str().unwrap_or("资源库"));
    let absolute_path = path.to_string_lossy().into_owned();
    let node_id = upsert_node(
        connection,
        root_id,
        parent_node_id,
        &absolute_path,
        folder_name,
        token,
    )?;

    let existing = connection
        .query_row(
            "SELECT node_type,manual_type_override,total_video_count,direct_video_count,
             child_media_branch_count FROM nodes WHERE id=?1",
            [node_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, bool>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;
    if existing.1 && existing.0 == "IGNORED" {
        // Keep the branch's index and user metadata so "restore automatic detection" remains
        // reversible. Ignored branches are excluded from their parent's media counts.
        return Ok(0);
    }

    let errors_before_read = control.progress().errors;
    let read_dir = match fs::read_dir(&canonical) {
        Ok(entries) => entries,
        Err(error) => {
            update_progress(app, control, path, |progress| {
                progress.errors += 1;
                progress.message = Some(format!("无法读取 {}：{error}", path.display()));
            });
            return Ok(existing.2.max(0));
        }
    };

    let mut directories = Vec::new();
    let mut video_files = Vec::new();
    let mut resource_files = Vec::new();
    for entry_result in read_dir {
        check_cancel(control)?;
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(error) => {
                update_progress(app, control, path, |progress| {
                    progress.errors += 1;
                    progress.message = Some(format!("读取目录项失败：{error}"));
                });
                continue;
            }
        };
        let entry_path = entry.path();
        let logical_entry_path = path.join(entry.file_name());
        let file_type = match entry.file_type() {
            Ok(value) => value,
            Err(error) => {
                update_progress(app, control, &logical_entry_path, |progress| {
                    progress.errors += 1;
                    progress.message = Some(format!("读取文件类型失败：{error}"));
                });
                continue;
            }
        };
        // Never follow symlinks/junction-like links: this avoids cycles and out-of-root traversal.
        if file_type.is_symlink() {
            continue;
        }
        let scan_path = ScanEntryPath {
            logical: logical_entry_path,
            filesystem: entry_path,
        };
        if file_type.is_dir() {
            directories.push(scan_path);
        } else if file_type.is_file() {
            if is_video(&scan_path.logical, extensions) {
                video_files.push(scan_path);
            } else {
                resource_files.push(scan_path);
            }
        }
    }
    let entries_complete = control.progress().errors == errors_before_read;
    let mut files_complete = entries_complete;

    let bdmv_directory = directories
        .iter()
        .find(|child| file_name_eq(&child.logical, "BDMV"))
        .cloned();
    let has_bdmv = if let Some(bdmv) = &bdmv_directory {
        match bdmv_stream_entries(bdmv, canonical_root) {
            Ok(Some(mut stream_videos)) => {
                video_files.append(&mut stream_videos);
                true
            }
            Ok(None) => false,
            Err(message) => {
                files_complete = false;
                update_progress(app, control, &bdmv.logical, |progress| {
                    progress.errors += 1;
                    progress.message = Some(message);
                });
                false
            }
        }
    } else {
        false
    };
    if let Some(bdmv) = &bdmv_directory {
        directories.retain(|child| child != bdmv);
    }

    video_files.sort_by(|a, b| {
        crate::db::natural_cmp(
            a.logical
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default(),
            b.logical
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default(),
        )
    });
    for video in &video_files {
        check_cancel(control)?;
        let canonical_file =
            match canonical_file_within_library_root(&video.filesystem, canonical_root) {
                Ok(path) => path,
                Err(message) => {
                    files_complete = false;
                    update_progress(app, control, &video.logical, |progress| {
                        progress.errors += 1;
                        progress.message = Some(message);
                    });
                    continue;
                }
            };
        match index_media_file(connection, node_id, &video.logical, &canonical_file, token) {
            Ok(()) => update_progress(app, control, &video.logical, |progress| {
                progress.videos_found += 1
            }),
            Err(error) => {
                files_complete = false;
                update_progress(app, control, &video.logical, |progress| {
                    progress.errors += 1;
                    progress.message = Some(error);
                });
            }
        }
    }

    resource_files.sort_by(|left, right| {
        crate::db::natural_cmp(
            left.logical
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
            right
                .logical
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
        )
    });
    for resource in &resource_files {
        check_cancel(control)?;
        let canonical_file =
            match canonical_file_within_library_root(&resource.filesystem, canonical_root) {
                Ok(path) => path,
                Err(message) => {
                    files_complete = false;
                    update_progress(app, control, &resource.logical, |progress| {
                        progress.errors += 1;
                        progress.message = Some(message);
                    });
                    continue;
                }
            };
        if let Err(error) = index_resource_file(
            connection,
            node_id,
            &resource.logical,
            &canonical_file,
            token,
        ) {
            files_complete = false;
            update_progress(app, control, &resource.logical, |progress| {
                progress.errors += 1;
                progress.message = Some(error);
            });
        }
    }
    if let Some(bdmv) = bdmv_directory.as_ref() {
        if !index_transparent_bdmv_resources(
            app,
            connection,
            node_id,
            bdmv,
            canonical_root,
            control,
            extensions,
            token,
        )? {
            files_complete = false;
        }
    }

    directories.sort_by(|a, b| {
        crate::db::natural_cmp(
            a.logical
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default(),
            b.logical
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default(),
        )
    });
    let mut children_complete = files_complete;
    for child in directories {
        let errors_before = control.progress().errors;
        scan_directory(
            app,
            connection,
            root_id,
            &child.logical,
            &child.filesystem,
            canonical_root,
            Some(node_id),
            control,
            extensions,
            token,
            visited,
        )?;
        if control.progress().errors > errors_before {
            children_complete = false;
        }
    }
    check_cancel(control)?;

    // Stale records are index-only cleanup. Source files are never changed.
    if children_complete {
        connection
            .execute(
                "DELETE FROM media_files WHERE node_id=?1 AND last_seen_at<>?2",
                params![node_id, token],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "DELETE FROM resource_files WHERE node_id=?1 AND last_seen_at<>?2",
                params![node_id, token],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "DELETE FROM nodes WHERE parent_node_id=?1 AND last_seen_at<>?2",
                params![node_id, token],
            )
            .map_err(|error| error.to_string())?;
    }

    let child_summary = indexed_child_media_summary(connection, node_id)?;
    let (direct_video_count, child_media_branch_count, total_video_count) = if children_complete {
        let direct = video_files.len() as i64;
        (
            direct,
            child_summary.branch_count,
            direct + child_summary.total_videos,
        )
    } else {
        // A permission/transient read failure must not turn a previously indexed work into an
        // empty container or prune data that could not be observed in this pass.
        (existing.3, existing.4, existing.2)
    };
    let automatic_type = classify_directory(
        direct_video_count,
        child_media_branch_count,
        child_summary.supplementary_branch_count,
        has_bdmv,
        false,
    );
    connection
        .execute(
            "UPDATE nodes SET
                node_type=CASE WHEN manual_type_override=1 THEN node_type ELSE ?1 END,
                direct_video_count=?2,child_media_branch_count=?3,total_video_count=?4,
                last_seen_at=?5,updated_at=CURRENT_TIMESTAMP
             WHERE id=?6",
            params![
                automatic_type.as_db(),
                direct_video_count,
                child_media_branch_count,
                total_video_count,
                token,
                node_id
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(total_video_count.max(0))
}

pub(crate) fn upsert_node(
    connection: &Connection,
    root_id: i64,
    parent_node_id: Option<i64>,
    absolute_path: &str,
    folder_name: &str,
    token: &str,
) -> Result<i64, ScanAbort> {
    connection
        .execute(
            "INSERT INTO nodes(
                library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,last_seen_at
             ) VALUES (?1,?2,?3,?4,?4,'CONTAINER',?5)
             ON CONFLICT(absolute_path) DO UPDATE SET
                library_root_id=excluded.library_root_id,
                parent_node_id=excluded.parent_node_id,
                folder_name=excluded.folder_name,
                last_seen_at=excluded.last_seen_at,
                updated_at=CURRENT_TIMESTAMP",
            params![root_id, parent_node_id, absolute_path, folder_name, token],
        )
        .map_err(|error| ScanAbort::Failed(format!("索引目录失败：{error}")))?;
    connection
        .query_row(
            "SELECT id FROM nodes WHERE absolute_path=?1 COLLATE NOCASE",
            [absolute_path],
            |row| row.get(0),
        )
        .map_err(|error| ScanAbort::Failed(format!("读取目录索引失败：{error}")))
}

fn refresh_ancestors(
    connection: &Connection,
    mut node_id: Option<i64>,
    canonical_root: &Path,
) -> Result<(), ScanAbort> {
    while let Some(id) = node_id {
        let (path, direct_videos, parent_id) = connection
            .query_row(
                "SELECT absolute_path,direct_video_count,parent_node_id FROM nodes WHERE id=?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
            .map_err(|error| error.to_string())?;
        let child_summary = indexed_child_media_summary(connection, id)?;
        let automatic_type = classify_directory(
            direct_videos,
            child_summary.branch_count,
            child_summary.supplementary_branch_count,
            has_typical_bdmv(Path::new(&path), canonical_root),
            false,
        );
        connection
            .execute(
                "UPDATE nodes SET
                    node_type=CASE WHEN manual_type_override=1 THEN node_type ELSE ?1 END,
                    child_media_branch_count=?2,total_video_count=?3,updated_at=CURRENT_TIMESTAMP
                 WHERE id=?4",
                params![
                    automatic_type.as_db(),
                    child_summary.branch_count,
                    direct_videos + child_summary.total_videos,
                    id
                ],
            )
            .map_err(|error| error.to_string())?;
        node_id = parent_id;
    }
    Ok(())
}

fn index_media_file(
    connection: &Connection,
    node_id: i64,
    path: &Path,
    filesystem_path: &Path,
    token: &str,
) -> AppResult<()> {
    let metadata = fs::metadata(filesystem_path)
        .map_err(|error| format!("无法读取视频元数据 {}：{error}", path.to_string_lossy()))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let modified_at = metadata
        .modified()
        .map(DateTime::<Utc>::from)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_default();
    connection
        .execute(
            "INSERT INTO media_files(
                node_id,absolute_path,file_name,extension,file_size,modified_at,last_seen_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(absolute_path) DO UPDATE SET
                node_id=excluded.node_id,file_name=excluded.file_name,extension=excluded.extension,
                file_size=excluded.file_size,modified_at=excluded.modified_at,last_seen_at=excluded.last_seen_at",
            params![
                node_id,
                path.to_string_lossy(),
                file_name,
                extension,
                metadata.len() as i64,
                modified_at,
                token
            ],
        )
        .map_err(|error| format!("写入视频索引失败：{error}"))?;
    Ok(())
}

pub(crate) fn index_resource_file(
    connection: &Connection,
    node_id: i64,
    path: &Path,
    filesystem_path: &Path,
    token: &str,
) -> AppResult<()> {
    let metadata = fs::metadata(filesystem_path)
        .map_err(|error| format!("无法读取附属资源元数据 {}：{error}", path.to_string_lossy()))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let modified_at = metadata
        .modified()
        .map(DateTime::<Utc>::from)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_default();
    let resource_type = resource_type_for_extension(&extension);
    connection
        .execute(
            "INSERT INTO resource_files(
                node_id,absolute_path,file_name,extension,file_size,modified_at,resource_type,last_seen_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(absolute_path) DO UPDATE SET
                node_id=excluded.node_id,file_name=excluded.file_name,extension=excluded.extension,
                file_size=excluded.file_size,modified_at=excluded.modified_at,
                resource_type=excluded.resource_type,last_seen_at=excluded.last_seen_at",
            params![
                node_id,
                path.to_string_lossy(),
                file_name,
                extension,
                metadata.len() as i64,
                modified_at,
                resource_type.as_db(),
                token
            ],
        )
        .map_err(|error| format!("写入附属资源索引失败：{error}"))?;
    Ok(())
}

/// A BDMV folder is transparent in the library tree: its STREAM/*.m2ts files belong to the
/// containing work and it must not create its own card. Walk the rest of that tree here so its
/// non-video files remain available as resources on the same parent node.
#[allow(clippy::too_many_arguments)]
fn index_transparent_bdmv_resources(
    app: Option<&AppHandle>,
    connection: &Connection,
    node_id: i64,
    bdmv: &ScanEntryPath,
    canonical_root: &Path,
    control: &ScanControl,
    extensions: &HashSet<String>,
    token: &str,
) -> Result<bool, ScanAbort> {
    let mut complete = true;
    let mut pending = vec![bdmv.clone()];
    let mut visited = HashSet::new();
    while let Some(directory) = pending.pop() {
        check_cancel(control)?;
        let canonical_directory =
            match canonicalize_within_library_root(&directory.filesystem, canonical_root) {
                Ok(path) if path.is_dir() => path,
                Ok(_) => {
                    complete = false;
                    update_progress(app, control, &directory.logical, |progress| {
                        progress.errors += 1;
                        progress.message = Some(format!(
                            "BDMV 附属资源路径不是目录，已跳过：{}",
                            directory.logical.display()
                        ));
                    });
                    continue;
                }
                Err(message) => {
                    complete = false;
                    update_progress(app, control, &directory.logical, |progress| {
                        progress.errors += 1;
                        progress.message = Some(message);
                    });
                    continue;
                }
            };
        if !visited.insert(canonical_directory.clone()) {
            continue;
        }
        let entries = match fs::read_dir(&canonical_directory) {
            Ok(entries) => entries,
            Err(error) => {
                complete = false;
                update_progress(app, control, &directory.logical, |progress| {
                    progress.errors += 1;
                    progress.message = Some(format!(
                        "无法读取 BDMV 附属资源目录 {}：{error}",
                        directory.logical.display()
                    ));
                });
                continue;
            }
        };
        for entry_result in entries {
            check_cancel(control)?;
            let entry = match entry_result {
                Ok(entry) => entry,
                Err(error) => {
                    complete = false;
                    update_progress(app, control, &directory.logical, |progress| {
                        progress.errors += 1;
                        progress.message = Some(format!("读取 BDMV 目录项失败：{error}"));
                    });
                    continue;
                }
            };
            let filesystem_path = entry.path();
            let logical_path = directory.logical.join(entry.file_name());
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(error) => {
                    complete = false;
                    update_progress(app, control, &logical_path, |progress| {
                        progress.errors += 1;
                        progress.message = Some(format!("读取 BDMV 文件类型失败：{error}"));
                    });
                    continue;
                }
            };
            // Keep the transparent walk inside the source tree and avoid link cycles.
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(ScanEntryPath {
                    logical: logical_path,
                    filesystem: filesystem_path,
                });
                continue;
            }
            if !file_type.is_file()
                || is_video(&logical_path, extensions)
                || has_extension(&logical_path, "m2ts")
            {
                continue;
            }
            let canonical_file =
                match canonical_file_within_library_root(&filesystem_path, canonical_root) {
                    Ok(path) => path,
                    Err(message) => {
                        complete = false;
                        update_progress(app, control, &logical_path, |progress| {
                            progress.errors += 1;
                            progress.message = Some(message);
                        });
                        continue;
                    }
                };
            if let Err(error) =
                index_resource_file(connection, node_id, &logical_path, &canonical_file, token)
            {
                complete = false;
                update_progress(app, control, &logical_path, |progress| {
                    progress.errors += 1;
                    progress.message = Some(error);
                });
            }
        }
    }
    Ok(complete)
}

pub fn resource_type_for_extension(extension: &str) -> ResourceType {
    match extension
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "ass" | "ssa" | "srt" | "sup" | "vtt" | "sub" | "idx" | "lrc" => ResourceType::Subtitle,
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "tif" | "tiff" | "svg" => {
            ResourceType::Image
        }
        "flac" | "wav" | "mp3" | "m4a" | "aac" | "ogg" | "opus" | "ape" | "wv" | "dts" | "ac3"
        | "eac3" | "truehd" => ResourceType::Audio,
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" => ResourceType::Archive,
        "ttf" | "otf" | "ttc" | "woff" | "woff2" => ResourceType::Font,
        "m3u" | "m3u8" | "pls" | "cue" | "mpls" => ResourceType::Playlist,
        "pdf" | "txt" | "log" | "nfo" | "xml" | "json" | "md" | "html" | "htm" | "doc" | "docx"
        | "rtf" | "csv" | "yaml" | "yml" => ResourceType::Document,
        _ => ResourceType::Other,
    }
}

pub(crate) fn update_progress(
    app: Option<&AppHandle>,
    control: &ScanControl,
    path: &Path,
    update: impl FnOnce(&mut ScanProgress),
) {
    let mut progress = control
        .progress
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    progress.current_path = path.to_string_lossy().into_owned();
    update(&mut progress);
    let snapshot = progress.clone();
    drop(progress);
    if let Some(app) = app.filter(|_| !snapshot.background) {
        let _ = app.emit("scan-progress", snapshot);
    }
}

pub(crate) fn check_cancel(control: &ScanControl) -> Result<(), ScanAbort> {
    if control.cancel.load(Ordering::Relaxed) {
        Err(ScanAbort::Cancelled)
    } else {
        Ok(())
    }
}

fn is_video(path: &Path, extensions: &HashSet<String>) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extensions.contains(&extension.to_ascii_lowercase()))
}

fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn file_name_eq(path: &Path, expected: &str) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn find_stream_directory(bdmv: &Path, canonical_root: &Path) -> Option<PathBuf> {
    let canonical_bdmv = canonicalize_within_library_root(bdmv, canonical_root).ok()?;
    if !canonical_bdmv.is_dir() {
        return None;
    }
    fs::read_dir(canonical_bdmv)
        .ok()?
        .flatten()
        .find_map(|entry| {
            let path = entry.path();
            let file_type = entry.file_type().ok()?;
            if file_type.is_symlink() || !file_type.is_dir() || !file_name_eq(&path, "STREAM") {
                return None;
            }
            let canonical = canonicalize_within_library_root(&path, canonical_root).ok()?;
            canonical.is_dir().then_some(canonical)
        })
}

pub fn has_typical_bdmv(path: &Path, library_root: &Path) -> bool {
    let Ok(canonical_root) = fs::canonicalize(library_root) else {
        return false;
    };
    let Ok(canonical_path) = canonicalize_within_library_root(path, &canonical_root) else {
        return false;
    };
    if !canonical_path.is_dir() {
        return false;
    }
    fs::read_dir(canonical_path)
        .ok()
        .and_then(|entries| {
            entries.flatten().find_map(|entry| {
                let candidate = entry.path();
                let file_type = entry.file_type().ok()?;
                if file_type.is_symlink()
                    || !file_type.is_dir()
                    || !file_name_eq(&candidate, "BDMV")
                {
                    None
                } else {
                    find_stream_directory(&candidate, &canonical_root)
                }
            })
        })
        .is_some()
}

pub(crate) fn indexed_child_media_summary(
    connection: &Connection,
    node_id: i64,
) -> AppResult<ChildMediaSummary> {
    let mut statement = connection
        .prepare(
            "SELECT folder_name,total_video_count,node_type,manual_type_override
             FROM nodes WHERE parent_node_id=?1",
        )
        .map_err(|error| error.to_string())?;
    let children = statement
        .query_map([node_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut summary = ChildMediaSummary::default();
    for child in children {
        let (folder_name, total_videos, node_type, manually_classified) =
            child.map_err(|error| error.to_string())?;
        if total_videos <= 0 || node_type == NodeType::Ignored.as_db() {
            continue;
        }
        summary.branch_count += 1;
        summary.total_videos += total_videos;
        if !manually_classified && is_supplementary_directory_name(&folder_name) {
            summary.supplementary_branch_count += 1;
        }
    }
    Ok(summary)
}

/// A deliberately small allow-list for folders that normally belong to the work whose
/// main episodes are stored directly in the parent. It only influences automatic detection
/// when the parent already has direct videos; it never turns an otherwise empty parent into a
/// work, and an explicit child classification always wins over this hint.
pub fn is_supplementary_directory_name(folder_name: &str) -> bool {
    let compact = folder_name
        .trim()
        .to_lowercase()
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect::<String>();
    if compact.is_empty() {
        return false;
    }

    const EXACT_NAMES: &[&str] = &[
        "sp",
        "sps",
        "special",
        "specials",
        "extra",
        "extras",
        "bonus",
        "bonuses",
        "bonusdisc",
        "bonusdiscs",
        "bonusfeature",
        "bonusfeatures",
        "ova",
        "ovas",
        "oad",
        "oads",
        "ona",
        "onas",
        "omake",
        "ncop",
        "nced",
        "ncoped",
        "ncedop",
        "ncopnced",
        "ncedncop",
        "pv",
        "pvs",
        "cm",
        "cms",
        "trailer",
        "trailers",
        "menu",
        "menus",
        "creditless",
        "creditlessop",
        "creditlessed",
        "promotionalvideo",
        "promotionalvideos",
        "preview",
        "previews",
        "sponsor",
        "sponsors",
    ];
    if EXACT_NAMES.contains(&compact.as_str()) {
        return true;
    }

    const NUMBERABLE_NAMES: &[&str] = &[
        "sp", "sps", "special", "specials", "extra", "extras", "ova", "ovas", "oad", "oads", "ona",
        "onas", "ncop", "nced", "pv", "pvs", "cm", "cms",
    ];
    if NUMBERABLE_NAMES.iter().any(|prefix| {
        compact
            .strip_prefix(prefix)
            .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()))
    }) {
        return true;
    }

    const CJK_NAMES: &[&str] = &[
        "特典",
        "特典映像",
        "映像特典",
        "影像特典",
        "特别篇",
        "特別篇",
        "番外篇",
        "番外",
        "附加内容",
        "附加內容",
        "花絮",
    ];
    CJK_NAMES.iter().any(|name| {
        compact == *name
            || compact.strip_prefix(name).is_some_and(|suffix| {
                suffix.chars().all(|c| c.is_ascii_digit())
                    || suffix.strip_prefix("vol").is_some_and(|number| {
                        !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())
                    })
            })
    })
}

/// Returns whether an automatically classified child is structural supplementary content of a
/// parent work that already stores its main episodes directly. Matching candidate selection uses
/// this scanner-owned name rule so SP/OVA/Extras allow-lists cannot drift between scanning and
/// Bangumi matching. An explicit child classification always wins.
#[cfg(test)]
pub(crate) fn is_automatic_supplementary_child(
    folder_name: &str,
    manually_classified: bool,
    parent_direct_video_count: i64,
) -> bool {
    !manually_classified
        && parent_direct_video_count > 0
        && is_supplementary_directory_name(folder_name)
}

pub fn classify_directory(
    direct_video_count: i64,
    child_media_branch_count: i64,
    supplementary_branch_count: i64,
    has_bdmv: bool,
    ignored: bool,
) -> NodeType {
    if ignored {
        NodeType::Ignored
    } else if has_bdmv
        || (direct_video_count > 0 && child_media_branch_count == supplementary_branch_count)
    {
        NodeType::AutoWork
    } else if direct_video_count == 0 && child_media_branch_count > 0 {
        NodeType::Container
    } else if direct_video_count > 0 && child_media_branch_count > 0 {
        NodeType::Mixed
    } else {
        NodeType::Container
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use tempfile::TempDir;

    #[test]
    fn partial_comic_scans_can_match_readable_nodes_but_failed_scans_cannot() {
        use crate::models::LibraryMediaKind::{Comic, Video};
        assert!(scan_outcome_allows_matching("SUCCESS", Video));
        assert!(scan_outcome_allows_matching("SUCCESS", Comic));
        assert!(scan_outcome_allows_matching("PARTIAL", Comic));
        assert!(!scan_outcome_allows_matching("PARTIAL", Video));
        for outcome in ["FAILED", "CANCELLED"] {
            assert!(!scan_outcome_allows_matching(outcome, Comic));
            assert!(!scan_outcome_allows_matching(outcome, Video));
        }
    }

    #[test]
    fn auto_match_progress_snapshot_includes_live_outcome_counts() {
        let control = ScanControl {
            unchanged_directories: Default::default(),
            scan_id: "progress".into(),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(ScanProgress {
                background: false,
                library_changed: None,
                scan_id: "progress".into(),
                root_id: 1,
                current_path: String::new(),
                folders_scanned: 0,
                videos_found: 0,
                comic_books_found: 0,
                status: ScanStatus::Running,
                errors: 0,
                message: None,
                phase: ScanPhase::Scanning,
                auto_match_current: 0,
                auto_match_total: 19,
                auto_match_matched: 0,
                auto_match_pending: 0,
                auto_match_unmatched: 0,
                auto_match_errors: 0,
            })),
        };
        let node = crate::models::MediaNode {
            media_kind: crate::models::LibraryMediaKind::Video,
            direct_comic_book_count: 0,
            child_comic_branch_count: 0,
            total_comic_book_count: 0,
            latest_file_modified_at: None,
            last_watched_at: None,
            id: 13,
            library_root_id: 1,
            parent_node_id: Some(1),
            absolute_path: r"C:\media\current".into(),
            folder_name: "current".into(),
            display_name: "current".into(),
            node_type: NodeType::AutoWork,
            manual_type_override: false,
            cover_source: crate::models::CoverSource::Placeholder,
            cover_cache_path: None,
            direct_video_count: 1,
            child_media_branch_count: 0,
            total_video_count: 1,
            created_at: String::new(),
            updated_at: String::new(),
            last_seen_at: String::new(),
            binding: None,
            user_tags: Vec::new(),
        };

        update_auto_match_progress(
            None,
            &control,
            13,
            19,
            &node,
            auto_match::AutoMatchReport {
                examined: 13,
                matched: 4,
                unmatched: 5,
                errors: 1,
            },
        );

        let progress = control.progress();
        assert_eq!(progress.current_path, node.absolute_path);
        assert_eq!(progress.phase, ScanPhase::AutoMatching);
        assert_eq!(progress.auto_match_current, 13);
        assert_eq!(progress.auto_match_total, 19);
        assert_eq!(progress.auto_match_matched, 4);
        assert_eq!(progress.auto_match_pending, 0);
        assert_eq!(progress.auto_match_unmatched, 5);
        assert_eq!(progress.auto_match_errors, 1);
    }

    #[test]
    fn conservative_classification_matches_product_rules() {
        assert_eq!(
            classify_directory(12, 0, 0, false, false),
            NodeType::AutoWork
        );
        assert_eq!(
            classify_directory(0, 3, 0, false, false),
            NodeType::Container
        );
        assert_eq!(classify_directory(1, 2, 0, false, false), NodeType::Mixed);
        assert_eq!(classify_directory(0, 0, 0, true, false), NodeType::AutoWork);
        assert_eq!(classify_directory(5, 1, 1, false, true), NodeType::Ignored);
    }

    #[test]
    fn supplementary_names_are_conservative_and_case_insensitive() {
        for name in [
            "SP",
            "sp 02",
            "Specials",
            "Specials 02",
            "EXTRAS",
            "Extras 2",
            "OVA",
            "OAD_1",
            "NCOP",
            "NCED 02",
            "NCOP & NCED",
            "Menu",
            "Creditless OP",
            "Promotional Videos",
            "Previews",
            "映像特典",
            "影像特典 Vol. 2",
            "番外篇",
            "花絮",
        ] {
            assert!(is_supplementary_directory_name(name), "{name}");
        }
        for name in [
            "Season 01",
            "Disc 1",
            "作品 SPY×FAMILY",
            "OVA Collection 2024",
            "特典作品全集",
            "Another Show",
            "Movie",
        ] {
            assert!(!is_supplementary_directory_name(name), "{name}");
        }

        assert!(is_automatic_supplementary_child("SP 01", false, 12));
        assert!(!is_automatic_supplementary_child("SP 01", true, 12));
        assert!(!is_automatic_supplementary_child("SP 01", false, 0));
        assert!(!is_automatic_supplementary_child(
            "OVA Collection 2024",
            false,
            12
        ));

        assert_eq!(
            classify_directory(12, 4, 4, false, false),
            NodeType::AutoWork
        );
        assert_eq!(classify_directory(12, 4, 3, false, false), NodeType::Mixed);
        // Folder-name hints alone cannot create a work without direct main episodes.
        assert_eq!(
            classify_directory(0, 4, 4, false, false),
            NodeType::Container
        );
    }

    #[test]
    fn scan_treats_direct_episodes_plus_supplements_as_one_work() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("媒体");
        let work = root_path.join("本篇作品");
        let sp = work.join("sp 01");
        let ova = work.join("OVA");
        let extras = work.join("影像特典 Vol. 2");
        let unrelated = work.join("另一部作品");
        for directory in [&sp, &ova, &extras, &unrelated] {
            fs::create_dir_all(directory).unwrap();
        }
        for episode in 1..=12 {
            fs::write(work.join(format!("EP{episode:02}.mkv")), b"episode").unwrap();
        }
        fs::write(sp.join("SP01.mkv"), b"sp").unwrap();
        fs::write(ova.join("OVA01.mp4"), b"ova").unwrap();
        fs::write(extras.join("NCOP.webm"), b"extra").unwrap();
        fs::write(unrelated.join("01.mkv"), b"other").unwrap();

        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let run = |scan_id: &str| {
            database.start_scan_run(scan_id, root.id).unwrap();
            let control = ScanControl {
                unchanged_directories: Default::default(),
                scan_id: scan_id.to_string(),
                cancel: Arc::new(AtomicBool::new(false)),
                progress: Arc::new(Mutex::new(ScanProgress {
                    background: false,
                    library_changed: None,
                    scan_id: scan_id.to_string(),
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
            };
            run_scan(
                None,
                &database,
                vec![ScanTarget {
                    root: root.clone(),
                    path: root_path.clone(),
                    parent_node_id: None,
                }],
                &control,
                &crate::db::default_video_extensions(),
            );
            assert_eq!(control.progress().status, ScanStatus::Completed);
        };

        run("with-real-child");
        let connection = database.connect().unwrap();
        let work_id: i64 = connection
            .query_row(
                "SELECT id FROM nodes WHERE folder_name='本篇作品'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let kind: String = connection
            .query_row(
                "SELECT node_type FROM nodes WHERE id=?1",
                [work_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(kind, "MIXED");
        connection
            .execute(
                "DELETE FROM nodes WHERE parent_node_id=?1 AND folder_name='另一部作品'",
                [work_id],
            )
            .unwrap();
        drop(connection);
        fs::remove_dir_all(&unrelated).unwrap();

        run("supplements-only");
        let connection = database.connect().unwrap();
        let (kind, branches, total): (String, i64, i64) = connection
            .query_row(
                "SELECT node_type,child_media_branch_count,total_video_count FROM nodes WHERE id=?1",
                [work_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(kind, "AUTO_WORK");
        assert_eq!(branches, 3);
        assert_eq!(total, 15);
    }

    #[test]
    fn partial_supplement_scan_refreshes_parent_as_auto_work() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("媒体");
        let work = root_path.join("局部扫描作品");
        let sp = work.join("SP");
        fs::create_dir_all(&sp).unwrap();
        fs::write(work.join("01.mkv"), b"episode").unwrap();
        fs::write(sp.join("SP01.mkv"), b"special").unwrap();

        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        database.start_scan_run("full", root.id).unwrap();
        let full_scan = scan_control("full", &root);
        run_scan(
            None,
            &database,
            vec![ScanTarget {
                root: root.clone(),
                path: root_path.clone(),
                parent_node_id: None,
            }],
            &full_scan,
            &crate::db::default_video_extensions(),
        );

        let connection = database.connect().unwrap();
        let (work_id, sp_id): (i64, i64) = (
            connection
                .query_row(
                    "SELECT id FROM nodes WHERE folder_name='局部扫描作品'",
                    [],
                    |row| row.get(0),
                )
                .unwrap(),
            connection
                .query_row("SELECT id FROM nodes WHERE folder_name='SP'", [], |row| {
                    row.get(0)
                })
                .unwrap(),
        );
        connection
            .execute("UPDATE nodes SET node_type='MIXED' WHERE id=?1", [work_id])
            .unwrap();
        drop(connection);

        database.start_scan_run("partial", root.id).unwrap();
        let partial_scan = scan_control("partial", &root);
        run_scan(
            None,
            &database,
            vec![ScanTarget {
                root: root.clone(),
                path: sp.clone(),
                parent_node_id: Some(work_id),
            }],
            &partial_scan,
            &crate::db::default_video_extensions(),
        );

        assert_eq!(partial_scan.progress().status, ScanStatus::Completed);
        assert_eq!(
            database.get_node(work_id).unwrap().node_type,
            NodeType::AutoWork
        );
        assert_eq!(
            database.get_node(sp_id).unwrap().node_type,
            NodeType::AutoWork
        );
    }

    #[test]
    fn reset_old_mixed_uses_supplement_rule_and_manual_parent_survives_scan() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("root");
        let work = root_path.join("work");
        let extras = work.join("Extras");
        fs::create_dir_all(&extras).unwrap();
        fs::write(work.join("01.mkv"), b"episode").unwrap();
        fs::write(extras.join("NCOP.mkv"), b"extra").unwrap();

        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        database.start_scan_run("initial", root.id).unwrap();
        let initial_scan = scan_control("initial", &root);
        run_scan(
            None,
            &database,
            vec![ScanTarget {
                root: root.clone(),
                path: root_path.clone(),
                parent_node_id: None,
            }],
            &initial_scan,
            &crate::db::default_video_extensions(),
        );

        let connection = database.connect().unwrap();
        let work_id: i64 = connection
            .query_row("SELECT id FROM nodes WHERE folder_name='work'", [], |row| {
                row.get(0)
            })
            .unwrap();
        connection
            .execute(
                "UPDATE nodes SET node_type='MIXED',manual_type_override=0 WHERE id=?1",
                [work_id],
            )
            .unwrap();
        drop(connection);
        assert_eq!(
            database.reset_node_type(work_id).unwrap().node_type,
            NodeType::AutoWork
        );

        let manually_set = database
            .set_node_type(work_id, NodeType::Container)
            .unwrap();
        assert!(manually_set.manual_type_override);
        database.start_scan_run("after-manual", root.id).unwrap();
        let rescan = scan_control("after-manual", &root);
        run_scan(
            None,
            &database,
            vec![ScanTarget {
                root: root.clone(),
                path: root_path,
                parent_node_id: None,
            }],
            &rescan,
            &crate::db::default_video_extensions(),
        );
        let preserved = database.get_node(work_id).unwrap();
        assert_eq!(preserved.node_type, NodeType::Container);
        assert!(preserved.manual_type_override);
    }

    fn scan_control(scan_id: &str, root: &LibraryRoot) -> ScanControl {
        ScanControl {
            unchanged_directories: Default::default(),
            scan_id: scan_id.to_string(),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(ScanProgress {
                background: false,
                library_changed: None,
                scan_id: scan_id.to_string(),
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

    fn incremental_scan(
        database: &Database,
        root: &LibraryRoot,
        id: &str,
        cancel: bool,
    ) -> ScanProgress {
        database.start_scan_run(id, root.id).unwrap();
        let control = scan_control(id, root);
        control.progress.lock().unwrap().background = true;
        control.progress.lock().unwrap().library_changed = Some(false);
        control.cancel.store(cancel, Ordering::Relaxed);
        run_scan(
            None,
            database,
            vec![ScanTarget {
                root: root.clone(),
                path: PathBuf::from(&root.path),
                parent_node_id: None,
            }],
            &control,
            &crate::db::default_video_extensions(),
        );
        control.progress()
    }

    #[test]
    fn incremental_scan_reuses_unchanged_subtrees_and_updates_deep_changes() {
        let temp = crate::db::test_temp_dir();
        let root_path = temp.path().join("媒体库");
        let show = root_path.join("2026").join("连载");
        let stable = root_path.join("归档").join("旧作品");
        fs::create_dir_all(&show).unwrap();
        fs::create_dir_all(&stable).unwrap();
        fs::write(show.join("01.mkv"), b"episode one").unwrap();
        fs::write(stable.join("01.mkv"), b"stable source").unwrap();
        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let baseline = incremental_scan(&database, &root, "baseline", false);
        assert_eq!(baseline.status, ScanStatus::Completed);
        assert_eq!(baseline.library_changed, Some(true));
        let connection = database.connect().unwrap();
        let id: i64 = connection
            .query_row(
                "SELECT id FROM nodes WHERE absolute_path=?1",
                [show.to_string_lossy()],
                |row| row.get(0),
            )
            .unwrap();
        database.create_or_assign_user_tag(id, "追番").unwrap();
        database.set_node_type(id, NodeType::Work).unwrap();
        let stable_before: String = connection
            .query_row(
                "SELECT last_seen_at FROM nodes WHERE absolute_path=?1",
                [stable.to_string_lossy()],
                |row| row.get(0),
            )
            .unwrap();
        let unchanged = incremental_scan(&database, &root, "unchanged", false);
        assert_eq!(unchanged.library_changed, Some(false));
        assert_eq!(unchanged.folders_scanned, 0);
        assert_eq!(unchanged.videos_found, 0);
        assert_eq!(unchanged.auto_match_total, 0);
        fs::write(show.join("02.mkv"), b"episode two").unwrap();
        let changed = incremental_scan(&database, &root, "episode", false);
        assert_eq!(changed.errors, 0);
        assert_eq!(changed.library_changed, Some(true));
        assert_eq!(
            changed.folders_scanned, 1,
            "only the changed deep directory is indexed"
        );
        let node = database.get_node(id).unwrap();
        assert_eq!(node.total_video_count, 2);
        assert_eq!(node.node_type, NodeType::Work);
        assert!(node.manual_type_override);
        assert_eq!(node.user_tags[0].name, "追番");
        let root_count: i64 = connection
            .query_row(
                "SELECT total_video_count FROM nodes WHERE absolute_path=?1",
                [root_path.to_string_lossy()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(root_count, 3, "partial updates refresh ancestor counts");
        let new_show = root_path.join("新作品");
        fs::create_dir(&new_show).unwrap();
        fs::write(new_show.join("01.mkv"), b"new work").unwrap();
        let added = incremental_scan(&database, &root, "new-show", false);
        assert_eq!(
            added.folders_scanned, 2,
            "scan root and new work, reuse existing branches"
        );
        let stable_after: String = connection
            .query_row(
                "SELECT last_seen_at FROM nodes WHERE absolute_path=?1",
                [stable.to_string_lossy()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stable_before, stable_after);
        fs::remove_file(show.join("02.mkv")).unwrap();
        incremental_scan(&database, &root, "deleted-episode", false);
        assert_eq!(database.get_node(id).unwrap().total_video_count, 1);
        fs::remove_file(new_show.join("01.mkv")).unwrap();
        fs::remove_dir(&new_show).unwrap();
        let deleted = incremental_scan(&database, &root, "deleted-show", false);
        assert_eq!(deleted.errors, 0);
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE absolute_path=?1",
                [new_show.to_string_lossy()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(
            incremental_scan(&database, &root, "unchanged-again", false).library_changed,
            Some(false)
        );
        assert_eq!(fs::read(stable.join("01.mkv")).unwrap(), b"stable source");
    }

    #[test]
    fn incremental_scan_preserves_baseline_when_cancelled_or_offline_and_restores_hidden() {
        let temp = crate::db::test_temp_dir();
        let root_path = temp.path().join("library");
        let show = root_path.join("show");
        fs::create_dir_all(&show).unwrap();
        fs::write(show.join("01.mkv"), b"one").unwrap();
        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let cancelled = incremental_scan(&database, &root, "cancel-baseline", true);
        assert_eq!(cancelled.status, ScanStatus::Cancelled);
        let connection = database.connect().unwrap();
        let baseline_json = || {
            connection
                .query_row(
                    "SELECT snapshot_json FROM library_scan_snapshots WHERE library_root_id=?1",
                    [root.id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .unwrap()
        };
        assert!(baseline_json().is_none());
        incremental_scan(&database, &root, "baseline", false);
        let baseline = baseline_json();
        fs::write(show.join("02.mkv"), b"two").unwrap();
        incremental_scan(&database, &root, "cancel-change", true);
        assert_eq!(baseline_json(), baseline);
        let offline = temp.path().join("offline");
        fs::rename(&root_path, &offline).unwrap();
        let failed = incremental_scan(&database, &root, "offline", false);
        assert!(failed.errors > 0);
        assert_eq!(failed.library_changed, Some(false));
        assert_eq!(baseline_json(), baseline);
        fs::rename(&offline, &root_path).unwrap();
        assert_eq!(
            incremental_scan(&database, &root, "online", false).library_changed,
            Some(true)
        );
        let id: i64 = connection
            .query_row(
                "SELECT id FROM nodes WHERE absolute_path=?1",
                [show.to_string_lossy()],
                |row| row.get(0),
            )
            .unwrap();
        database.set_node_type(id, NodeType::Ignored).unwrap();
        incremental_scan(&database, &root, "hide", false);
        fs::write(show.join("03.mkv"), b"three").unwrap();
        assert_eq!(
            incremental_scan(&database, &root, "hidden-change", false).library_changed,
            Some(false)
        );
        database.reset_node_type(id).unwrap();
        incremental_scan(&database, &root, "restore", false);
        assert_eq!(database.get_node(id).unwrap().total_video_count, 3);
    }

    #[test]
    fn incremental_file_mode_detects_modified_files_and_manual_scan_invalidates_baseline() {
        let temp = crate::db::test_temp_dir();
        let root_path = temp.path().join("library");
        fs::create_dir_all(&root_path).unwrap();
        let file = root_path.join("Show - 01.mkv");
        fs::write(&file, b"one").unwrap();
        let stable = root_path.join("stable");
        fs::create_dir(&stable).unwrap();
        fs::write(stable.join("02.mkv"), b"stable video").unwrap();
        fs::write(stable.join("notes.txt"), b"stable notes").unwrap();
        let stream = root_path.join("Disc").join("BDMV").join("STREAM");
        fs::create_dir_all(&stream).unwrap();
        fs::write(stream.join("00001.m2ts"), b"disc one").unwrap();
        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database
            .add_root_with_mode(&root_path, None, LibraryRecognitionMode::VideoFile)
            .unwrap();
        incremental_scan(&database, &root, "baseline", false);
        let connection = database.connect().unwrap();
        let stable_marker = || {
            connection
                .query_row(
                    "SELECT last_seen_at FROM media_files WHERE absolute_path=?1",
                    [stable.join("02.mkv").to_string_lossy()],
                    |row| row.get::<_, String>(0),
                )
                .unwrap()
        };
        let before = stable_marker();
        assert_eq!(
            incremental_scan(&database, &root, "unchanged", false).library_changed,
            Some(false)
        );
        fs::write(&file, b"changed size").unwrap();
        let modified = incremental_scan(&database, &root, "modified", false);
        assert_eq!(modified.library_changed, Some(true));
        assert_eq!(modified.folders_scanned, 1);
        assert_eq!(modified.videos_found, 1);
        assert_eq!(stable_marker(), before);
        let resources: i64 = connection
            .query_row("SELECT COUNT(*) FROM resource_files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            resources, 1,
            "unchanged root-owned attachments survive cleanup"
        );
        let root_count: i64 = connection
            .query_row(
                "SELECT total_video_count FROM nodes WHERE absolute_path=?1",
                [root_path.to_string_lossy()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            root_count, 3,
            "flat root counts include reused videos and discs"
        );
        fs::write(stream.join("00002.m2ts"), b"disc two").unwrap();
        assert_eq!(
            incremental_scan(&database, &root, "disc-change", false).library_changed,
            Some(true)
        );
        let discs: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM media_files WHERE extension='m2ts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(discs, 2);
        let size: i64 = connection
            .query_row(
                "SELECT file_size FROM media_files WHERE absolute_path=?1",
                [file.to_string_lossy()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(size, 12);
        let control = scan_control("manual", &root);
        run_scan(
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
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM library_scan_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(
            incremental_scan(&database, &root, "new-baseline", false).library_changed,
            Some(true)
        );
    }

    #[test]
    fn incremental_scan_handles_bdmv_and_extension_configuration_changes() {
        let temp = crate::db::test_temp_dir();
        let root_path = temp.path().join("library");
        let stream = root_path.join("Disc").join("BDMV").join("STREAM");
        fs::create_dir_all(&stream).unwrap();
        fs::write(stream.join("00001.m2ts"), b"one").unwrap();
        fs::write(root_path.join("extra.custom"), b"custom extension").unwrap();
        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        incremental_scan(&database, &root, "baseline", false);
        fs::write(stream.join("00002.m2ts"), b"two").unwrap();
        let changed = incremental_scan(&database, &root, "disc-change", false);
        assert_eq!(changed.errors, 0);
        assert_eq!(changed.library_changed, Some(true));
        let connection = database.connect().unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM media_files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let control = scan_control("extensions", &root);
        control.progress.lock().unwrap().background = true;
        let mut extensions = crate::db::default_video_extensions();
        extensions.push(".custom".into());
        run_scan(
            None,
            &database,
            vec![ScanTarget {
                root: root.clone(),
                path: root_path,
                parent_node_id: None,
            }],
            &control,
            &extensions,
        );
        assert_eq!(control.progress().library_changed, Some(true));
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM media_files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn incremental_scan_saves_accessible_baselines_when_another_root_is_offline() {
        let temp = TempDir::new().unwrap();
        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root_path = temp.path().join("online");
        let offline_path = temp.path().join("offline");
        fs::create_dir(&root_path).unwrap();
        fs::create_dir(&offline_path).unwrap();
        fs::write(root_path.join("01.mkv"), b"one").unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let offline = database.add_root(&offline_path, None).unwrap();
        fs::remove_dir(&offline_path).unwrap();
        let control = scan_control("mixed-roots", &root);
        control.progress.lock().unwrap().background = true;
        run_scan(
            None,
            &database,
            vec![
                ScanTarget {
                    root: offline.clone(),
                    path: offline_path,
                    parent_node_id: None,
                },
                ScanTarget {
                    root: root.clone(),
                    path: root_path,
                    parent_node_id: None,
                },
            ],
            &control,
            &crate::db::default_video_extensions(),
        );
        let online_health = database.get_root(root.id).unwrap().scan_health.unwrap();
        let offline_health = database.get_root(offline.id).unwrap().scan_health.unwrap();
        assert_eq!(online_health.outcome, "SUCCESS");
        assert!(online_health.last_success_at.is_some());
        assert_eq!(offline_health.outcome, "FAILED");
        assert!(offline_health.last_success_at.is_none());
        assert_eq!(control.progress().errors, 1);
        assert_eq!(control.progress().library_changed, Some(true));
        let unchanged = incremental_scan(&database, &root, "online-unchanged", false);
        assert_eq!(unchanged.library_changed, Some(false));
        assert_eq!(unchanged.folders_scanned, 0);
    }

    #[test]
    fn work_catalogue_refreshes_episodes_and_preserves_source_metadata() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("媒体库");
        let nested = root_path.join("2026").join("连载");
        fs::create_dir_all(&nested).unwrap();
        let first = nested.join("Steins;Gate - 01.mkv");
        fs::write(&first, b"read-only episode one").unwrap();
        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database
            .add_root_with_mode(&root_path, None, LibraryRecognitionMode::VideoFile)
            .unwrap();
        let scan = |id: &str| {
            database.start_scan_run(id, root.id).unwrap();
            let control = scan_control(id, &root);
            run_scan(
                None,
                &database,
                vec![ScanTarget {
                    root: root.clone(),
                    path: root_path.clone(),
                    parent_node_id: None,
                }],
                &control,
                &crate::db::default_video_extensions(),
            );
            assert_eq!(control.progress().status, ScanStatus::Completed);
        };
        scan("initial-work");
        let initial = database.list_all_resources().unwrap();
        assert_eq!(initial.works.len(), 1);
        let node_id = initial.works[0].node.id;
        database
            .create_or_assign_user_tag(node_id, "追番中")
            .unwrap();
        let favorite = database.create_favorite_folder("本季").unwrap();
        database
            .batch_add_nodes_to_favorite(favorite.id, &[node_id])
            .unwrap();
        fs::write(nested.join("Steins;Gate - 02.mkv"), b"episode two").unwrap();
        fs::write(nested.join("Another Show - 01.mkv"), b"another work").unwrap();
        fs::write(nested.join("Steins;Gate S02E01.mkv"), b"different season").unwrap();
        fs::write(
            nested.join("Steins;Gate S02E02.mkv"),
            b"season two episode two",
        )
        .unwrap();
        scan("new-episodes");
        let refreshed = database.list_all_resources().unwrap();
        assert_eq!(
            refreshed.works.len(),
            3,
            "same-season episodes merge, other works/seasons do not"
        );
        let detail = crate::works::work_detail(&database, node_id).unwrap();
        assert_eq!(detail.media_files.len(), 2);
        assert_eq!(detail.work_sources.as_ref().unwrap().len(), 2);
        assert_eq!(detail.node.user_tags[0].name, "追番中");
        assert_eq!(
            database.list_favorite_folder_nodes(favorite.id).unwrap()[0].id,
            node_id
        );
        assert_eq!(fs::read(&first).unwrap(), b"read-only episode one");
    }

    #[test]
    fn work_catalogue_flattens_folders_and_regroups_after_binding_changes() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("library");
        for folder in ["archive/release-a", "current/release-b"] {
            let directory = root_path.join(folder);
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("01.mkv"), b"video").unwrap();
        }
        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        database.start_scan_run("folders", root.id).unwrap();
        let control = scan_control("folders", &root);
        run_scan(
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
        let nodes = database.list_work_sources().unwrap();
        assert_eq!(
            nodes.len(),
            2,
            "grouping folders must not become work cards"
        );
        let mut subject = crate::models::BangumiSubject {
            subject_id: 42,
            title: "Example Work".into(),
            title_cn: None,
            title_en: None,
            title_ja: None,
            title_ko: None,
            match_aliases: vec![],
            date: None,
            image_url: None,
            summary: None,
            subject_type: 2,
        };
        for node in &nodes {
            database.save_confirmed_binding(node.id, &subject).unwrap();
        }
        assert_eq!(database.list_all_resources().unwrap().works.len(), 1);
        assert_eq!(
            crate::works::work_detail(&database, nodes[0].id)
                .unwrap()
                .media_files
                .len(),
            2
        );
        subject.subject_id = 43;
        database
            .save_confirmed_binding(nodes[1].id, &subject)
            .unwrap();
        assert_eq!(database.list_all_resources().unwrap().works.len(), 2);
        database
            .set_node_type(nodes[0].parent_node_id.unwrap(), NodeType::Ignored)
            .unwrap();
        assert_eq!(
            database.list_all_resources().unwrap().works.len(),
            1,
            "ignored subtrees stay hidden"
        );
    }

    #[test]
    fn video_file_mode_flattens_each_video_into_an_independent_work() {
        let temp = crate::db::test_temp_dir();
        let root_path = temp.path().join("逐文件媒体库");
        let nested = root_path.join("子目录 [字幕组]");
        let bdmv_stream = root_path.join("蓝光电影").join("BDMV").join("STREAM");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(&bdmv_stream).unwrap();
        let first = root_path.join("命运石之门 01.mkv");
        let second = nested.join("Steins;Gate 02.mp4");
        fs::write(&first, b"one").unwrap();
        fs::write(&second, b"two").unwrap();
        fs::write(root_path.join("说明.txt"), b"attachment").unwrap();
        fs::write(bdmv_stream.join("00000.m2ts"), b"stream one").unwrap();
        fs::write(bdmv_stream.join("00001.m2ts"), b"stream two").unwrap();

        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database
            .add_root_with_mode(&root_path, None, LibraryRecognitionMode::VideoFile)
            .unwrap();
        database.start_scan_run("file-mode", root.id).unwrap();
        let control = scan_control("file-mode", &root);
        run_scan(
            None,
            &database,
            vec![ScanTarget {
                root: root.clone(),
                path: root_path.clone(),
                parent_node_id: None,
            }],
            &control,
            &crate::db::default_video_extensions(),
        );

        assert_eq!(control.progress().status, ScanStatus::Completed);
        assert_eq!(control.progress().videos_found, 4);
        let all = database.list_all_resources().unwrap();
        assert_eq!(all.total_count, 3);
        assert!(all.nodes.iter().all(|node| node.parent_node_id.is_some()));
        assert!(all
            .nodes
            .iter()
            .all(|node| node.node_type == NodeType::AutoWork));
        let first_node = all
            .nodes
            .iter()
            .find(|node| node.folder_name == "命运石之门 01")
            .unwrap();
        assert_eq!(first_node.absolute_path, first.to_string_lossy());
        assert_eq!(database.list_media(first_node.id).unwrap().len(), 1);
        let bdmv_node = all
            .nodes
            .iter()
            .find(|node| node.folder_name == "蓝光电影")
            .unwrap();
        assert_eq!(database.list_media(bdmv_node.id).unwrap().len(), 2);

        let hidden = database.hidden_root_node_id(&root).unwrap().unwrap();
        assert_eq!(database.list_resources(hidden).unwrap().len(), 1);
        assert_eq!(fs::read(&first).unwrap(), b"one");
    }

    #[test]
    fn canonical_boundary_rejects_a_sibling_with_the_same_text_prefix() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("Media");
        let child = root_path.join("Work");
        let prefix_sibling = temp.path().join("Media-Outside");
        fs::create_dir_all(&child).unwrap();
        fs::create_dir_all(&prefix_sibling).unwrap();

        let canonical_root = fs::canonicalize(&root_path).unwrap();
        assert!(canonicalize_within_library_root(&child, &canonical_root).is_ok());
        let error = canonicalize_within_library_root(&prefix_sibling, &canonical_root).unwrap_err();
        assert!(error.contains("超出资源库根目录"), "{error}");
    }

    #[test]
    fn bdmv_detection_never_reads_a_complete_structure_outside_the_library_root() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("Library");
        let inside = root_path.join("Inside");
        let outside = temp.path().join("Library-Outside");
        fs::create_dir_all(inside.join("BDMV").join("STREAM")).unwrap();
        fs::create_dir_all(outside.join("BDMV").join("STREAM")).unwrap();

        assert!(has_typical_bdmv(&inside, &root_path));
        assert!(
            !has_typical_bdmv(&outside, &root_path),
            "BDMV probing must canonicalize before enumerating outside a Library Root"
        );
    }

    #[test]
    fn partial_scan_rejects_a_target_outside_the_canonical_library_root() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("Library");
        let outside = temp.path().join("Library-Outside");
        fs::create_dir_all(&root_path).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let outside_video = outside.join("private.mkv");
        fs::write(&outside_video, b"outside").unwrap();

        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        database.start_scan_run("outside-partial", root.id).unwrap();
        let control = scan_control("outside-partial", &root);
        run_scan(
            None,
            &database,
            vec![ScanTarget {
                root: root.clone(),
                path: outside,
                parent_node_id: Some(9_999),
            }],
            &control,
            &crate::db::default_video_extensions(),
        );

        let progress = control.progress();
        assert_eq!(progress.status, ScanStatus::Failed);
        assert_eq!(progress.videos_found, 0);
        assert!(
            progress
                .message
                .as_deref()
                .is_some_and(|message| message.contains("超出资源库根目录")),
            "{:?}",
            progress.message
        );
        let connection = database.connect().unwrap();
        let node_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE library_root_id=?1",
                [root.id],
                |row| row.get(0),
            )
            .unwrap();
        let media_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM media_files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(node_count, 0);
        assert_eq!(media_count, 0);
        assert_eq!(fs::read(&outside_video).unwrap(), b"outside");
    }

    #[cfg(unix)]
    fn create_directory_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn create_directory_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_dir(target, link)
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn directory_symlinks_remain_skipped_and_cannot_be_partial_scan_escape_hatches() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("Library");
        let inside = root_path.join("Inside");
        let outside = temp.path().join("Outside");
        fs::create_dir_all(&inside).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(inside.join("01.mkv"), b"inside").unwrap();
        let outside_video = outside.join("private.mkv");
        fs::write(&outside_video, b"outside").unwrap();
        let link = root_path.join("LinkedOutside");
        if let Err(error) = create_directory_symlink(&outside, &link) {
            // Creating symlinks may require Developer Mode or SeCreateSymbolicLinkPrivilege on
            // Windows. The platform-independent outside-target test above still covers the
            // canonical boundary when that privilege is unavailable.
            #[cfg(windows)]
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(1314)
            {
                return;
            }
            panic!("failed to create test directory symlink: {error}");
        }

        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();

        database.start_scan_run("linked-partial", root.id).unwrap();
        let partial = scan_control("linked-partial", &root);
        run_scan(
            None,
            &database,
            vec![ScanTarget {
                root: root.clone(),
                path: link.clone(),
                parent_node_id: Some(9_999),
            }],
            &partial,
            &crate::db::default_video_extensions(),
        );
        assert_eq!(partial.progress().status, ScanStatus::Failed);
        assert_eq!(partial.progress().videos_found, 0);

        database.start_scan_run("linked-full", root.id).unwrap();
        let full = scan_control("linked-full", &root);
        run_scan(
            None,
            &database,
            vec![ScanTarget {
                root: root.clone(),
                path: root_path,
                parent_node_id: None,
            }],
            &full,
            &crate::db::default_video_extensions(),
        );
        assert_eq!(full.progress().status, ScanStatus::Completed);
        assert_eq!(full.progress().videos_found, 1);
        let connection = database.connect().unwrap();
        let escaped_nodes: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE absolute_path=?1 COLLATE NOCASE",
                [link.to_string_lossy()],
                |row| row.get(0),
            )
            .unwrap();
        let escaped_media: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM media_files WHERE absolute_path=?1 COLLATE NOCASE",
                [outside_video.to_string_lossy()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(escaped_nodes, 0);
        assert_eq!(escaped_media, 0);
        assert_eq!(fs::read(outside_video).unwrap(), b"outside");
    }

    #[test]
    fn recursive_scan_indexes_unicode_mixed_and_bdmv_without_touching_sources() {
        let temp = TempDir::new().unwrap();
        let media_root = temp.path().join("媒体 [收藏]");
        let series = media_root.join("动画合集");
        let work_a = series.join("作品 A");
        let work_b = series.join("日本語 作品");
        let mixed = media_root.join("混合");
        let nested = mixed.join("下级").join("三层");
        let bdmv_stream = media_root.join("蓝光作品").join("BDMV").join("STREAM");
        for directory in [&work_a, &work_b, &nested, &bdmv_stream] {
            fs::create_dir_all(directory).unwrap();
        }
        for episode in 1..=12 {
            fs::write(work_a.join(format!("[{episode:02}].mkv")), b"test").unwrap();
        }
        fs::write(work_b.join("01.mp4"), b"test").unwrap();
        fs::write(mixed.join("本目录.webm"), b"test").unwrap();
        fs::write(nested.join("EP2.m2ts"), b"test").unwrap();
        fs::write(bdmv_stream.join("00000.m2ts"), b"test").unwrap();
        fs::write(media_root.join("not-media.txt"), b"keep me").unwrap();

        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&media_root, None).unwrap();
        let scan_id = "test-scan".to_string();
        database.start_scan_run(&scan_id, root.id).unwrap();
        let control = ScanControl {
            unchanged_directories: Default::default(),
            scan_id: scan_id.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(ScanProgress {
                background: false,
                library_changed: None,
                scan_id,
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
        };
        run_scan(
            None,
            &database,
            vec![ScanTarget {
                root: root.clone(),
                path: media_root.clone(),
                parent_node_id: None,
            }],
            &control,
            &crate::db::default_video_extensions(),
        );
        assert_eq!(control.progress().status, ScanStatus::Completed);
        assert_eq!(control.progress().videos_found, 16);
        assert_eq!(
            fs::read_to_string(media_root.join("not-media.txt")).unwrap(),
            "keep me"
        );

        let connection = database.connect().unwrap();
        let kind: String = connection
            .query_row(
                "SELECT node_type FROM nodes WHERE folder_name='混合'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(kind, "MIXED");
        let bdmv_kind: String = connection
            .query_row(
                "SELECT node_type FROM nodes WHERE folder_name='蓝光作品'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(bdmv_kind, "AUTO_WORK");
        let bdmv_nodes: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE folder_name='BDMV'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(bdmv_nodes, 0);
    }

    #[test]
    fn bdmv_resources_attach_to_parent_without_nodes_or_duplicate_videos() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("蓝光库");
        let work = root_path.join("蓝光作品");
        let bdmv = work.join("BDMV");
        let stream = bdmv.join("STREAM");
        let clipinf = bdmv.join("CLIPINF");
        let playlist = bdmv.join("PLAYLIST");
        let artwork = bdmv.join("META").join("DL");
        for directory in [&stream, &clipinf, &playlist, &artwork] {
            fs::create_dir_all(directory).unwrap();
        }

        let movie = stream.join("00000.m2ts");
        let index = bdmv.join("index.bdmv");
        let clip = clipinf.join("00000.clpi");
        let play_list = playlist.join("00000.mpls");
        let cover = artwork.join("封面.jpg");
        let extensionless = bdmv.join("README");
        fs::write(&movie, b"video").unwrap();
        fs::write(&index, b"index").unwrap();
        fs::write(&clip, b"clip info").unwrap();
        fs::write(&play_list, b"playlist").unwrap();
        fs::write(&cover, b"cover").unwrap();
        fs::write(&extensionless, b"notes").unwrap();

        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let run = |scan_id: &str| {
            database.start_scan_run(scan_id, root.id).unwrap();
            let control = scan_control(scan_id, &root);
            run_scan(
                None,
                &database,
                vec![ScanTarget {
                    root: root.clone(),
                    path: root_path.clone(),
                    parent_node_id: None,
                }],
                &control,
                &crate::db::default_video_extensions(),
            );
            assert_eq!(control.progress().status, ScanStatus::Completed);
            assert_eq!(control.progress().videos_found, 1);
        };
        run("bdmv-resources-first");

        let connection = database.connect().unwrap();
        let work_id: i64 = connection
            .query_row(
                "SELECT id FROM nodes WHERE absolute_path=?1 COLLATE NOCASE",
                [work.to_string_lossy()],
                |row| row.get(0),
            )
            .unwrap();
        let transparent_node_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE absolute_path IN (?1,?2,?3,?4,?5)",
                params![
                    bdmv.to_string_lossy(),
                    stream.to_string_lossy(),
                    clipinf.to_string_lossy(),
                    playlist.to_string_lossy(),
                    artwork.to_string_lossy()
                ],
                |row| row.get(0),
            )
            .unwrap();
        drop(connection);
        assert_eq!(transparent_node_count, 0);

        let node = database.get_node(work_id).unwrap();
        assert_eq!(node.node_type, NodeType::AutoWork);
        assert_eq!(node.direct_video_count, 1);
        assert_eq!(node.child_media_branch_count, 0);
        assert_eq!(node.total_video_count, 1);

        let media = database.list_media(work_id).unwrap();
        assert_eq!(media.len(), 1);
        assert_eq!(Path::new(&media[0].absolute_path), movie);

        let resources = database.list_resources(work_id).unwrap();
        assert_eq!(resources.len(), 5);
        assert!(resources.iter().all(|file| file.node_id == work_id));
        assert!(resources
            .iter()
            .all(|file| !file.extension.eq_ignore_ascii_case("m2ts")));
        assert!(resources
            .iter()
            .any(|file| Path::new(&file.absolute_path) == index));
        assert!(resources.iter().any(|file| {
            Path::new(&file.absolute_path) == play_list
                && file.resource_type == ResourceType::Playlist
        }));
        assert!(resources
            .iter()
            .any(|file| Path::new(&file.absolute_path) == cover));
        assert!(resources
            .iter()
            .any(|file| Path::new(&file.absolute_path) == extensionless));

        fs::remove_file(&clip).unwrap();
        run("bdmv-resources-second");
        let resources = database.list_resources(work_id).unwrap();
        assert_eq!(resources.len(), 4);
        assert!(!resources
            .iter()
            .any(|file| Path::new(&file.absolute_path) == clip));
        assert!(movie.is_file());
        assert_eq!(fs::read(&index).unwrap(), b"index");
        let node = database.get_node(work_id).unwrap();
        assert_eq!(node.direct_video_count, 1);
        assert_eq!(node.child_media_branch_count, 0);
        assert_eq!(node.total_video_count, 1);
    }

    #[test]
    fn rescan_preserves_manual_type_and_only_removes_index_rows() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("root");
        let work = root_path.join("work");
        fs::create_dir_all(&work).unwrap();
        let source = work.join("01.mkv");
        fs::write(&source, b"immutable media").unwrap();
        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();

        let run_once = |id: &str| {
            database.start_scan_run(id, root.id).unwrap();
            let control = ScanControl {
                unchanged_directories: Default::default(),
                scan_id: id.to_string(),
                cancel: Arc::new(AtomicBool::new(false)),
                progress: Arc::new(Mutex::new(ScanProgress {
                    background: false,
                    library_changed: None,
                    scan_id: id.to_string(),
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
            };
            run_scan(
                None,
                &database,
                vec![ScanTarget {
                    root: root.clone(),
                    path: root_path.clone(),
                    parent_node_id: None,
                }],
                &control,
                &crate::db::default_video_extensions(),
            );
        };
        run_once("first");
        let connection = database.connect().unwrap();
        let node_id: i64 = connection
            .query_row("SELECT id FROM nodes WHERE folder_name='work'", [], |row| {
                row.get(0)
            })
            .unwrap();
        drop(connection);
        database
            .set_node_type(node_id, NodeType::Container)
            .unwrap();
        run_once("second");
        assert_eq!(
            database.get_node(node_id).unwrap().node_type,
            NodeType::Container
        );
        assert_eq!(fs::read(&source).unwrap(), b"immutable media");
        database.remove_root(root.id).unwrap();
        assert!(source.exists(), "deleting a library removes only its index");
    }

    #[test]
    fn scan_indexes_non_video_resources_without_changing_project_counts() {
        let temp = TempDir::new().unwrap();
        let root_path = temp.path().join("媒体库");
        let work = root_path.join("作品");
        let fonts = work.join("Fonts");
        let specials = work.join("SP");
        fs::create_dir_all(&fonts).unwrap();
        fs::create_dir_all(&specials).unwrap();

        fs::write(work.join("Movie.mkv"), b"video").unwrap();
        fs::write(work.join("Movie.ass"), b"subtitle").unwrap();
        fs::write(work.join("Movie.sup"), b"subtitle").unwrap();
        fs::write(work.join("cover.jpg"), b"image fixture").unwrap();
        fs::write(work.join("booklet.pdf"), b"document").unwrap();
        fs::write(work.join("data.xyzabc"), b"unknown").unwrap();
        fs::write(fonts.join("font1.ttf"), b"font").unwrap();
        fs::write(fonts.join("readme.txt"), b"readme").unwrap();
        fs::write(specials.join("SP01.mkv"), b"special video").unwrap();
        fs::write(specials.join("SP01.ass"), b"special subtitle").unwrap();

        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root = database.add_root(&root_path, None).unwrap();
        let run = |scan_id: &str| {
            database.start_scan_run(scan_id, root.id).unwrap();
            let control = scan_control(scan_id, &root);
            run_scan(
                None,
                &database,
                vec![ScanTarget {
                    root: root.clone(),
                    path: root_path.clone(),
                    parent_node_id: None,
                }],
                &control,
                &crate::db::default_video_extensions(),
            );
            assert_eq!(control.progress().status, ScanStatus::Completed);
        };
        run("resources-first");

        let connection = database.connect().unwrap();
        let work_id: i64 = connection
            .query_row(
                "SELECT id FROM nodes WHERE folder_name='作品'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let fonts_id: i64 = connection
            .query_row(
                "SELECT id FROM nodes WHERE folder_name='Fonts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let specials_id: i64 = connection
            .query_row("SELECT id FROM nodes WHERE folder_name='SP'", [], |row| {
                row.get(0)
            })
            .unwrap();
        drop(connection);

        let direct = database.list_resources(work_id).unwrap();
        assert_eq!(direct.len(), 5);
        assert!(direct.iter().any(|file| {
            file.file_name == "Movie.ass" && file.resource_type == ResourceType::Subtitle
        }));
        assert!(direct.iter().any(|file| {
            file.file_name == "cover.jpg" && file.resource_type == ResourceType::Image
        }));
        assert!(direct.iter().any(|file| {
            file.file_name == "booklet.pdf" && file.resource_type == ResourceType::Document
        }));
        assert!(direct.iter().any(|file| {
            file.file_name == "data.xyzabc" && file.resource_type == ResourceType::Other
        }));
        assert_eq!(database.list_resources(fonts_id).unwrap().len(), 2);
        assert_eq!(database.list_resources(specials_id).unwrap().len(), 1);

        let refreshed_root = database.get_root(root.id).unwrap();
        assert_eq!(refreshed_root.node_count, Some(1));
        let all = database.list_all_resources().unwrap();
        assert_eq!(all.total_count, 1);
        assert_eq!(all.nodes[0].id, work_id);
        assert_eq!(all.nodes[0].library_root_id, root.id);

        fs::remove_file(work.join("data.xyzabc")).unwrap();
        run("resources-second");
        assert!(!database
            .list_resources(work_id)
            .unwrap()
            .iter()
            .any(|file| file.file_name == "data.xyzabc"));
        assert!(work.join("booklet.pdf").is_file());
    }

    #[test]
    fn resource_type_mapping_keeps_unknown_and_extensionless_files() {
        assert_eq!(resource_type_for_extension("ass"), ResourceType::Subtitle);
        assert_eq!(resource_type_for_extension("FLAC"), ResourceType::Audio);
        assert_eq!(resource_type_for_extension("7z"), ResourceType::Archive);
        assert_eq!(resource_type_for_extension("xyzabc"), ResourceType::Other);
        assert_eq!(resource_type_for_extension(""), ResourceType::Other);
    }

    #[test]
    fn all_resources_unifies_two_roots_without_merging_same_named_projects() {
        let temp = TempDir::new().unwrap();
        let root_a_path = temp.path().join("动画库");
        let root_b_path = temp.path().join("电影库");
        let work_a = root_a_path.join("同名作品");
        let work_b = root_b_path.join("同名作品");
        fs::create_dir_all(&work_a).unwrap();
        fs::create_dir_all(&work_b).unwrap();
        fs::write(work_a.join("01.mkv"), b"a").unwrap();
        fs::write(work_b.join("01.mkv"), b"b").unwrap();

        let database = Database::new(temp.path().join("test.db"));
        database.migrate().unwrap();
        let root_a = database.add_root(&root_a_path, None).unwrap();
        let root_b = database.add_root(&root_b_path, None).unwrap();
        database.start_scan_run("two-roots", root_a.id).unwrap();
        database.start_scan_run("two-roots", root_b.id).unwrap();
        let control = scan_control("two-roots", &root_a);
        run_scan(
            None,
            &database,
            vec![
                ScanTarget {
                    root: root_a.clone(),
                    path: root_a_path.clone(),
                    parent_node_id: None,
                },
                ScanTarget {
                    root: root_b.clone(),
                    path: root_b_path.clone(),
                    parent_node_id: None,
                },
            ],
            &control,
            &crate::db::default_video_extensions(),
        );
        assert_eq!(control.progress().status, ScanStatus::Completed);

        let all = database.list_all_resources().unwrap();
        assert_eq!(all.total_count, 2);
        assert_eq!(
            all.nodes
                .iter()
                .map(|node| node.library_root_id)
                .collect::<HashSet<_>>(),
            HashSet::from([root_a.id, root_b.id])
        );
        assert!(all.nodes.iter().all(|node| node.folder_name == "同名作品"));
        assert_eq!(database.get_root(root_a.id).unwrap().node_count, Some(1));
        assert_eq!(database.get_root(root_b.id).unwrap().node_count, Some(1));

        let renamed = database
            .update_root_display_name(root_a.id, "动画收藏")
            .unwrap();
        assert_eq!(renamed.display_name, "动画收藏");
        assert_eq!(renamed.path, root_a.path);
        assert!(root_a_path.is_dir());
    }
}

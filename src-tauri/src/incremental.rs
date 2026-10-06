//! Startup snapshots contain directory-entry metadata only, never media contents.
use crate::{
    db::{AppResult, Database},
    models::LibraryRecognitionMode,
    scanner::{ScanControl, ScanTarget},
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::UNIX_EPOCH,
};

#[derive(Serialize, Deserialize)]
struct Snapshot {
    configuration: String,
    directories: BTreeMap<PathBuf, String>,
}

#[derive(Default)]
pub(crate) struct Plan {
    pub targets: Vec<ScanTarget>,
    pub unchanged: HashSet<PathBuf>,
    snapshots: Vec<(i64, String)>,
    pub failed_roots: BTreeMap<i64, (bool, String)>,
}

// Fault injection is thread-local and compiled only into the synthetic regression harness.
#[cfg(test)]
thread_local! {
    static FAILED_DIRECTORY: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) struct DirectoryReadFailure(Option<PathBuf>);

#[cfg(test)]
impl Drop for DirectoryReadFailure {
    fn drop(&mut self) {
        FAILED_DIRECTORY.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}

#[cfg(test)]
pub(crate) fn fail_directory_read(path: PathBuf) -> DirectoryReadFailure {
    DirectoryReadFailure(FAILED_DIRECTORY.with(|slot| slot.replace(Some(path))))
}

fn cancelled(control: &ScanControl) -> AppResult<()> {
    if control.cancel.load(Ordering::Relaxed) {
        Err("扫描已停止。".into())
    } else {
        Ok(())
    }
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn inventory(
    root: &Path,
    ignored: &[PathBuf],
    control: &ScanControl,
) -> AppResult<BTreeMap<PathBuf, String>> {
    let canonical_root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let mut directories = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        cancelled(control)?;
        if ignored.iter().any(|ignored| path.starts_with(ignored)) {
            continue;
        }
        let canonical = fs::canonicalize(&path).map_err(|e| e.to_string())?;
        if !canonical.starts_with(&canonical_root) {
            return Err("目录超出资源库范围。".into());
        }
        let mut entries = Vec::new();
        #[cfg(test)]
        if FAILED_DIRECTORY.with(|slot| slot.borrow().as_ref() == Some(&path)) {
            return Err("Synthetic deep-directory read failure".into());
        }
        for entry in fs::read_dir(&canonical).map_err(|e| e.to_string())? {
            cancelled(control)?;
            let entry = entry.map_err(|e| e.to_string())?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if is_link(&metadata) {
                continue;
            }
            let logical = path.join(entry.file_name());
            if metadata.is_dir() {
                entries.push((entry.file_name().to_string_lossy().into_owned(), true, 0, 0));
                pending.push(logical);
            } else if metadata.is_file() {
                let modified = metadata
                    .modified()
                    .map_err(|e| e.to_string())?
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos();
                entries.push((
                    entry.file_name().to_string_lossy().into_owned(),
                    false,
                    metadata.len(),
                    modified,
                ));
            }
        }
        entries.sort();
        let bytes = serde_json::to_vec(&entries).map_err(|e| e.to_string())?;
        directories.insert(path, format!("{:x}", Sha256::digest(bytes)));
    }
    Ok(directories)
}

fn prepare_root(
    database: &Database,
    target: &ScanTarget,
    control: &ScanControl,
    extensions: &HashSet<String>,
) -> AppResult<Plan> {
    database.validate_scan_root(&target.root)?;
    let connection = database.connect()?;
    let mut statement = connection
        .prepare(
            "SELECT absolute_path,parent_node_id,node_type FROM nodes WHERE library_root_id=?1",
        )
        .map_err(|e| e.to_string())?;
    let nodes = statement
        .query_map([target.root.id], |row| {
            Ok((
                PathBuf::from(row.get::<_, String>(0)?),
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut ignored = nodes
        .iter()
        .filter(|node| node.2 == "IGNORED")
        .map(|node| node.0.clone())
        .collect::<Vec<_>>();
    ignored.sort();
    let mut extensions = extensions.iter().collect::<Vec<_>>();
    extensions.sort();
    let configuration =
        serde_json::to_string(&(1, &target.root.recognition_mode, extensions, &ignored))
            .map_err(|e| e.to_string())?;
    let previous = connection
        .query_row(
            "SELECT snapshot_json FROM library_scan_snapshots WHERE library_root_id=?1",
            [target.root.id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .and_then(|json| serde_json::from_str::<Snapshot>(&json).ok());
    let current = Snapshot {
        configuration,
        directories: inventory(Path::new(&target.root.path), &ignored, control)?,
    };
    let previous = previous.filter(|old| old.configuration == current.configuration);
    let dirty = if let Some(old) = &previous {
        old.directories
            .keys()
            .chain(current.directories.keys())
            .filter(|path| old.directories.get(*path) != current.directories.get(*path))
            .cloned()
            .collect::<HashSet<_>>()
    } else {
        HashSet::from([PathBuf::from(&target.root.path)])
    };
    if dirty.is_empty() {
        return Ok(Plan::default());
    }
    let mut plan = Plan::default();
    // A subtree is reusable only if neither it nor any descendant changed.
    if previous.is_some() {
        let affected = dirty
            .iter()
            .flat_map(|path| path.ancestors().map(Path::to_path_buf))
            .collect::<HashSet<_>>();
        plan.unchanged = current
            .directories
            .keys()
            .filter(|path| !affected.contains(*path))
            .cloned()
            .collect();
        let unchanged = plan.unchanged;
        plan.unchanged = unchanged
            .iter()
            .filter(|path| {
                !path
                    .ancestors()
                    .skip(1)
                    .any(|ancestor| unchanged.contains(ancestor))
            })
            .cloned()
            .collect();
    }
    if previous.is_none()
        || matches!(
            target.root.recognition_mode,
            LibraryRecognitionMode::VideoFile
        )
    {
        // File mode owns flat Nodes at the root, including BDMV: retain root-scoped cleanup
        // while its walker reuses the unchanged physical subtrees.
        plan.targets.push(target.clone());
    } else {
        let mut selected = BTreeMap::new();
        let indexed = nodes
            .iter()
            .map(|node| (node.0.clone(), node.1))
            .collect::<BTreeMap<_, _>>();
        for changed in dirty {
            let owner = changed.ancestors().find(|ancestor| {
                current.directories.contains_key(*ancestor) && indexed.contains_key(*ancestor)
            });
            if let Some(owner) = owner {
                selected.insert(owner.to_path_buf(), indexed[owner]);
            } else {
                selected.insert(target.path.clone(), target.parent_node_id);
            }
        }
        for (path, parent_node_id) in &selected {
            if path
                .ancestors()
                .skip(1)
                .any(|ancestor| selected.contains_key(ancestor))
            {
                continue;
            }
            plan.targets.push(ScanTarget {
                root: target.root.clone(),
                path: path.clone(),
                parent_node_id: *parent_node_id,
            });
        }
    }
    plan.snapshots.push((
        target.root.id,
        serde_json::to_string(&current).map_err(|e| e.to_string())?,
    ));
    Ok(plan)
}

pub(crate) fn prepare(
    database: &Database,
    targets: &[ScanTarget],
    control: &ScanControl,
    extensions: &HashSet<String>,
) -> AppResult<Plan> {
    let mut result = Plan::default();
    for target in targets {
        cancelled(control)?;
        match prepare_root(database, target, control, extensions) {
            Ok(plan) => {
                result.targets.extend(plan.targets);
                result.unchanged.extend(plan.unchanged);
                result.snapshots.extend(plan.snapshots);
            }
            Err(error) => {
                cancelled(control)?;
                let mut progress = control.progress.lock().unwrap_or_else(|e| e.into_inner());
                progress.errors += 1;
                progress.message = Some(format!("增量检查失败，保留已有索引：{error}"));
                let partial = fs::read_dir(&target.root.path).is_ok();
                result.failed_roots.insert(target.root.id, (partial, error));
            }
        }
    }
    Ok(result)
}

impl Plan {
    pub fn discard_failed_snapshots(&mut self, failed: &HashSet<i64>) {
        self.snapshots.retain(|(id, _)| !failed.contains(id));
    }
    pub fn save(&self, database: &Database) -> AppResult<()> {
        if self.snapshots.is_empty() {
            return Ok(());
        }
        let mut connection = database.connect()?;
        let transaction = connection.transaction().map_err(|e| e.to_string())?;
        for (root_id, json) in &self.snapshots {
            transaction.execute("INSERT INTO library_scan_snapshots(library_root_id,snapshot_json) VALUES(?1,?2) ON CONFLICT(library_root_id) DO UPDATE SET snapshot_json=excluded.snapshot_json", params![root_id, json]).map_err(|e| e.to_string())?;
        }
        transaction.commit().map_err(|e| e.to_string())
    }
}

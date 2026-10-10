use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use rusqlite::{Connection, OpenFlags};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::ZipArchive;

use crate::{
    cache,
    db::{Database, DatabaseUpdateBarrier},
    models::UpdateDistribution,
    update::{
        self, DownloadedUpdate, APP_ID, MAX_ARTIFACT_BYTES, PORTABLE_MARKER_FILE,
        PORTABLE_PLATFORM, UPDATER_FILE,
    },
};

const REQUEST_SCHEMA_VERSION: u32 = 1;
const MARKER_SCHEMA_VERSION: u32 = 1;
const APPLY_TIMEOUT_SECONDS: u64 = 120;
const PARENT_EXIT_TIMEOUT: Duration = Duration::from_secs(120);
const HELPER_READY_TIMEOUT: Duration = Duration::from_secs(30);
const POST_HEALTH_SURVIVAL_GRACE: Duration = Duration::from_secs(3);
const CHILD_TERMINATION_TIMEOUT: Duration = Duration::from_secs(5);
const UPDATE_MUTEX_NAME: &str = "Local\\M2Shelf-app.morimediashelf.desktop-update-v1";
const MAX_REQUEST_BYTES: u64 = 64 * 1024;
const MAX_ARCHIVE_FILES: usize = 16;
const MAX_EXTRACTED_BYTES: u64 = 640 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DATABASE_BACKUP_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const REQUIRED_PAYLOAD_FILES: [&str; 8] = [
    "M2Shelf.exe",
    UPDATER_FILE,
    PORTABLE_MARKER_FILE,
    "README_zh-CN.txt",
    "SHA256SUMS.txt",
    "M2ShelfMobi.exe",
    "M2ShelfMobi-source.zip",
    "THIRD-PARTY-NOTICES.txt",
];
const ALLOWED_PAYLOAD_FILES: [&str; 8] = [
    "M2Shelf.exe",
    UPDATER_FILE,
    PORTABLE_MARKER_FILE,
    "README_zh-CN.txt",
    "SHA256SUMS.txt",
    "M2ShelfMobi.exe",
    "M2ShelfMobi-source.zip",
    "THIRD-PARTY-NOTICES.txt",
];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableMarker {
    pub schema_version: u32,
    pub app_id: String,
    pub distribution: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyRequest {
    schema_version: u32,
    transaction_id: String,
    parent_pid: u32,
    install_directory: PathBuf,
    archive_path: PathBuf,
    archive_size: u64,
    archive_sha256: String,
    archive_signature: String,
    expected_version: String,
    platform: String,
    database_path: PathBuf,
    database_backup_path: PathBuf,
    database_backup_size: u64,
    database_backup_sha256: String,
    staging_directory: PathBuf,
    backup_directory: PathBuf,
    health_marker_path: PathBuf,
    helper_ready_path: PathBuf,
    timeout_seconds: u64,
}

pub struct PreparedPortableUpdate {
    pub helper_path: PathBuf,
    pub request_path: PathBuf,
    helper_ready_path: PathBuf,
    expected_version: String,
    // Acquired before the rollback snapshot and deliberately retained through process exit.
    _database_update_barrier: DatabaseUpdateBarrier,
    abort_cleanup: PreparedTransactionCleanup,
}

struct PreparedTransactionCleanup {
    update_cache: PathBuf,
    transaction_id: String,
    library_roots: Vec<PathBuf>,
    armed: bool,
}

impl PreparedTransactionCleanup {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PreparedTransactionCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Err(error) = cleanup_pre_ready_transaction(
            &self.update_cache,
            &self.transaction_id,
            &self.library_roots,
        ) {
            eprintln!("failed to clean aborted Portable update transaction: {error}");
            if let Err(state_error) = write_aborted_transaction_state(
                &self.update_cache,
                &self.transaction_id,
                &self.library_roots,
                &error,
            ) {
                eprintln!("failed to mark aborted Portable update transaction: {state_error}");
            }
        }
    }
}

#[derive(Debug)]
struct InstalledFile {
    target: PathBuf,
    backup: PathBuf,
    had_original: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ApplyPhase {
    Ready,
    ArchiveSealed,
    ParentExited,
    Staged,
    Replacing,
    Launched,
    Completed,
    RolledBack,
    RollbackFailed,
    Aborted,
}

impl ApplyPhase {
    fn is_non_terminal(self) -> bool {
        matches!(
            self,
            Self::Ready
                | Self::ArchiveSealed
                | Self::ParentExited
                | Self::Staged
                | Self::Replacing
                | Self::Launched
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TransactionState {
    schema_version: u32,
    transaction_id: String,
    phase: ApplyPhase,
    updated_at: String,
    error: Option<String>,
    recovery_material_preserved: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RollbackNotice {
    schema_version: u32,
    version: String,
    occurred_at: String,
    outcome: RecoveryOutcome,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RecoveryOutcome {
    RolledBack,
    RecoveryRequired,
}

#[cfg(windows)]
struct UpdateMutexGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for UpdateMutexGuard {
    fn drop(&mut self) {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::ReleaseMutex};
        // SAFETY: this guard owns both the wait acquisition and handle returned by CreateMutexW.
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

#[cfg(not(windows))]
struct UpdateMutexGuard;

impl RecoveryOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::RolledBack => "ROLLED_BACK",
            Self::RecoveryRequired => "RECOVERY_REQUIRED",
        }
    }
}

pub fn detect_distribution() -> Result<UpdateDistribution, String> {
    let executable =
        std::env::current_exe().map_err(|error| format!("无法确定当前程序位置：{error}"))?;
    let install_directory = executable
        .parent()
        .ok_or_else(|| "当前程序没有父目录。".to_string())?;
    let marker = install_directory.join(PORTABLE_MARKER_FILE);
    if !marker.exists() {
        return Ok(UpdateDistribution::Nsis);
    }
    read_and_validate_marker(&marker)?;
    Ok(UpdateDistribution::Portable)
}

/// Parses the canonical update transaction argument before normal single-instance startup.
/// A syntactically valid transaction child is the only app process allowed to bypass the update
/// mutex because the helper must launch it while continuing to own that mutex.
pub(crate) fn current_update_transaction_id() -> Result<Option<String>, String> {
    update_transaction_arg_from(&std::env::args_os().collect::<Vec<_>>())
}

/// Authenticates the exceptional startup path used only by the child launched from the trusted
/// Portable helper. A syntactically valid UUID is not sufficient: it must name the exact active
/// transaction for this executable and compiled version in this application's update cache.
pub(crate) fn authenticate_update_transaction_before_mutex(
    transaction_id: &str,
) -> Result<(), String> {
    let update_cache = startup_update_cache()?;
    let current_executable =
        std::env::current_exe().map_err(|error| format!("无法确定更新子进程位置：{error}"))?;
    authenticate_update_transaction(
        &update_cache,
        transaction_id,
        env!("CARGO_PKG_VERSION"),
        &current_executable,
    )
}

fn authenticate_update_transaction(
    update_cache: &Path,
    transaction_id: &str,
    current_version: &str,
    current_executable: &Path,
) -> Result<(), String> {
    let parsed = Uuid::parse_str(transaction_id)
        .map_err(|_| "Portable 更新启动事务 ID 无效。".to_string())?;
    if parsed.to_string() != transaction_id {
        return Err("Portable 更新启动事务 ID 必须使用规范格式。".into());
    }
    let transaction_directory = update::validate_existing_update_subdirectory(
        update_cache,
        &["transactions", transaction_id],
        &[],
    )?;
    let request_path = transaction_directory.join("apply-request.json");
    let request: ApplyRequest = read_json_bounded(&request_path, MAX_REQUEST_BYTES)?;
    validate_request_structure(&request_path, &request)?;
    reject_request_paths_from_database(&request_path, &request)?;
    if request.expected_version != current_version {
        return Err("Portable 更新启动事务版本与当前程序不一致。".into());
    }
    ensure_plain_file(current_executable, "Portable 更新子进程")?;
    let expected_executable = request.install_directory.join("M2Shelf.exe");
    ensure_plain_file(&expected_executable, "Portable 更新目标程序")?;
    if !cache::is_equal_or_within_checked(current_executable, &expected_executable)?
        || !cache::is_equal_or_within_checked(&expected_executable, current_executable)?
    {
        return Err("Portable 更新启动事务不属于当前可执行文件。".into());
    }

    let state: TransactionState = read_json_bounded(
        &transaction_directory.join("transaction-state.json"),
        MAX_REQUEST_BYTES,
    )?;
    if state.schema_version != REQUEST_SCHEMA_VERSION
        || state.transaction_id != transaction_id
        || state.phase != ApplyPhase::Launched
        || !state.recovery_material_preserved
        || chrono::DateTime::parse_from_rfc3339(&state.updated_at).is_err()
    {
        return Err("Portable 更新启动事务不处于可信的新版启动阶段。".into());
    }
    ensure_plain_file(&request.helper_ready_path, "Portable 更新 helper 就绪回执")?;
    let helper_ready = fs::read_to_string(&request.helper_ready_path)
        .map_err(|error| format!("无法读取 Portable 更新 helper 就绪回执：{error}"))?;
    if helper_ready != current_version {
        return Err("Portable 更新 helper 就绪回执版本无效。".into());
    }
    Ok(())
}

#[cfg(windows)]
fn startup_update_cache() -> Result<PathBuf, String> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr};
    use windows_sys::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath},
    };

    let mut raw_path = ptr::null_mut();
    // SAFETY: SHGetKnownFolderPath initializes an allocated, NUL-terminated UTF-16 string on
    // success. It is copied before being released with the paired COM task allocator.
    let result = unsafe {
        SHGetKnownFolderPath(&FOLDERID_RoamingAppData, 0, ptr::null_mut(), &mut raw_path)
    };
    if result < 0 || raw_path.is_null() {
        if !raw_path.is_null() {
            // SAFETY: even on failure, release a non-null buffer returned by the shell API with
            // its documented paired allocator.
            unsafe { CoTaskMemFree(raw_path.cast()) };
        }
        return Err(format!(
            "无法确定 Portable 更新启动缓存目录：HRESULT {result:#010x}"
        ));
    }
    let mut length = 0_usize;
    // SAFETY: raw_path is a valid NUL-terminated buffer returned above.
    unsafe {
        while *raw_path.add(length) != 0 {
            length += 1;
        }
    }
    // SAFETY: the slice is bounded by the NUL scan over the system-owned UTF-16 buffer.
    let path = unsafe { OsString::from_wide(std::slice::from_raw_parts(raw_path, length)) };
    // SAFETY: raw_path was allocated by SHGetKnownFolderPath.
    unsafe { CoTaskMemFree(raw_path.cast()) };
    Ok(PathBuf::from(path).join(APP_ID).join("updates"))
}

#[cfg(not(windows))]
fn startup_update_cache() -> Result<PathBuf, String> {
    Err("Portable 更新事务启动只支持 Windows。".into())
}

/// Prevents an ordinary manual launch from entering the install/database directories while the
/// Portable helper is replacing or rolling back files. The acquired mutex is released before the
/// ordinary application takes its normal single-instance guard.
pub(crate) fn wait_for_update_mutex_before_startup(
    active_transaction_id: Option<&str>,
) -> Result<(), String> {
    if active_transaction_id.is_some() {
        return Ok(());
    }
    drop(acquire_update_mutex()?);
    Ok(())
}

#[cfg(windows)]
fn acquire_update_mutex() -> Result<UpdateMutexGuard, String> {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0},
        System::Threading::{CreateMutexW, WaitForSingleObject, INFINITE},
    };
    let name = OsStr::new(UPDATE_MUTEX_NAME)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: the optional security descriptor is null and the name is NUL terminated.
    let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(format!(
            "无法创建 Portable 更新互斥锁：{}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: handle was returned by CreateMutexW and remains owned until the match below.
    let wait = unsafe { WaitForSingleObject(handle, INFINITE) };
    if wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED {
        Ok(UpdateMutexGuard(handle))
    } else {
        // SAFETY: the mutex was not acquired, but this process still owns the kernel handle.
        unsafe { CloseHandle(handle) };
        Err(format!(
            "等待 Portable 更新互斥锁失败：{}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(not(windows))]
fn acquire_update_mutex() -> Result<UpdateMutexGuard, String> {
    Ok(UpdateMutexGuard)
}

pub(crate) fn prepare_portable_update(
    downloaded: &DownloadedUpdate,
    database: &Database,
    update_cache: &Path,
) -> Result<PreparedPortableUpdate, String> {
    if downloaded.checked.distribution != UpdateDistribution::Portable
        || downloaded.checked.platform != PORTABLE_PLATFORM
    {
        return Err("下载包不是 Portable 更新。".into());
    }
    update::ensure_newer_version(env!("CARGO_PKG_VERSION"), &downloaded.checked.version)?;
    let library_roots = database
        .list_roots()?
        .into_iter()
        .map(|root| PathBuf::from(root.path))
        .collect::<Vec<_>>();
    update::ensure_safe_update_subdirectory(
        update_cache,
        &[downloaded.checked.version.as_str()],
        &library_roots,
    )?;
    update::ensure_safe_update_file(
        &downloaded.path,
        update_cache,
        &library_roots,
        "Portable 更新包",
    )?;
    update::verify_file_against_manifest(
        &downloaded.path,
        &downloaded.checked.version,
        PORTABLE_PLATFORM,
        downloaded.checked.asset.size,
        &downloaded.checked.asset.sha256,
        &downloaded.checked.asset.signature,
    )?;

    let executable =
        std::env::current_exe().map_err(|error| format!("无法确定当前程序位置：{error}"))?;
    let install_directory = executable
        .parent()
        .ok_or_else(|| "当前程序没有父目录。".to_string())?
        .to_path_buf();
    read_and_validate_marker(&install_directory.join(PORTABLE_MARKER_FILE))?;
    reject_library_root_overlap(&install_directory, library_roots.iter().cloned())?;

    let installed_helper = install_directory.join(UPDATER_FILE);
    ensure_plain_file(&installed_helper, "Portable 更新 helper")
        .map_err(|_| "Portable 目录缺少可信的 M2ShelfUpdater.exe；请手动更新一次。".to_string())?;

    let transaction_id = Uuid::new_v4().to_string();
    let transactions_directory =
        update::ensure_safe_update_subdirectory(update_cache, &["transactions"], &library_roots)?;
    let transaction_directory = transactions_directory.join(&transaction_id);
    fs::create_dir(&transaction_directory)
        .map_err(|error| format!("无法安全创建更新事务目录：{error}"))?;
    let abort_cleanup = PreparedTransactionCleanup {
        update_cache: update_cache.to_path_buf(),
        transaction_id: transaction_id.clone(),
        library_roots: library_roots.clone(),
        armed: true,
    };
    update::validate_existing_update_subdirectory(
        update_cache,
        &["transactions", transaction_id.as_str()],
        &library_roots,
    )?;
    let helper_path = transaction_directory.join(UPDATER_FILE);
    fs::copy(&installed_helper, &helper_path)
        .map_err(|error| format!("无法复制 Portable 更新 helper：{error}"))?;

    let database_backup_path = transaction_directory.join("database-backup.db");
    let database_update_barrier = database.backup_for_portable_update(&database_backup_path)?;
    let (database_backup_size, database_backup_digest) = hash_database_file(&database_backup_path)?;
    let request = ApplyRequest {
        schema_version: REQUEST_SCHEMA_VERSION,
        transaction_id: transaction_id.clone(),
        parent_pid: std::process::id(),
        install_directory: install_directory.clone(),
        archive_path: downloaded.path.clone(),
        archive_size: downloaded.checked.asset.size,
        archive_sha256: downloaded.checked.asset.sha256.clone(),
        archive_signature: downloaded.checked.asset.signature.clone(),
        expected_version: downloaded.checked.version.clone(),
        platform: PORTABLE_PLATFORM.into(),
        database_path: database.path().to_path_buf(),
        database_backup_path,
        database_backup_size,
        database_backup_sha256: update::encode_sha256(&database_backup_digest),
        staging_directory: install_directory
            .join(format!(".m2shelf-update-staging-{transaction_id}")),
        backup_directory: install_directory
            .join(format!(".m2shelf-update-backup-{transaction_id}")),
        health_marker_path: transaction_directory.join("healthy"),
        helper_ready_path: transaction_directory.join("helper-ready"),
        timeout_seconds: APPLY_TIMEOUT_SECONDS,
    };
    let request_path = transaction_directory.join("apply-request.json");
    write_json_create_new(&request_path, &request)?;
    write_transaction_state(&request, ApplyPhase::Ready, None, true)?;
    Ok(PreparedPortableUpdate {
        helper_path,
        request_path,
        helper_ready_path: request.helper_ready_path.clone(),
        expected_version: request.expected_version.clone(),
        _database_update_barrier: database_update_barrier,
        abort_cleanup,
    })
}

pub(crate) fn launch_prepared_helper(prepared: &mut PreparedPortableUpdate) -> Result<(), String> {
    let mut command = Command::new(&prepared.helper_path);
    command
        .arg("apply")
        .arg("--request")
        .arg(&prepared.request_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动 Portable 更新 helper：{error}"))?;
    let deadline = Instant::now() + HELPER_READY_TIMEOUT;
    while Instant::now() < deadline {
        if prepared.helper_ready_path.is_file() {
            let value = match fs::read_to_string(&prepared.helper_ready_path) {
                Ok(value) => value,
                Err(error) => {
                    return abort_helper_before_ready(
                        prepared,
                        &mut child,
                        format!("无法读取 Portable 更新 helper 就绪回执：{error}"),
                    );
                }
            };
            if value != prepared.expected_version {
                return abort_helper_before_ready(
                    prepared,
                    &mut child,
                    "Portable 更新 helper 就绪回执版本无效。".into(),
                );
            }
            match child.try_wait() {
                Ok(None) => {}
                Ok(Some(_)) => {
                    return Err("Portable 更新 helper 在提交就绪回执后意外退出。".into());
                }
                Err(error) => {
                    prepared.abort_cleanup.disarm();
                    return Err(format!(
                        "无法确认 Portable 更新 helper 就绪后的状态；已保留事务材料：{error}"
                    ));
                }
            }
            prepared.abort_cleanup.disarm();
            return Ok(());
        }
        match child.try_wait() {
            Ok(Some(_)) => {
                return Err("Portable 更新 helper 在准备完成前退出；当前版本保持运行。".into());
            }
            Ok(None) => {}
            Err(error) => {
                prepared.abort_cleanup.disarm();
                return Err(format!(
                    "无法确认 Portable 更新 helper 状态；已保留事务材料：{error}"
                ));
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    abort_helper_before_ready(
        prepared,
        &mut child,
        "等待 Portable 更新 helper 准备完成超时；当前版本保持运行。".into(),
    )
}

fn abort_helper_before_ready(
    prepared: &mut PreparedPortableUpdate,
    child: &mut Child,
    error: String,
) -> Result<(), String> {
    match terminate_child_for_rollback(child) {
        Ok(()) => Err(error),
        Err(termination_error) => {
            // A helper that might still hold files or continue toward apply must never race with
            // cleanup. Preserve the transaction so a later startup can report real uncertainty.
            prepared.abort_cleanup.disarm();
            Err(format!(
                "{error}；无法确认 helper 已终止，已保留事务材料：{termination_error}"
            ))
        }
    }
}

pub fn mark_running_update_healthy(
    update_cache: &Path,
    current_version: &str,
) -> Result<(), String> {
    let Some(transaction_id) = current_update_transaction_id()? else {
        return Ok(());
    };
    let request_path = update_cache
        .join("transactions")
        .join(&transaction_id)
        .join("apply-request.json");
    let request: ApplyRequest = read_json_bounded(&request_path, MAX_REQUEST_BYTES)?;
    validate_request_structure(&request_path, &request)?;
    reject_request_paths_from_database(&request_path, &request)?;
    if request.transaction_id != transaction_id {
        return Err("更新健康回执的事务或版本不匹配。".into());
    }
    ensure_running_version_matches(current_version, &request.expected_version)?;
    write_atomic_text_marker(&request.health_marker_path, current_version, "更新健康回执")
}

fn update_transaction_arg_from(args: &[std::ffi::OsString]) -> Result<Option<String>, String> {
    let mut found = None;
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--m2shelf-update-transaction" {
            if found.is_some() || index + 1 >= args.len() {
                return Err("更新事务启动参数无效。".into());
            }
            let value = args[index + 1]
                .to_str()
                .ok_or_else(|| "更新事务 ID 不是有效文本。".to_string())?;
            let parsed = Uuid::parse_str(value).map_err(|_| "更新事务 ID 无效。".to_string())?;
            if parsed.to_string() != value {
                return Err("更新事务 ID 必须使用规范格式。".into());
            }
            found = Some(value.to_owned());
            index += 2;
        } else {
            index += 1;
        }
    }
    Ok(found)
}

fn ensure_running_version_matches(current: &str, expected: &str) -> Result<(), String> {
    let parsed_current =
        Version::parse(current).map_err(|_| "当前程序版本不是有效的 SemVer。".to_string())?;
    let parsed_expected =
        Version::parse(expected).map_err(|_| "更新事务版本不是有效的 SemVer。".to_string())?;
    if parsed_current.to_string() != current || parsed_expected.to_string() != expected {
        return Err("更新健康回执版本必须使用规范 SemVer。".into());
    }
    if parsed_current != parsed_expected {
        return Err("更新健康回执版本与正在运行的程序不匹配。".into());
    }
    Ok(())
}

fn write_atomic_text_marker(path: &Path, value: &str, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            ensure_plain_file(path, label)?;
            let existing = fs::read_to_string(path)
                .map_err(|error| format!("无法读取已有{label}：{error}"))?;
            return if existing == value {
                Ok(())
            } else {
                Err(format!("已有{label}内容不匹配。"))
            };
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("无法检查已有{label}：{error}")),
    }
    let directory = path.parent().ok_or_else(|| format!("{label}目录无效。"))?;
    let temporary = directory.join(format!(".marker-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut marker = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("无法写入{label}：{error}"))?;
        marker
            .write_all(value.as_bytes())
            .and_then(|_| marker.sync_all())
            .map_err(|error| format!("无法同步{label}：{error}"))?;
        drop(marker);
        ensure_plain_replace_target_or_missing(path, label)?;
        atomic_replace(&temporary, path, None).map_err(|error| format!("无法提交{label}：{error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn run_apply_request(request_path: &Path) -> Result<(), String> {
    // Keep this guard for the entire helper lifetime, including verification, parent shutdown,
    // replacement, health observation, rollback, and recovery relaunch.
    let _update_mutex = acquire_update_mutex()?;
    if !request_path.is_absolute() {
        return Err("apply request 必须使用绝对路径。".into());
    }
    let request: ApplyRequest = read_json_bounded(request_path, MAX_REQUEST_BYTES)?;
    validate_request_structure(request_path, &request)?;
    reject_request_paths_from_database(request_path, &request)?;
    update::ensure_newer_version(env!("CARGO_PKG_VERSION"), &request.expected_version)?;

    // Complete all fallible request and package verification while the current app is still
    // running. The caller exits only after observing this atomic ready marker.
    let sealed_archive = seal_verified_archive(request_path, &request)?;
    write_transaction_state(&request, ApplyPhase::ArchiveSealed, None, true)?;
    write_atomic_text_marker(
        &request.helper_ready_path,
        &request.expected_version,
        "Portable 更新 helper 就绪回执",
    )?;

    if let Err(error) = wait_for_process_exit(request.parent_pid, PARENT_EXIT_TIMEOUT) {
        // The main process exits only after the helper-ready receipt. If waiting for that exit
        // fails, no payload or database mutation has started yet, but returning directly would
        // leave the user with a closed application and no rollback notice. Use the same recorded
        // recovery path as later apply failures so the old executable is relaunched explicitly.
        return recover_after_failed_apply(&request, None, &[], false, &error);
    }

    let mut installed = Vec::new();
    let mut child: Option<Child> = None;
    let mut database_may_have_changed = false;
    let apply_result: Result<(), String> = (|| {
        write_transaction_state(&request, ApplyPhase::ParentExited, None, true)?;

        fs::create_dir(&request.staging_directory)
            .map_err(|error| format!("无法创建同卷更新暂存目录：{error}"))?;
        ensure_plain_directory(&request.staging_directory)?;
        let extracted = extract_portable_zip_from_file(
            sealed_archive,
            &request.staging_directory,
            MAX_ARCHIVE_FILES,
            MAX_EXTRACTED_BYTES,
        )?;
        read_and_validate_marker(&request.staging_directory.join(PORTABLE_MARKER_FILE))?;
        verify_windows_product_version(
            &request.staging_directory.join("M2Shelf.exe"),
            &request.expected_version,
        )?;
        write_transaction_state(&request, ApplyPhase::Staged, None, true)?;

        fs::create_dir(&request.backup_directory)
            .map_err(|error| format!("无法创建同卷更新备份目录：{error}"))?;
        ensure_plain_directory(&request.backup_directory)?;
        write_transaction_state(&request, ApplyPhase::Replacing, None, true)?;
        replace_payload_files(&request, &extracted, &mut installed)?;

        // Persist the only phase accepted by the authenticated child before spawning it, avoiding
        // a race where the child starts faster than the helper can commit its state.
        write_transaction_state(&request, ApplyPhase::Launched, None, true)?;
        let launched = Command::new(request.install_directory.join("M2Shelf.exe"))
            .arg("--m2shelf-update-transaction")
            .arg(&request.transaction_id)
            .spawn()
            .map_err(|error| format!("新版 M²Shelf 无法启动：{error}"))?;
        child = Some(launched);
        database_may_have_changed = true;
        if !wait_for_health(&request, child.as_mut().expect("new process was stored"))? {
            return Err("新版未在超时前提交有效健康回执。".into());
        }
        Ok(())
    })();

    match apply_result {
        Ok(()) => {
            // Commit the terminal state before deleting rollback material. If power is lost in
            // the small cleanup window, startup sees a completed transaction (never an
            // indeterminate apply) and can safely resume the bounded cleanup.
            write_transaction_state(&request, ApplyPhase::Completed, None, true)?;
            cleanup_success_material(request_path, &request)?;
            write_transaction_state(&request, ApplyPhase::Completed, None, false)?;
            Ok(())
        }
        Err(error) => recover_after_failed_apply(
            &request,
            child.as_mut(),
            &installed,
            database_may_have_changed,
            &error,
        ),
    }
}

fn seal_verified_archive(request_path: &Path, request: &ApplyRequest) -> Result<File, String> {
    let mut source = open_locked_for_read(&request.archive_path)?;
    update::verify_open_file_against_manifest(
        &mut source,
        &request.expected_version,
        PORTABLE_PLATFORM,
        request.archive_size,
        &request.archive_sha256,
        &request.archive_signature,
    )?;
    source
        .seek(SeekFrom::Start(0))
        .map_err(|error| format!("无法重置已验证更新包：{error}"))?;

    let transaction_directory = request_path
        .parent()
        .ok_or_else(|| "更新事务文件没有父目录。".to_string())?;
    let sealed_path = transaction_directory.join("verified-payload.zip");
    let mut sealed = create_locked_for_read_write(&sealed_path)?;
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = source
            .read(&mut buffer)
            .map_err(|error| format!("无法读取已验证更新包：{error}"))?;
        if count == 0 {
            break;
        }
        copied = copied
            .checked_add(count as u64)
            .ok_or_else(|| "更新包副本大小溢出。".to_string())?;
        if copied > request.archive_size || copied > MAX_ARTIFACT_BYTES {
            return Err("更新包副本超过声明大小或安全上限。".into());
        }
        sealed
            .write_all(&buffer[..count])
            .map_err(|error| format!("无法写入密封更新包副本：{error}"))?;
    }
    if copied != request.archive_size {
        return Err("更新包在复制期间发生变化。".into());
    }
    sealed
        .sync_all()
        .map_err(|error| format!("无法同步密封更新包副本：{error}"))?;
    update::verify_open_file_against_manifest(
        &mut sealed,
        &request.expected_version,
        PORTABLE_PLATFORM,
        request.archive_size,
        &request.archive_sha256,
        &request.archive_signature,
    )?;
    sealed
        .seek(SeekFrom::Start(0))
        .map_err(|error| format!("无法重置密封更新包副本：{error}"))?;
    Ok(sealed)
}

#[cfg(windows)]
fn open_locked_for_read(path: &Path) -> Result<File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .map_err(|error| format!("无法锁定更新包进行验证：{error}"))
}

#[cfg(not(windows))]
fn open_locked_for_read(path: &Path) -> Result<File, String> {
    File::open(path).map_err(|error| format!("无法打开更新包：{error}"))
}

#[cfg(windows)]
fn create_locked_for_read_write(path: &Path) -> Result<File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .map_err(|error| format!("无法创建密封更新包副本：{error}"))
}

#[cfg(not(windows))]
fn create_locked_for_read_write(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("无法创建更新包副本：{error}"))
}

fn recover_after_failed_apply(
    request: &ApplyRequest,
    child: Option<&mut Child>,
    installed: &[InstalledFile],
    database_may_have_changed: bool,
    original_error: &str,
) -> Result<(), String> {
    recover_after_failed_apply_with_launcher(
        request,
        child,
        installed,
        database_may_have_changed,
        original_error,
        |executable| {
            Command::new(executable)
                .spawn()
                .map(|_| ())
                .map_err(|error| error.to_string())
        },
    )
}

fn recover_after_failed_apply_with_launcher(
    request: &ApplyRequest,
    child: Option<&mut Child>,
    installed: &[InstalledFile],
    database_may_have_changed: bool,
    original_error: &str,
    restart: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    if let Some(child) = child {
        if let Err(termination_error) = terminate_child_for_rollback(child) {
            let combined = format!("{original_error}；{termination_error}");
            let persistence_error = persist_recovery_outcome(
                request,
                ApplyPhase::RollbackFailed,
                RecoveryOutcome::RecoveryRequired,
                &combined,
            )
            .err();
            return Err(append_recovery_persistence_error(
                combined,
                persistence_error,
            ));
        }
    }
    let file_rollback = rollback_files(request, installed);
    let database_rollback = if database_may_have_changed {
        restore_database(request)
    } else {
        Ok(())
    };
    let rollback_error = match (file_rollback, database_rollback) {
        (Ok(()), Ok(())) => None,
        (Err(file_error), Ok(())) => Some(file_error),
        (Ok(()), Err(database_error)) => Some(database_error),
        (Err(file_error), Err(database_error)) => Some(format!("{file_error}；{database_error}")),
    };
    if let Some(error) = rollback_error {
        let combined = format!("{original_error}；{error}");
        let persistence_error = persist_recovery_outcome(
            request,
            ApplyPhase::RollbackFailed,
            RecoveryOutcome::RecoveryRequired,
            &combined,
        )
        .err();
        return Err(append_recovery_persistence_error(
            combined,
            persistence_error,
        ));
    }

    // Deliberately retain the backup directory, DB snapshot, request, and state for diagnostics
    // and manual recovery. Only completed transactions are eligible for bounded startup cleanup.
    let persistence_error = persist_recovery_outcome(
        request,
        ApplyPhase::RolledBack,
        RecoveryOutcome::RolledBack,
        original_error,
    )
    .err();
    restart(&request.install_directory.join("M2Shelf.exe")).map_err(|error| {
        format!("{original_error}；更新已回滚，但无法重新启动旧版 M²Shelf：{error}")
    })?;
    let state_note = persistence_error
        .map(|error| format!("；但无法完整记录回滚状态：{error}"))
        .unwrap_or_default();
    Err(format!(
        "{original_error}；更新已回滚并重新启动旧版 M²Shelf{state_note}。"
    ))
}

fn persist_recovery_outcome(
    request: &ApplyRequest,
    phase: ApplyPhase,
    outcome: RecoveryOutcome,
    error: &str,
) -> Result<(), String> {
    // The notice is the user-visible durable record. Write it before making the transaction
    // terminal so a failed notice write leaves a discoverable non-terminal transaction that
    // startup can conservatively surface instead of silently losing the recovery warning.
    write_rollback_notice(request, outcome)?;
    write_transaction_state(request, phase, Some(error), true)
}

fn append_recovery_persistence_error(message: String, persistence_error: Option<String>) -> String {
    persistence_error.map_or(message.clone(), |error| {
        format!("{message}；无法完整记录恢复状态：{error}")
    })
}

fn terminate_child_for_rollback(child: &mut Child) -> Result<(), String> {
    if child
        .try_wait()
        .map_err(|error| format!("无法检查新版进程退出状态：{error}"))?
        .is_some()
    {
        return Ok(());
    }
    if let Err(kill_error) = child.kill() {
        if child
            .try_wait()
            .map_err(|error| format!("无法在终止失败后检查新版进程：{error}"))?
            .is_none()
        {
            return Err(format!(
                "无法终止仍在运行的新版进程，已保留恢复材料且未回滚实时数据：{kill_error}"
            ));
        }
        return Ok(());
    }
    let deadline = Instant::now() + CHILD_TERMINATION_TIMEOUT;
    while Instant::now() < deadline {
        if child
            .try_wait()
            .map_err(|error| format!("无法确认新版进程已终止：{error}"))?
            .is_some()
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err("新版进程终止确认超时，已保留恢复材料且未回滚实时数据。".into())
}

fn write_rollback_notice(request: &ApplyRequest, outcome: RecoveryOutcome) -> Result<(), String> {
    let transaction_directory = request
        .health_marker_path
        .parent()
        .ok_or_else(|| "更新事务目录无效。".to_string())?;
    let update_cache = transaction_directory
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "更新缓存目录无效。".to_string())?;
    let destination = update_cache.join("last-rollback.json");
    let temporary = update_cache.join(format!(".last-rollback-{}.tmp", Uuid::new_v4()));
    let notice = RollbackNotice {
        schema_version: REQUEST_SCHEMA_VERSION,
        version: request.expected_version.clone(),
        occurred_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        outcome,
    };
    write_json_create_new(&temporary, &notice)?;
    ensure_plain_replace_target_or_missing(&destination, "更新回滚通知")?;
    atomic_replace(&temporary, &destination, None)
        .map_err(|error| format!("无法提交更新回滚通知：{error}"))
}

pub(crate) fn read_rollback_notice(update_cache: &Path) -> Result<Option<String>, String> {
    let path = update_cache.join("last-rollback.json");
    if !path.exists() {
        return Ok(None);
    }
    update::validate_existing_update_cache(update_cache, &[])?;
    ensure_plain_file(&path, "更新回滚通知")?;
    let notice: RollbackNotice = read_json_bounded(&path, MAX_REQUEST_BYTES)?;
    let valid_version =
        Version::parse(&notice.version).is_ok_and(|version| version.to_string() == notice.version);
    if notice.schema_version != REQUEST_SCHEMA_VERSION
        || !valid_version
        || chrono::DateTime::parse_from_rfc3339(&notice.occurred_at).is_err()
    {
        return Err("更新回滚通知无效。".into());
    }
    Ok(Some(notice.outcome.as_str().to_owned()))
}

pub(crate) fn clear_rollback_notice(update_cache: &Path) -> Result<(), String> {
    update::validate_existing_update_cache(update_cache, &[])?;
    let path = update_cache.join("last-rollback.json");
    if path.exists() {
        ensure_plain_file(&path, "更新回滚通知")?;
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法清理更新回滚通知：{error}")),
    }
}

fn write_transaction_state(
    request: &ApplyRequest,
    phase: ApplyPhase,
    error: Option<&str>,
    recovery_material_preserved: bool,
) -> Result<(), String> {
    let directory = request
        .health_marker_path
        .parent()
        .ok_or_else(|| "更新事务状态目录无效。".to_string())?;
    write_transaction_state_at(
        directory,
        &request.transaction_id,
        phase,
        error,
        recovery_material_preserved,
    )
}

fn write_transaction_state_at(
    directory: &Path,
    transaction_id: &str,
    phase: ApplyPhase,
    error: Option<&str>,
    recovery_material_preserved: bool,
) -> Result<(), String> {
    let state = TransactionState {
        schema_version: REQUEST_SCHEMA_VERSION,
        transaction_id: transaction_id.to_owned(),
        phase,
        updated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        error: error.map(str::to_owned),
        recovery_material_preserved,
    };
    let destination = directory.join("transaction-state.json");
    let temporary = directory.join(format!(".transaction-state-{}.tmp", Uuid::new_v4()));
    write_json_create_new(&temporary, &state)?;
    ensure_plain_replace_target_or_missing(&destination, "更新事务状态")?;
    atomic_replace(&temporary, &destination, None)
        .map_err(|error| format!("无法提交更新事务状态：{error}"))
}

fn write_aborted_transaction_state(
    update_cache: &Path,
    transaction_id: &str,
    library_roots: &[PathBuf],
    error: &str,
) -> Result<(), String> {
    let directory = update::validate_existing_update_subdirectory(
        update_cache,
        &["transactions", transaction_id],
        library_roots,
    )?;
    write_transaction_state_at(
        &directory,
        transaction_id,
        ApplyPhase::Aborted,
        Some(error),
        false,
    )
}

fn cleanup_pre_ready_transaction(
    update_cache: &Path,
    transaction_id: &str,
    library_roots: &[PathBuf],
) -> Result<(), String> {
    if Uuid::parse_str(transaction_id)
        .ok()
        .map(|uuid| uuid.to_string())
        != Some(transaction_id.to_owned())
    {
        return Err("中止更新事务 ID 无效。".into());
    }
    let directory =
        match fs::symlink_metadata(update_cache.join("transactions").join(transaction_id)) {
            Ok(_) => update::validate_existing_update_subdirectory(
                update_cache,
                &["transactions", transaction_id],
                library_roots,
            )?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("无法检查中止更新事务目录：{error}")),
        };
    let entries =
        fs::read_dir(&directory).map_err(|error| format!("无法枚举中止更新事务目录：{error}"))?;
    let mut files = Vec::new();
    for entry in entries.take(33) {
        let entry = entry.map_err(|error| format!("无法读取中止更新事务目录：{error}"))?;
        let name = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| "中止更新事务包含未知文件。".to_string())?;
        if !is_pre_ready_transaction_file_name(&name) {
            return Err(format!("中止更新事务包含未知文件 {name}；已保留全部材料。"));
        }
        ensure_plain_file(&entry.path(), "中止更新事务文件")?;
        files.push((name, entry.path()));
    }
    if files.len() > 32 {
        return Err("中止更新事务文件数量异常；已保留全部材料。".into());
    }
    // Remove the non-terminal state first. If this best-effort cleanup is interrupted, startup
    // will ignore the remaining orphan instead of reporting a recovery incident for an apply
    // that never passed helper-ready while the old process was still running.
    files.sort_by_key(|(name, _)| {
        if name == "transaction-state.json" {
            0
        } else {
            1
        }
    });
    for (_, file) in files {
        fs::remove_file(file).map_err(|error| format!("无法清理中止更新事务文件：{error}"))?;
    }
    fs::remove_dir(&directory).map_err(|error| format!("无法清理中止更新事务目录：{error}"))
}

fn is_pre_ready_transaction_file_name(name: &str) -> bool {
    if matches!(
        name,
        UPDATER_FILE
            | "database-backup.db"
            | "database-backup.db-journal"
            | "database-backup.db-wal"
            | "database-backup.db-shm"
            | "apply-request.json"
            | "transaction-state.json"
            | "verified-payload.zip"
            | "helper-ready"
            | "healthy"
    ) {
        return true;
    }
    [
        ".transaction-state-",
        ".helper-ready-",
        ".healthy-",
        ".marker-",
    ]
    .iter()
    .any(|prefix| {
        name.strip_prefix(prefix)
            .and_then(|value| value.strip_suffix(".tmp"))
            .is_some_and(|value| Uuid::parse_str(value).is_ok_and(|uuid| uuid.to_string() == value))
    })
}

fn ensure_plain_replace_target_or_missing(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(_) => ensure_plain_file(path, label),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法检查{label}目标：{error}")),
    }
}

fn cleanup_success_material(request_path: &Path, request: &ApplyRequest) -> Result<(), String> {
    for directory in [&request.backup_directory, &request.staging_directory] {
        match fs::symlink_metadata(directory) {
            Ok(_) => remove_flat_payload_directory(directory)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法检查已完成更新目录：{error}")),
        }
    }
    remove_plain_file_if_exists(&request.database_backup_path, "已完成更新的数据库快照")?;
    if let Some(directory) = request_path.parent() {
        ensure_plain_directory(directory)?;
        for file in [
            directory.join("verified-payload.zip"),
            request.health_marker_path.clone(),
            request.helper_ready_path.clone(),
        ] {
            remove_plain_file_if_exists(&file, "已完成更新文件")?;
        }
    }
    Ok(())
}

fn remove_plain_file_if_exists(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            ensure_plain_file(path, label)?;
            fs::remove_file(path).map_err(|error| format!("无法清理{label}：{error}"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法检查{label}：{error}")),
    }
}

fn remove_flat_payload_directory(directory: &Path) -> Result<(), String> {
    ensure_plain_directory(directory)?;
    let entries =
        fs::read_dir(directory).map_err(|error| format!("无法枚举已完成更新目录：{error}"))?;
    let mut files = Vec::new();
    for entry in entries.take(MAX_ARCHIVE_FILES + 1) {
        let entry = entry.map_err(|error| format!("无法读取已完成更新目录：{error}"))?;
        let name = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| "已完成更新目录包含未知文件。".to_string())?;
        if !ALLOWED_PAYLOAD_FILES.contains(&name.as_str()) {
            return Err("已完成更新目录包含非 M²Shelf 文件；已停止自动清理。".into());
        }
        ensure_plain_file(&entry.path(), "已完成更新文件")?;
        files.push(entry.path());
    }
    if files.len() > MAX_ARCHIVE_FILES {
        return Err("已完成更新目录文件数量异常；已停止自动清理。".into());
    }
    for file in files {
        fs::remove_file(file).map_err(|error| format!("无法清理已完成更新文件：{error}"))?;
    }
    fs::remove_dir(directory).map_err(|error| format!("无法清理已完成更新目录：{error}"))
}

/// Converts a strictly identified non-terminal transaction left by an abnormally terminated
/// helper into a persistent recovery-required state. This intentionally does not guess which
/// files were replaced: automatic recovery without the helper's in-memory InstalledFile journal
/// could destroy the only good copy. The current transaction child is excluded because it starts
/// normally while its helper still owns the named update mutex.
pub(crate) fn mark_interrupted_transactions_recovery_required(
    update_cache: &Path,
    active_transaction_id: Option<&str>,
) -> Result<bool, String> {
    let root = update_cache.join("transactions");
    match fs::symlink_metadata(&root) {
        Ok(_) => {
            update::validate_existing_update_subdirectory(update_cache, &["transactions"], &[])?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("无法检查中断的更新事务目录：{error}")),
    }
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("无法枚举中断的更新事务：{error}")),
    };
    let mut found = false;
    for entry in entries {
        let entry = entry.map_err(|error| format!("无法读取中断的更新事务：{error}"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if Uuid::parse_str(name).ok().map(|value| value.to_string()) != Some(name.to_owned()) {
            continue;
        }
        let transaction_directory = entry.path();
        if ensure_plain_directory(&transaction_directory).is_err() {
            continue;
        }
        let request_path = transaction_directory.join("apply-request.json");
        if ensure_plain_file(&request_path, "Portable 更新事务请求").is_err() {
            continue;
        }
        let request: ApplyRequest = match read_json_bounded(&request_path, MAX_REQUEST_BYTES) {
            Ok(request) => request,
            Err(_) => continue,
        };
        if validate_request_identity(&request_path, &request).is_err() {
            continue;
        }
        let state_path = transaction_directory.join("transaction-state.json");
        if ensure_plain_file(&state_path, "Portable 更新事务状态").is_err() {
            continue;
        }
        let state: TransactionState = match read_json_bounded(&state_path, MAX_REQUEST_BYTES) {
            Ok(state) => state,
            Err(_) => continue,
        };
        if state.schema_version != REQUEST_SCHEMA_VERSION
            || state.transaction_id != name
            || chrono::DateTime::parse_from_rfc3339(&state.updated_at).is_err()
            || !state.recovery_material_preserved
            || !state.phase.is_non_terminal()
        {
            continue;
        }
        if active_transaction_id == Some(name) {
            continue;
        }

        let error = format!(
            "Portable 更新 helper 在 {} 阶段异常中断；未执行不可靠的自动回滚。",
            format!("{:?}", state.phase).to_ascii_uppercase()
        );
        // Write the user-visible persistent notice first. If the subsequent state transition is
        // interrupted, the next startup will safely rediscover the still non-terminal state.
        write_rollback_notice(&request, RecoveryOutcome::RecoveryRequired)?;
        write_transaction_state(&request, ApplyPhase::RollbackFailed, Some(&error), true)?;
        found = true;
    }
    Ok(found)
}

/// Removes only explicitly completed, UUID-named updater transactions. Rolled-back and
/// rollback-failed transactions are retained with their recovery material.
pub fn cleanup_completed_transactions(
    update_cache: &Path,
    library_roots: &[PathBuf],
) -> Result<(), String> {
    let root = update_cache.join("transactions");
    match fs::symlink_metadata(&root) {
        Ok(_) => {
            update::validate_existing_update_subdirectory(update_cache, &["transactions"], &[])?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("无法检查旧更新事务目录：{error}")),
    }
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("无法枚举旧更新事务：{error}")),
    };
    for entry in entries.take(64) {
        let entry = entry.map_err(|error| format!("无法读取旧更新事务：{error}"))?;
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if Uuid::parse_str(name).ok().map(|value| value.to_string()) != Some(name.to_owned()) {
            continue;
        }
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| format!("无法检查旧更新事务：{error}"))?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        let state: TransactionState =
            match read_json_bounded(&path.join("transaction-state.json"), MAX_REQUEST_BYTES) {
                Ok(state) => state,
                Err(_) => continue,
            };
        if state.schema_version != REQUEST_SCHEMA_VERSION || state.transaction_id != name {
            continue;
        }
        if state.phase == ApplyPhase::Aborted {
            cleanup_pre_ready_transaction(update_cache, name, library_roots)?;
            continue;
        }
        if state.phase != ApplyPhase::Completed {
            continue;
        }
        if state.recovery_material_preserved {
            let request_path = path.join("apply-request.json");
            let request: ApplyRequest = match read_json_bounded(&request_path, MAX_REQUEST_BYTES) {
                Ok(request) => request,
                Err(_) => continue,
            };
            if validate_completed_cleanup(update_cache, library_roots, &request_path, &request)
                .is_err()
            {
                continue;
            }
            cleanup_success_material(&request_path, &request)?;
            write_transaction_state(&request, ApplyPhase::Completed, None, false)?;
        }
        // Once the terminal state confirms that recovery material has been removed, startup
        // deletes only this UUID child inside application data.
        update::validate_existing_update_subdirectory(update_cache, &["transactions", name], &[])?;
        remove_completed_transaction_directory(&path)?;
    }
    Ok(())
}

fn validate_completed_cleanup(
    update_cache: &Path,
    library_roots: &[PathBuf],
    request_path: &Path,
    request: &ApplyRequest,
) -> Result<(), String> {
    // A crash can occur after one or more recovery artifacts were already removed but before the
    // final `recovery_material_preserved = false` state was committed.  Resume cleanup from the
    // immutable, path-bound request identity instead of requiring the live database, original
    // archive, and snapshot to all still exist.  Every artifact that does remain is independently
    // checked below before it is removed.
    validate_request_identity(request_path, request)?;
    ensure_plain_file(request_path, "Portable 更新事务请求")?;
    let request_cache = request_update_cache(request)?;
    if !cache::is_equal_or_within_checked(&request_cache, update_cache)?
        || !cache::is_equal_or_within_checked(update_cache, &request_cache)?
    {
        return Err("已完成更新事务不属于当前应用更新缓存。".into());
    }
    reject_library_root_overlap(&request.install_directory, library_roots.iter().cloned())?;
    update::validate_existing_update_subdirectory(
        update_cache,
        &["transactions", request.transaction_id.as_str()],
        library_roots,
    )?;
    update::ensure_safe_update_file(
        request_path,
        update_cache,
        library_roots,
        "Portable 更新事务请求",
    )?;

    for directory in [&request.backup_directory, &request.staging_directory] {
        match fs::symlink_metadata(directory) {
            Ok(_) => {
                ensure_plain_directory(directory)?;
                if !cache::is_equal_or_within_checked(directory, &request.install_directory)? {
                    return Err("已完成更新清理目录不属于 Portable 安装目录。".into());
                }
                reject_library_root_overlap(directory, library_roots.iter().cloned())?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法检查已完成更新清理目录：{error}")),
        }
    }

    for (file, label) in [
        (&request.database_backup_path, "已完成更新数据库快照"),
        (&request.health_marker_path, "已完成更新健康回执"),
        (&request.helper_ready_path, "已完成更新 helper 回执"),
    ] {
        match fs::symlink_metadata(file) {
            Ok(_) => update::ensure_safe_update_file(file, update_cache, library_roots, label)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法检查{label}：{error}")),
        }
    }
    if let Some(directory) = request_path.parent() {
        let sealed = directory.join("verified-payload.zip");
        match fs::symlink_metadata(&sealed) {
            Ok(_) => update::ensure_safe_update_file(
                &sealed,
                update_cache,
                library_roots,
                "已完成更新密封包",
            )?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法检查已完成更新密封包：{error}")),
        }
    }
    Ok(())
}

fn remove_completed_transaction_directory(path: &Path) -> Result<(), String> {
    ensure_plain_directory(path)?;
    let allowed = [UPDATER_FILE, "apply-request.json", "transaction-state.json"];
    let entries =
        fs::read_dir(path).map_err(|error| format!("无法枚举已完成更新事务内容：{error}"))?;
    let mut files = Vec::new();
    for entry in entries.take(16) {
        let entry = entry.map_err(|error| format!("无法读取已完成更新事务内容：{error}"))?;
        let name = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| "已完成更新事务包含未知文件。".to_string())?;
        if !allowed.contains(&name.as_str()) {
            return Ok(());
        }
        ensure_plain_file(&entry.path(), "已完成更新事务文件")?;
        files.push(entry.path());
    }
    if files.len() > allowed.len() {
        return Ok(());
    }
    for file in files {
        fs::remove_file(file).map_err(|error| format!("无法清理已完成更新事务文件：{error}"))?;
    }
    fs::remove_dir(path).map_err(|error| format!("无法清理已完成更新事务目录：{error}"))
}

fn validate_request_structure(request_path: &Path, request: &ApplyRequest) -> Result<(), String> {
    validate_request_identity(request_path, request)?;
    ensure_plain_file(request_path, "Portable 更新事务请求")?;
    ensure_plain_directory(&request.install_directory)?;
    ensure_plain_file(&request.database_path, "M²Shelf 数据库")?;
    let update_cache = request_update_cache(request)?;
    update::validate_existing_update_subdirectory(
        &update_cache,
        &["transactions", request.transaction_id.as_str()],
        &[],
    )?;
    update::validate_existing_update_subdirectory(
        &update_cache,
        &[request.expected_version.as_str()],
        &[],
    )?;
    for (path, label) in [
        (request_path, "Portable 更新事务请求"),
        (&request.archive_path, "Portable 更新包"),
        (&request.database_backup_path, "Portable 更新数据库快照"),
    ] {
        update::ensure_safe_update_file(path, &update_cache, &[], label)?;
    }
    Ok(())
}

/// Validates immutable transaction identity and path ownership without requiring every recovery
/// artifact to remain present. Startup interruption detection uses this stricter structural layer
/// because a helper may have been terminated halfway through successful-material cleanup.
fn validate_request_identity(request_path: &Path, request: &ApplyRequest) -> Result<(), String> {
    if request.schema_version != REQUEST_SCHEMA_VERSION {
        return Err("不支持此 Portable 更新事务版本。".into());
    }
    let transaction_id = Uuid::parse_str(&request.transaction_id)
        .map_err(|_| "Portable 更新事务 ID 无效。".to_string())?;
    if transaction_id.to_string() != request.transaction_id {
        return Err("Portable 更新事务 ID 必须使用规范格式。".into());
    }
    if request.parent_pid == 0
        || request.platform != PORTABLE_PLATFORM
        || request.timeout_seconds != APPLY_TIMEOUT_SECONDS
    {
        return Err("Portable 更新事务参数无效。".into());
    }
    let parsed_version = Version::parse(&request.expected_version)
        .map_err(|_| "Portable 更新事务版本不是 SemVer。".to_string())?;
    if parsed_version.to_string() != request.expected_version
        || !parsed_version.pre.is_empty()
        || !parsed_version.build.is_empty()
    {
        return Err("Portable 更新事务版本必须使用规范 SemVer。".into());
    }
    if request.archive_size == 0
        || request.archive_size > MAX_ARTIFACT_BYTES
        || update::decode_sha256(&request.archive_sha256).is_err()
        || update::decode_signature(&request.archive_signature).is_err()
    {
        return Err("Portable 更新包大小无效。".into());
    }
    if request.database_backup_size == 0
        || request.database_backup_size > MAX_DATABASE_BACKUP_BYTES
        || update::decode_sha256(&request.database_backup_sha256).is_err()
    {
        return Err("Portable 更新数据库快照校验信息无效。".into());
    }
    for path in [
        request_path,
        &request.install_directory,
        &request.archive_path,
        &request.database_path,
        &request.database_backup_path,
        &request.staging_directory,
        &request.backup_directory,
        &request.health_marker_path,
        &request.helper_ready_path,
    ] {
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err("Portable 更新事务包含非绝对路径或路径穿越。".into());
        }
    }
    let database_parent = request
        .database_path
        .parent()
        .ok_or_else(|| "数据库路径没有父目录。".to_string())?;
    let expected_transaction = database_parent
        .join("updates")
        .join("transactions")
        .join(&request.transaction_id);
    let expected_archive = database_parent
        .join("updates")
        .join(&request.expected_version)
        .join(format!(
            "M2Shelf-Portable-{}-x64.zip",
            request.expected_version
        ));
    if request_path != expected_transaction.join("apply-request.json")
        || request.archive_path != expected_archive
        || request.database_backup_path != expected_transaction.join("database-backup.db")
        || request.health_marker_path != expected_transaction.join("healthy")
        || request.helper_ready_path != expected_transaction.join("helper-ready")
        || request.staging_directory
            != request.install_directory.join(format!(
                ".m2shelf-update-staging-{}",
                request.transaction_id
            ))
        || request.backup_directory
            != request
                .install_directory
                .join(format!(".m2shelf-update-backup-{}", request.transaction_id))
    {
        return Err("Portable 更新事务路径不属于预期的应用目录。".into());
    }
    read_and_validate_marker(&request.install_directory.join(PORTABLE_MARKER_FILE))?;
    Ok(())
}

fn read_and_validate_marker(path: &Path) -> Result<PortableMarker, String> {
    ensure_plain_file(path, "Portable 标记")?;
    let marker: PortableMarker = read_json_bounded(path, 4 * 1024)?;
    if marker
        != (PortableMarker {
            schema_version: MARKER_SCHEMA_VERSION,
            app_id: APP_ID.into(),
            distribution: "portable".into(),
        })
    {
        return Err("M2Shelf.portable.json 内容无效。".into());
    }
    Ok(marker)
}

fn reject_library_root_overlap<I>(install_directory: &Path, roots: I) -> Result<(), String>
where
    I: IntoIterator<Item = PathBuf>,
{
    for root in roots {
        if cache::paths_overlap_checked(install_directory, &root)? {
            return Err("Portable 程序目录不能等于、包含或位于任一媒体资源库内。".into());
        }
    }
    Ok(())
}

fn request_update_cache(request: &ApplyRequest) -> Result<PathBuf, String> {
    request
        .database_path
        .parent()
        .map(|parent| parent.join("updates"))
        .ok_or_else(|| "数据库路径没有父目录。".to_string())
}

fn reject_request_paths_from_database(
    request_path: &Path,
    request: &ApplyRequest,
) -> Result<(), String> {
    let connection = Connection::open_with_flags(
        &request.database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("helper 无法只读打开数据库：{error}"))?;
    let mut statement = connection
        .prepare("SELECT path FROM library_roots")
        .map_err(|error| format!("helper 无法读取媒体资源库：{error}"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("helper 无法查询媒体资源库：{error}"))?;
    let mut roots = Vec::new();
    for row in rows {
        roots.push(PathBuf::from(
            row.map_err(|error| format!("helper 无法解析媒体资源库：{error}"))?,
        ));
    }
    reject_library_root_overlap(&request.install_directory, roots.iter().cloned())?;
    let update_cache = request_update_cache(request)?;
    update::validate_existing_update_subdirectory(
        &update_cache,
        &["transactions", request.transaction_id.as_str()],
        &roots,
    )?;
    update::validate_existing_update_subdirectory(
        &update_cache,
        &[request.expected_version.as_str()],
        &roots,
    )?;
    for (path, label) in [
        (request_path, "Portable 更新事务请求"),
        (&request.archive_path, "Portable 更新包"),
        (&request.database_backup_path, "Portable 更新数据库快照"),
    ] {
        update::ensure_safe_update_file(path, &update_cache, &roots, label)?;
    }
    Ok(())
}

#[cfg(test)]
fn extract_portable_zip_with_limits(
    archive_path: &Path,
    staging: &Path,
    max_files: usize,
    max_total: u64,
) -> Result<Vec<String>, String> {
    let archive_file =
        File::open(archive_path).map_err(|error| format!("无法打开 Portable 更新 ZIP：{error}"))?;
    extract_portable_zip_from_file(archive_file, staging, max_files, max_total)
}

fn extract_portable_zip_from_file(
    archive_file: File,
    staging: &Path,
    max_files: usize,
    max_total: u64,
) -> Result<Vec<String>, String> {
    ensure_plain_directory(staging)?;
    let mut archive = ZipArchive::new(archive_file)
        .map_err(|error| format!("Portable 更新 ZIP 无效：{error}"))?;
    if archive.is_empty() || archive.len() > max_files {
        return Err("Portable 更新 ZIP 文件数量无效或超过上限。".into());
    }
    let allowed = ALLOWED_PAYLOAD_FILES
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut total = 0_u64;
    let mut extracted = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("无法读取 Portable 更新 ZIP 项：{error}"))?;
        let raw_name = entry.name().to_owned();
        validate_archive_name(&raw_name)?;
        if entry.is_dir() {
            return Err("Portable 更新 ZIP 不允许目录项。".into());
        }
        if let Some(mode) = entry.unix_mode() {
            let kind = mode & 0o170000;
            if kind != 0 && kind != 0o100000 {
                return Err("Portable 更新 ZIP 不允许符号链接或特殊文件。".into());
            }
        }
        let folded = raw_name.to_ascii_lowercase();
        if !allowed.contains(&folded) || !ALLOWED_PAYLOAD_FILES.contains(&raw_name.as_str()) {
            return Err(format!("Portable 更新 ZIP 包含未知文件：{raw_name}"));
        }
        if !seen.insert(folded) {
            return Err("Portable 更新 ZIP 包含重复文件名。".into());
        }
        if entry.size() > MAX_ENTRY_BYTES {
            return Err("Portable 更新 ZIP 单个文件超过上限。".into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| "Portable 更新 ZIP 解压大小溢出。".to_string())?;
        if total > max_total {
            return Err("Portable 更新 ZIP 解压总大小超过上限。".into());
        }
        ensure_plain_directory(staging)?;
        let destination = staging.join(&raw_name);
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|error| format!("无法创建 Portable 暂存文件：{error}"))?;
        let mut actual = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = entry
                .read(&mut buffer)
                .map_err(|error| format!("无法解压 Portable 更新文件：{error}"))?;
            if count == 0 {
                break;
            }
            actual = actual
                .checked_add(count as u64)
                .ok_or_else(|| "Portable 更新文件大小溢出。".to_string())?;
            if actual > entry.size() || actual > MAX_ENTRY_BYTES {
                return Err("Portable 更新文件实际大小超过声明或安全上限。".into());
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| format!("无法写入 Portable 暂存文件：{error}"))?;
        }
        if actual != entry.size() {
            return Err("Portable 更新文件实际大小与 ZIP 声明不符。".into());
        }
        output
            .sync_all()
            .map_err(|error| format!("无法同步 Portable 暂存文件：{error}"))?;
        extracted.push(raw_name);
    }
    for required in REQUIRED_PAYLOAD_FILES {
        if !seen.contains(&required.to_ascii_lowercase()) {
            return Err(format!("Portable 更新 ZIP 缺少 {required}。"));
        }
    }
    Ok(extracted)
}

fn validate_archive_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 160
        || name.contains('\0')
        || name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == ".."
        || name.starts_with(['/', '\\'])
        || name.as_bytes().get(1) == Some(&b':')
    {
        return Err("Portable 更新 ZIP 包含绝对路径、路径穿越或嵌套路径。".into());
    }
    let path = Path::new(name);
    if path.is_absolute()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err("Portable 更新 ZIP 文件名不安全。".into());
    }
    Ok(())
}

fn ensure_plain_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("无法检查 Portable 更新目录：{error}"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("Portable 更新目录不是普通目录。".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err("Portable 更新目录不能是重解析点。".into());
        }
    }
    Ok(())
}

fn replace_payload_files(
    request: &ApplyRequest,
    names: &[String],
    installed: &mut Vec<InstalledFile>,
) -> Result<(), String> {
    let mut ordered = names.to_vec();
    ordered.sort_by_key(|name| {
        if name.eq_ignore_ascii_case("M2Shelf.exe") {
            2_u8
        } else if name.eq_ignore_ascii_case(PORTABLE_MARKER_FILE) {
            1_u8
        } else {
            0_u8
        }
    });
    for name in &ordered {
        let staged = request.staging_directory.join(name);
        let target = request.install_directory.join(name);
        let backup = request.backup_directory.join(name);
        let had_original = target.exists();
        atomic_replace(&staged, &target, had_original.then_some(backup.as_path()))?;
        installed.push(InstalledFile {
            target,
            backup,
            had_original,
        });
    }
    Ok(())
}

fn rollback_files(request: &ApplyRequest, installed: &[InstalledFile]) -> Result<(), String> {
    let mut errors = Vec::new();
    for file in installed.iter().rev() {
        let result = if file.had_original {
            let restore = request.backup_directory.join(format!(
                ".restore-{}-{}",
                request.transaction_id,
                file.target
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("payload")
            ));
            fs::copy(&file.backup, &restore)
                .map_err(|error| format!("无法复制旧版恢复文件：{error}"))
                .and_then(|_| atomic_replace(&restore, &file.target, None))
        } else if file.target.exists() {
            fs::remove_file(&file.target).map_err(|error| format!("无法移除新增更新文件：{error}"))
        } else {
            Ok(())
        };
        if let Err(error) = result {
            errors.push(error);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("更新文件回滚失败：{}", errors.join("；")))
    }
}

fn restore_database(request: &ApplyRequest) -> Result<(), String> {
    verify_database_snapshot(
        &request.database_backup_path,
        request.database_backup_size,
        &request.database_backup_sha256,
    )?;
    let restore = request
        .database_path
        .with_extension(format!("restore-{}.db", request.transaction_id));
    fs::copy(&request.database_backup_path, &restore)
        .map_err(|error| format!("无法复制数据库回滚快照：{error}"))?;
    File::options()
        .write(true)
        .open(&restore)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("无法同步数据库回滚副本：{error}"))?;
    verify_database_snapshot(
        &restore,
        request.database_backup_size,
        &request.database_backup_sha256,
    )?;

    let transaction_directory = request
        .database_backup_path
        .parent()
        .ok_or_else(|| "数据库备份没有事务目录。".to_string())?;
    let moved_sidecars = isolate_database_sidecars(&request.database_path, transaction_directory)?;

    let failed_current = transaction_directory.join("failed-current-database.db");
    if failed_current.exists() {
        let _ = restore_moved_sidecars(&moved_sidecars);
        return Err("数据库当前版本恢复目标已存在；已停止回滚。".into());
    }
    if let Err(error) = atomic_replace(&restore, &request.database_path, Some(&failed_current)) {
        let sidecar_error = restore_moved_sidecars(&moved_sidecars).err();
        return Err(format!(
            "无法原子恢复数据库快照：{error}{}",
            sidecar_error
                .map(|value| format!("；{value}"))
                .unwrap_or_default()
        ));
    }
    Ok(())
}

fn isolate_database_sidecars(
    database_path: &Path,
    transaction_directory: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    isolate_database_sidecars_with(
        database_path,
        transaction_directory,
        |source, destination| fs::rename(source, destination),
    )
}

fn isolate_database_sidecars_with(
    database_path: &Path,
    transaction_directory: &Path,
    mut move_file: impl FnMut(&Path, &Path) -> std::io::Result<()>,
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    // Preflight every source and destination before moving the first sidecar. Otherwise a bad
    // second sidecar could return early after the WAL had already been detached from the live DB.
    let mut planned = Vec::new();
    for suffix in ["-wal", "-shm"] {
        let mut value = database_path.as_os_str().to_os_string();
        value.push(suffix);
        let sidecar = PathBuf::from(value);
        if !sidecar.exists() {
            continue;
        }
        ensure_plain_file(&sidecar, "数据库边车文件")?;
        let preserved = transaction_directory.join(format!("failed-current-database{suffix}"));
        if preserved.exists() {
            return Err("数据库边车恢复目标已存在；已停止回滚。".into());
        }
        planned.push((sidecar, preserved));
    }

    let mut moved = Vec::new();
    for (sidecar, preserved) in planned {
        if let Err(error) = move_file(&sidecar, &preserved) {
            let restoration = restore_moved_sidecars(&moved);
            return Err(format!(
                "无法隔离数据库边车文件：{error}{}",
                restoration
                    .err()
                    .map(|value| format!("；{value}"))
                    .unwrap_or_default()
            ));
        }
        moved.push((sidecar, preserved));
    }
    Ok(moved)
}

fn restore_moved_sidecars(moved: &[(PathBuf, PathBuf)]) -> Result<(), String> {
    let mut errors = Vec::new();
    for (original, preserved) in moved.iter().rev() {
        if let Err(error) = fs::rename(preserved, original) {
            errors.push(format!(
                "无法恢复数据库边车 {}：{error}",
                original.display()
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("；"))
    }
}

fn verify_database_snapshot(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), String> {
    let (actual_size, digest) = hash_database_file(path)?;
    let expected_digest = update::decode_sha256(expected_sha256)?;
    if actual_size != expected_size || digest != expected_digest {
        return Err("数据库回滚快照大小或 SHA-256 校验失败；当前数据库保持不变。".into());
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("无法只读打开数据库回滚快照：{error}"))?;
    let quick_check = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get::<_, String>(0))
        .map_err(|error| format!("无法校验数据库回滚快照：{error}"))?;
    if quick_check != "ok" {
        return Err("数据库回滚快照未通过 SQLite 完整性检查；当前数据库保持不变。".into());
    }
    Ok(())
}

fn hash_database_file(path: &Path) -> Result<(u64, [u8; 32]), String> {
    ensure_plain_file(path, "数据库回滚快照")?;
    let metadata =
        fs::metadata(path).map_err(|error| format!("无法读取数据库快照信息：{error}"))?;
    if metadata.len() == 0 || metadata.len() > MAX_DATABASE_BACKUP_BYTES {
        return Err("数据库回滚快照为空或超过安全大小上限。".into());
    }
    let mut file = File::open(path).map_err(|error| format!("无法打开数据库回滚快照：{error}"))?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("无法读取数据库回滚快照：{error}"))?;
        if count == 0 {
            break;
        }
        size = size
            .checked_add(count as u64)
            .ok_or_else(|| "数据库回滚快照大小溢出。".to_string())?;
        if size > metadata.len() || size > MAX_DATABASE_BACKUP_BYTES {
            return Err("数据库回滚快照在校验期间发生变化或超过上限。".into());
        }
        hasher.update(&buffer[..count]);
    }
    if size != metadata.len() {
        return Err("数据库回滚快照在校验期间发生变化。".into());
    }
    Ok((size, hasher.finalize().into()))
}

fn ensure_plain_file(path: &Path, label: &str) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法检查{label}：{error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!("{label}不是普通文件。"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(format!("{label}不能是重解析点。"));
        }
    }
    Ok(())
}

fn wait_for_health(request: &ApplyRequest, child: &mut Child) -> Result<bool, String> {
    let deadline = Instant::now() + Duration::from_secs(request.timeout_seconds);
    while Instant::now() < deadline {
        if request.health_marker_path.is_file() {
            let mut value = String::new();
            File::open(&request.health_marker_path)
                .and_then(|file| file.take(256).read_to_string(&mut value))
                .map_err(|error| format!("无法读取更新健康回执：{error}"))?;
            if value == request.expected_version {
                return wait_for_survival_grace(POST_HEALTH_SURVIVAL_GRACE, || {
                    child
                        .try_wait()
                        .map(|status| status.is_none())
                        .map_err(|error| format!("无法确认新版进程健康状态：{error}"))
                });
            }
            return Ok(false);
        }
        if child
            .try_wait()
            .map_err(|error| format!("无法检查新版进程状态：{error}"))?
            .is_some()
        {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(250));
    }
    Ok(false)
}

fn wait_for_survival_grace(
    grace: Duration,
    mut is_running: impl FnMut() -> Result<bool, String>,
) -> Result<bool, String> {
    // The health receipt deadline limits how long a new version may take to report ready. Once a
    // valid receipt arrives, always observe the complete grace interval, even if the receipt was
    // written near that deadline.
    let grace_deadline = Instant::now() + grace;
    loop {
        if !is_running()? {
            return Ok(false);
        }
        let now = Instant::now();
        if now >= grace_deadline {
            return Ok(true);
        }
        thread::sleep(std::cmp::min(
            Duration::from_millis(100),
            grace_deadline.saturating_duration_since(now),
        ));
    }
}

fn read_json_bounded<T: for<'de> Deserialize<'de>>(path: &Path, limit: u64) -> Result<T, String> {
    ensure_plain_file(path, "更新 JSON 文件")?;
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法读取更新 JSON 文件：{error}"))?;
    if metadata.len() == 0 || metadata.len() > limit {
        return Err("更新 JSON 文件为空或超过安全上限。".into());
    }
    let file = File::open(path).map_err(|error| format!("无法打开更新 JSON：{error}"))?;
    serde_json::from_reader(file).map_err(|error| format!("更新 JSON 格式无效：{error}"))
}

fn write_json_create_new<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|error| format!("无法编码更新事务：{error}"))?;
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        return Err("更新事务超过安全大小上限。".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("无法创建更新事务：{error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("无法写入更新事务：{error}"))
}

#[cfg(windows)]
fn wait_for_process_exit(pid: u32, timeout: Duration) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            // The process completed before the helper acquired a handle.
            return Ok(());
        }
        return Err(format!("无法等待 M²Shelf 主进程退出：{error}"));
    }
    let milliseconds = timeout.as_millis().min(u32::MAX as u128) as u32;
    let result = unsafe { WaitForSingleObject(handle, milliseconds) };
    unsafe { CloseHandle(handle) };
    match result {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => Err("等待 M²Shelf 主进程退出超时。".into()),
        _ => Err("等待 M²Shelf 主进程退出失败。".into()),
    }
}

#[cfg(not(windows))]
fn wait_for_process_exit(_pid: u32, _timeout: Duration) -> Result<(), String> {
    Err("Portable 更新只支持 Windows。".into())
}

#[cfg(windows)]
fn atomic_replace(staged: &Path, target: &Path, backup: Option<&Path>) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH, REPLACEFILE_WRITE_THROUGH,
    };
    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    let staged_wide = wide(staged);
    let target_wide = wide(target);
    let result = if target.exists() {
        let backup_wide = backup.map(wide);
        unsafe {
            ReplaceFileW(
                target_wide.as_ptr(),
                staged_wide.as_ptr(),
                backup_wide
                    .as_ref()
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
                REPLACEFILE_WRITE_THROUGH,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        }
    } else {
        unsafe {
            MoveFileExW(
                staged_wide.as_ptr(),
                target_wide.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if result == 0 {
        return Err(format!(
            "无法原子替换 {}：{}",
            target.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(staged: &Path, target: &Path, backup: Option<&Path>) -> Result<(), String> {
    if let Some(backup) = backup {
        fs::rename(target, backup).map_err(|error| error.to_string())?;
    } else if target.exists() {
        fs::remove_file(target).map_err(|error| error.to_string())?;
    }
    fs::rename(staged, target).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn verify_windows_product_version(executable: &Path, expected: &str) -> Result<(), String> {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    };
    let version = Version::parse(expected).map_err(|_| "预期版本不是 SemVer。".to_string())?;
    let path = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut handle = 0_u32;
    let size = unsafe { GetFileVersionInfoSizeW(path.as_ptr(), &mut handle) };
    if size == 0 {
        return Err(format!(
            "无法读取新版 M2Shelf.exe 的 ProductVersion：{}",
            std::io::Error::last_os_error()
        ));
    }
    let mut buffer = vec![0_u8; size as usize];
    if unsafe { GetFileVersionInfoW(path.as_ptr(), 0, size, buffer.as_mut_ptr().cast()) } == 0 {
        return Err("无法读取新版 M2Shelf.exe 的版本资源。".into());
    }
    let query = ['\\' as u16, 0];
    let mut pointer: *mut c_void = std::ptr::null_mut();
    let mut length = 0_u32;
    if unsafe {
        VerQueryValueW(
            buffer.as_ptr().cast(),
            query.as_ptr(),
            &mut pointer,
            &mut length,
        )
    } == 0
        || pointer.is_null()
        || length < std::mem::size_of::<VS_FIXEDFILEINFO>() as u32
    {
        return Err("新版 M2Shelf.exe 缺少固定版本资源。".into());
    }
    let info = unsafe { &*(pointer.cast::<VS_FIXEDFILEINFO>()) };
    let actual = (
        (info.dwFileVersionMS >> 16) as u64,
        (info.dwFileVersionMS & 0xffff) as u64,
        (info.dwFileVersionLS >> 16) as u64,
        (info.dwFileVersionLS & 0xffff) as u64,
    );
    if actual != (version.major, version.minor, version.patch, 0) {
        return Err(format!(
            "新版 M2Shelf.exe ProductVersion {}.{}.{}.{} 与清单 {} 不一致。",
            actual.0, actual.1, actual.2, actual.3, expected
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn verify_windows_product_version(_executable: &Path, _expected: &str) -> Result<(), String> {
    Err("Portable 更新只支持 Windows。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
    use tempfile::TempDir;
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn write_zip(path: &Path, entries: &[(&str, &[u8], Option<u32>)]) {
        let file = File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);
        for (name, bytes, mode) in entries {
            let mut options = SimpleFileOptions::default();
            if let Some(mode) = mode {
                options = options.unix_permissions(*mode);
            }
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    fn attempt(entries: &[(&str, &[u8], Option<u32>)]) -> Result<Vec<String>, String> {
        let temp = TempDir::new().unwrap();
        let archive = temp.path().join("payload.zip");
        let staging = temp.path().join("staging");
        fs::create_dir(&staging).unwrap();
        write_zip(&archive, entries);
        extract_portable_zip_with_limits(&archive, &staging, MAX_ARCHIVE_FILES, MAX_EXTRACTED_BYTES)
    }

    fn test_request(temp: &TempDir) -> ApplyRequest {
        let transaction_id = Uuid::new_v4().to_string();
        let update_cache = temp.path().join("updates");
        let transaction_directory = update_cache.join("transactions").join(&transaction_id);
        let install_directory = temp.path().join("portable");
        fs::create_dir_all(&transaction_directory).unwrap();
        fs::create_dir_all(&install_directory).unwrap();
        fs::write(
            install_directory.join(PORTABLE_MARKER_FILE),
            serde_json::to_vec(&PortableMarker {
                schema_version: MARKER_SCHEMA_VERSION,
                app_id: APP_ID.into(),
                distribution: "portable".into(),
            })
            .unwrap(),
        )
        .unwrap();
        ApplyRequest {
            schema_version: REQUEST_SCHEMA_VERSION,
            transaction_id: transaction_id.clone(),
            parent_pid: 1,
            install_directory: install_directory.clone(),
            archive_path: update_cache
                .join("1.2.4")
                .join("M2Shelf-Portable-1.2.4-x64.zip"),
            archive_size: 1,
            archive_sha256: "00".repeat(32),
            archive_signature: BASE64_STANDARD.encode([0_u8; 64]),
            expected_version: "1.2.4".into(),
            platform: PORTABLE_PLATFORM.into(),
            database_path: temp.path().join("morimediashelf.db"),
            database_backup_path: transaction_directory.join("database-backup.db"),
            database_backup_size: 1,
            database_backup_sha256: "00".repeat(32),
            staging_directory: install_directory
                .join(format!(".m2shelf-update-staging-{transaction_id}")),
            backup_directory: install_directory
                .join(format!(".m2shelf-update-backup-{transaction_id}")),
            health_marker_path: transaction_directory.join("healthy"),
            helper_ready_path: transaction_directory.join("helper-ready"),
            timeout_seconds: APPLY_TIMEOUT_SECONDS,
        }
    }

    fn write_authentication_fixture(
        request: &ApplyRequest,
        phase: ApplyPhase,
    ) -> (PathBuf, PathBuf) {
        let transaction_directory = request.health_marker_path.parent().unwrap();
        let update_cache = transaction_directory
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let version_directory = request.archive_path.parent().unwrap();
        fs::create_dir_all(version_directory).unwrap();
        fs::write(&request.archive_path, b"signed-package-placeholder").unwrap();
        fs::write(
            &request.database_backup_path,
            b"database-backup-placeholder",
        )
        .unwrap();
        let database = Connection::open(&request.database_path).unwrap();
        database
            .execute_batch("CREATE TABLE library_roots(path TEXT NOT NULL);")
            .unwrap();
        drop(database);
        let current_executable = request.install_directory.join("M2Shelf.exe");
        fs::write(&current_executable, b"current-executable").unwrap();
        write_json_create_new(&transaction_directory.join("apply-request.json"), request).unwrap();
        write_transaction_state(request, phase, None, true).unwrap();
        fs::write(&request.helper_ready_path, &request.expected_version).unwrap();
        (update_cache, current_executable)
    }

    fn database_sidecar(database: &Path, suffix: &str) -> PathBuf {
        let mut value = database.as_os_str().to_os_string();
        value.push(suffix);
        PathBuf::from(value)
    }

    #[test]
    fn marker_schema_is_exact() {
        let marker: PortableMarker = serde_json::from_str(
            r#"{"schemaVersion":1,"appId":"app.morimediashelf.desktop","distribution":"portable"}"#,
        )
        .unwrap();
        assert_eq!(marker.schema_version, 1);
        assert!(serde_json::from_str::<PortableMarker>(
            r#"{"schemaVersion":1,"appId":"app.morimediashelf.desktop","distribution":"portable","extra":true}"#
        )
        .is_err());
    }

    #[test]
    fn health_receipt_requires_the_exact_running_version() {
        assert!(ensure_running_version_matches("1.2.3", "1.2.3").is_ok());
        assert!(ensure_running_version_matches("1.2.2", "1.2.3").is_err());
        assert!(ensure_running_version_matches("1.2.4", "1.2.3").is_err());
        assert!(ensure_running_version_matches("01.2.3", "1.2.3").is_err());
    }

    #[test]
    fn update_transaction_argument_requires_one_canonical_uuid() {
        let transaction_id = Uuid::new_v4().to_string();
        let args = vec![
            "M2Shelf.exe".into(),
            "--m2shelf-update-transaction".into(),
            transaction_id.clone().into(),
        ];
        assert_eq!(
            update_transaction_arg_from(&args).unwrap(),
            Some(transaction_id.clone())
        );

        let duplicate = vec![
            "M2Shelf.exe".into(),
            "--m2shelf-update-transaction".into(),
            transaction_id.clone().into(),
            "--m2shelf-update-transaction".into(),
            transaction_id.into(),
        ];
        assert!(update_transaction_arg_from(&duplicate).is_err());
        let noncanonical = vec![
            "M2Shelf.exe".into(),
            "--m2shelf-update-transaction".into(),
            "AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA".into(),
        ];
        assert!(update_transaction_arg_from(&noncanonical).is_err());
    }

    #[test]
    fn an_arbitrary_canonical_uuid_cannot_authenticate_the_mutex_bypass() {
        let temp = TempDir::new().unwrap();
        let update_cache = temp.path().join("updates");
        fs::create_dir_all(update_cache.join("transactions")).unwrap();
        let executable = temp.path().join("M2Shelf.exe");
        fs::write(&executable, b"untrusted-launch").unwrap();

        assert!(authenticate_update_transaction(
            &update_cache,
            &Uuid::new_v4().to_string(),
            "1.2.4",
            &executable,
        )
        .is_err());
    }

    #[test]
    fn only_the_path_bound_launched_transaction_authenticates_the_mutex_bypass() {
        let temp = TempDir::new().unwrap();
        let request = test_request(&temp);
        let (update_cache, current_executable) =
            write_authentication_fixture(&request, ApplyPhase::Ready);

        // A caller cannot manufacture an exception merely by copying a canonical transaction ID
        // from disk: the helper must already have committed the exact LAUNCHED state.
        assert!(authenticate_update_transaction(
            &update_cache,
            &request.transaction_id,
            &request.expected_version,
            &current_executable,
        )
        .is_err());

        write_transaction_state(&request, ApplyPhase::Launched, None, true).unwrap();
        let other_directory = temp.path().join("other-portable");
        fs::create_dir(&other_directory).unwrap();
        let other_executable = other_directory.join("M2Shelf.exe");
        fs::write(&other_executable, b"different-executable").unwrap();
        assert!(authenticate_update_transaction(
            &update_cache,
            &request.transaction_id,
            &request.expected_version,
            &other_executable,
        )
        .is_err());

        assert!(authenticate_update_transaction(
            &update_cache,
            &request.transaction_id,
            &request.expected_version,
            &current_executable,
        )
        .is_ok());
    }

    #[test]
    fn pre_ready_cleanup_guard_removes_only_a_strict_owned_uuid_transaction() {
        let temp = TempDir::new().unwrap();
        let request = test_request(&temp);
        let directory = request.health_marker_path.parent().unwrap().to_path_buf();
        let update_cache = directory.parent().unwrap().parent().unwrap().to_path_buf();
        fs::write(directory.join(UPDATER_FILE), b"helper").unwrap();
        fs::write(&request.database_backup_path, b"backup").unwrap();
        write_json_create_new(&directory.join("apply-request.json"), &request).unwrap();
        write_transaction_state(&request, ApplyPhase::Ready, None, true).unwrap();

        {
            let _cleanup = PreparedTransactionCleanup {
                update_cache,
                transaction_id: request.transaction_id.clone(),
                library_roots: Vec::new(),
                armed: true,
            };
        }

        assert!(!directory.exists());
    }

    #[test]
    fn failed_pre_ready_cleanup_is_marked_aborted_and_never_reported_as_recovery() {
        let temp = TempDir::new().unwrap();
        let request = test_request(&temp);
        let directory = request.health_marker_path.parent().unwrap().to_path_buf();
        let update_cache = directory.parent().unwrap().parent().unwrap().to_path_buf();
        let unknown = directory.join("user-file.txt");
        fs::write(&unknown, b"preserve").unwrap();
        write_json_create_new(&directory.join("apply-request.json"), &request).unwrap();
        write_transaction_state(&request, ApplyPhase::Ready, None, true).unwrap();

        {
            let _cleanup = PreparedTransactionCleanup {
                update_cache: update_cache.clone(),
                transaction_id: request.transaction_id.clone(),
                library_roots: Vec::new(),
                armed: true,
            };
        }

        assert_eq!(fs::read(&unknown).unwrap(), b"preserve");
        let state: TransactionState =
            read_json_bounded(&directory.join("transaction-state.json"), MAX_REQUEST_BYTES)
                .unwrap();
        assert_eq!(state.phase, ApplyPhase::Aborted);
        assert!(!state.recovery_material_preserved);
        assert!(!mark_interrupted_transactions_recovery_required(&update_cache, None).unwrap());
    }

    #[test]
    fn helper_ready_disarms_pre_ready_cleanup() {
        let temp = TempDir::new().unwrap();
        let request = test_request(&temp);
        let directory = request.health_marker_path.parent().unwrap().to_path_buf();
        let update_cache = directory.parent().unwrap().parent().unwrap().to_path_buf();
        let sentinel = directory.join("apply-request.json");
        fs::write(&sentinel, b"ready").unwrap();

        {
            let mut cleanup = PreparedTransactionCleanup {
                update_cache,
                transaction_id: request.transaction_id.clone(),
                library_roots: Vec::new(),
                armed: true,
            };
            cleanup.disarm();
        }

        assert_eq!(fs::read(sentinel).unwrap(), b"ready");
    }

    #[test]
    fn startup_marks_only_inactive_valid_nonterminal_transactions_for_recovery() {
        let temp = TempDir::new().unwrap();
        let request = test_request(&temp);
        let transaction_directory = request.health_marker_path.parent().unwrap();
        let request_path = transaction_directory.join("apply-request.json");
        write_json_create_new(&request_path, &request).unwrap();
        write_transaction_state(&request, ApplyPhase::Replacing, None, true).unwrap();
        let update_cache = transaction_directory.parent().unwrap().parent().unwrap();

        assert!(!mark_interrupted_transactions_recovery_required(
            update_cache,
            Some(&request.transaction_id)
        )
        .unwrap());
        let active_state: TransactionState = read_json_bounded(
            &transaction_directory.join("transaction-state.json"),
            MAX_REQUEST_BYTES,
        )
        .unwrap();
        assert_eq!(active_state.phase, ApplyPhase::Replacing);
        assert!(!update_cache.join("last-rollback.json").exists());

        assert!(mark_interrupted_transactions_recovery_required(update_cache, None).unwrap());
        let interrupted_state: TransactionState = read_json_bounded(
            &transaction_directory.join("transaction-state.json"),
            MAX_REQUEST_BYTES,
        )
        .unwrap();
        assert_eq!(interrupted_state.phase, ApplyPhase::RollbackFailed);
        assert!(interrupted_state.recovery_material_preserved);
        let notice: RollbackNotice =
            read_json_bounded(&update_cache.join("last-rollback.json"), MAX_REQUEST_BYTES).unwrap();
        assert_eq!(notice.outcome, RecoveryOutcome::RecoveryRequired);
    }

    #[test]
    fn recovery_notice_failure_leaves_the_transaction_nonterminal_for_startup() {
        let temp = TempDir::new().unwrap();
        let request = test_request(&temp);
        let transaction_directory = request.health_marker_path.parent().unwrap();
        write_transaction_state(&request, ApplyPhase::Replacing, None, true).unwrap();
        let update_cache = transaction_directory.parent().unwrap().parent().unwrap();
        fs::create_dir(update_cache.join("last-rollback.json")).unwrap();

        assert!(persist_recovery_outcome(
            &request,
            ApplyPhase::RollbackFailed,
            RecoveryOutcome::RecoveryRequired,
            "injected failure",
        )
        .is_err());

        let state: TransactionState = read_json_bounded(
            &transaction_directory.join("transaction-state.json"),
            MAX_REQUEST_BYTES,
        )
        .unwrap();
        assert_eq!(state.phase, ApplyPhase::Replacing);
        assert!(state.recovery_material_preserved);
    }

    #[test]
    fn startup_finishes_cleanup_after_a_completed_state_was_committed() {
        let temp = TempDir::new().unwrap();
        let request = test_request(&temp);
        let transaction_directory = request.health_marker_path.parent().unwrap();
        let request_path = transaction_directory.join("apply-request.json");
        write_json_create_new(&request_path, &request).unwrap();
        write_transaction_state(&request, ApplyPhase::Completed, None, true).unwrap();
        fs::write(transaction_directory.join(UPDATER_FILE), b"helper").unwrap();
        fs::write(&request.database_backup_path, b"snapshot").unwrap();
        fs::write(transaction_directory.join("verified-payload.zip"), b"zip").unwrap();
        fs::write(&request.health_marker_path, b"healthy").unwrap();
        fs::write(&request.helper_ready_path, b"ready").unwrap();
        fs::create_dir(&request.staging_directory).unwrap();
        fs::create_dir(&request.backup_directory).unwrap();
        fs::write(request.staging_directory.join("M2Shelf.exe"), b"new").unwrap();
        fs::write(request.backup_directory.join("M2Shelf.exe"), b"old").unwrap();
        let update_cache = transaction_directory.parent().unwrap().parent().unwrap();

        cleanup_completed_transactions(update_cache, &[]).unwrap();

        assert!(!transaction_directory.exists());
        assert!(!request.staging_directory.exists());
        assert!(!request.backup_directory.exists());
    }

    #[test]
    fn health_receipt_requires_the_new_process_to_survive_a_grace_period() {
        assert!(!wait_for_survival_grace(Duration::from_millis(1), || Ok(false),).unwrap());
        assert!(wait_for_survival_grace(Duration::from_millis(1), || Ok(true),).unwrap());
    }

    #[test]
    fn database_snapshot_verification_rejects_hash_and_sqlite_corruption() {
        let temp = TempDir::new().unwrap();
        let database_path = temp.path().join("snapshot.db");
        let connection = Connection::open(&database_path).unwrap();
        connection
            .execute_batch("CREATE TABLE sample(value TEXT); INSERT INTO sample VALUES('ok');")
            .unwrap();
        drop(connection);

        let (size, digest) = hash_database_file(&database_path).unwrap();
        let sha256 = update::encode_sha256(&digest);
        verify_database_snapshot(&database_path, size, &sha256).unwrap();

        OpenOptions::new()
            .append(true)
            .open(&database_path)
            .unwrap()
            .write_all(b"tampered")
            .unwrap();
        assert!(verify_database_snapshot(&database_path, size, &sha256).is_err());

        let invalid_path = temp.path().join("invalid.db");
        fs::write(&invalid_path, b"not a sqlite database").unwrap();
        let (invalid_size, invalid_digest) = hash_database_file(&invalid_path).unwrap();
        assert!(verify_database_snapshot(
            &invalid_path,
            invalid_size,
            &update::encode_sha256(&invalid_digest),
        )
        .is_err());
    }

    #[test]
    fn sidecar_preflight_and_move_failure_leave_the_live_pair_attached() {
        let temp = TempDir::new().unwrap();
        let database = temp.path().join("current.db");
        let transaction = temp.path().join("transaction");
        fs::create_dir(&transaction).unwrap();
        fs::write(&database, b"database").unwrap();
        let wal = database_sidecar(&database, "-wal");
        let shm = database_sidecar(&database, "-shm");
        fs::write(&wal, b"wal").unwrap();
        fs::write(&shm, b"shm").unwrap();

        let blocked_shm = transaction.join("failed-current-database-shm");
        fs::write(&blocked_shm, b"occupied").unwrap();
        assert!(isolate_database_sidecars(&database, &transaction).is_err());
        assert_eq!(fs::read(&wal).unwrap(), b"wal");
        assert_eq!(fs::read(&shm).unwrap(), b"shm");
        assert!(!transaction.join("failed-current-database-wal").exists());

        fs::remove_file(blocked_shm).unwrap();
        let mut failed_second_move = false;
        let result =
            isolate_database_sidecars_with(&database, &transaction, |source, destination| {
                if source == shm && !failed_second_move {
                    failed_second_move = true;
                    return Err(std::io::Error::other("injected second move failure"));
                }
                fs::rename(source, destination)
            });
        assert!(result.is_err());
        assert_eq!(fs::read(&wal).unwrap(), b"wal");
        assert_eq!(fs::read(&shm).unwrap(), b"shm");
        assert!(!transaction.join("failed-current-database-wal").exists());
    }

    #[test]
    fn pre_replacement_failure_records_rollback_and_restarts_the_old_version() {
        let temp = TempDir::new().unwrap();
        let request = test_request(&temp);
        let mut restarted = false;
        let result = recover_after_failed_apply_with_launcher(
            &request,
            None,
            &[],
            false,
            "等待主进程失败",
            |executable| {
                assert_eq!(executable, request.install_directory.join("M2Shelf.exe"));
                restarted = true;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(restarted);

        let transaction_directory = request.health_marker_path.parent().unwrap();
        let state: TransactionState = read_json_bounded(
            &transaction_directory.join("transaction-state.json"),
            MAX_REQUEST_BYTES,
        )
        .unwrap();
        assert_eq!(state.phase, ApplyPhase::RolledBack);
        assert!(state.recovery_material_preserved);

        let update_cache = transaction_directory.parent().unwrap().parent().unwrap();
        let notice: RollbackNotice =
            read_json_bounded(&update_cache.join("last-rollback.json"), MAX_REQUEST_BYTES).unwrap();
        assert_eq!(notice.outcome, RecoveryOutcome::RolledBack);
    }

    #[test]
    fn complete_payload_includes_replaceable_worker_and_licensing_source() {
        let entries: Vec<_> = REQUIRED_PAYLOAD_FILES
            .iter()
            .map(|name| (*name, b"payload".as_slice(), None))
            .collect();
        assert_eq!(attempt(&entries).unwrap().len(), 8);
        for required in [
            "M2ShelfMobi.exe",
            "M2ShelfMobi-source.zip",
            "THIRD-PARTY-NOTICES.txt",
        ] {
            let incomplete: Vec<_> = entries
                .iter()
                .copied()
                .filter(|(name, _, _)| *name != required)
                .collect();
            assert!(attempt(&incomplete).is_err(), "missing {required}");
        }
    }
    #[test]
    fn zip_rejects_traversal_absolute_backslash_duplicate_symlink_and_unknown() {
        for name in [
            "../M2Shelf.exe",
            "/M2Shelf.exe",
            "C:\\M2Shelf.exe",
            "..\\M2Shelf.exe",
        ] {
            assert!(attempt(&[(name, b"x", None)]).is_err(), "accepted {name}");
        }
        assert!(attempt(&[("M2Shelf.exe", b"a", None), ("m2shelf.EXE", b"b", None),]).is_err());
        assert!(attempt(&[("M2Shelf.exe", b"target", Some(0o120777))]).is_err());
        assert!(attempt(&[("evil.dll", b"x", None)]).is_err());
    }

    #[test]
    fn zip_rejects_file_count_and_total_size_limits() {
        let temp = TempDir::new().unwrap();
        let archive = temp.path().join("payload.zip");
        write_zip(
            &archive,
            &[("M2Shelf.exe", b"x", None), (UPDATER_FILE, b"x", None)],
        );
        let staging = temp.path().join("staging-count");
        fs::create_dir(&staging).unwrap();
        assert!(extract_portable_zip_with_limits(&archive, &staging, 1, 100).is_err());

        let staging = temp.path().join("staging-size");
        fs::create_dir(&staging).unwrap();
        assert!(extract_portable_zip_with_limits(&archive, &staging, 10, 1).is_err());
    }
}

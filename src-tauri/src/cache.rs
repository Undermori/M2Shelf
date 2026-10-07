use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use uuid::Uuid;

#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

use crate::{db::AppResult, models::CacheStats};

const CACHE_MARKER_NAME: &str = ".m2shelf-cover-cache";
const CACHE_MARKER_CONTENT: &[u8] = b"M2Shelf cover cache v1\n";
const MAX_COVER_BYTES: u64 = 15 * 1024 * 1024;
const MAX_COVER_DIMENSION: u32 = 16_384;
const MAX_COVER_PIXELS: u64 = 40_000_000;

/// Cache operations may run on Tauri worker threads and on the background matcher at the same
/// time. Ordinary reads and atomic writes can coexist, while an explicit cache clear waits for all
/// of them so it cannot remove an in-flight download or clear a freshly committed database path.
static COVER_CACHE_CLEAR_BARRIER: RwLock<()> = RwLock::new(());

pub(crate) type CoverCacheOperationGuard = RwLockReadGuard<'static, ()>;
pub(crate) type CoverCacheClearGuard = RwLockWriteGuard<'static, ()>;

pub(crate) fn begin_cover_cache_operation() -> CoverCacheOperationGuard {
    COVER_CACHE_CLEAR_BARRIER
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn begin_cover_cache_clear() -> CoverCacheClearGuard {
    COVER_CACHE_CLEAR_BARRIER
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Creates or upgrades the application-owned default cache. Custom locations must first pass
/// `initialize_custom_cache` so an arbitrary non-empty user directory cannot be claimed.
pub fn ensure_directories(cache_root: &Path) -> AppResult<()> {
    if cache_root.exists() && !cache_root.is_dir() {
        return Err("封面缓存路径是普通文件，必须选择文件夹。".into());
    }
    fs::create_dir_all(cache_root).map_err(|error| format!("无法创建封面缓存目录：{error}"))?;
    ensure_marker(cache_root)?;
    ensure_named_directories(cache_root)
}

/// Initializes a user-selected cache directory. Existing non-empty folders are accepted only
/// when they already carry M²Shelf's marker, preventing the cache cleaner from being pointed at
/// a general-purpose documents folder.
pub fn initialize_custom_cache(cache_root: &Path) -> AppResult<()> {
    if cache_root.exists() && !cache_root.is_dir() {
        return Err("封面缓存路径是普通文件，必须选择文件夹。".into());
    }
    if cache_root.is_dir() && !has_valid_marker(cache_root)? {
        let mut entries =
            fs::read_dir(cache_root).map_err(|error| format!("无法读取封面缓存目录：{error}"))?;
        if entries.next().is_some() {
            return Err("自定义封面缓存目录必须为空，或是已由 M²Shelf 创建的缓存目录。".into());
        }
    }
    fs::create_dir_all(cache_root).map_err(|error| format!("无法创建封面缓存目录：{error}"))?;
    ensure_marker(cache_root)?;
    ensure_named_directories(cache_root)
}

/// Reopens an already configured custom cache without silently claiming an unrelated folder.
pub fn ensure_existing_custom_cache(cache_root: &Path) -> AppResult<()> {
    if !cache_root.is_dir() {
        return Err("已配置的封面缓存目录不存在或不可访问。".into());
    }
    if !has_valid_marker(cache_root)? {
        return Err("已配置的封面缓存目录缺少 M²Shelf 标记，已拒绝写入或清理。".into());
    }
    ensure_named_directories(cache_root)
}

fn ensure_named_directories(cache_root: &Path) -> AppResult<()> {
    for directory in [cache_root.join("bangumi"), cache_root.join("manual")] {
        fs::create_dir_all(&directory).map_err(|error| format!("无法创建封面缓存目录：{error}"))?;
        let metadata = fs::symlink_metadata(&directory)
            .map_err(|error| format!("无法校验封面缓存目录：{error}"))?;
        if !metadata.file_type().is_dir() || !is_equal_or_within(&directory, cache_root) {
            return Err("封面缓存子目录不是安全的应用目录。".into());
        }
    }
    Ok(())
}

fn ensure_marker(cache_root: &Path) -> AppResult<()> {
    let marker = cache_root.join(CACHE_MARKER_NAME);
    if marker.exists() {
        if has_valid_marker(cache_root)? {
            return Ok(());
        }
        return Err("封面缓存目录中的 M²Shelf 标记无效。".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .map_err(|error| format!("无法创建封面缓存目录标记：{error}"))?;
    file.write_all(CACHE_MARKER_CONTENT)
        .map_err(|error| format!("无法写入封面缓存目录标记：{error}"))
}

fn has_valid_marker(cache_root: &Path) -> AppResult<bool> {
    let marker = cache_root.join(CACHE_MARKER_NAME);
    if !marker.exists() {
        return Ok(false);
    }
    if !marker.is_file() {
        return Err("封面缓存目录标记不是普通文件。".into());
    }
    let metadata = fs::symlink_metadata(&marker)
        .map_err(|error| format!("无法校验封面缓存目录标记：{error}"))?;
    if !metadata.file_type().is_file() || !is_equal_or_within(&marker, cache_root) {
        return Err("封面缓存目录标记不是安全的普通文件。".into());
    }
    fs::read(&marker)
        .map(|bytes| bytes == CACHE_MARKER_CONTENT)
        .map_err(|error| format!("无法读取封面缓存目录标记：{error}"))
}

pub fn validate_cache_location(
    candidate: &Path,
    library_roots: impl IntoIterator<Item = PathBuf>,
) -> AppResult<PathBuf> {
    if !candidate.is_absolute() {
        return Err("封面缓存目录必须是绝对路径。".into());
    }
    if candidate.exists() && !candidate.is_dir() {
        return Err("封面缓存路径是普通文件，必须选择文件夹。".into());
    }
    let resolved = resolve_for_comparison(candidate)?;
    if resolved.parent().is_none() {
        return Err("不能把磁盘卷根目录设为封面缓存目录。".into());
    }
    for library_root in library_roots {
        let library_root = resolve_for_comparison(&library_root)?;
        if paths_overlap_resolved(&resolved, &library_root) {
            return Err("封面缓存目录不能等于、位于媒体资源库内，或包含媒体资源库。".into());
        }
    }
    Ok(normalize_lexically(candidate))
}

pub fn paths_overlap(left: &Path, right: &Path) -> bool {
    paths_overlap_checked(left, right).unwrap_or(false)
}

/// Security-sensitive callers must not treat an unresolvable path as non-overlapping. Keep the
/// historical boolean wrapper for presentation/cache ownership checks, and expose this fail-closed
/// variant for updater and source-read-only boundaries.
pub(crate) fn paths_overlap_checked(left: &Path, right: &Path) -> AppResult<bool> {
    let left = resolve_for_comparison(left)?;
    let right = resolve_for_comparison(right)?;
    Ok(paths_overlap_resolved(&left, &right))
}

pub fn is_equal_or_within(candidate: &Path, root: &Path) -> bool {
    is_equal_or_within_checked(candidate, root).unwrap_or(false)
}

pub(crate) fn is_equal_or_within_checked(candidate: &Path, root: &Path) -> AppResult<bool> {
    let candidate = resolve_for_comparison(candidate)?;
    let root = resolve_for_comparison(root)?;
    Ok(path_starts_with(&candidate, &root))
}

fn paths_overlap_resolved(left: &Path, right: &Path) -> bool {
    path_starts_with(left, right) || path_starts_with(right, left)
}

fn path_starts_with(candidate: &Path, root: &Path) -> bool {
    let mut candidate = candidate.components();
    for expected in root.components() {
        let Some(actual) = candidate.next() else {
            return false;
        };
        if !component_eq(actual, expected) {
            return false;
        }
    }
    true
}

fn component_eq(left: Component<'_>, right: Component<'_>) -> bool {
    #[cfg(target_os = "windows")]
    {
        left.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
    }
    #[cfg(not(target_os = "windows"))]
    {
        left == right
    }
}

pub(crate) fn resolve_for_comparison(path: &Path) -> AppResult<PathBuf> {
    if !path.is_absolute() {
        return Err("路径必须是绝对路径。".into());
    }
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical);
    }

    let mut probe = normalize_lexically(path);
    let mut missing = Vec::new();
    while !probe.exists() {
        let name = probe
            .file_name()
            .ok_or_else(|| "无法解析所选封面缓存目录。".to_string())?;
        missing.push(name.to_os_string());
        if !probe.pop() {
            return Err("无法解析所选封面缓存目录。".into());
        }
    }
    let mut resolved = probe.canonicalize().unwrap_or(probe);
    for name in missing.into_iter().rev() {
        resolved.push(name);
    }
    Ok(normalize_lexically(&resolved))
}

fn normalize_lexically(path: &Path) -> PathBuf {
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

pub fn copy_manual_cover(
    _cache_operation: &CoverCacheOperationGuard,
    cache_root: &Path,
    node_id: i64,
    source: &Path,
) -> AppResult<PathBuf> {
    if node_id <= 0 {
        return Err("目录节点无效。".into());
    }
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let expected_format = CoverFormat::from_extension(&extension)
        .ok_or_else(|| "封面仅支持 JPG、PNG 或 WEBP。".to_string())?;

    let source_metadata = fs::metadata(source).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "所选封面文件不存在。".to_string()
        } else {
            format!("无法读取所选封面文件：{error}")
        }
    })?;
    if !source_metadata.is_file() {
        return Err("所选封面文件不存在。".into());
    }
    if source_metadata.len() == 0 {
        return Err("所选封面文件为空。".into());
    }
    if source_metadata.len() > MAX_COVER_BYTES {
        return Err("所选封面超过 15 MiB 安全限制。".into());
    }

    let source_file =
        File::open(source).map_err(|error| format!("无法读取所选封面文件：{error}"))?;
    let mut bytes = Vec::with_capacity(source_metadata.len() as usize);
    source_file
        .take(MAX_COVER_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("无法读取所选封面文件：{error}"))?;
    if bytes.is_empty() {
        return Err("所选封面文件为空。".into());
    }
    if bytes.len() as u64 > MAX_COVER_BYTES {
        return Err("所选封面超过 15 MiB 安全限制。".into());
    }
    validate_manual_cover(&bytes, expected_format)?;

    ensure_existing_custom_cache(cache_root)?;
    let manual_directory = cache_root.join("manual");
    let destination = manual_directory.join(format!("node-{node_id}.{extension}"));
    let (mut pending_file, mut temporary_file) = create_pending_cache_file(&destination, false)
        .map_err(|error| format!("无法创建封面缓存临时文件：{error}"))?;
    temporary_file
        .write_all(&bytes)
        .map_err(|error| format!("复制封面到应用缓存失败：{error}"))?;
    temporary_file
        .flush()
        .map_err(|error| format!("刷新封面缓存临时文件失败：{error}"))?;
    temporary_file
        .sync_all()
        .map_err(|error| format!("同步封面缓存临时文件失败：{error}"))?;
    drop(temporary_file);

    pending_file
        .commit_to(&destination)
        .map_err(|error| format!("提交封面缓存失败：{error}"))?;
    Ok(destination)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CoverFormat {
    Jpeg,
    Png,
    Webp,
}

impl CoverFormat {
    fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }
}

fn validate_manual_cover(bytes: &[u8], expected_format: CoverFormat) -> AppResult<()> {
    let detected_format = detect_cover_format(bytes)
        .ok_or_else(|| "所选文件不是支持的 JPG、PNG 或 WEBP 图片。".to_string())?;
    if detected_format != expected_format {
        return Err("封面文件扩展名与实际图片格式不一致。".into());
    }
    validate_cover_dimensions(bytes, detected_format)
}

/// Validates application-cached cover bytes before they are committed or decoded by the WebView.
/// This catches tiny compressed files with maliciously large declared dimensions without loading
/// their pixel buffer.
pub(crate) fn validate_cover_payload(bytes: &[u8]) -> AppResult<()> {
    cover_payload_dimensions(bytes).map(|_| ())
}

pub(crate) fn cover_payload_dimensions(bytes: &[u8]) -> AppResult<(u32, u32)> {
    let detected_format = detect_cover_format(bytes)
        .ok_or_else(|| "封面不是支持的 JPG、PNG 或 WEBP 图片。".to_string())?;
    validate_cover_dimensions(bytes, detected_format)?;
    image_dimensions(bytes, detected_format)
}

fn validate_cover_dimensions(bytes: &[u8], detected_format: CoverFormat) -> AppResult<()> {
    let (width, height) = image_dimensions(bytes, detected_format)?;
    if width == 0 || height == 0 {
        return Err("封面图片尺寸无效。".into());
    }
    if width > MAX_COVER_DIMENSION
        || height > MAX_COVER_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_COVER_PIXELS
    {
        return Err(format!(
            "封面图片尺寸过大（{width}×{height}），请使用不超过 {MAX_COVER_DIMENSION} 像素边长且总像素不超过 {MAX_COVER_PIXELS} 的图片。"
        ));
    }
    Ok(())
}

fn detect_cover_format(bytes: &[u8]) -> Option<CoverFormat> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(CoverFormat::Jpeg)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(CoverFormat::Png)
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some(CoverFormat::Webp)
    } else {
        None
    }
}

fn image_dimensions(bytes: &[u8], format: CoverFormat) -> AppResult<(u32, u32)> {
    match format {
        CoverFormat::Jpeg => jpeg_dimensions(bytes),
        CoverFormat::Png => png_dimensions(bytes),
        CoverFormat::Webp => webp_dimensions(bytes),
    }
}

fn png_dimensions(bytes: &[u8]) -> AppResult<(u32, u32)> {
    if bytes.len() < 24
        || &bytes[8..12] != 13_u32.to_be_bytes().as_slice()
        || &bytes[12..16] != b"IHDR"
    {
        return Err("PNG 封面缺少有效的 IHDR 尺寸信息。".into());
    }
    Ok((
        u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
        u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
    ))
}

fn jpeg_dimensions(bytes: &[u8]) -> AppResult<(u32, u32)> {
    let mut offset = 2_usize;
    while offset < bytes.len() {
        while offset < bytes.len() && bytes[offset] != 0xff {
            offset += 1;
        }
        while offset < bytes.len() && bytes[offset] == 0xff {
            offset += 1;
        }
        if offset >= bytes.len() {
            break;
        }
        let marker = bytes[offset];
        offset += 1;
        if marker == 0x00 || marker == 0x01 || (0xd0..=0xd8).contains(&marker) {
            continue;
        }
        if marker == 0xd9 || marker == 0xda || offset + 2 > bytes.len() {
            break;
        }
        let segment_length = usize::from(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]));
        if segment_length < 2 || offset + segment_length > bytes.len() {
            return Err("JPEG 封面包含无效的分段长度。".into());
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            if segment_length < 7 {
                return Err("JPEG 封面的尺寸分段无效。".into());
            }
            let height = u16::from_be_bytes([bytes[offset + 3], bytes[offset + 4]]);
            let width = u16::from_be_bytes([bytes[offset + 5], bytes[offset + 6]]);
            return Ok((u32::from(width), u32::from(height)));
        }
        offset += segment_length;
    }
    Err("JPEG 封面缺少有效的尺寸信息。".into())
}

fn webp_dimensions(bytes: &[u8]) -> AppResult<(u32, u32)> {
    if bytes.len() < 20 {
        return Err("WEBP 封面缺少有效的尺寸信息。".into());
    }
    let chunk_size = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    match &bytes[12..16] {
        b"VP8X" if chunk_size >= 10 && bytes.len() >= 30 => {
            let width = 1 + read_u24_le(&bytes[24..27]);
            let height = 1 + read_u24_le(&bytes[27..30]);
            Ok((width, height))
        }
        b"VP8L" if chunk_size >= 5 && bytes.len() >= 25 && bytes[20] == 0x2f => {
            let dimensions = u32::from_le_bytes(bytes[21..25].try_into().unwrap());
            Ok(((dimensions & 0x3fff) + 1, ((dimensions >> 14) & 0x3fff) + 1))
        }
        b"VP8 " if chunk_size >= 10 && bytes.len() >= 30 && bytes[23..26] == [0x9d, 0x01, 0x2a] => {
            let width = u16::from_le_bytes(bytes[26..28].try_into().unwrap()) & 0x3fff;
            let height = u16::from_le_bytes(bytes[28..30].try_into().unwrap()) & 0x3fff;
            Ok((u32::from(width), u32::from(height)))
        }
        _ => Err("WEBP 封面缺少有效的尺寸信息。".into()),
    }
}

fn read_u24_le(bytes: &[u8]) -> u32 {
    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16)
}

pub(crate) struct PendingCacheFile {
    path: PathBuf,
    committed: bool,
}

impl PendingCacheFile {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn commit_to(&mut self, destination: &Path) -> std::io::Result<()> {
        replace_file_atomically(self.path(), destination)?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for PendingCacheFile {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Creates a collision-resistant temporary file beside its destination. Same-directory staging is
/// required for an atomic rename/replace on Windows and avoids cross-volume fallbacks.
pub(crate) fn create_pending_cache_file(
    destination: &Path,
    readable: bool,
) -> std::io::Result<(PendingCacheFile, File)> {
    let parent = destination.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "cache destination has no parent directory",
        )
    })?;
    let stem = destination
        .file_stem()
        .and_then(OsStr::to_str)
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "cache destination has no valid file stem",
            )
        })?;
    let temporary_path = parent.join(format!(".{stem}-{}.tmp", Uuid::new_v4().as_simple()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).read(readable);
    let file = options.open(&temporary_path)?;
    Ok((PendingCacheFile::new(temporary_path), file))
}

#[cfg(target_os = "windows")]
fn replace_file_atomically(source: &Path, destination: &Path) -> std::io::Result<()> {
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: Both paths are NUL-terminated UTF-16 buffers that remain alive for the call.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
fn replace_file_atomically(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

pub fn bangumi_cover_path(cache_root: &Path, subject_id: i64, image_url: &str) -> PathBuf {
    let extension = image_url
        .split('?')
        .next()
        .and_then(|value| value.rsplit('.').next())
        .map(str::to_ascii_lowercase)
        .filter(|value| matches!(value.as_str(), "jpg" | "jpeg" | "png" | "webp"))
        .unwrap_or_else(|| "jpg".to_string());
    cache_root
        .join("bangumi")
        .join(format!("{subject_id}.{extension}"))
}

pub fn remove_cached_file(
    _cache_operation: &CoverCacheOperationGuard,
    path: &Path,
    cache_root: &Path,
) -> AppResult<()> {
    remove_cached_file_inner(path, cache_root)
}

fn remove_cached_file_inner(path: &Path, cache_root: &Path) -> AppResult<()> {
    if !is_owned_cache_file(path, cache_root) {
        return Err("拒绝删除不是由 M²Shelf 命名的缓存文件。".into());
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("删除封面缓存失败：{error}")),
    }
}

pub fn clear_cover_cache(_cache_clear: &CoverCacheClearGuard, cache_root: &Path) -> AppResult<()> {
    ensure_existing_custom_cache(cache_root)?;
    for directory in [cache_root.join("bangumi"), cache_root.join("manual")] {
        for entry in
            fs::read_dir(&directory).map_err(|error| format!("读取缓存目录失败：{error}"))?
        {
            let entry = entry.map_err(|error| format!("读取缓存文件失败：{error}"))?;
            if entry
                .file_type()
                .map_err(|error| format!("读取缓存文件类型失败：{error}"))?
                .is_file()
                && is_owned_cache_file(&entry.path(), cache_root)
            {
                remove_cached_file_inner(&entry.path(), cache_root)?;
            }
        }
    }
    Ok(())
}

pub fn stats(cache_root: &Path) -> AppResult<CacheStats> {
    ensure_existing_custom_cache(cache_root)?;
    let mut file_count = 0_u64;
    let mut total_bytes = 0_u64;
    for directory in [cache_root.join("bangumi"), cache_root.join("manual")] {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("读取缓存目录失败：{error}"))?
            .flatten()
        {
            match entry.metadata() {
                Ok(metadata)
                    if metadata.is_file() && is_owned_cache_file(&entry.path(), cache_root) =>
                {
                    file_count += 1;
                    total_bytes += metadata.len();
                }
                _ => {}
            }
        }
    }
    Ok(CacheStats {
        file_count,
        total_bytes,
        cache_directory: cache_root.to_string_lossy().into_owned(),
    })
}

pub fn cover_data_url(
    _cache_operation: &CoverCacheOperationGuard,
    path: &Path,
) -> AppResult<String> {
    let bytes = read_cover_payload(path)?;
    let mime = image_mime(&bytes)
        .ok_or_else(|| "封面缓存不是支持的 JPG、PNG 或 WEBP 图片。".to_string())?;
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

pub(crate) fn cached_cover_is_valid(path: &Path) -> bool {
    read_cover_payload(path).is_ok()
}

fn read_cover_payload(path: &Path) -> AppResult<Vec<u8>> {
    if !path.is_absolute() || !path.is_file() {
        return Err("封面缓存文件不存在。".into());
    }
    let metadata = fs::metadata(path).map_err(|error| format!("读取封面缓存失败：{error}"))?;
    if metadata.len() == 0 {
        return Err("封面缓存文件为空。".into());
    }
    if metadata.len() > MAX_COVER_BYTES {
        return Err("封面缓存超过 15 MiB 安全限制。".into());
    }
    let file = File::open(path).map_err(|error| format!("读取封面缓存失败：{error}"))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_COVER_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("读取封面缓存失败：{error}"))?;
    if bytes.len() as u64 > MAX_COVER_BYTES {
        return Err("封面缓存超过 15 MiB 安全限制。".into());
    }
    validate_cover_payload(&bytes)?;
    Ok(bytes)
}

fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn is_owned_cache_file(path: &Path, cache_root: &Path) -> bool {
    if !is_equal_or_within(path, cache_root) {
        return false;
    }
    let Some(parent) = path.parent() else {
        return false;
    };
    let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    if is_same_path(parent, &cache_root.join("bangumi")) {
        return is_bangumi_file_name(file_name);
    }
    if is_same_path(parent, &cache_root.join("manual")) {
        return is_manual_file_name(file_name);
    }
    false
}

fn is_same_path(left: &Path, right: &Path) -> bool {
    is_equal_or_within(left, right) && is_equal_or_within(right, left)
}

fn is_bangumi_file_name(name: &str) -> bool {
    if is_unique_temporary_name(name, false) {
        return true;
    }
    let path = Path::new(name);
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or_default();
    let extension = path
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    positive_decimal(stem)
        && matches!(
            extension.as_str(),
            "jpg" | "jpeg" | "png" | "webp" | "download"
        )
}

fn is_manual_file_name(name: &str) -> bool {
    if is_unique_temporary_name(name, true) {
        return true;
    }
    let path = Path::new(name);
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or_default();
    let extension = path
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    stem.strip_prefix("node-").is_some_and(positive_decimal)
        && matches!(extension.as_str(), "jpg" | "jpeg" | "png" | "webp")
}

fn is_unique_temporary_name(name: &str, manual: bool) -> bool {
    let Some(body) = name
        .strip_prefix('.')
        .and_then(|value| value.strip_suffix(".tmp"))
    else {
        return false;
    };
    let Some((stem, uuid)) = body.rsplit_once('-') else {
        return false;
    };
    let valid_stem = if manual {
        stem.strip_prefix("node-").is_some_and(positive_decimal)
    } else {
        positive_decimal(stem)
    };
    valid_stem && uuid.len() == 32 && uuid.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn positive_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u64>().is_ok_and(|number| number > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};
    use tempfile::TempDir;

    fn png_fixture(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec();
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    fn jpeg_fixture(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xc0, 0x00, 0x0b, 0x08];
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
        bytes
    }

    fn webp_vp8x_fixture(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"RIFF\x16\x00\x00\x00WEBPVP8X\x0a\x00\x00\x00\x00\x00\x00\x00".to_vec();
        let width = width - 1;
        let height = height - 1;
        bytes.extend_from_slice(&width.to_le_bytes()[..3]);
        bytes.extend_from_slice(&height.to_le_bytes()[..3]);
        bytes
    }

    #[test]
    fn cache_deletion_rejects_source_media_paths() {
        let temp = TempDir::new().unwrap();
        let cache = temp.path().join("cache");
        let source = temp.path().join("source.jpg");
        ensure_directories(&cache).unwrap();
        fs::write(&source, b"source").unwrap();
        let cache_operation = begin_cover_cache_operation();
        assert!(remove_cached_file(&cache_operation, &source, &cache).is_err());
        assert!(source.exists());
    }

    #[test]
    fn cache_clear_removes_only_owned_names() {
        let temp = TempDir::new().unwrap();
        let cache = temp.path().join("cache");
        ensure_directories(&cache).unwrap();
        let owned_bangumi = cache.join("bangumi/123.jpg");
        let owned_legacy_download = cache.join("bangumi/123.download");
        let owned_bangumi_temporary =
            cache.join("bangumi/.123-0123456789abcdef0123456789abcdef.tmp");
        let owned_manual = cache.join("manual/node-7.png");
        let owned_manual_temporary =
            cache.join("manual/.node-7-abcdef0123456789abcdef0123456789.tmp");
        let foreign_bangumi = cache.join("bangumi/family.jpg");
        let foreign_temporary = cache.join("bangumi/.123-not-a-uuid.tmp");
        let foreign_manual = cache.join("manual/notes.txt");
        fs::write(&owned_bangumi, b"owned").unwrap();
        fs::write(&owned_legacy_download, b"owned").unwrap();
        fs::write(&owned_bangumi_temporary, b"owned").unwrap();
        fs::write(&owned_manual, b"owned").unwrap();
        fs::write(&owned_manual_temporary, b"owned").unwrap();
        fs::write(&foreign_bangumi, b"foreign").unwrap();
        fs::write(&foreign_temporary, b"foreign").unwrap();
        fs::write(&foreign_manual, b"foreign").unwrap();
        assert_eq!(stats(&cache).unwrap().file_count, 5);

        let cache_clear = begin_cover_cache_clear();
        clear_cover_cache(&cache_clear, &cache).unwrap();

        assert!(!owned_bangumi.exists());
        assert!(!owned_legacy_download.exists());
        assert!(!owned_bangumi_temporary.exists());
        assert!(!owned_manual.exists());
        assert!(!owned_manual_temporary.exists());
        assert!(foreign_bangumi.exists());
        assert!(foreign_temporary.exists());
        assert!(foreign_manual.exists());
        assert_eq!(stats(&cache).unwrap().file_count, 0);
    }

    #[test]
    fn pending_cache_files_are_unique_same_directory_and_preserve_destination_on_failure() {
        let temp = TempDir::new().unwrap();
        let cache = temp.path().join("cache");
        ensure_directories(&cache).unwrap();
        let destination = cache.join("bangumi/123.jpg");
        fs::write(&destination, b"old-cover").unwrap();
        let cache_operation = begin_cover_cache_operation();

        let (mut first, mut first_file) = create_pending_cache_file(&destination, false).unwrap();
        let (second, second_file) = create_pending_cache_file(&destination, false).unwrap();
        assert_eq!(first.path().parent(), destination.parent());
        assert_eq!(second.path().parent(), destination.parent());
        assert_ne!(first.path(), second.path());
        first_file.write_all(b"new-cover").unwrap();
        first_file.flush().unwrap();
        first_file.sync_all().unwrap();
        drop(first_file);
        first.commit_to(&destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"new-cover");
        drop(second_file);
        drop(second);

        let (mut failed, mut failed_file) = create_pending_cache_file(&destination, false).unwrap();
        failed_file.write_all(b"must-not-replace").unwrap();
        failed_file.flush().unwrap();
        drop(failed_file);
        fs::remove_file(failed.path()).unwrap();
        assert!(failed.commit_to(&destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"new-cover");
        drop(cache_operation);
    }

    #[test]
    fn cache_clear_waits_for_an_in_flight_read_or_write_operation() {
        let cache_operation = begin_cover_cache_operation();
        let (started_sender, started_receiver) = mpsc::channel();
        let (acquired_sender, acquired_receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            started_sender.send(()).unwrap();
            let _cache_clear = begin_cover_cache_clear();
            acquired_sender.send(()).unwrap();
        });

        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        assert!(acquired_receiver
            .recv_timeout(Duration::from_millis(50))
            .is_err());
        drop(cache_operation);
        acquired_receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn custom_cache_rejects_unmarked_non_empty_directory() {
        let temp = TempDir::new().unwrap();
        let custom = temp.path().join("pictures");
        fs::create_dir(&custom).unwrap();
        fs::write(custom.join("family.jpg"), b"not a cache").unwrap();
        assert!(initialize_custom_cache(&custom).is_err());
        assert!(!custom.join(CACHE_MARKER_NAME).exists());
    }

    #[test]
    fn cache_location_rejects_overlap_in_both_directions() {
        let temp = TempDir::new().unwrap();
        let library = temp.path().join("library");
        fs::create_dir(&library).unwrap();
        assert!(validate_cache_location(&library.join("cache"), [library.clone()]).is_err());
        assert!(validate_cache_location(temp.path(), [library]).is_err());
    }

    #[test]
    fn cover_data_url_uses_content_signature() {
        let temp = TempDir::new().unwrap();
        let cover = temp.path().join("cover.bin");
        fs::write(&cover, png_fixture(600, 900)).unwrap();
        let cache_operation = begin_cover_cache_operation();
        let data_url = cover_data_url(&cache_operation, &cover).unwrap();
        assert!(data_url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn cover_data_url_rejects_abnormal_cached_pixel_dimensions() {
        let temp = TempDir::new().unwrap();
        let cover = temp.path().join("oversized.png");
        fs::write(&cover, png_fixture(20_000, 20_000)).unwrap();
        let cache_operation = begin_cover_cache_operation();

        let error = cover_data_url(&cache_operation, &cover).unwrap_err();

        assert!(error.contains("尺寸过大"));
    }

    #[test]
    fn manual_cover_rejects_files_over_size_limit_before_copying() {
        let temp = TempDir::new().unwrap();
        let cache = temp.path().join("cache");
        let source = temp.path().join("large.png");
        ensure_directories(&cache).unwrap();
        File::create(&source)
            .unwrap()
            .set_len(MAX_COVER_BYTES + 1)
            .unwrap();

        let cache_operation = begin_cover_cache_operation();
        let error = copy_manual_cover(&cache_operation, &cache, 1, &source).unwrap_err();

        assert!(error.contains("15 MiB"));
        assert!(!cache.join("manual/node-1.png").exists());
    }

    #[test]
    fn manual_cover_rejects_extension_signature_mismatch() {
        let temp = TempDir::new().unwrap();
        let cache = temp.path().join("cache");
        let source = temp.path().join("cover.jpg");
        ensure_directories(&cache).unwrap();
        fs::write(&source, png_fixture(600, 900)).unwrap();

        let cache_operation = begin_cover_cache_operation();
        let error = copy_manual_cover(&cache_operation, &cache, 2, &source).unwrap_err();

        assert!(error.contains("扩展名"));
        assert!(!cache.join("manual/node-2.jpg").exists());
    }

    #[test]
    fn manual_cover_rejects_abnormal_pixel_dimensions() {
        let temp = TempDir::new().unwrap();
        let cache = temp.path().join("cache");
        let source = temp.path().join("cover.png");
        ensure_directories(&cache).unwrap();
        fs::write(&source, png_fixture(20_000, 20_000)).unwrap();

        let cache_operation = begin_cover_cache_operation();
        let error = copy_manual_cover(&cache_operation, &cache, 3, &source).unwrap_err();

        assert!(error.contains("尺寸过大"));
        assert!(!cache.join("manual/node-3.png").exists());
    }

    #[test]
    fn manual_cover_atomically_replaces_existing_cache_file() {
        let temp = TempDir::new().unwrap();
        let cache = temp.path().join("cache");
        let source = temp.path().join("cover.png");
        ensure_directories(&cache).unwrap();
        fs::write(&source, png_fixture(600, 900)).unwrap();
        let cache_operation = begin_cover_cache_operation();
        let destination = copy_manual_cover(&cache_operation, &cache, 4, &source).unwrap();
        fs::write(&source, png_fixture(800, 1_200)).unwrap();

        let replaced = copy_manual_cover(&cache_operation, &cache, 4, &source).unwrap();

        assert_eq!(replaced, destination);
        assert_eq!(fs::read(destination).unwrap(), png_fixture(800, 1_200));
    }

    #[test]
    fn manual_cover_removes_temporary_file_when_commit_fails() {
        let temp = TempDir::new().unwrap();
        let cache = temp.path().join("cache");
        let source = temp.path().join("cover.png");
        ensure_directories(&cache).unwrap();
        fs::write(&source, png_fixture(600, 900)).unwrap();
        fs::create_dir(cache.join("manual/node-5.png")).unwrap();

        let cache_operation = begin_cover_cache_operation();
        assert!(copy_manual_cover(&cache_operation, &cache, 5, &source).is_err());
        let has_temporary_file = fs::read_dir(cache.join("manual"))
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"));
        assert!(!has_temporary_file);
    }

    #[test]
    fn manual_cover_dimension_parsers_support_allowed_formats() {
        assert_eq!(png_dimensions(&png_fixture(600, 900)).unwrap(), (600, 900));
        assert_eq!(
            jpeg_dimensions(&jpeg_fixture(640, 960)).unwrap(),
            (640, 960)
        );
        assert_eq!(
            webp_dimensions(&webp_vp8x_fixture(720, 1_080)).unwrap(),
            (720, 1_080)
        );
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeType {
    AutoWork,
    Work,
    Container,
    Mixed,
    Ignored,
}

impl NodeType {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::AutoWork => "AUTO_WORK",
            Self::Work => "WORK",
            Self::Container => "CONTAINER",
            Self::Mixed => "MIXED",
            Self::Ignored => "IGNORED",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "AUTO_WORK" => Self::AutoWork,
            "WORK" => Self::Work,
            "MIXED" => Self::Mixed,
            "IGNORED" => Self::Ignored,
            _ => Self::Container,
        }
    }

    pub fn is_work(self) -> bool {
        matches!(self, Self::AutoWork | Self::Work)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CoverSource {
    Bangumi,
    Manual,
    Placeholder,
}

impl CoverSource {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::Bangumi => "BANGUMI",
            Self::Manual => "MANUAL",
            Self::Placeholder => "PLACEHOLDER",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "BANGUMI" => Self::Bangumi,
            "MANUAL" => Self::Manual,
            _ => Self::Placeholder,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ViewMode {
    Grid,
    List,
}

impl ViewMode {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::Grid => "GRID",
            Self::List => "LIST",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CollectionSort {
    #[default]
    TitleAsc,
    TitleDesc,
    AddedDesc,
    AddedAsc,
    ModifiedDesc,
    ModifiedAsc,
    WatchedAsc,
    WatchedDesc,
}

impl CollectionSort {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::TitleAsc => "title-asc",
            Self::TitleDesc => "title-desc",
            Self::AddedDesc => "added-desc",
            Self::AddedAsc => "added-asc",
            Self::ModifiedDesc => "modified-desc",
            Self::ModifiedAsc => "modified-asc",
            Self::WatchedAsc => "watched-asc",
            Self::WatchedDesc => "watched-desc",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "title-desc" => Self::TitleDesc,
            "added-desc" => Self::AddedDesc,
            "added-asc" => Self::AddedAsc,
            "modified-desc" => Self::ModifiedDesc,
            "modified-asc" => Self::ModifiedAsc,
            "watched-asc" => Self::WatchedAsc,
            "watched-desc" => Self::WatchedDesc,
            _ => Self::TitleAsc,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CollectionSortScope {
    All,
    Browse,
    Favorites,
}

impl CollectionSortScope {
    pub fn setting_key(self) -> &'static str {
        match self {
            Self::All => "collection_sort_all",
            Self::Browse => "collection_sort_browse",
            Self::Favorites => "collection_sort_favorites",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSortPreferences {
    pub all: CollectionSort,
    pub browse: CollectionSort,
    pub favorites: CollectionSort,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LibraryRecognitionMode {
    Folder,
    VideoFile,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LibraryMediaKind {
    #[default]
    Video,
    Animation,
    LiveAction,
    Comic,
    Ebook,
}

impl LibraryMediaKind {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::Video => "VIDEO",
            Self::Animation => "ANIMATION",
            Self::LiveAction => "LIVE_ACTION",
            Self::Comic => "COMIC",
            Self::Ebook => "EBOOK",
        }
    }
    pub fn from_db(value: &str) -> Self {
        match value {
            "COMIC" => Self::Comic,
            "EBOOK" => Self::Ebook,
            "ANIMATION" => Self::Animation,
            "LIVE_ACTION" => Self::LiveAction,
            _ => Self::Video,
        }
    }
    pub fn is_book(self) -> bool {
        matches!(self, Self::Comic | Self::Ebook)
    }
    pub fn accepts_subject(self, subject_type: i64) -> bool {
        match self {
            Self::Video => matches!(subject_type, 2 | 6),
            Self::Animation => subject_type == 2,
            Self::LiveAction => subject_type == 6,
            Self::Comic | Self::Ebook => subject_type == 1,
        }
    }
}

impl LibraryRecognitionMode {
    pub fn as_db(&self) -> &'static str {
        match self {
            Self::Folder => "FOLDER",
            Self::VideoFile => "VIDEO_FILE",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "VIDEO_FILE" => Self::VideoFile,
            _ => Self::Folder,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRoot {
    pub media_kind: LibraryMediaKind,
    pub scan_health: Option<ScanHealth>,
    pub id: i64,
    pub path: String,
    pub display_name: String,
    pub created_at: String,
    pub last_scan_at: Option<String>,
    pub recognition_mode: LibraryRecognitionMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanHealth {
    #[serde(default)]
    pub warnings_ignored: bool,
    pub last_auto_attempt_at: Option<String>,
    pub last_success_at: Option<String>,
    pub outcome: String,
    pub error_count: u64,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataBinding {
    #[serde(default)]
    pub provider_aliases: Vec<String>,
    pub id: i64,
    pub node_id: i64,
    pub provider: String,
    pub provider_subject_id: i64,
    pub provider_subject_type: i64,
    pub provider_title: String,
    pub provider_title_cn: Option<String>,
    pub provider_title_en: Option<String>,
    pub provider_title_ja: Option<String>,
    pub provider_title_ko: Option<String>,
    pub provider_date: Option<String>,
    pub provider_image_url: Option<String>,
    pub bound_at: String,
    pub updated_at: String,
    pub cover_cache_path: Option<String>,
    pub cover_download_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserTag {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserTagMembership {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub assigned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteFolder {
    pub id: i64,
    pub name: String,
    pub item_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct BatchMutationResult {
    pub requested: u64,
    pub updated: u64,
    pub skipped: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaNode {
    #[serde(default)]
    pub media_kind: LibraryMediaKind,
    #[serde(default)]
    pub direct_comic_book_count: i64,
    #[serde(default)]
    pub child_comic_branch_count: i64,
    #[serde(default)]
    pub total_comic_book_count: i64,
    /// Latest source-file modification in this visible subtree, derived from the existing index.
    #[serde(default)]
    pub latest_file_modified_at: Option<String>,
    #[serde(default)]
    pub last_watched_at: Option<String>,
    pub id: i64,
    pub library_root_id: i64,
    pub parent_node_id: Option<i64>,
    pub absolute_path: String,
    pub folder_name: String,
    pub display_name: String,
    pub node_type: NodeType,
    pub manual_type_override: bool,
    pub cover_source: CoverSource,
    pub cover_cache_path: Option<String>,
    pub direct_video_count: i64,
    pub child_media_branch_count: i64,
    pub total_video_count: i64,
    pub created_at: String,
    pub updated_at: String,
    pub last_seen_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<MetadataBinding>,
    #[serde(default)]
    pub user_tags: Vec<UserTag>,
}

impl MediaNode {
    pub fn can_bind_bangumi(&self) -> bool {
        self.node_type.is_work()
            || (self.node_type == NodeType::Container
                && (self.total_video_count > 0 || self.total_comic_book_count > 0))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFile {
    pub id: i64,
    pub node_id: i64,
    pub absolute_path: String,
    pub file_name: String,
    pub extension: String,
    pub file_size: i64,
    pub modified_at: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub codec: Option<String>,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourceType {
    Document,
    Image,
    Audio,
    Subtitle,
    Archive,
    Font,
    Playlist,
    Other,
}

impl ResourceType {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::Document => "DOCUMENT",
            Self::Image => "IMAGE",
            Self::Audio => "AUDIO",
            Self::Subtitle => "SUBTITLE",
            Self::Archive => "ARCHIVE",
            Self::Font => "FONT",
            Self::Playlist => "PLAYLIST",
            Self::Other => "OTHER",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "DOCUMENT" => Self::Document,
            "IMAGE" => Self::Image,
            "AUDIO" => Self::Audio,
            "SUBTITLE" => Self::Subtitle,
            "ARCHIVE" => Self::Archive,
            "FONT" => Self::Font,
            "PLAYLIST" => Self::Playlist,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceFile {
    pub id: i64,
    pub node_id: i64,
    pub absolute_path: String,
    pub file_name: String,
    pub extension: String,
    pub file_size: i64,
    pub modified_at: String,
    pub resource_type: ResourceType,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrap {
    pub name: &'static str,
    pub version: &'static str,
    pub database_url: &'static str,
    pub build_date: &'static str,
    pub architecture: &'static str,
    pub website_url: &'static str,
    pub x_url: &'static str,
    pub update_recovery_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BreadcrumbItem {
    pub id: i64,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseResult {
    pub comic_books: Vec<crate::comics::ComicBook>,
    pub root: LibraryRoot,
    pub breadcrumbs: Vec<BreadcrumbItem>,
    pub nodes: Vec<MediaNode>,
    pub media_files: Vec<MediaFile>,
    pub resource_files: Vec<ResourceFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllResourcesResult {
    pub comic_nodes: Vec<MediaNode>,
    pub recognition_warnings: Vec<MediaNode>,
    pub nodes: Vec<MediaNode>,
    pub total_count: i64,
    pub works: Vec<WorkGroup>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkGroup {
    pub target: WorkTarget,
    pub node: MediaNode,
    pub sources: Vec<MediaNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkTarget {
    pub source_node_ids: Vec<i64>,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NestedMediaFile {
    pub file: MediaFile,
    pub source_node_id: i64,
    pub source_name: String,
    pub relative_directory: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentlyWatchedEntry {
    pub comic_book_id: Option<i64>,
    pub node: MediaNode,
    pub watched_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comic_books: Option<Vec<crate::comics::ComicBook>>,
    pub node: MediaNode,
    pub children: Vec<MediaNode>,
    pub media_files: Vec<MediaFile>,
    pub resource_files: Vec<ResourceFile>,
    pub breadcrumbs: Vec<BreadcrumbItem>,
    pub binding: Option<MetadataBinding>,
    pub work_sources: Option<Vec<MediaNode>>,
    pub work_target: Option<WorkTarget>,
    pub nested_media_files: Vec<NestedMediaFile>,
    pub expanded_folder_ids: Vec<i64>,
    pub recognition_warnings: Vec<MediaNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BangumiSearchPrefill {
    pub original_name: String,
    pub extracted_name: String,
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SearchHitKind {
    Node,
    MediaFile,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub kind: SearchHitKind,
    pub node: MediaNode,
    pub media_file: Option<MediaFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BangumiSubject {
    pub subject_id: i64,
    pub title: String,
    pub title_cn: Option<String>,
    #[serde(default)]
    pub title_en: Option<String>,
    #[serde(default)]
    pub title_ja: Option<String>,
    #[serde(default)]
    pub title_ko: Option<String>,
    /// Official aliases used for candidate ranking and persisted with bindings for local search.
    #[serde(default)]
    pub match_aliases: Vec<String>,
    pub date: Option<String>,
    pub image_url: Option<String>,
    pub summary: Option<String>,
    pub subject_type: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScanStatus {
    Idle,
    Running,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScanPhase {
    #[default]
    Scanning,
    AutoMatching,
}

impl ScanStatus {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::Idle => "IDLE",
            Self::Running => "RUNNING",
            Self::Cancelling => "CANCELLING",
            Self::Completed => "COMPLETED",
            Self::Cancelled => "CANCELLED",
            Self::Failed => "FAILED",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::Cancelling)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    #[serde(default)]
    pub background: bool,
    #[serde(default)]
    pub library_changed: Option<bool>,
    pub scan_id: String,
    pub root_id: i64,
    pub current_path: String,
    pub folders_scanned: u64,
    pub videos_found: u64,
    #[serde(default)]
    pub comic_books_found: u64,
    pub status: ScanStatus,
    pub errors: u64,
    pub message: Option<String>,
    #[serde(default)]
    pub phase: ScanPhase,
    #[serde(default)]
    pub auto_match_current: u64,
    #[serde(default)]
    pub auto_match_total: u64,
    #[serde(default)]
    pub auto_match_matched: u64,
    #[serde(default)]
    pub auto_match_pending: u64,
    #[serde(default)]
    pub auto_match_unmatched: u64,
    #[serde(default)]
    pub auto_match_errors: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStarted {
    pub scan_id: String,
}

pub type RebuildResult = ScanStarted;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub comic_reader: ComicReaderSettings,
    pub mpv_path: Option<String>,
    pub default_view_mode: ViewMode,
    pub video_extensions: Vec<String>,
    pub bangumi_search_enabled: bool,
    pub cover_cache_directory: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_auto_check_updates")]
    pub auto_check_updates: bool,
    #[serde(default = "default_auto_check_updates")]
    pub auto_scan_on_startup: bool,
    #[serde(default)]
    pub all_resources_flattened: bool,
}

pub const MAX_SETTINGS_PATH_CHARS: usize = 32_767;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ComicReaderSettings {
    pub direction: String,
    pub layout: String,
    pub mode: String,
    pub wide_page_alone: bool,
}
impl Default for ComicReaderSettings {
    fn default() -> Self {
        Self {
            direction: "LTR".into(),
            layout: "DOUBLE".into(),
            mode: "PAGED".into(),
            wide_page_alone: true,
        }
    }
}

impl AppSettings {
    pub fn validate_path_lengths(&self) -> Result<(), String> {
        if !matches!(self.comic_reader.direction.as_str(), "RTL" | "LTR")
            || !matches!(self.comic_reader.layout.as_str(), "SINGLE" | "DOUBLE")
            || !matches!(
                self.comic_reader.mode.as_str(),
                "PAGED" | "SCROLL" | "WEBTOON"
            )
        {
            return Err("COMIC_INVALID_SETTINGS".into());
        }
        if self
            .mpv_path
            .as_deref()
            .is_some_and(|path| path.chars().count() > MAX_SETTINGS_PATH_CHARS)
        {
            return Err(format!(
                "播放器路径不能超过 {MAX_SETTINGS_PATH_CHARS} 个字符。"
            ));
        }
        if self.cover_cache_directory.chars().count() > MAX_SETTINGS_PATH_CHARS {
            return Err(format!(
                "封面缓存目录不能超过 {MAX_SETTINGS_PATH_CHARS} 个字符。"
            ));
        }
        Ok(())
    }
}

/// Application-owned logical dimensions for the main window. This is intentionally separate
/// from `AppSettings`: frontend settings snapshots must not overwrite native window lifecycle
/// state with a stale value.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

fn default_language() -> String {
    "zh-CN".into()
}

fn default_theme() -> String {
    "system".into()
}

fn default_auto_check_updates() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub version: String,
    pub published_at: String,
    pub release_notes: std::collections::BTreeMap<String, String>,
    pub file_name: String,
    pub download_size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub distribution: UpdateDistribution,
    pub checked_at: String,
    pub update: Option<AvailableUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDownloadStatus {
    pub phase: UpdatePhase,
    pub version: Option<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
    pub can_install: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpdateDistribution {
    Portable,
    Nsis,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpdatePhase {
    Idle,
    Checking,
    Downloading,
    Ready,
    Applying,
    Failed,
}

impl Default for UpdateDownloadStatus {
    fn default() -> Self {
        Self {
            phase: UpdatePhase::Idle,
            version: None,
            downloaded_bytes: 0,
            total_bytes: None,
            error: None,
            can_install: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub file_count: u64,
    pub total_bytes: u64,
    pub cache_directory: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerTestResult {
    pub ok: bool,
    pub message: String,
    pub version: Option<String>,
}

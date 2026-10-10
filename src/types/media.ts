export type NodeType =
  | "AUTO_WORK"
  | "WORK"
  | "CONTAINER"
  | "MIXED"
  | "IGNORED";

export type CoverSource = "BANGUMI" | "MANUAL" | "PLACEHOLDER";
export type ViewMode = "grid" | "list";
export type CollectionSort = "title-asc" | "title-desc" | "added-desc" | "added-asc" | "modified-desc" | "modified-asc" | "watched-asc" | "watched-desc";
export type CollectionSortScope = "all" | "browse" | "favorites";
export type LibraryRecognitionMode = "FOLDER" | "VIDEO_FILE";
export type LibraryMediaKind = "VIDEO" | "ANIMATION" | "LIVE_ACTION" | "COMIC" | "EBOOK" | "DOUJIN" | "ARTBOOK";
export interface CollectionSortPreferences {
  all: CollectionSort;
  browse: CollectionSort;
  favorites: CollectionSort;
}
export type AppLanguage = "zh-CN" | "en-US" | "ja-JP" | "ko-KR";
export type AppTheme = "system" | "light" | "dark";
export type ScanStatus = "IDLE" | "RUNNING" | "CANCELLING" | "COMPLETED" | "CANCELLED" | "FAILED";
export type UpdatePhase = "IDLE" | "CHECKING" | "DOWNLOADING" | "READY" | "APPLYING" | "FAILED";
export type UpdateDistribution = "PORTABLE" | "NSIS";
export type UpdateRecoveryNotice = "ROLLED_BACK" | "RECOVERY_REQUIRED";
export type ResourceType =
  | "DOCUMENT"
  | "IMAGE"
  | "AUDIO"
  | "SUBTITLE"
  | "ARCHIVE"
  | "FONT"
  | "PLAYLIST"
  | "OTHER";

export interface ScanHealth {
  warningsIgnored?: boolean;
  lastAutoAttemptAt: string | null;
  lastSuccessAt: string | null;
  outcome: "SUCCESS" | "PARTIAL" | "FAILED" | "CANCELLED";
  errorCount: number;
  detail: string | null;
}
export interface WorkTarget { sourceNodeIds: number[]; snapshot: string; }
export interface NestedMediaFile { file: MediaFile; sourceNodeId: number; sourceName: string; relativeDirectory: string; }
export interface LibraryRoot {
  bookOrganizationStrategy?: "LEGACY" | "SMART_MIXED";
  autoBangumi?: boolean;
  mediaKind?: LibraryMediaKind;
  scanHealth?: ScanHealth | null;
  id: number;
  path: string;
  displayName: string;
  createdAt: string;
  lastScanAt: string | null;
  recognitionMode: LibraryRecognitionMode;
  nodeCount?: number;
  mediaCount?: number;
}

export interface MetadataBinding {
  providerAliases?: string[];
  id?: number;
  nodeId: number;
  provider: "BANGUMI";
  providerSubjectId: number;
  providerSubjectType: 1 | 2 | 6;
  providerTitle: string;
  providerTitleCn: string | null;
  providerTitleEn: string | null;
  providerTitleJa: string | null;
  providerTitleKo: string | null;
  providerDate: string | null;
  providerImageUrl: string | null;
  boundAt: string;
  updatedAt: string;
  coverCachePath: string | null;
  coverDownloadError: string | null;
}

/** A user-owned label stored in M²Shelf and never written into a media source. */
export interface UserTag {
  id: number;
  name: string;
  createdAt: string;
  updatedAt: string;
}

/** A global user tag together with its assignment state for one Node. */
export interface UserTagMembership extends UserTag {
  assigned: boolean;
}

/** A named, app-owned favorites folder. Membership never changes source media. */
export interface FavoriteFolder {
  id: number;
  name: string;
  itemCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface MediaNode {
  tmdbBinding?: import("./tmdb").TmdbBinding | null;
  mediaKind?: LibraryMediaKind;
  directComicBookCount?: number;
  childComicBranchCount?: number;
  totalComicBookCount?: number;
  workTarget?: WorkTarget;
  lastWatchedAt?: string | null;
  latestFileModifiedAt?: string | null;
  id: number;
  libraryRootId: number;
  parentNodeId: number | null;
  absolutePath: string;
  folderName: string;
  displayName: string;
  nodeType: NodeType;
  manualTypeOverride: boolean;
  coverSource: CoverSource;
  coverCachePath: string | null;
  directVideoCount?: number;
  childMediaBranchCount?: number;
  totalVideoCount?: number;
  createdAt: string;
  updatedAt: string;
  lastSeenAt: string;
  binding?: MetadataBinding | null;
  userTags: UserTag[];
  /** Frontend-only cache-busting token; never persisted or sent to the media source. */
  clientCoverRevision?: number;
  /** Presentation-only marker: open a fresh aggregate detail instead of one source node. */
  workView?: boolean;
}

export interface MediaFile {
  id: number;
  nodeId: number;
  absolutePath: string;
  fileName: string;
  extension: string;
  fileSize: number;
  modifiedAt: string;
  durationMs: number | null;
  width: number | null;
  height: number | null;
  codec: string | null;
  lastSeenAt: string;
}

/** A non-video file indexed as an attachment of its directory node. */
export interface ResourceFile {
  id: number;
  nodeId: number;
  absolutePath: string;
  fileName: string;
  extension: string;
  fileSize: number;
  modifiedAt: string;
  resourceType: ResourceType;
  lastSeenAt: string;
}

export interface BreadcrumbItem {
  id: number;
  displayName: string;
}

export interface BrowseResult {
  comicBooks?: import('./comic').ComicBook[];
  root: LibraryRoot;
  breadcrumbs: BreadcrumbItem[];
  nodes: MediaNode[];
  mediaFiles: MediaFile[];
  resourceFiles: ResourceFile[];
}

export interface AllResourcesResult {
  bookLibraries?: {root:LibraryRoot;catalogue:import('./catalogue').BookCatalogue}[];
  comicNodes?: MediaNode[];
  nodes: MediaNode[];
  totalCount: number;
  works: { node: MediaNode; sources: MediaNode[]; target?: WorkTarget }[];
  recognitionWarnings?: MediaNode[];
}

/** A locally recorded playback, ordered newest first by the native API. */
export interface RecentlyWatchedEntry {
  comicBook?:import('./comic').ComicBook|null;
  comicBookId?: number | null;
  node: MediaNode;
  watchedAt: string;
}

export interface SearchHit {
  kind: "NODE" | "MEDIA_FILE" | "COMIC_BOOK";
  comicBook?:import('./comic').ComicBook|null;
  node: MediaNode;
  mediaFile?: MediaFile | null;
}

export interface BangumiSubject {
  subjectId: number;
  title: string;
  titleCn: string | null;
  titleEn: string | null;
  titleJa: string | null;
  titleKo: string | null;
  matchAliases: string[];
  date: string | null;
  imageUrl: string | null;
  summary: string | null;
  subjectType: number;
}

export interface ScanProgress {
  background?: boolean;
  libraryChanged?: boolean | null;
  scanId: string;
  rootId: number;
  currentPath: string;
  foldersScanned: number;
  videosFound: number;
  comicBooksFound?:number;
  status: ScanStatus;
  errors: number;
  message?: string | null;
  phase: "SCANNING" | "AUTO_MATCHING";
  autoMatchCurrent: number;
  autoMatchTotal: number;
  autoMatchMatched: number;
  autoMatchPending: number;
  autoMatchUnmatched: number;
  autoMatchErrors: number;
}

/** Summary returned by one transactional multi-node metadata mutation. */
export interface BatchMutationResult {
  requested: number;
  updated: number;
  skipped: number;
}

export interface AppSettings {
  comicReader?: import('./comic').ComicReaderSettings;
  mpvPath: string | null;
  defaultViewMode: "GRID" | "LIST";
  videoExtensions: string[];
  bangumiSearchEnabled: boolean;
  autoCheckUpdates: boolean;
  autoScanOnStartup: boolean;
  allResourcesFlattened: boolean;
  language: AppLanguage;
  theme: AppTheme;
  coverCacheDirectory: string;
}

export interface AvailableUpdate {
  version: string;
  publishedAt: string;
  releaseNotes: Record<AppLanguage, string>;
  fileName: string;
  downloadSize: number;
  sha256: string;
}

export interface UpdateCheckResult {
  currentVersion: string;
  distribution: UpdateDistribution;
  checkedAt: string;
  update: AvailableUpdate | null;
}

export interface UpdateDownloadStatus {
  phase: UpdatePhase;
  version?: string | null;
  downloadedBytes: number;
  totalBytes?: number | null;
  error?: string | null;
  canInstall: boolean;
}

export interface CacheStats {
  fileCount: number;
  totalBytes: number;
  cacheDirectory: string;
}

export interface PosterCacheStatus {
  phase: "IDLE" | "QUEUED" | "RUNNING" | "COMPLETED" | "CANCELLED" | "FAILED";
  processed: number;
  total: number;
  failed: number;
  deferred: number;
  error: string | null;
}

export interface PosterCacheFailure {
  nodeId: number;
  name: string;
  reason: "SOURCE_READ" | "SOURCE_CHANGED" | "PROCESSING";
  detail: string;
}

export interface AppBootstrap {
  name: string;
  version: string;
  databaseUrl: string;
  buildDate: string;
  architecture: string;
  websiteUrl: string;
  xUrl: string;
  updateRecoveryNotice: UpdateRecoveryNotice | null;
}

export interface BangumiSearchPrefill {
  originalName: string;
  extractedName: string;
  candidates: string[];
}

export interface RebuildResult {
  scanId: string;
}

export interface ScanStarted {
  scanId: string;
}

export interface NodeDetail {
  comicBooks?: import('./comic').ComicBook[];
  workTarget?: WorkTarget | null;
  nestedMediaFiles?: NestedMediaFile[];
  expandedFolderIds?: number[];
  recognitionWarnings?: MediaNode[];
  node: MediaNode;
  children: MediaNode[];
  mediaFiles: MediaFile[];
  resourceFiles: ResourceFile[];
  breadcrumbs: BreadcrumbItem[];
  binding: MetadataBinding | null;
  workSources?: MediaNode[] | null;
}

export interface PlayerTestResult {
  ok: boolean;
  message: string;
  version: string | null;
}

export const isBookKind = (kind: LibraryMediaKind | undefined) => kind === "COMIC" || kind === "EBOOK" || kind === "DOUJIN" || kind === "ARTBOOK";
export type MediaKindFilter = "ALL" | Exclude<LibraryMediaKind, "VIDEO">;

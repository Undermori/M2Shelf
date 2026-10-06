import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AppBootstrap,
  AppSettings,
  AllResourcesResult,
  BangumiSubject,
  BangumiSearchPrefill,
  BatchMutationResult,
  BrowseResult,
  CacheStats,
  CollectionSort,
  CollectionSortPreferences,
  CollectionSortScope,
  FavoriteFolder,
  LibraryRoot,
  LibraryRecognitionMode,
  MediaNode,
  WorkTarget,
  MetadataBinding,
  NodeDetail,
  PlayerTestResult,
  RebuildResult,
  RecentlyWatchedEntry,
  ScanProgress,
  ScanStarted,
  SearchHit,
  UserTag,
  UserTagMembership,
  UpdateCheckResult,
  UpdateDownloadStatus,
  UpdateRecoveryNotice,
} from "../types/media";
import { translateActive } from "./i18n";

export class DesktopOnlyError extends Error {
  constructor() {
    super(translateActive("error.desktopOnly"));
    this.name = "DesktopOnlyError";
  }
}

export class M2ShelfError extends Error {
  constructor(message: string, readonly causeValue?: unknown, readonly command?: string) {
    super(message);
    this.name = "M2ShelfError";
  }
}

export function isStaleWorkError(error: unknown): boolean {
  return error instanceof M2ShelfError && String(error.causeValue).includes("WORK_TARGET_STALE");
}

export function isUnavailableNodeError(error: unknown): boolean {
  return String(error instanceof M2ShelfError ? error.causeValue : error).includes("NODE_NOT_VISIBLE");
}

const commandErrorKeys = {
  get_app_bootstrap: "error.initializationFailed",
  search_library: "error.localSearchFailed",
  list_recently_watched: "error.recentFailed",
  start_scan: "error.scanFailed",
  cancel_scan: "error.scanFailed",
  get_scan_status: "error.scanFailed",
  rebuild_index: "error.scanFailed",
  match_existing_content: "error.matchFailed",
  get_bangumi_search_prefill: "error.bangumiFailed",
  search_bangumi: "error.bangumiFailed",
  bind_bangumi: "error.bangumiFailed",
  bind_work_bangumi: "error.bangumiFailed",
  clear_work_bangumi_binding: "error.bangumiFailed",
  retry_work_bangumi_cover: "error.coverFailed",
  clear_bangumi_binding: "error.bangumiFailed",
  retry_bangumi_cover: "error.coverFailed",
  sync_pending_bangumi_aliases: "error.bangumiFailed",
  set_container_cover: "error.coverFailed",
  clear_node_cover: "error.coverFailed",
  get_cover_data_url: "error.coverFailed",
  play_media: "error.playerFailed",
  test_mpv: "error.playerFailed",
  open_library_root_in_explorer: "error.fileFailed",
  open_node_in_explorer: "error.fileFailed",
  open_media_in_explorer: "error.fileFailed",
  open_resource_file: "error.fileFailed",
  open_resource_in_explorer: "error.fileFailed",
  get_settings: "error.settingsFailed",
  update_settings: "error.settingsFailed",
  get_collection_sort_preferences: "error.settingsFailed",
  update_collection_sort_preference: "error.settingsFailed",
  get_cache_stats: "error.cacheFailed",
  open_cover_cache_directory: "error.cacheFailed",
  clear_cover_cache: "error.cacheFailed",
  open_bangumi_subject: "error.externalLinkFailed",
  open_external_url: "error.externalLinkFailed",
  list_user_tags: "error.tagsFailed",
  create_or_assign_user_tag: "error.tagsFailed",
  assign_user_tag: "error.tagsFailed",
  rename_user_tag: "error.tagsFailed",
  unassign_user_tag: "error.tagsFailed",
  delete_user_tag: "error.tagsFailed",
  batch_set_node_type: "error.batchFailed",
  batch_reset_node_type: "error.batchFailed",
  batch_assign_tag: "error.batchFailed",
  batch_create_and_assign_tag: "error.batchFailed",
  list_favorite_folders: "error.favoritesFailed",
  create_favorite_folder: "error.favoritesFailed",
  rename_favorite_folder: "error.favoritesFailed",
  delete_favorite_folder: "error.favoritesFailed",
  list_favorite_folder_nodes: "error.favoritesFailed",
  batch_add_nodes_to_favorite: "error.favoritesFailed",
  batch_remove_nodes_from_favorite: "error.favoritesFailed",
  check_for_update: "error.updateCheckFailed",
  download_update: "error.updateDownloadFailed",
  get_update_download_status: "error.updateDownloadFailed",
  install_downloaded_update: "error.updateInstallFailed",
  acknowledge_update_recovery_notice: "error.updateRecoveryAcknowledgeFailed",
} as const;

function commandErrorMessage(command: string): string {
  const key = commandErrorKeys[command as keyof typeof commandErrorKeys] ?? "error.commandFailed";
  return translateActive(key);
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) throw new DesktopOnlyError();
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    // Rust keeps precise diagnostics for logs/tests. UI receives stable localized copy instead
    // of leaking a Chinese backend string into English, Japanese, or Korean interfaces.
    throw new M2ShelfError(String(error).includes("WORK_TARGET_STALE") ? translateActive("works.changed") : commandErrorMessage(command), error, command);
  }
}

export const desktopAvailable = isTauri();

export const api = {
  bindWorkBangumi: (target: WorkTarget, subject: BangumiSubject) => call<MetadataBinding>("bind_work_bangumi", { target, subject }),
  retryWorkBangumiCover: (target: WorkTarget, failedSourceNodeIds: number[] = []) => call<MetadataBinding>("retry_work_bangumi_cover", { target, failedSourceNodeIds }),
  clearWorkBangumi: (target: WorkTarget) => call<void>("clear_work_bangumi_binding", { target }),
  bootstrap: () => call<AppBootstrap>("get_app_bootstrap"),
  acknowledgeUpdateRecoveryNotice: (notice: UpdateRecoveryNotice) =>
    call<void>("acknowledge_update_recovery_notice", { notice }),
  showMainWindow: () => call<void>("show_main_window"),
  listRoots: () => call<LibraryRoot[]>("list_library_roots"),
  addRoot: (path: string, recognitionMode: LibraryRecognitionMode) =>
    call<LibraryRoot>("add_library_root", { path, displayName: null, recognitionMode }),
  removeRoot: (rootId: number) => call<void>("remove_library_root", { rootId }),
  renameRoot: (rootId: number, displayName: string) =>
    call<LibraryRoot>("update_library_root_name", { rootId, displayName }),
  openRootInExplorer: (rootId: number) =>
    call<void>("open_library_root_in_explorer", { rootId }),
  browse: (rootId: number, parentNodeId: number | null = null) =>
    call<BrowseResult>("browse_library", { rootId, parentNodeId }),
  allResources: () => call<AllResourcesResult>("get_all_resources"),
  listRecentlyWatched: () => call<RecentlyWatchedEntry[]>("list_recently_watched"),
  nodeDetail: async (nodeId: number, workView = false) => {
    const detail = workView
      ? await call<NodeDetail>("get_work_detail", { nodeId })
      : await call<NodeDetail>("get_node_detail", { nodeId });
    if (workView) { detail.node.workView = true; detail.node.workTarget = detail.workTarget ?? undefined; }
    return detail;
  },
  search: (query: string, rootId?: number) =>
    call<SearchHit[]>("search_library", { query, rootId: rootId ?? null }),
  startScan: (rootId?: number, nodeId?: number, background = false) =>
    call<ScanStarted>("start_scan", { rootId: rootId ?? null, nodeId: nodeId ?? null, background }),
  cancelScan: (scanId: string) => call<boolean>("cancel_scan", { scanId }),
  scanStatus: () => call<ScanProgress | null>("get_scan_status"),
  setNodeType: (nodeId: number, nodeType: "WORK" | "CONTAINER" | "MIXED") =>
    call<MediaNode>("set_node_type", { nodeId, nodeType }),
  ignoreNode: (nodeId: number) => call<MediaNode>("set_node_type", { nodeId, nodeType: "IGNORED" }),
  listHiddenNodes: () => call<MediaNode[]>("list_hidden_nodes"),
  resetNodeType: (nodeId: number) => call<MediaNode>("reset_node_type", { nodeId }),
  renameNode: (nodeId: number, displayName: string) =>
    call<MediaNode>("set_node_display_name", { nodeId, displayName }),
  bangumiPrefill: (nodeId: number) =>
    call<BangumiSearchPrefill>("get_bangumi_search_prefill", { nodeId }),
  searchBangumi: (keyword: string, limit = 20) =>
    call<BangumiSubject[]>("search_bangumi", { keyword, limit }),
  bindBangumi: (nodeId: number, subject: BangumiSubject) =>
    call<MetadataBinding>("bind_bangumi", { nodeId, subject }),
  syncPendingBangumiAliases: () => call<boolean>("sync_pending_bangumi_aliases"),
  retryBangumiCover: (nodeId: number) =>
    call<MetadataBinding>("retry_bangumi_cover", { nodeId }),
  clearBangumi: (nodeId: number) => call<MediaNode>("clear_bangumi_binding", { nodeId }),
  setContainerCover: (nodeId: number, sourcePath: string) =>
    call<MediaNode>("set_container_cover", { nodeId, sourcePath }),
  clearNodeCover: (nodeId: number) => call<MediaNode>("clear_node_cover", { nodeId }),
  getCoverDataUrl: (nodeId: number) => call<string | null>("get_cover_data_url", { nodeId }),
  playMedia: (mediaFileId: number) => call<void>("play_media", { mediaFileId }),
  openInExplorer: (nodeId: number) => call<void>("open_node_in_explorer", { nodeId }),
  openMediaInExplorer: (mediaFileId: number) => call<void>("open_media_in_explorer", { mediaFileId }),
  openResourceFile: (resourceFileId: number) =>
    call<void>("open_resource_file", { resourceFileId }),
  openResourceInExplorer: (resourceFileId: number) =>
    call<void>("open_resource_in_explorer", { resourceFileId }),
  getSettings: () => call<AppSettings>("get_settings"),
  updateSettings: (settings: AppSettings) => call<AppSettings>("update_settings", { settings }),
  getCollectionSortPreferences: () =>
    call<CollectionSortPreferences>("get_collection_sort_preferences"),
  updateCollectionSortPreference: (scope: CollectionSortScope, sort: CollectionSort) =>
    call<CollectionSort>("update_collection_sort_preference", { scope, sort }),
  testMpv: (path?: string) => call<PlayerTestResult>("test_mpv", { path: path ?? null }),
  cacheStats: () => call<CacheStats>("get_cache_stats"),
  openCoverCacheDirectory: () => call<void>("open_cover_cache_directory"),
  clearCoverCache: () => call<CacheStats>("clear_cover_cache"),
  rebuildIndex: () => call<RebuildResult>("rebuild_index"),
  matchExistingContent: (nodeIds: number[] | null = null, rematchExisting = false) =>
    call<ScanStarted>("match_existing_content", { nodeIds, rematchExisting }),
  openBangumiSubject: (nodeId: number) => call<void>("open_bangumi_subject", { nodeId }),
  openExternalUrl: (url: string) => call<void>("open_external_url", { url }),
  listUserTags: (nodeId: number) =>
    call<UserTagMembership[]>("list_user_tags", { nodeId }),
  createOrAssignUserTag: (nodeId: number, name: string) =>
    call<UserTag>("create_or_assign_user_tag", { nodeId, name }),
  assignUserTag: (nodeId: number, tagId: number) =>
    call<UserTag>("assign_user_tag", { nodeId, tagId }),
  renameUserTag: (tagId: number, name: string) =>
    call<UserTag>("rename_user_tag", { tagId, name }),
  unassignUserTag: (nodeId: number, tagId: number) =>
    call<void>("unassign_user_tag", { nodeId, tagId }),
  deleteUserTag: (tagId: number) => call<void>("delete_user_tag", { tagId }),
  batchSetNodeType: (nodeIds: number[], nodeType: "WORK" | "CONTAINER" | "MIXED" | "IGNORED") =>
    call<BatchMutationResult>("batch_set_node_type", { nodeIds, nodeType }),
  batchResetNodeType: (nodeIds: number[]) =>
    call<BatchMutationResult>("batch_reset_node_type", { nodeIds }),
  batchAssignTag: (nodeIds: number[], tagId: number) =>
    call<BatchMutationResult>("batch_assign_tag", { nodeIds, tagId }),
  batchCreateAndAssignTag: (nodeIds: number[], name: string) =>
    call<BatchMutationResult>("batch_create_and_assign_tag", { nodeIds, name }),
  listFavoriteFolders: () => call<FavoriteFolder[]>("list_favorite_folders"),
  createFavoriteFolder: (name: string) =>
    call<FavoriteFolder>("create_favorite_folder", { name }),
  renameFavoriteFolder: (folderId: number, name: string) =>
    call<FavoriteFolder>("rename_favorite_folder", { folderId, name }),
  deleteFavoriteFolder: (folderId: number) =>
    call<void>("delete_favorite_folder", { folderId }),
  listFavoriteFolderNodes: (folderId: number) =>
    call<MediaNode[]>("list_favorite_folder_nodes", { folderId }),
  batchAddNodesToFavorite: (folderId: number, nodeIds: number[]) =>
    call<BatchMutationResult>("batch_add_nodes_to_favorite", { folderId, nodeIds }),
  batchRemoveNodesFromFavorite: (folderId: number, nodeIds: number[]) =>
    call<BatchMutationResult>("batch_remove_nodes_from_favorite", { folderId, nodeIds }),
  checkForUpdate: () => call<UpdateCheckResult>("check_for_update"),
  downloadUpdate: (version: string) =>
    call<UpdateDownloadStatus>("download_update", { version }),
  getUpdateDownloadStatus: () =>
    call<UpdateDownloadStatus>("get_update_download_status"),
  installDownloadedUpdate: (version: string) =>
    call<void>("install_downloaded_update", { version }),
};

export async function chooseDirectory(): Promise<string | null> {
  if (!isTauri()) throw new DesktopOnlyError();
  const selected = await localizedOpen({ directory: true, multiple: false, title: translateActive("dialog.mediaDirectory") });
  return typeof selected === "string" ? selected : null;
}

export async function choosePlayerExecutable(): Promise<string | null> {
  if (!isTauri()) throw new DesktopOnlyError();
  const selected = await localizedOpen({
    directory: false,
    multiple: false,
    title: translateActive("dialog.playerExecutable"),
    filters: [{ name: translateActive("dialog.windowsProgram"), extensions: ["exe"] }],
  });
  return typeof selected === "string" ? selected : null;
}

export async function chooseCoverImage(): Promise<string | null> {
  if (!isTauri()) throw new DesktopOnlyError();
  const selected = await localizedOpen({
    directory: false,
    multiple: false,
    title: translateActive("dialog.containerCover"),
    filters: [{ name: translateActive("dialog.images"), extensions: ["jpg", "jpeg", "png", "webp"] }],
  });
  return typeof selected === "string" ? selected : null;
}

export async function chooseCoverCacheDirectory(): Promise<string | null> {
  if (!isTauri()) throw new DesktopOnlyError();
  const selected = await localizedOpen({
    directory: true,
    multiple: false,
    title: translateActive("dialog.coverCacheDirectory"),
  });
  return typeof selected === "string" ? selected : null;
}

async function localizedOpen(options: Parameters<typeof open>[0]) {
  try {
    return await open(options);
  } catch (error) {
    throw new M2ShelfError(translateActive("error.dialogFailed"), error);
  }
}

export function onScanProgress(handler: (progress: ScanProgress) => void): Promise<UnlistenFn> {
  if (!isTauri()) return Promise.resolve(() => undefined);
  return listen<ScanProgress>("scan-progress", ({ payload }) => handler(payload));
}

export function onScanFinished(handler: (progress: ScanProgress) => void): Promise<UnlistenFn> {
  if (!isTauri()) return Promise.resolve(() => undefined);
  return listen<ScanProgress>("scan-completed", ({ payload }) => handler(payload));
}

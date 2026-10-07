import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { AllResourcesResult, AppBootstrap, AppSettings, AppTheme, BatchMutationResult, BrowseResult, CollectionSort, CollectionSortPreferences, CollectionSortScope, FavoriteFolder, LibraryRecognitionMode, LibraryRoot, MediaFile, MediaNode, MetadataBinding, NodeDetail, RecentlyWatchedEntry, ResourceFile, ScanProgress, SearchHit, UpdateCheckResult, UpdateDownloadStatus, UpdateRecoveryNotice, ViewMode } from "./types/media";
import { api, isScanBusyError, isStaleWorkError, isUnavailableNodeError, chooseCoverImage, chooseDirectory, desktopAvailable, onScanFinished, onScanProgress } from "./lib/api";
import { canBindBangumi, errorMessage } from "./lib/format";
import { SourceChoiceDialog } from "./components/SourceChoiceDialog";
import { BangumiModal } from "./components/BangumiModal";
import { BatchContextMenu, type BatchNodeAction } from "./components/BatchContextMenu";
import { BatchTagDialog } from "./components/BatchTagDialog";
import { HiddenNodesDialog } from "./components/HiddenNodesDialog";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { ContextMenu, type NodeAction } from "./components/ContextMenu";
import { EmptyState } from "./components/EmptyState";
import { FavoriteAssignmentDialog } from "./components/FavoriteAssignmentDialog";
import { FavoriteFolderDialog } from "./components/FavoriteFolderDialog";
import { Icon } from "./components/Icon";
import { LibraryRootContextMenu, type LibraryRootAction } from "./components/LibraryRootContextMenu";
import { LibraryRootRenameDialog } from "./components/LibraryRootRenameDialog";
import { LibraryRecognitionModeDialog } from "./components/LibraryRecognitionModeDialog";
import { RenameDialog } from "./components/RenameDialog";
import { ScanBanner } from "./components/ScanBanner";
import { Sidebar, type AppPage } from "./components/Sidebar";
import { TagManagerDialog } from "./components/TagManagerDialog";
import { ToastStack, type ToastKind, type ToastMessage } from "./components/Toast";
import { UpdateBanner } from "./components/UpdateBanner";
import { UpdateDialog } from "./components/UpdateDialog";
import { UpdateRecoveryDialog } from "./components/UpdateRecoveryDialog";
import type { UpdateFailureAction } from "./components/UpdateDialog";
import { BrowsePage } from "./pages/BrowsePage";
import { AllResourcesPage } from "./pages/AllResourcesPage";
import { OnboardingPage } from "./pages/OnboardingPage";
import { SearchPage } from "./pages/SearchPage";
import { RecentlyWatchedPage } from "./pages/RecentlyWatchedPage";
import { FavoritesPage } from "./pages/FavoritesPage";
import { SettingsPage } from "./pages/SettingsPage";
import { WorkDetailPage } from "./pages/WorkDetailPage";
import { ComicDetailPage } from './pages/ComicDetailPage';
import { ComicReaderPage } from './pages/ComicReaderPage';
import type {ComicBook} from './types/comic';
import { useI18n } from "./lib/i18n";

type ContextState = { node: MediaNode; x: number; y: number } | null;
type RootContextState = { root: LibraryRoot; x: number; y: number } | null;
type BatchContextState = { x: number; y: number } | null;
type ConfirmState = { title: string; description: string; confirmLabel?: string; destructive?: boolean; run: () => Promise<void> } | null;
type PendingRootAdd = { path: string; onboarding: boolean; playerPath: string | null } | null;
type NavigationSnapshot = {
  readerBookId:number|null;
  allMediaKind: 'ALL'|'VIDEO'|'COMIC'|'EBOOK';
  page: AppPage;
  selectedRootId: number | null;
  browseData: BrowseResult | null;
  currentNode: MediaNode | null;
  detail: NodeDetail | null;
  searchQuery: string;
  searchRootId: number | null;
  viewMode: ViewMode;
  allFilter: string;
  allTagFilterId: number | null;
  allSort: CollectionSort;
  browseFilter: string;
  browseTagFilterId: number | null;
  browseSort: CollectionSort;
  selectedFavoriteFolderId: number | null;
  favoriteNodes: MediaNode[] | null;
  favoriteFilter: string;
  favoriteSort: CollectionSort;
  scrollTop: number;
};
type BrowsingSectionKey = "all" | "search" | "recent" | "favorites" | `library:${number}`;
type HydratedNavigation = { snapshot: NavigationSnapshot; favoriteFolders?: FavoriteFolder[] };

const idleUpdateStatus: UpdateDownloadStatus = {
  phase: "IDLE",
  version: null,
  downloadedBytes: 0,
  totalBytes: null,
  error: null,
  canInstall: false,
};

function snapshotSort(snapshot: NavigationSnapshot, scope: CollectionSortScope): CollectionSort {
  if (scope === "all") return snapshot.allSort;
  if (scope === "browse") return snapshot.browseSort;
  return snapshot.favoriteSort;
}

function withSnapshotSort(snapshot: NavigationSnapshot, scope: CollectionSortScope, sort: CollectionSort): NavigationSnapshot {
  if (scope === "all") return { ...snapshot, allSort: sort };
  if (scope === "browse") return { ...snapshot, browseSort: sort };
  return { ...snapshot, favoriteSort: sort };
}

function withoutRemovedRoot(snapshot: NavigationSnapshot, rootId: number): NavigationSnapshot {
  if (snapshot.page === "library" && snapshot.selectedRootId === rootId) {
    return {
      ...snapshot,
      page: "all",
      selectedRootId: null,
      browseData: null,
      currentNode: null,
      detail: null,
      searchRootId: null,
      scrollTop: 0,
    };
  }
  if (snapshot.selectedRootId !== rootId && snapshot.searchRootId !== rootId) return snapshot;
  return {
    ...snapshot,
    selectedRootId: snapshot.selectedRootId === rootId ? null : snapshot.selectedRootId,
    searchRootId: snapshot.searchRootId === rootId ? null : snapshot.searchRootId,
  };
}

function withoutRemovedFavoriteFolder(snapshot: NavigationSnapshot, folderId: number): NavigationSnapshot {
  if (snapshot.page !== "favorites" || snapshot.selectedFavoriteFolderId !== folderId) return snapshot;
  return {
    ...snapshot,
    selectedFavoriteFolderId: null,
    favoriteNodes: null,
    favoriteFilter: "",
    scrollTop: 0,
  };
}

function patchNodeList(nodes: MediaNode[], nodeId: number, update: (node: MediaNode) => MediaNode): MediaNode[] {
  const index = nodes.findIndex((node) => node.id === nodeId);
  if (index < 0) return nodes;
  const next = nodes.slice();
  next[index] = update(nodes[index]);
  return next;
}

function nodeWithBinding(node: MediaNode, binding: MetadataBinding): MediaNode {
  if (node.id !== binding.nodeId) return node;
  const manualCover = node.coverSource === "MANUAL";
  const coverCachePath = manualCover ? node.coverCachePath : binding.coverCachePath;
  return {
    ...node,
    binding,
    coverCachePath,
    coverSource: manualCover ? "MANUAL" : coverCachePath ? "BANGUMI" : "PLACEHOLDER",
    updatedAt: binding.updatedAt,
    clientCoverRevision: (node.clientCoverRevision ?? 0) + 1,
  };
}

function nodeWithRefreshedCover(current: MediaNode, refreshed: MediaNode): MediaNode {
  return {
    ...refreshed,
    clientCoverRevision: (current.clientCoverRevision ?? 0) + 1,
  };
}

function browsingSectionKey(snapshot: NavigationSnapshot): BrowsingSectionKey | null {
  if (snapshot.page === "library") {
    return snapshot.detail == null && snapshot.selectedRootId != null
      ? `library:${snapshot.selectedRootId}`
      : null;
  }
  return snapshot.page === "all" || snapshot.page === "search" || snapshot.page === "recent" || snapshot.page === "favorites"
    ? snapshot.page
    : null;
}

function patchNavigationSnapshot(snapshot: NavigationSnapshot, nodeId: number, update: (node: MediaNode) => MediaNode): NavigationSnapshot {
  const browseNodes = snapshot.browseData ? patchNodeList(snapshot.browseData.nodes, nodeId, update) : null;
  const browseData = snapshot.browseData && browseNodes !== snapshot.browseData.nodes
    ? { ...snapshot.browseData, nodes: browseNodes! }
    : snapshot.browseData;
  const snapshotNode = snapshot.currentNode?.id === nodeId ? update(snapshot.currentNode) : snapshot.currentNode;
  const detailNode = snapshot.detail?.node.id === nodeId ? update(snapshot.detail.node) : snapshot.detail?.node;
  const detailChildren = snapshot.detail ? patchNodeList(snapshot.detail.children, nodeId, update) : null;
  const snapshotDetail = snapshot.detail && (detailNode !== snapshot.detail.node || detailChildren !== snapshot.detail.children)
    ? { ...snapshot.detail, node: detailNode!, children: detailChildren!, binding: detailNode?.id === nodeId ? detailNode.binding ?? null : snapshot.detail.binding }
    : snapshot.detail;
  const favoriteNodes = snapshot.favoriteNodes ? patchNodeList(snapshot.favoriteNodes, nodeId, update) : null;
  if (browseData === snapshot.browseData && snapshotNode === snapshot.currentNode && snapshotDetail === snapshot.detail && favoriteNodes === snapshot.favoriteNodes) return snapshot;
  return { ...snapshot, browseData, currentNode: snapshotNode, detail: snapshotDetail, favoriteNodes };
}

function App() {
  const { language, setLanguage, t, number } = useI18n();
  const [bootstrap, setBootstrap] = useState<AppBootstrap | null>(null);
  const [initialized, setInitialized] = useState(false);
  const [startupPresentationReady, setStartupPresentationReady] = useState(!desktopAvailable);
  const [roots, setRoots] = useState<LibraryRoot[]>([]);
  const [rootsLoading, setRootsLoading] = useState(desktopAvailable);
  const [page, setPage] = useState<AppPage>("all");
  const [selectedRootId, setSelectedRootId] = useState<number | null>(null);
  const [browseData, setBrowseData] = useState<BrowseResult | null>(null);
  const [allResources, setAllResources] = useState<AllResourcesResult | null>(null);
  const [allResourcesLoading, setAllResourcesLoading] = useState(desktopAvailable);
  const [recentlyWatched, setRecentlyWatched] = useState<RecentlyWatchedEntry[] | null>(null);
  const [recentlyWatchedLoading, setRecentlyWatchedLoading] = useState(desktopAvailable);
  const [favoriteFolders, setFavoriteFolders] = useState<FavoriteFolder[] | null>(null);
  const [favoriteNodes, setFavoriteNodes] = useState<MediaNode[] | null>(null);
  const [selectedFavoriteFolderId, setSelectedFavoriteFolderId] = useState<number | null>(null);
  const [favoritesLoading, setFavoritesLoading] = useState(false);
  const [favoriteFilter, setFavoriteFilter] = useState("");
  const [favoriteSort, setFavoriteSort] = useState<CollectionSort>("title-asc");
  const [currentNode, setCurrentNode] = useState<MediaNode | null>(null);
  const [detail, setDetail] = useState<NodeDetail | null>(null);
  const [readerBookId,setReaderBookId]=useState<number|null>(null);
  const [allMediaKind,setAllMediaKind]=useState<'ALL'|'VIDEO'|'COMIC'|'EBOOK'>('ALL');
  const [contentLoading, setContentLoading] = useState(false);
  const [viewMode, setViewMode] = useState<ViewMode>("grid");
  const [allFilter, setAllFilter] = useState("");
  const [allTagFilterId, setAllTagFilterId] = useState<number | null>(null);
  const [allResourcesFlattened, setAllResourcesFlattened] = useState(false);
  const allGrouping = allResourcesFlattened ? "works" : "folders";
  const startupScanAttempted = useRef(false);
  const [startupScanSettled, setStartupScanSettled] = useState(false);
  const refreshWorkGroupsRef = useRef<(nodeId: number) => void>(() => undefined);
  const [startupScanEnabled, setStartupScanEnabled] = useState(false);
  const [scanListenersReady, setScanListenersReady] = useState(false);
  const [allSort, setAllSort] = useState<CollectionSort>("title-asc");
  const [browseFilter, setBrowseFilter] = useState("");
  const [browseTagFilterId, setBrowseTagFilterId] = useState<number | null>(null);
  const [browseSort, setBrowseSort] = useState<CollectionSort>("title-asc");
  const [theme, setTheme] = useState<AppTheme>("system");
  // Per-node updatedAt/binding.updatedAt cache keys invalidate only the changed cover.
  const coverRevision = 0;
  const [scan, setScan] = useState<ScanProgress | null>(null);
  const [context, setContext] = useState<ContextState>(null);
  const [rootContext, setRootContext] = useState<RootContextState>(null);
  const [batchContext, setBatchContext] = useState<BatchContextState>(null);
  const [editMode, setEditMode] = useState(false);
  const [selectedNodeIds, setSelectedNodeIds] = useState<Set<number>>(() => new Set());
  const [batchTagsOpen, setBatchTagsOpen] = useState(false);
  const [favoriteAssignmentNodeIds, setFavoriteAssignmentNodeIds] = useState<number[]>([]);
  const [favoriteFolderDialog, setFavoriteFolderDialog] = useState<FavoriteFolder | "new" | null>(null);
  const [favoriteBusy, setFavoriteBusy] = useState(false);
  const [matchBusy, setMatchBusy] = useState(false);
  const [sourceChoice, setSourceChoice] = useState<{ action: NodeAction; sources: MediaNode[] } | null>(null);
  const [bangumiNode, setBangumiNode] = useState<MediaNode | null>(null);
  const [tagNode, setTagNode] = useState<MediaNode | null>(null);
  const [renameNode, setRenameNode] = useState<MediaNode | null>(null);
  const [renameRoot, setRenameRoot] = useState<LibraryRoot | null>(null);
  const [confirm, setConfirm] = useState<ConfirmState>(null);
  const [dialogBusy, setDialogBusy] = useState(false);
  const [hiddenNodesOpen, setHiddenNodesOpen] = useState(false);
  const [pendingRootAdd, setPendingRootAdd] = useState<PendingRootAdd>(null);
  const [rootAddBusy, setRootAddBusy] = useState(false);
  const [addedRootScanQueue, setAddedRootScanQueue] = useState<number[]>([]);
  const [addedRootScanAttempt, setAddedRootScanAttempt] = useState(0);
  const addedRootScanInFlight = useRef(false);
  const [toasts, setToasts] = useState<ToastMessage[]>([]);
  const [autoCheckUpdates, setAutoCheckUpdates] = useState(false);
  const [updateCheckResult, setUpdateCheckResult] = useState<UpdateCheckResult | null>(null);
  const [updateDownloadStatus, setUpdateDownloadStatus] = useState<UpdateDownloadStatus>(idleUpdateStatus);
  const [updateFailureAction, setUpdateFailureAction] = useState<UpdateFailureAction>(null);
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
  const [updateBannerDismissed, setUpdateBannerDismissed] = useState(false);
  const [updateInstallRequest, setUpdateInstallRequest] = useState(0);
  const [updateRecoveryBusy, setUpdateRecoveryBusy] = useState(false);
  const [updateRecoveryError, setUpdateRecoveryError] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchRootId, setSearchRootId] = useState<number | null>(null);
  const toastSequence = useRef(0);
  const updateCheckInFlight = useRef(false);
  const updateDownloadInFlight = useRef(false);
  const updateInstallInFlight = useRef(false);
  const updateRecoveryAcknowledgeInFlight = useRef(false);
  const autoUpdateCheckStarted = useRef(false);
  const updateBannerDismissedRef = useRef(false);
  const settingsAppearanceRef = useRef({ language, theme, autoCheckUpdates, allResourcesFlattened });
  const settingsAppearanceRevision = useRef(0);
  settingsAppearanceRef.current = { language, theme, autoCheckUpdates, allResourcesFlattened };
  const mainWindowShowRequested = useRef(false);
  const contentScrollRef = useRef<HTMLDivElement>(null);
  const navigationSnapshots = useRef<Map<number, NavigationSnapshot>>(new Map());
  const sectionSnapshots = useRef<Map<BrowsingSectionKey, NavigationSnapshot>>(new Map());
  const historyTrail = useRef<number[]>([0]);
  const historyCursor = useRef(0);
  const navigationSequence = useRef(0);
  const navigationGeneration = useRef(0);
  const rootsLoadGeneration = useRef(0);
  const allResourcesLoadGeneration = useRef(0);
  const recentlyWatchedLoadGeneration = useRef(0);
  const favoriteFoldersLoadGeneration = useRef(0);
  const favoriteNodesLoadGeneration = useRef(0);
  const currentRefreshGeneration = useRef(0);
  const favoritesLoadingState = useRef({ epoch: 0, sequence: 0, pending: new Set<number>() });
  const pendingScrollTop = useRef<number | null>(null);
  const matchOnlyScanIds = useRef<Set<string>>(new Set());
  // A match-only run can finish before its Tauri command Promise resolves (especially when
  // there are no eligible Nodes). Remember completed IDs so that the command continuation
  // cannot resurrect an already-finished run as a synthetic RUNNING banner.
  const finishedScanIds = useRef<Set<string>>(new Set());
  const [scrollRestoreEpoch, setScrollRestoreEpoch] = useState(0);
  const persistedSortPreferences = useRef<CollectionSortPreferences>({ all: "title-asc", browse: "title-asc", favorites: "title-asc" });
  const currentSortPreferences = useRef<CollectionSortPreferences>({ all: "title-asc", browse: "title-asc", favorites: "title-asc" });
  const displayedSortPreferences = useRef<CollectionSortPreferences>({ all: allSort, browse: browseSort, favorites: favoriteSort });
  displayedSortPreferences.current = { all: allSort, browse: browseSort, favorites: favoriteSort };
  const sortSaveRevisions = useRef<Record<CollectionSortScope, number>>({ all: 0, browse: 0, favorites: 0 });
  const sortSaveQueue = useRef<Promise<void>>(Promise.resolve());
  const activeSectionKey = page === "library"
    ? detail == null && selectedRootId != null ? `library:${selectedRootId}` as BrowsingSectionKey : null
    : page === "all" || page === "search" || page === "recent" || page === "favorites" ? page : null;
  const activeSectionKeyRef = useRef<BrowsingSectionKey | null>(activeSectionKey);
  activeSectionKeyRef.current = activeSectionKey;

  const beginFavoritesLoading = useCallback(() => {
    const state = favoritesLoadingState.current;
    const token = ++state.sequence;
    state.pending.add(token);
    setFavoritesLoading(true);
    return { epoch: state.epoch, token };
  }, []);

  const endFavoritesLoading = useCallback((request: { epoch: number; token: number }) => {
    const state = favoritesLoadingState.current;
    if (state.epoch !== request.epoch) return;
    state.pending.delete(request.token);
    if (state.pending.size === 0) setFavoritesLoading(false);
  }, []);

  const invalidateFavoritesLoads = useCallback(() => {
    const state = favoritesLoadingState.current;
    state.epoch += 1;
    state.pending.clear();
    favoriteFoldersLoadGeneration.current += 1;
    favoriteNodesLoadGeneration.current += 1;
    setFavoritesLoading(false);
  }, []);

  const queueScroll = useCallback((scrollTop: number) => {
    pendingScrollTop.current = scrollTop;
    setScrollRestoreEpoch((epoch) => epoch + 1);
  }, []);

  const applyPendingScroll = useCallback(() => {
    if (pendingScrollTop.current == null || !contentScrollRef.current) return;
    contentScrollRef.current.scrollTop = pendingScrollTop.current;
    pendingScrollTop.current = null;
  }, []);

  const captureNavigation = useCallback((): NavigationSnapshot => ({
    readerBookId,allMediaKind,
    page,
    selectedRootId,
    browseData,
    currentNode,
    detail,
    searchQuery,
    searchRootId,
    viewMode,
    allFilter,
    allTagFilterId,
    allSort,
    browseFilter,
    browseTagFilterId,
    browseSort,
    selectedFavoriteFolderId,
    favoriteNodes,
    favoriteFilter,
    favoriteSort,
    scrollTop: contentScrollRef.current?.scrollTop ?? 0,
  }), [readerBookId,allMediaKind,allFilter, allSort, allTagFilterId, browseData, browseFilter, browseSort, browseTagFilterId, currentNode, detail, favoriteFilter, favoriteNodes, favoriteSort, page, searchQuery, searchRootId, selectedFavoriteFolderId, selectedRootId, viewMode]);
  const captureNavigationRef = useRef(captureNavigation);
  captureNavigationRef.current = captureNavigation;

  const rememberSectionNavigation = useCallback((snapshot: NavigationSnapshot) => {
    const key = browsingSectionKey(snapshot);
    if (key) sectionSnapshots.current.set(key, snapshot);
  }, []);

  const rememberNavigation = useCallback((snapshot: NavigationSnapshot) => {
    rememberSectionNavigation(snapshot);
    const currentSequence = historyTrail.current[historyCursor.current];
    navigationSnapshots.current.set(currentSequence, snapshot);
    const nextSequence = ++navigationSequence.current;
    historyTrail.current = [
      ...historyTrail.current.slice(0, historyCursor.current + 1),
      nextSequence,
    ];
    historyCursor.current = historyTrail.current.length - 1;
    window.history.pushState(
      { m2ShelfNavigation: true, sequence: nextSequence },
      "",
      window.location.href,
    );
  }, [rememberSectionNavigation]);

  const restorePreviousNavigation = useCallback((previous: NavigationSnapshot) => {
    setReaderBookId(previous.readerBookId);setAllMediaKind(previous.allMediaKind);
    setPage(previous.page);
    setSelectedRootId(previous.selectedRootId);
    setBrowseData(previous.browseData);
    setCurrentNode(previous.currentNode);
    setDetail(previous.detail);
    setSearchQuery(previous.searchQuery);
    setSearchRootId(previous.searchRootId);
    setViewMode(previous.viewMode);
    setAllFilter(previous.allFilter);
    setAllTagFilterId(previous.allTagFilterId);
    setAllSort(previous.allSort);
    setBrowseFilter(previous.browseFilter);
    setBrowseTagFilterId(previous.browseTagFilterId);
    setBrowseSort(previous.browseSort);
    setSelectedFavoriteFolderId(previous.selectedFavoriteFolderId);
    setFavoriteNodes(previous.favoriteNodes);
    setFavoriteFilter(previous.favoriteFilter);
    setFavoriteSort(previous.favoriteSort);
    queueScroll(previous.scrollTop);
  }, [queueScroll]);

  const restoreSectionNavigation = useCallback((previous: NavigationSnapshot) => {
    setReaderBookId(null);setAllMediaKind(previous.allMediaKind);
    setPage(previous.page);
    setViewMode(previous.viewMode);
    setDetail(null);
    if (previous.page === "all") {
      setCurrentNode(null);
      setSearchRootId(null);
      setAllFilter(previous.allFilter);
      setAllTagFilterId(previous.allTagFilterId);
      setAllSort(previous.allSort);
    } else if (previous.page === "search") {
      setCurrentNode(null);
      const validSearchRootId = previous.searchRootId != null && roots.some((root) => root.id === previous.searchRootId)
        ? previous.searchRootId
        : null;
      setSelectedRootId(validSearchRootId ?? (previous.selectedRootId != null && roots.some((root) => root.id === previous.selectedRootId) ? previous.selectedRootId : roots[0]?.id ?? null));
      setSearchQuery(previous.searchQuery);
      setSearchRootId(validSearchRootId);
    } else if (previous.page === "recent") {
      setCurrentNode(null);
      setSearchRootId(null);
    } else if (previous.page === "favorites") {
      setCurrentNode(null);
      setSearchRootId(null);
      setSelectedFavoriteFolderId(previous.selectedFavoriteFolderId);
      setFavoriteNodes(previous.favoriteNodes);
      setFavoriteFilter(previous.favoriteFilter);
      setFavoriteSort(previous.favoriteSort);
    } else if (previous.page === "library") {
      setSelectedRootId(previous.selectedRootId);
      setBrowseData(previous.browseData);
      setCurrentNode(previous.currentNode);
      setBrowseFilter(previous.browseFilter);
      setBrowseTagFilterId(previous.browseTagFilterId);
      setBrowseSort(previous.browseSort);
    }
    queueScroll(previous.scrollTop);
  }, [queueScroll, roots]);

  useLayoutEffect(() => {
    if (page === "search") return;
    if ((page === "all" && allResourcesLoading)
      || (page === "recent" && recentlyWatchedLoading)
      || (page === "favorites" && favoritesLoading)
      || (page === "library" && contentLoading)) return;
    applyPendingScroll();
  }, [allResourcesLoading, applyPendingScroll, contentLoading, currentNode?.id, detail?.node.id, favoritesLoading, page, recentlyWatchedLoading, scrollRestoreEpoch]);

  useLayoutEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const resolved = theme === "system" ? (media.matches ? "dark" : "light") : theme;
      document.documentElement.dataset.theme = resolved;
      document.documentElement.dataset.themePreference = theme;
      document.documentElement.style.colorScheme = resolved;
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);

  useEffect(() => {
    if (!desktopAvailable || !startupPresentationReady || mainWindowShowRequested.current) return;
    mainWindowShowRequested.current = true;
    // Passive effects run after React has committed the shell, and the layout effect above has
    // already applied the persisted theme. The hidden native window therefore becomes visible at
    // its restored size with a painted first frame instead of flashing the configured fallback.
    void api.showMainWindow().catch((error) => {
      mainWindowShowRequested.current = false;
      console.error("failed to show the prepared main window", error);
    });
  }, [startupPresentationReady]);

  const toast = useCallback((text: string, kind: ToastKind = "info") => {
    const id = ++toastSequence.current;
    setToasts((items) => [...items, { id, text, kind }].slice(-4));
    window.setTimeout(() => setToasts((items) => items.filter((item) => item.id !== id)), 5000);
  }, []);

  const applySettingsAppearance = useCallback((settings: AppSettings): number => {
    if (settingsAppearanceRef.current.allResourcesFlattened !== settings.allResourcesFlattened) {
      setEditMode(false);
      setSelectedNodeIds(new Set());
    }
    settingsAppearanceRevision.current += 1;
    settingsAppearanceRef.current = {
      language: settings.language,
      theme: settings.theme,
      autoCheckUpdates: settings.autoCheckUpdates,
      allResourcesFlattened: settings.allResourcesFlattened,
    };
    setLanguage(settings.language);
    setTheme(settings.theme);
    setAutoCheckUpdates(settings.autoCheckUpdates);
    setAllResourcesFlattened(settings.allResourcesFlattened);
    return settingsAppearanceRevision.current;
  }, [setLanguage]);

  const handleSettingsPersistenceFailure = useCallback((failed: AppSettings, rollback: AppSettings | null, message: string, appearanceRevision: number) => {
    const current = settingsAppearanceRef.current;
    const failedAppearanceIsCurrent = current.language === failed.language
      && current.theme === failed.theme
      && current.autoCheckUpdates === failed.autoCheckUpdates
      && current.allResourcesFlattened === failed.allResourcesFlattened
      && settingsAppearanceRevision.current === appearanceRevision;
    if (rollback && failedAppearanceIsCurrent) applySettingsAppearance(rollback);
    toast(message, "error");
  }, [applySettingsAppearance, toast]);

  const acknowledgeUpdateRecoveryNotice = useCallback(async (notice: UpdateRecoveryNotice) => {
    if (updateRecoveryAcknowledgeInFlight.current) return;
    updateRecoveryAcknowledgeInFlight.current = true;
    setUpdateRecoveryBusy(true);
    setUpdateRecoveryError(false);
    try {
      await api.acknowledgeUpdateRecoveryNotice(notice);
      setBootstrap((current) => current?.updateRecoveryNotice === notice
        ? { ...current, updateRecoveryNotice: null }
        : current);
    } catch {
      // The modal isolates the background (including ToastStack), so keep this localized error
      // inside the active alertdialog where keyboard and assistive-technology users can reach it.
      setUpdateRecoveryError(true);
    } finally {
      updateRecoveryAcknowledgeInFlight.current = false;
      setUpdateRecoveryBusy(false);
    }
  }, []);

  const checkForUpdate = useCallback(async (manual: boolean) => {
    if (!desktopAvailable || updateCheckInFlight.current || updateDownloadInFlight.current || updateInstallInFlight.current) return;
    // Any explicit or automatic check consumes the one startup check for this app session. This
    // prevents the delayed startup timer from clearing a just-verified download selected by the
    // user during the first second after launch.
    autoUpdateCheckStarted.current = true;
    updateCheckInFlight.current = true;
    setUpdateFailureAction(null);
    setUpdateDownloadStatus({ ...idleUpdateStatus, phase: "CHECKING" });
    try {
      const result = await api.checkForUpdate();
      setUpdateCheckResult(result);
      setUpdateDownloadStatus(idleUpdateStatus);
      if (result.update) {
        if (manual) {
          setUpdateDialogOpen(true);
          setUpdateInstallRequest(0);
        } else if (!updateBannerDismissedRef.current) {
          setUpdateBannerDismissed(false);
        }
      } else {
        setUpdateBannerDismissed(true);
        if (manual) toast(t("update.currentSummary", { version: result.currentVersion }), "success");
      }
    } catch {
      // Startup checks are deliberately silent: offline use and missing release metadata must
      // never interrupt the first frame or surface an error. Manual checks use a localized toast
      // without exposing the native diagnostic string or opening an empty update dialog.
      if (manual) {
        setUpdateFailureAction(null);
        setUpdateDownloadStatus(idleUpdateStatus);
        toast(t("error.updateCheckFailed"), "error");
      } else {
        setUpdateDownloadStatus(idleUpdateStatus);
      }
    } finally {
      updateCheckInFlight.current = false;
    }
  }, [t, toast]);

  const downloadUpdate = useCallback(async (version: string) => {
    if (updateDownloadInFlight.current || updateInstallInFlight.current || updateCheckInFlight.current) return;
    updateDownloadInFlight.current = true;
    setUpdateFailureAction(null);
    updateBannerDismissedRef.current = true;
    setUpdateBannerDismissed(true);
    setUpdateDialogOpen(true);
    setUpdateDownloadStatus({ phase: "DOWNLOADING", version, downloadedBytes: 0, totalBytes: null, error: null, canInstall: false });
    try {
      const status = await api.downloadUpdate(version);
      setUpdateDownloadStatus({ ...status, error: null });
      if (status.phase === "FAILED") setUpdateFailureAction("download");
    } catch {
      setUpdateFailureAction("download");
      setUpdateDownloadStatus({ phase: "FAILED", version, downloadedBytes: 0, totalBytes: null, error: null, canInstall: false });
    } finally {
      updateDownloadInFlight.current = false;
    }
  }, []);

  const installDownloadedUpdate = useCallback(async (version: string) => {
    if (updateInstallInFlight.current || updateDownloadInFlight.current || updateCheckInFlight.current) return;
    updateInstallInFlight.current = true;
    setUpdateFailureAction(null);
    setUpdateDownloadStatus((status) => ({ ...status, phase: "APPLYING", version, error: null }));
    try {
      await api.installDownloadedUpdate(version);
    } catch {
      setUpdateFailureAction("install");
      setUpdateDownloadStatus((status) => ({ ...status, phase: "FAILED", version, error: null, canInstall: true }));
    } finally {
      updateInstallInFlight.current = false;
    }
  }, []);

  const requestUpdateInstallation = useCallback(() => {
    setUpdateDialogOpen(true);
    setUpdateInstallRequest((request) => request + 1);
  }, []);

  const retryUpdate = useCallback(() => {
    const version = updateDownloadStatus.version ?? updateCheckResult?.update?.version;
    if (updateFailureAction === "download" && version) {
      void downloadUpdate(version);
      return;
    }
    if (updateFailureAction === "install" && version) {
      setUpdateFailureAction(null);
      setUpdateDownloadStatus((status) => ({ ...status, phase: "READY", error: null, canInstall: true }));
      requestUpdateInstallation();
      return;
    }
    void checkForUpdate(true);
  }, [checkForUpdate, downloadUpdate, requestUpdateInstallation, updateCheckResult?.update?.version, updateDownloadStatus.version, updateFailureAction]);

  const closeUpdateDialog = useCallback(() => {
    setUpdateDialogOpen(false);
    setUpdateInstallRequest(0);
    updateBannerDismissedRef.current = true;
    setUpdateBannerDismissed(true);
  }, []);

  useEffect(() => {
    if (updateDownloadStatus.phase !== "DOWNLOADING") return;
    let active = true;
    let timer = 0;
    const poll = async () => {
      try {
        const status = await api.getUpdateDownloadStatus();
        if (!active) return;
        setUpdateDownloadStatus({ ...status, error: null });
        if (status.phase === "FAILED") {
          setUpdateFailureAction("download");
          return;
        }
        if (status.phase === "DOWNLOADING") timer = window.setTimeout(() => void poll(), 650);
      } catch {
        if (!active) return;
        // A progress-poll failure is not authoritative: the long-running download command still
        // owns completion and failure. Retry the read without enabling a duplicate download.
        timer = window.setTimeout(() => void poll(), 1000);
      }
    };
    timer = window.setTimeout(() => void poll(), 650);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [updateDownloadStatus.phase, updateDownloadStatus.version]);

  const hydrateNavigationSnapshot = useCallback(async (snapshot: NavigationSnapshot): Promise<HydratedNavigation> => {
    if (!desktopAvailable) return { snapshot };
    if (snapshot.page === "search") {
      const searchRootId = snapshot.searchRootId != null && roots.some((root) => root.id === snapshot.searchRootId)
        ? snapshot.searchRootId
        : null;
      return {
        snapshot: {
          ...snapshot,
          searchRootId,
          selectedRootId: searchRootId ?? (snapshot.selectedRootId != null && roots.some((root) => root.id === snapshot.selectedRootId) ? snapshot.selectedRootId : roots[0]?.id ?? null),
        },
      };
    }
    if (snapshot.page === "favorites") {
      const folders = await api.listFavoriteFolders();
      const selectedFavoriteFolderId = snapshot.selectedFavoriteFolderId != null
        && folders.some((folder) => folder.id === snapshot.selectedFavoriteFolderId)
        ? snapshot.selectedFavoriteFolderId
        : null;
      const favoriteNodes = selectedFavoriteFolderId != null
        ? await api.listFavoriteFolderNodes(selectedFavoriteFolderId)
        : null;
      return {
        snapshot: {
          ...snapshot,
          selectedFavoriteFolderId,
          favoriteNodes,
          scrollTop: selectedFavoriteFolderId == null && snapshot.selectedFavoriteFolderId != null ? 0 : snapshot.scrollTop,
        },
        favoriteFolders: folders,
      };
    }
    if (snapshot.page !== "library" || snapshot.selectedRootId == null) return { snapshot };
    const rootId = snapshot.selectedRootId;
    if (!roots.some((root) => root.id === rootId)) return { snapshot: withoutRemovedRoot(snapshot, rootId) };
    if (snapshot.detail) {
      try {
        const detail = await api.nodeDetail(snapshot.detail.node.id, snapshot.detail.node.workView);
        return { snapshot: { ...snapshot, detail, currentNode: detail.node } };
      } catch {
        const browseData = await api.browse(rootId, null);
        return { snapshot: { ...snapshot, browseData, currentNode: null, detail: null, scrollTop: 0 } };
      }
    }
    const parentNodeId = snapshot.currentNode?.id ?? null;
    try {
      const [browseData, currentNode] = await Promise.all([
        api.browse(rootId, parentNodeId),
        parentNodeId != null ? api.nodeDetail(parentNodeId).then((value) => value.node) : Promise.resolve(null),
      ]);
      return { snapshot: { ...snapshot, browseData, currentNode } };
    } catch (error) {
      if (parentNodeId == null) throw error;
      const browseData = await api.browse(rootId, null);
      return { snapshot: { ...snapshot, browseData, currentNode: null, detail: null, scrollTop: 0 } };
    }
  }, [roots]);

  const persistCollectionSort = useCallback((scope: CollectionSortScope, sort: CollectionSort, rollback: (value: CollectionSort) => void) => {
    const revision = ++sortSaveRevisions.current[scope];
    const sourceSectionKey = activeSectionKeyRef.current;
    const sourceSequence = historyTrail.current[historyCursor.current];
    currentSortPreferences.current = { ...currentSortPreferences.current, [scope]: sort };
    if (!desktopAvailable) return;
    sortSaveQueue.current = sortSaveQueue.current
      .catch(() => undefined)
      .then(async () => {
        try {
          const saved = await api.updateCollectionSortPreference(scope, sort);
          persistedSortPreferences.current = { ...persistedSortPreferences.current, [scope]: saved };
        } catch (error) {
          // An older failed request must never undo a newer A -> B -> A choice or
          // report an error for a preference that has already been superseded.
          if (sortSaveRevisions.current[scope] !== revision) return;
          const persisted = persistedSortPreferences.current[scope];
          currentSortPreferences.current = { ...currentSortPreferences.current, [scope]: persisted };
          if (activeSectionKeyRef.current === sourceSectionKey && displayedSortPreferences.current[scope] === sort) {
            displayedSortPreferences.current = { ...displayedSortPreferences.current, [scope]: persisted };
            rollback(persisted);
          }
          if (sourceSectionKey) {
            const sectionSnapshot = sectionSnapshots.current.get(sourceSectionKey);
            if (sectionSnapshot && snapshotSort(sectionSnapshot, scope) === sort) {
              sectionSnapshots.current.set(sourceSectionKey, withSnapshotSort(sectionSnapshot, scope, persisted));
            }
            const sourceSnapshot = navigationSnapshots.current.get(sourceSequence);
            if (sourceSnapshot && browsingSectionKey(sourceSnapshot) === sourceSectionKey && snapshotSort(sourceSnapshot, scope) === sort) {
              navigationSnapshots.current.set(sourceSequence, withSnapshotSort(sourceSnapshot, scope, persisted));
            }
          }
          toast(errorMessage(error), "error");
        }
      });
  }, [toast]);

  const changeAllSort = useCallback((sort: CollectionSort) => {
    setAllSort(sort);
    persistCollectionSort("all", sort, setAllSort);
  }, [persistCollectionSort]);

  const changeBrowseSort = useCallback((sort: CollectionSort) => {
    setBrowseSort(sort);
    persistCollectionSort("browse", sort, setBrowseSort);
  }, [persistCollectionSort]);

  const changeFavoriteSort = useCallback((sort: CollectionSort) => {
    setFavoriteSort(sort);
    persistCollectionSort("favorites", sort, setFavoriteSort);
  }, [persistCollectionSort]);

  const patchNodeEverywhere = useCallback((nodeId: number, update: (node: MediaNode) => MediaNode) => {
    setAllResources((current) => {
      if (!current) return current;
      const nodes = patchNodeList(current.nodes, nodeId, update);
      const comicNodes=current.comicNodes?patchNodeList(current.comicNodes,nodeId,update):undefined;
      return nodes === current.nodes && comicNodes===current.comicNodes ? current : { ...current, nodes,comicNodes };
    });
    setBrowseData((current) => {
      if (!current) return current;
      const nodes = patchNodeList(current.nodes, nodeId, update);
      return nodes === current.nodes ? current : { ...current, nodes };
    });
    setRecentlyWatched((current) => {
      if (!current) return current;
      const index = current.findIndex((entry) => entry.node.id === nodeId);
      if (index < 0) return current;
      const next = current.slice();
      next[index] = { ...current[index], node: update(current[index].node) };
      return next;
    });
    setFavoriteNodes((current) => current ? patchNodeList(current, nodeId, update) : current);
    setCurrentNode((current) => current?.id === nodeId ? update(current) : current);
    setBangumiNode((current) => current?.id === nodeId ? update(current) : current);
    setDetail((current) => {
      if (!current) return current;
      const node = current.node.id === nodeId ? update(current.node) : current.node;
      const children = patchNodeList(current.children, nodeId, update);
      if (node === current.node && children === current.children) return current;
      return { ...current, node, children, binding: node.id === nodeId ? node.binding ?? null : current.binding };
    });
    for (const [sequence, snapshot] of navigationSnapshots.current) {
      const patched = patchNavigationSnapshot(snapshot, nodeId, update);
      if (patched !== snapshot) navigationSnapshots.current.set(sequence, patched);
    }
    for (const [key, snapshot] of sectionSnapshots.current) {
      const patched = patchNavigationSnapshot(snapshot, nodeId, update);
      if (patched !== snapshot) sectionSnapshots.current.set(key, patched);
    }
    // A binding can split or join a work. Recompute that projection while keeping the ordinary
    // source-node patch path (including exact cover revisions) intact.
    refreshWorkGroupsRef.current(nodeId);
  }, []);

  const changeEditMode = useCallback((active: boolean) => {
    setEditMode(active);
    setBatchContext(null);
    setBatchTagsOpen(false);
    setFavoriteAssignmentNodeIds([]);
    if (!active) setSelectedNodeIds(new Set());
  }, []);

  const toggleNodeSelection = useCallback((node: MediaNode) => {
    setSelectedNodeIds((current) => {
      const next = new Set(current);
      if (next.has(node.id)) next.delete(node.id); else next.add(node.id);
      return next;
    });
  }, []);

  const selectNodeIds = useCallback((nodeIds: number[]) => {
    setSelectedNodeIds((current) => new Set([...current, ...nodeIds]));
  }, []);

  const clearNodeSelection = useCallback(() => setSelectedNodeIds(new Set()), []);

  const openNodeMenu = useCallback((event: React.MouseEvent, node: MediaNode) => {
    if (editMode) {
      setSelectedNodeIds((current) => current.has(node.id) ? current : new Set([...current, node.id]));
      setBatchContext({ x: event.clientX, y: event.clientY });
      return;
    }
    setContext({ node, x: event.clientX, y: event.clientY });
  }, [editMode]);

  const openBatchMenu = useCallback((event: React.MouseEvent<HTMLButtonElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    setBatchContext({ x: bounds.right, y: bounds.bottom + 4 });
  }, []);

  useEffect(() => {
    setEditMode(false);
    setSelectedNodeIds(new Set());
    setBatchContext(null);
    setBatchTagsOpen(false);
    setFavoriteAssignmentNodeIds([]);
  }, [currentNode?.id, detail?.node.id, page, selectedFavoriteFolderId, selectedRootId]);

  const loadRoots = useCallback(async (preferredRootId?: number | null) => {
    if (!desktopAvailable) return [] as LibraryRoot[];
    const requestGeneration = ++rootsLoadGeneration.current;
    const nextRoots = await api.listRoots();
    if (requestGeneration !== rootsLoadGeneration.current) return nextRoots;
    setRoots(nextRoots);
    setSelectedRootId((current) => {
      const candidate = preferredRootId ?? current;
      return nextRoots.some((root) => root.id === candidate) ? candidate : nextRoots[0]?.id ?? null;
    });
    return nextRoots;
  }, []);

  const loadAllResources = useCallback(async () => {
    if (!desktopAvailable) return null;
    const requestGeneration = ++allResourcesLoadGeneration.current;
    setAllResourcesLoading(true);
    try {
      const result = await api.allResources();
      if (requestGeneration === allResourcesLoadGeneration.current) setAllResources(result);
      return result;
    } catch (error) {
      if (requestGeneration !== allResourcesLoadGeneration.current) return null;
      throw error;
    } finally {
      if (requestGeneration === allResourcesLoadGeneration.current) setAllResourcesLoading(false);
    }
  }, []);

  const loadRecentlyWatched = useCallback(async () => {
    if (!desktopAvailable) return [] as RecentlyWatchedEntry[];
    const requestGeneration = ++recentlyWatchedLoadGeneration.current;
    setRecentlyWatchedLoading(true);
    try {
      const result = await api.listRecentlyWatched();
      if (requestGeneration === recentlyWatchedLoadGeneration.current) setRecentlyWatched(result);
      return result;
    } catch (error) {
      if (requestGeneration !== recentlyWatchedLoadGeneration.current) return [] as RecentlyWatchedEntry[];
      throw error;
    } finally {
      if (requestGeneration === recentlyWatchedLoadGeneration.current) setRecentlyWatchedLoading(false);
    }
  }, []);

  const loadFavoriteFolders = useCallback(async () => {
    if (!desktopAvailable) return [] as FavoriteFolder[];
    const requestGeneration = ++favoriteFoldersLoadGeneration.current;
    const loadingRequest = beginFavoritesLoading();
    try {
      const result = await api.listFavoriteFolders();
      if (requestGeneration === favoriteFoldersLoadGeneration.current) {
        setFavoriteFolders(result);
        setSelectedFavoriteFolderId((current) => current != null && result.some((folder) => folder.id === current) ? current : null);
      }
      return result;
    } catch (error) {
      if (requestGeneration !== favoriteFoldersLoadGeneration.current) return [] as FavoriteFolder[];
      throw error;
    } finally {
      endFavoritesLoading(loadingRequest);
    }
  }, [beginFavoritesLoading, endFavoritesLoading]);

  const loadFavoriteFolderNodes = useCallback(async (folderId: number) => {
    if (!desktopAvailable) return [] as MediaNode[];
    const requestGeneration = ++favoriteNodesLoadGeneration.current;
    const loadingRequest = beginFavoritesLoading();
    try {
      const result = await api.listFavoriteFolderNodes(folderId);
      if (requestGeneration === favoriteNodesLoadGeneration.current) setFavoriteNodes(result);
      return result;
    } catch (error) {
      if (requestGeneration !== favoriteNodesLoadGeneration.current) return [] as MediaNode[];
      throw error;
    } finally {
      endFavoritesLoading(loadingRequest);
    }
  }, [beginFavoritesLoading, endFavoritesLoading]);

  useEffect(() => {
    let active = true;
    if (!desktopAvailable) {
      setRootsLoading(false);
      setRecentlyWatchedLoading(false);
      setInitialized(true);
      return;
    }
    const settingsPromise = api.getSettings();
    const bootstrapPromise = api.bootstrap();
    // Recovery notices are durable until explicit acknowledgement. Store bootstrap as soon as it
    // resolves so an unrelated index/settings request cannot suppress the warning.
    void bootstrapPromise.then(
      (app) => { if (active) setBootstrap(app); },
      () => undefined,
    );
    // Apply persisted visual settings independently from heavier index queries. This lets the
    // prepared window appear promptly while the normal loading UI continues to hydrate.
    void settingsPromise.then(
      (settings) => {
        if (!active) return;
        setViewMode(settings.defaultViewMode === "LIST" ? "list" : "grid");
        setLanguage(settings.language ?? "zh-CN");
        setTheme(settings.theme ?? "system");
        setAutoCheckUpdates(settings.autoCheckUpdates ?? true);
        setStartupPresentationReady(true);
      },
      () => {
        if (active) setStartupPresentationReady(true);
      },
    );
    void Promise.all([bootstrapPromise, api.listRoots(), settingsPromise, api.getCollectionSortPreferences(), api.scanStatus(), api.allResources(), api.listRecentlyWatched().catch((error) => { toast(errorMessage(error), "error"); return [] as RecentlyWatchedEntry[]; })])
      .then(([_app, nextRoots, settings, sortPreferences, activeScan, resources, recentEntries]) => {
        if (!active) return;
        setAutoCheckUpdates(settings.autoCheckUpdates ?? true);
        setStartupScanEnabled(settings.autoScanOnStartup ?? true);
        setAllResourcesFlattened(settings.allResourcesFlattened ?? false);
        setRoots(nextRoots);
        setSelectedRootId(nextRoots[0]?.id ?? null);
        persistedSortPreferences.current = sortPreferences;
        currentSortPreferences.current = sortPreferences;
        setAllSort(sortPreferences.all);
        setBrowseSort(sortPreferences.browse);
        setFavoriteSort(sortPreferences.favorites);
        setAllResources(resources);
        setRecentlyWatched(recentEntries);
        if (activeScan?.status === "RUNNING" || activeScan?.status === "CANCELLING") setScan(activeScan);
      })
      .catch((error) => active && toast(t("app.initializationFailed", { error: errorMessage(error) }), "error"))
      .finally(() => { if (active) { setRootsLoading(false); setAllResourcesLoading(false); setRecentlyWatchedLoading(false); setInitialized(true); } });
    return () => { active = false; };
  // Bootstrap runs once for this app mount; the initial error uses the startup locale.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [setLanguage, toast]);

  useEffect(() => {
    if (!desktopAvailable || !initialized || !bootstrap || !autoCheckUpdates || bootstrap.updateRecoveryNotice || autoUpdateCheckStarted.current) return;
    const timer = window.setTimeout(() => {
      if (autoUpdateCheckStarted.current) return;
      autoUpdateCheckStarted.current = true;
      void checkForUpdate(false);
    }, 900);
    return () => window.clearTimeout(timer);
  }, [autoCheckUpdates, bootstrap, checkForUpdate, initialized]);

  const refreshCurrent = useCallback(async () => {
    if (!desktopAvailable) return;
    const refreshGeneration = ++currentRefreshGeneration.current;
    const sourceNavigationGeneration = navigationGeneration.current;
    const isCurrentRefresh = () => refreshGeneration === currentRefreshGeneration.current
      && sourceNavigationGeneration === navigationGeneration.current;
    if (page === "recent") {
      await loadRecentlyWatched();
      return;
    }
    if (page === "all") {
      await loadAllResources();
      return;
    }
    if (page === "favorites") {
      invalidateFavoritesLoads();
      await Promise.all([
        loadFavoriteFolders(),
        selectedFavoriteFolderId != null ? loadFavoriteFolderNodes(selectedFavoriteFolderId) : Promise.resolve([] as MediaNode[]),
      ]);
      return;
    }
    if (!selectedRootId || page !== "library") return;
    try {
      if (detail) {
        const next = await api.nodeDetail(detail.node.id, detail.node.workView);
        if (!isCurrentRefresh()) return;
        setDetail(next);
        setCurrentNode(next.node);
        return;
      }
      const rootId = currentNode?.libraryRootId ?? selectedRootId;
      const [next, refreshedNode] = await Promise.all([
        api.browse(rootId, currentNode?.id ?? null),
        currentNode ? api.nodeDetail(currentNode.id).then((value) => value.node) : Promise.resolve(null),
      ]);
      if (!isCurrentRefresh()) return;
      setBrowseData(next);
      if (refreshedNode) setCurrentNode(refreshedNode);
    } catch (error) {
      if (!isUnavailableNodeError(error)) throw error;
      if (!isCurrentRefresh()) return;
      const next = await api.browse(selectedRootId, null);
      if (!isCurrentRefresh()) return;
      setDetail(null);
      setCurrentNode(null);
      setBrowseData(next);
    }
  }, [currentNode, detail, invalidateFavoritesLoads, loadAllResources, loadFavoriteFolderNodes, loadFavoriteFolders, loadRecentlyWatched, page, selectedFavoriteFolderId, selectedRootId]);

  const refreshCurrentAndAllResources = useCallback(async () => {
    await Promise.all([
      refreshCurrent(),
      page === "all" ? Promise.resolve(null) : loadAllResources(),
    ]);
  }, [loadAllResources, page, refreshCurrent]);

  refreshWorkGroupsRef.current = (nodeId) => {
    void Promise.all([
      loadAllResources(),
      detail?.workSources?.some((source) => source.id === nodeId) ? refreshCurrent() : Promise.resolve(),
    ]).catch((error) => toast(errorMessage(error), "error"));
  };

  const handleScanProgress = useCallback((progress: ScanProgress) => {
    if (!finishedScanIds.current.has(progress.scanId)) {
      setScan((current) => progress.background && current?.scanId === progress.scanId ? current : progress);
    }
  }, []);

  const handleScanFinished = useCallback((progress: ScanProgress) => {
    const finishedIds = finishedScanIds.current;
    if (finishedIds.has(progress.scanId)) return;
    finishedIds.add(progress.scanId);
    if (finishedIds.size > 32) {
      const oldest = finishedIds.values().next().value;
      if (oldest) finishedIds.delete(oldest);
    }
    setScan((current) => current?.scanId === progress.scanId ? null : current);
    if (progress.background && progress.libraryChanged !== true) {
      void loadRoots(selectedRootId).catch(() => undefined);
      return;
    }
    const matchOnly = matchOnlyScanIds.current.delete(progress.scanId)
      || (progress.foldersScanned === 0 && progress.videosFound === 0 && progress.phase === "AUTO_MATCHING");
    const labels: Record<string, string> = { COMPLETED: t("app.scanCompleted"), CANCELLED: t("app.scanCancelled"), FAILED: t("app.scanFailed") };
    const summaryKey=progress.comicBooksFound&&progress.videosFound?'comic.scanMixedSummary':progress.comicBooksFound||(!progress.videosFound&&['COMIC','EBOOK'].includes(roots.find(root=>root.id===progress.rootId)?.mediaKind??'VIDEO'))?'comic.scanSummary':'app.scanSummary';
    const summary = matchOnly ? t(progress.status === "CANCELLED" ? "app.matchCancelled" : progress.status === "FAILED" ? "app.matchFailed" : "app.matchCompleted") : t(summaryKey, {
      status: labels[progress.status] ?? t("app.scanEnded"),
      videos: number(progress.videosFound),
      books: number(progress.comicBooksFound??0),
      errors: progress.errors ? t("app.scanErrorsSuffix", { count: number(progress.errors) }) : "",
    });
    const unmatched = progress.autoMatchUnmatched + (progress.autoMatchPending ?? 0);
    const hasAutoMatchSummary = progress.autoMatchMatched > 0 || unmatched > 0 || progress.autoMatchErrors > 0;
    const autoMatchSummary = hasAutoMatchSummary ? t("app.scanAutoMatchSummary", {
      matched: number(progress.autoMatchMatched),
      unmatched: number(unmatched),
      errors: number(progress.autoMatchErrors),
    }) : null;
    if (!progress.background) toast(autoMatchSummary ? `${summary} · ${autoMatchSummary}` : summary, progress.status === "FAILED" ? "error" : "success");
    void Promise.all([
      loadRoots(selectedRootId),
      refreshCurrent(),
      page === "all" ? Promise.resolve(null) : loadAllResources(),
      page === "recent" ? Promise.resolve([]) : loadRecentlyWatched(),
    ]).catch((error) => { if (!progress.background) toast(errorMessage(error), "error"); });
  }, [loadAllResources, loadRecentlyWatched, loadRoots, number, page, refreshCurrent, roots, selectedRootId, t, toast]);

  const scanProgressHandlerRef = useRef(handleScanProgress);
  const scanFinishedHandlerRef = useRef(handleScanFinished);
  scanProgressHandlerRef.current = handleScanProgress;
  scanFinishedHandlerRef.current = handleScanFinished;

  useEffect(() => {
    if (!desktopAvailable) return;
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    const attach = async (registration: Promise<() => void>, eventName: string) => {
      try {
        const unlisten = await registration;
        if (disposed) unlisten();
        else unlisteners.push(unlisten);
      } catch (error) {
        if (!disposed) console.error(`failed to subscribe to ${eventName}`, error);
      }
    };
    // Register once for the app lifetime. Latest-handler refs avoid dependency-driven listener
    // churn, while `disposed` closes the async registration/cleanup gap used by StrictMode.
    void Promise.all([
      attach(onScanProgress((progress) => scanProgressHandlerRef.current(progress)), "scan-progress"),
      attach(onScanFinished((progress) => scanFinishedHandlerRef.current(progress)), "scan-completed"),
    ]).then(async () => {
      if (disposed) return;
      // A short scan can finish while the two native listeners are still registering. The native
      // control retains its terminal snapshot, so reconcile once after both subscriptions settle.
      const current = await api.scanStatus();
      if (disposed) return;
      setScanListenersReady(true);
      if (current == null) return;
      if (current.status === "RUNNING" || current.status === "CANCELLING") {
        scanProgressHandlerRef.current(current);
      } else if (!finishedScanIds.current.has(current.scanId)) {
        scanFinishedHandlerRef.current(current);
      }
    }).catch((error) => {
      if (!disposed) console.error("failed to reconcile scan listeners", error);
    });
    return () => {
      disposed = true;
      unlisteners.splice(0).forEach((unlisten) => unlisten());
    };
  }, []);

  const selectRoot = useCallback(async (rootId: number, recordHistory = true) => {
    const returnSnapshot = recordHistory ? captureNavigation() : null;
    const requestGeneration = ++navigationGeneration.current;
    invalidateFavoritesLoads();
    setContentLoading(true);
    try {
      const next = await api.browse(rootId, null);
      if (requestGeneration !== navigationGeneration.current) return;
      if (returnSnapshot) rememberNavigation(returnSnapshot);
      setPage("library");
      setSelectedRootId(rootId);
      setDetail(null);
      setCurrentNode(null);
      setBrowseData(next);
      setBrowseFilter("");
      setBrowseTagFilterId(null);
      setBrowseSort(currentSortPreferences.current.browse);
      queueScroll(0);
    }
    catch (error) {
      if (requestGeneration !== navigationGeneration.current) return;
      setBrowseData(null);
      toast(errorMessage(error), "error");
    }
    finally {
      if (requestGeneration === navigationGeneration.current) setContentLoading(false);
    }
  }, [captureNavigation, invalidateFavoritesLoads, queueScroll, rememberNavigation, toast]);

  const navigateRootSection = useCallback((rootId: number) => {
    const current = captureNavigation();
    const targetKey: BrowsingSectionKey = `library:${rootId}`;
    if (browsingSectionKey(current) === targetKey) return;
    const previous = sectionSnapshots.current.get(targetKey);
    if (!previous) {
      void selectRoot(rootId);
      return;
    }
    const requestGeneration = ++navigationGeneration.current;
    invalidateFavoritesLoads();
    setContentLoading(false);
    // Keep the real source page mounted until the cached destination has fresh
    // indexed rows. A cancelled request therefore cannot overwrite the saved
    // destination with a transient null/zero-scroll state.
    void hydrateNavigationSnapshot(previous)
      .then(({ snapshot }) => {
        if (requestGeneration !== navigationGeneration.current) return;
        rememberNavigation(captureNavigationRef.current());
        sectionSnapshots.current.set(targetKey, snapshot);
        restoreSectionNavigation(snapshot);
      })
      .catch((error) => {
        if (requestGeneration === navigationGeneration.current) toast(errorMessage(error), "error");
      });
  }, [captureNavigation, hydrateNavigationSnapshot, invalidateFavoritesLoads, rememberNavigation, restoreSectionNavigation, selectRoot, toast]);

  const navigate = useCallback((next: AppPage) => {
    const current = captureNavigation();
    const targetKey: BrowsingSectionKey | null = next === "all" || next === "search" || next === "recent" || next === "favorites" ? next : null;
    if (targetKey && browsingSectionKey(current) === targetKey) {
      if (next === "search") setSearchRootId(null);
      return;
    }
    if (!targetKey && next === page) return;
    const requestGeneration = ++navigationGeneration.current;
    invalidateFavoritesLoads();
    setContentLoading(false);
    const previous = targetKey ? sectionSnapshots.current.get(targetKey) : null;
    if (previous) {
      if (previous.page === "favorites") {
        void hydrateNavigationSnapshot(previous)
          .then(({ snapshot, favoriteFolders: folders }) => {
            if (requestGeneration !== navigationGeneration.current) return;
            rememberNavigation(captureNavigationRef.current());
            if (folders) setFavoriteFolders(folders);
            sectionSnapshots.current.set("favorites", snapshot);
            restoreSectionNavigation(snapshot);
          })
          .catch((error) => {
            if (requestGeneration === navigationGeneration.current) toast(errorMessage(error), "error");
          });
      } else {
        rememberNavigation(current);
        restoreSectionNavigation(next === "search" ? { ...previous, searchRootId: null } : previous);
      }
      return;
    }
    rememberNavigation(current);
    queueScroll(0);
    if (next === "all") {
      setPage("all");
      setDetail(null);
      setCurrentNode(null);
      setSearchRootId(null);
      void loadAllResources().catch((error) => toast(errorMessage(error), "error"));
      return;
    }
    if (next === "recent") {
      setPage("recent");
      setDetail(null);
      setCurrentNode(null);
      setSearchRootId(null);
      void loadRecentlyWatched().catch((error) => toast(errorMessage(error), "error"));
      return;
    }
    if (next === "favorites") {
      setPage("favorites");
      setDetail(null);
      setCurrentNode(null);
      setSearchRootId(null);
      setSelectedFavoriteFolderId(null);
      setFavoriteNodes(null);
      setFavoriteFilter("");
      void loadFavoriteFolders().catch((error) => toast(errorMessage(error), "error"));
      return;
    }
    if (next === "search") {
      setSearchQuery("");
      setSearchRootId(null);
    }
    setPage(next);
  }, [captureNavigation, hydrateNavigationSnapshot, invalidateFavoritesLoads, loadAllResources, loadFavoriteFolders, loadRecentlyWatched, page, queueScroll, rememberNavigation, restoreSectionNavigation, selectedRootId, toast]);

  const openFavoriteFolder = useCallback(async (folderId: number) => {
    const returnSnapshot = captureNavigation();
    const requestGeneration = ++navigationGeneration.current;
    invalidateFavoritesLoads();
    const nodesRequestGeneration = ++favoriteNodesLoadGeneration.current;
    const loadingRequest = beginFavoritesLoading();
    try {
      const nodes = await api.listFavoriteFolderNodes(folderId);
      if (requestGeneration !== navigationGeneration.current || nodesRequestGeneration !== favoriteNodesLoadGeneration.current) return;
      rememberNavigation(returnSnapshot);
      setPage("favorites");
      setSelectedFavoriteFolderId(folderId);
      setFavoriteNodes(nodes);
      setFavoriteFilter("");
      setFavoriteSort(currentSortPreferences.current.favorites);
      queueScroll(0);
    } catch (error) {
      if (requestGeneration === navigationGeneration.current && nodesRequestGeneration === favoriteNodesLoadGeneration.current) toast(errorMessage(error), "error");
    } finally {
      endFavoritesLoading(loadingRequest);
    }
  }, [beginFavoritesLoading, captureNavigation, endFavoritesLoading, invalidateFavoritesLoads, queueScroll, rememberNavigation, toast]);

  useEffect(() => {
    if (initialized && desktopAvailable && selectedRootId && page === "library" && !detail && !browseData && !contentLoading) void selectRoot(selectedRootId, false);
  }, [browseData, contentLoading, detail, initialized, page, selectRoot, selectedRootId]);

  useEffect(() => {
    if (initialized && desktopAvailable && page === "all" && !allResources && !allResourcesLoading) {
      void loadAllResources().catch((error) => toast(errorMessage(error), "error"));
    }
  }, [allResources, allResourcesLoading, initialized, loadAllResources, page, toast]);

  const openNode = useCallback(async (node: MediaNode) => {
    const returnSnapshot = captureNavigation();
    const opensDetail = node.nodeType === "WORK" || node.nodeType === "AUTO_WORK" || node.nodeType === "CONTAINER";
    const requestGeneration = ++navigationGeneration.current;
    invalidateFavoritesLoads();
    setContentLoading(true);
    try {
      if (opensDetail) {
        const next = await api.nodeDetail(node.id, node.workView);
        if (requestGeneration !== navigationGeneration.current) return;
        rememberNavigation(returnSnapshot);
        setPage("library");
        setSelectedRootId(node.libraryRootId);
        setDetail(next); setCurrentNode(next.node);
      } else {
        const next = await api.browse(node.libraryRootId, node.id);
        if (requestGeneration !== navigationGeneration.current) return;
        rememberNavigation(returnSnapshot);
        setPage("library");
        setSelectedRootId(node.libraryRootId);
        setBrowseData(next); setCurrentNode(node); setDetail(null);
        setBrowseFilter("");
        setBrowseTagFilterId(null);
        setBrowseSort(currentSortPreferences.current.browse);
      }
      queueScroll(0);
    } catch (error) {
      if (requestGeneration === navigationGeneration.current) toast(errorMessage(error), "error");
    }
    finally {
      if (requestGeneration === navigationGeneration.current) setContentLoading(false);
    }
  }, [captureNavigation, invalidateFavoritesLoads, queueScroll, rememberNavigation, toast]);

  const openBreadcrumb = useCallback(async (nodeId: number, recordHistory = true) => {
    const returnSnapshot = recordHistory ? captureNavigation() : null;
    const requestGeneration = ++navigationGeneration.current;
    invalidateFavoritesLoads();
    setContentLoading(true);
    try {
      const nextDetail = await api.nodeDetail(nodeId);
      if (requestGeneration !== navigationGeneration.current) return;
      const opensDetail = nextDetail.node.nodeType === "WORK" || nextDetail.node.nodeType === "AUTO_WORK" || nextDetail.node.nodeType === "CONTAINER";
      const nextBrowse = opensDetail ? null : await api.browse(nextDetail.node.libraryRootId, nodeId);
      if (requestGeneration !== navigationGeneration.current) return;
      if (returnSnapshot) rememberNavigation(returnSnapshot);
      setSelectedRootId(nextDetail.node.libraryRootId);
      if (opensDetail) {
        setDetail(nextDetail); setCurrentNode(nextDetail.node);
      } else {
        setBrowseData(nextBrowse); setCurrentNode(nextDetail.node); setDetail(null);
        setBrowseFilter("");
        setBrowseTagFilterId(null);
        setBrowseSort(currentSortPreferences.current.browse);
      }
      queueScroll(0);
    } catch (error) {
      if (requestGeneration === navigationGeneration.current) toast(errorMessage(error), "error");
    }
    finally {
      if (requestGeneration === navigationGeneration.current) setContentLoading(false);
    }
  }, [captureNavigation, invalidateFavoritesLoads, queueScroll, rememberNavigation, toast]);

  const goRoot = useCallback((recordHistory = true) => { if (selectedRootId) void selectRoot(selectedRootId, recordHistory); }, [selectRoot, selectedRootId]);
  const goBack = useCallback(() => {
    navigationGeneration.current += 1;
    invalidateFavoritesLoads();
    setContentLoading(false);
    if (historyCursor.current > 0) {
      window.history.back();
      return;
    }
    if (page !== "library") return;
    const breadcrumbs = detail?.breadcrumbs ?? browseData?.breadcrumbs ?? [];
    const parent = breadcrumbs.length > 1 ? breadcrumbs[breadcrumbs.length - 2] : null;
    if (parent) void openBreadcrumb(parent.id, false);
  }, [browseData?.breadcrumbs, detail?.breadcrumbs, invalidateFavoritesLoads, openBreadcrumb, page]);

  useEffect(() => {
    window.history.replaceState(
      { m2ShelfNavigation: true, sequence: 0 },
      "",
      window.location.href,
    );
    const previousScrollRestoration = window.history.scrollRestoration;
    window.history.scrollRestoration = "manual";
    return () => { window.history.scrollRestoration = previousScrollRestoration; };
  }, []);

  useEffect(() => {
    const onPopState = (event: PopStateEvent) => {
      const state = event.state as { m2ShelfNavigation?: boolean; sequence?: number } | null;
      if (!state?.m2ShelfNavigation || typeof state.sequence !== "number") return;
      const targetIndex = historyTrail.current.indexOf(state.sequence);
      if (targetIndex < 0 || targetIndex === historyCursor.current) return;
      const previous = navigationSnapshots.current.get(state.sequence);
      if (!previous) return;
      const requestGeneration = ++navigationGeneration.current;
      invalidateFavoritesLoads();
      setContentLoading(false);
      const currentSequence = historyTrail.current[historyCursor.current];
      const current = captureNavigation();
      navigationSnapshots.current.set(currentSequence, current);
      rememberSectionNavigation(current);
      historyCursor.current = targetIndex;
      restorePreviousNavigation(previous);
      if (previous.page === "library" || previous.page === "favorites" || previous.page === "search") {
        void hydrateNavigationSnapshot(previous)
          .then(({ snapshot, favoriteFolders: folders }) => {
            if (requestGeneration !== navigationGeneration.current) return;
            navigationSnapshots.current.set(state.sequence!, snapshot);
            rememberSectionNavigation(snapshot);
            if (folders) setFavoriteFolders(folders);
            if (snapshot.page !== previous.page) {
              restorePreviousNavigation(snapshot);
              return;
            }
            if (snapshot.page === "library") {
              const sameTarget = previous.detail?.node.id === snapshot.detail?.node.id
                && previous.currentNode?.id === snapshot.currentNode?.id;
              setSelectedRootId(snapshot.selectedRootId);
              setBrowseData(snapshot.browseData);
              setCurrentNode(snapshot.currentNode);
              setDetail(snapshot.detail);
              if (!sameTarget) queueScroll(snapshot.scrollTop);
            } else if (snapshot.page === "favorites") {
              const sameTarget = previous.selectedFavoriteFolderId === snapshot.selectedFavoriteFolderId;
              setSelectedFavoriteFolderId(snapshot.selectedFavoriteFolderId);
              setFavoriteNodes(snapshot.favoriteNodes);
              if (!sameTarget) queueScroll(snapshot.scrollTop);
            } else if (snapshot.page === "search") {
              setSelectedRootId(snapshot.selectedRootId);
              setSearchRootId(snapshot.searchRootId);
            }
          })
          .catch((error) => {
            if (requestGeneration === navigationGeneration.current) toast(errorMessage(error), "error");
          });
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.altKey && !event.ctrlKey && !event.metaKey && event.key === "ArrowLeft") {
        event.preventDefault();
        goBack();
      }
    };
    window.addEventListener("popstate", onPopState);
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("popstate", onPopState);
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [captureNavigation, goBack, hydrateNavigationSnapshot, invalidateFavoritesLoads, queueScroll, rememberSectionNavigation, restorePreviousNavigation, toast]);

  const addRoot = useCallback(async () => {
    try {
      const path = await chooseDirectory();
      if (!path) return;
      setPendingRootAdd({ path, onboarding: false, playerPath: null });
    } catch (error) { toast(errorMessage(error), "error"); }
  }, [toast]);

  const finishOnboarding = async (path: string, mpvPath: string | null) => {
    setPendingRootAdd({ path, onboarding: true, playerPath: mpvPath });
  };

  const confirmRootRecognitionMode = async (mode: LibraryRecognitionMode,kind:import('./types/media').LibraryMediaKind) => {
    const request = pendingRootAdd;
    if (!request || rootAddBusy) return;
    setRootAddBusy(true);
    let rootCreated = false;
    try {
      const root = await api.addRoot(request.path, mode,kind);
      rootCreated = true;
      setPendingRootAdd(null);
      setAddedRootScanQueue(queue => [...queue, root.id]);
      if (request.playerPath) {
        const settings = await api.getSettings();
        await api.updateSettings({ ...settings, mpvPath: request.playerPath });
      }
      await loadRoots(root.id);
      await selectRoot(root.id, !request.onboarding);
      toast(t(request.onboarding ? "app.libraryCreated" : "app.rootAdded"), "success");
    } catch (error) {
      if (rootCreated) setPendingRootAdd(null);
      toast(errorMessage(error), "error");
    } finally {
      setRootAddBusy(false);
    }
  };

  const startScan = useCallback(async (rootId?: number, nodeId?: number, background = false, queued = false) => {
    if (scan) { if (!background && !queued) toast(t("app.scanAlreadyRunning"), "info"); return "RETRY"; }
    try {
      const started = await api.startScan(rootId, nodeId, background);
      if (!finishedScanIds.current.has(started.scanId)) setScan({ background, libraryChanged: false, scanId: started.scanId, rootId: rootId ?? selectedRootId ?? 0, currentPath: t("app.scanPreparing"), foldersScanned: 0, videosFound: 0, errors: 0, status: "RUNNING", phase: "SCANNING", autoMatchCurrent: 0, autoMatchTotal: 0, autoMatchMatched: 0, autoMatchPending: 0, autoMatchUnmatched: 0, autoMatchErrors: 0 });
      return "STARTED";
    } catch (error) { if (queued && isScanBusyError(error)) return "RETRY"; if (!background) toast(errorMessage(error), "error"); return "FAILED"; }
  }, [scan, selectedRootId, t, toast]);

  useEffect(() => {
    if (scan || !scanListenersReady || !addedRootScanQueue.length || addedRootScanInFlight.current) return;
    // A newly added library waits for the current scanner to finish instead of being skipped.
    const rootId = addedRootScanQueue[0];
    const timer = window.setTimeout(() => {
      addedRootScanInFlight.current = true;
      void startScan(rootId, undefined, false, true).then(result => {
        addedRootScanInFlight.current = false;
        if (result === "RETRY") setAddedRootScanAttempt(attempt => attempt + 1);
        else setAddedRootScanQueue(queue => queue.filter(id => id !== rootId));
      });
    }, 300);
    return () => window.clearTimeout(timer);
  }, [scan, scanListenersReady, addedRootScanQueue, addedRootScanAttempt, startScan]);

  useEffect(() => {
    if (!desktopAvailable || !initialized || !scanListenersReady || !bootstrap
      || bootstrap.updateRecoveryNotice || startupScanAttempted.current) return;
    startupScanAttempted.current = true;
    if (startupScanEnabled && roots.length > 0 && !scan) void startScan(undefined, undefined, true).finally(() => setStartupScanSettled(true));
    else setStartupScanSettled(true);
  }, [bootstrap, initialized, roots.length, scan, scanListenersReady, startScan, startupScanEnabled]);

  const aliasSyncBusy = useRef(false);
  const aliasSyncAttempted = useRef(false);
  const refreshAliasResultsRef = useRef(refreshCurrentAndAllResources);
  refreshAliasResultsRef.current = refreshCurrentAndAllResources;
  useEffect(() => {
    if (!desktopAvailable || !initialized || !bootstrap || bootstrap.updateRecoveryNotice || scan || aliasSyncBusy.current || aliasSyncAttempted.current || !scanListenersReady || !startupScanSettled) return;
    const timer = window.setTimeout(() => {
      aliasSyncBusy.current = true;
      aliasSyncAttempted.current = true;
      void api.syncPendingBangumiAliases().then(async changed => {
        if (changed) await refreshAliasResultsRef.current();
      }).catch(() => undefined).finally(() => { aliasSyncBusy.current = false; });
    }, 0);
    return () => window.clearTimeout(timer);
  }, [bootstrap, initialized, scan, scanListenersReady, startupScanSettled]);

  const cancelScan = async () => {
    if (!scan) return;
    setScan({ ...scan, status: "CANCELLING" });
    try { await api.cancelScan(scan.scanId); } catch (error) { toast(errorMessage(error), "error"); }
  };

  const play = async (file: MediaFile) => {
    const generation = navigationGeneration.current;
    try { await api.playMedia(file.id); void Promise.all([loadRecentlyWatched(), generation === navigationGeneration.current ? refreshCurrentAndAllResources() : loadAllResources()]).catch(() => undefined); }
    catch (error) { toast(t("app.playerLaunchFailed", { error: errorMessage(error) }), "error"); }
  };

  const revealMedia = async (file: MediaFile) => {
    try { await api.openMediaInExplorer(file.id); } catch (error) { toast(errorMessage(error), "error"); }
  };

  const openResource = async (file: ResourceFile) => {
    try { await api.openResourceFile(file.id); }
    catch (error) { toast(t("app.fileOpenFailed", { error: errorMessage(error) }), "error"); }
  };

  const revealResource = async (file: ResourceFile) => {
    try { await api.openResourceInExplorer(file.id); }
    catch (error) { toast(errorMessage(error), "error"); }
  };

  const mutateNode = async (run: () => Promise<unknown>, success: string) => {
    try { await run(); await Promise.all([refreshCurrentAndAllResources(), loadRoots(selectedRootId)]); toast(success, "success"); }
    catch (error) { toast(errorMessage(error), "error"); }
  };

  const retryCover = useCallback(async (node: MediaNode, imageDecodeFailed = false) => {
    try {
      const binding = node.workTarget ? await api.retryWorkBangumiCover(node.workTarget, imageDecodeFailed ? [node.id] : []) : await api.retryBangumiCover(node.id);
      if (node.workTarget) await refreshCurrentAndAllResources();
      patchNodeEverywhere(node.id, (current) => nodeWithBinding(current, binding));
      toast(binding.coverDownloadError ? t("app.coverBindingKept", { error: t("error.coverFailed") }) : t("app.coverRetrieved"), binding.coverDownloadError ? "info" : "success");
    } catch (error) { if (isStaleWorkError(error)) await refreshCurrentAndAllResources(); toast(t("app.coverRetryFailed", { error: errorMessage(error) }), "error"); }
  }, [patchNodeEverywhere, refreshCurrentAndAllResources, t, toast]);

  const refreshAfterBatch = useCallback(async () => {
    await Promise.all([refreshCurrent(), loadRoots(selectedRootId)]);
    if (page !== "all") void loadAllResources().catch(() => undefined);
    if (page !== "recent") void loadRecentlyWatched().catch(() => undefined);
  }, [loadAllResources, loadRecentlyWatched, loadRoots, page, refreshCurrent, selectedRootId]);

  const matchExisting = useCallback(async (nodeIds: number[] | null, rematchExisting: boolean) => {
    if (scan) { toast(t("app.scanAlreadyRunning"), "info"); return; }
    if (matchBusy || (nodeIds && nodeIds.length === 0)) return;
    setMatchBusy(true);
    setBatchContext(null);
    try {
      const uniqueNodeIds = nodeIds ? [...new Set(nodeIds)] : null;
      const started = await api.matchExistingContent(uniqueNodeIds, rematchExisting);
      if (!finishedScanIds.current.has(started.scanId)) {
        matchOnlyScanIds.current.add(started.scanId);
      }
      const currentStatus = await api.scanStatus();
      const sameRunIsActive = currentStatus?.scanId === started.scanId
        && (currentStatus.status === "RUNNING" || currentStatus.status === "CANCELLING");
      if (!finishedScanIds.current.has(started.scanId) && sameRunIsActive) {
        setScan(currentStatus);
      }
    } catch (error) {
      toast(errorMessage(error), "error");
    } finally {
      setMatchBusy(false);
    }
  }, [matchBusy, scan, t, toast]);

  const finishBatchMutation = useCallback(async (result: BatchMutationResult) => {
    await refreshAfterBatch();
    toast(t("app.batchUpdated", { updated: number(result.updated), skipped: number(result.skipped) }), "success");
  }, [number, refreshAfterBatch, t, toast]);

  const refreshFavorites = useCallback(async () => {
    invalidateFavoritesLoads();
    await Promise.all([
      loadFavoriteFolders(),
      selectedFavoriteFolderId != null ? loadFavoriteFolderNodes(selectedFavoriteFolderId) : Promise.resolve([] as MediaNode[]),
    ]);
  }, [invalidateFavoritesLoads, loadFavoriteFolderNodes, loadFavoriteFolders, selectedFavoriteFolderId]);

  const handleFavoriteApplied = useCallback(async (result: BatchMutationResult, folder: FavoriteFolder) => {
    await refreshFavorites();
    toast(t("app.favoriteAdded", { updated: number(result.updated), name: folder.name }), "success");
  }, [number, refreshFavorites, t, toast]);

  const saveFavoriteFolder = useCallback(async (name: string) => {
    const target = favoriteFolderDialog;
    if (!target) return;
    setDialogBusy(true);
    try {
      if (target === "new") {
        await api.createFavoriteFolder(name);
        toast(t("app.favoriteFolderCreated"), "success");
      } else {
        await api.renameFavoriteFolder(target.id, name);
        toast(t("app.favoriteFolderRenamed"), "success");
      }
      await loadFavoriteFolders();
      setFavoriteFolderDialog(null);
    } catch (error) {
      toast(errorMessage(error), "error");
    } finally {
      setDialogBusy(false);
    }
  }, [favoriteFolderDialog, loadFavoriteFolders, t, toast]);

  const deleteFavoriteFolder = useCallback((folder: FavoriteFolder) => {
    setConfirm({
      title: t("favorites.deleteTitle", { name: folder.name }),
      description: t("favorites.deleteDescription"),
      confirmLabel: t("favorites.deleteConfirm"),
      destructive: true,
      run: async () => {
        navigationGeneration.current += 1;
        invalidateFavoritesLoads();
        setContentLoading(false);
        await api.deleteFavoriteFolder(folder.id);
        navigationGeneration.current += 1;
        invalidateFavoritesLoads();
        setContentLoading(false);
        const favoriteSnapshot = sectionSnapshots.current.get("favorites");
        if (favoriteSnapshot) sectionSnapshots.current.set("favorites", withoutRemovedFavoriteFolder(favoriteSnapshot, folder.id));
        for (const [sequence, snapshot] of navigationSnapshots.current) {
          const safeSnapshot = withoutRemovedFavoriteFolder(snapshot, folder.id);
          if (safeSnapshot !== snapshot) navigationSnapshots.current.set(sequence, safeSnapshot);
        }
        if (captureNavigationRef.current().selectedFavoriteFolderId === folder.id) {
          setSelectedFavoriteFolderId(null);
          setFavoriteNodes(null);
          queueScroll(0);
        }
        await loadFavoriteFolders();
        toast(t("app.favoriteFolderDeleted"), "success");
      },
    });
  }, [invalidateFavoritesLoads, loadFavoriteFolders, t, toast]);

  const removeSelectedFromFavorite = useCallback(async () => {
    if (selectedFavoriteFolderId == null || selectedNodeIds.size === 0 || favoriteBusy) return;
    setFavoriteBusy(true);
    try {
      const result = await api.batchRemoveNodesFromFavorite(selectedFavoriteFolderId, [...selectedNodeIds]);
      await refreshFavorites();
      clearNodeSelection();
      toast(t("app.favoriteRemoved", { updated: number(result.updated) }), "success");
    } catch (error) {
      toast(errorMessage(error), "error");
    } finally {
      setFavoriteBusy(false);
    }
  }, [clearNodeSelection, favoriteBusy, number, refreshFavorites, selectedFavoriteFolderId, selectedNodeIds, t, toast]);

  const runBatchType = useCallback(async (nodeType: "WORK" | "CONTAINER" | "MIXED" | "IGNORED" | "AUTO") => {
    const nodeIds = [...selectedNodeIds];
    if (nodeIds.length === 0) return;
    setMatchBusy(true);
    try {
      const result = nodeType === "AUTO"
        ? await api.batchResetNodeType(nodeIds)
        : await api.batchSetNodeType(nodeIds, nodeType);
      await finishBatchMutation(result);
      if (nodeType === "IGNORED") clearNodeSelection();
    } catch (error) {
      toast(errorMessage(error), "error");
    } finally {
      setMatchBusy(false);
    }
  }, [clearNodeSelection, finishBatchMutation, selectedNodeIds, toast]);

  const batchAction = useCallback((action: BatchNodeAction) => {
    const count = selectedNodeIds.size;
    if (count === 0) return;
    if (action === "tags") { setBatchTagsOpen(true); return; }
    if (action === "favorites") { setFavoriteAssignmentNodeIds([...selectedNodeIds]); return; }
    if (action === "rematch") { void matchExisting([...selectedNodeIds], true); return; }
    if (action === "ignore") {
      setConfirm({
        title: t("selection.ignoreTitle"),
        description: t("selection.ignoreDescription", { count: number(count) }),
        confirmLabel: t("selection.ignoreConfirm"),
        destructive: true,
        run: () => runBatchType("IGNORED"),
      });
      return;
    }
    const type = action === "work" ? "WORK" : action === "container" ? "CONTAINER" : action === "other" ? "MIXED" : "AUTO";
    void runBatchType(type);
  }, [matchExisting, number, runBatchType, selectedNodeIds, t]);

  const requestBangumi = useCallback((node: MediaNode) => {
    if (!canBindBangumi(node)) {
      toast(t("app.resourceOnlyNoBangumi"), "info");
      return;
    }
    if (node.workView && !node.workTarget) { toast(t("works.changed"), "info"); return; }
    setBangumiNode(node);
  }, [t, toast]);

  const openSearch = useCallback((query: string, rootId: number | null) => {
    navigationGeneration.current += 1;
    invalidateFavoritesLoads();
    setContentLoading(false);
    rememberNavigation(captureNavigation());
    setSearchQuery(query);
    setSearchRootId(rootId);
    setPage("search");
    queueScroll(0);
  }, [captureNavigation, invalidateFavoritesLoads, queueScroll, rememberNavigation]);

  const nodeAction = async (action: NodeAction, node: MediaNode): Promise<void> => {
    if (node.workView && !["bangumi", "retry-cover", "clear-bangumi"].includes(action)) {
      const sources = (detail?.node.id === node.id ? detail.workSources : undefined) ?? allResources?.works.find(work => work.node.id === node.id)?.sources ?? [];
      if (sources.length === 0) { await refreshCurrentAndAllResources(); toast(t("works.changed"), "info"); return; }
      if (sources.length === 1) return nodeAction(action, sources[0]);
      setSourceChoice({ action, sources });
      return;
    }
    switch (action) {
      case "work": await mutateNode(() => api.setNodeType(node.id, "WORK"), t("app.setAsWork")); break;
      case "container": await mutateNode(() => api.setNodeType(node.id, "CONTAINER"), t("app.setAsContainer")); break;
      case "other": await mutateNode(() => api.setNodeType(node.id, "MIXED"), t("app.setAsOtherResources")); break;
      case "reset": await mutateNode(() => api.resetNodeType(node.id), t("app.resetAutomatic")); break;
      case "ignore": setConfirm({ title: t("app.ignoreTitle"), description: t("app.ignoreDescription", { name: node.displayName }), confirmLabel: t("app.ignoreConfirm"), destructive: true, run: async () => { await api.ignoreNode(node.id); await Promise.all([refreshCurrentAndAllResources(), loadRoots(node.libraryRootId)]); toast(t("app.ignored"), "success"); } }); break;
      case "rename": setRenameNode(node); break;
      case "tags": setTagNode(node); break;
      case "favorites": setFavoriteAssignmentNodeIds([node.id]); break;
      case "bangumi": requestBangumi(node); break;
      case "retry-cover": await retryCover(node); break;
      case "clear-bangumi": setConfirm({ title: t("app.clearBindingTitle"), description: t(node.workTarget ? "works.clearDescription" : "app.clearBindingDescription"), confirmLabel: t("app.clearBindingConfirm"), destructive: true, run: async () => { if (node.workTarget) { await api.clearWorkBangumi(node.workTarget); await refreshCurrentAndAllResources(); } else { const refreshed = await api.clearBangumi(node.id); patchNodeEverywhere(node.id, (current) => nodeWithRefreshedCover(current, refreshed)); } toast(t("app.bindingCleared"), "success"); } }); break;
      case "cover": try { const path = await chooseCoverImage(); if (path) { const refreshed = await api.setContainerCover(node.id, path); patchNodeEverywhere(node.id, (current) => nodeWithRefreshedCover(current, refreshed)); toast(t("app.containerCoverUpdated"), "success"); } } catch (error) { toast(errorMessage(error), "error"); } break;
      case "clear-cover": try { const refreshed = await api.clearNodeCover(node.id); patchNodeEverywhere(node.id, (current) => nodeWithRefreshedCover(current, refreshed)); toast(t("app.customCoverCleared"), "success"); } catch (error) { toast(errorMessage(error), "error"); } break;
      case "explorer": try { await api.openInExplorer(node.id); } catch (error) { toast(errorMessage(error), "error"); } break;
      case "scan": await startScan(node.libraryRootId, node.id); break;
    }
  };

  const removeRoot = (root: LibraryRoot) => setConfirm({
    title: t("app.removeRootTitle", { name: root.displayName }),
    description: t("app.removeRootDescription"),
    confirmLabel: t("app.removeRootConfirm"),
    destructive: true,
    run: async () => {
      navigationGeneration.current += 1;
      invalidateFavoritesLoads();
      setContentLoading(false);
      await api.removeRoot(root.id);
      const deletionGeneration = ++navigationGeneration.current;
      invalidateFavoritesLoads();
      setContentLoading(false);
      sectionSnapshots.current.delete(`library:${root.id}`);
      for (const [key, snapshot] of sectionSnapshots.current) {
        const safeSnapshot = withoutRemovedRoot(snapshot, root.id);
        if (safeSnapshot !== snapshot) sectionSnapshots.current.set(key, safeSnapshot);
      }
      for (const [sequence, snapshot] of navigationSnapshots.current) {
        const safeSnapshot = withoutRemovedRoot(snapshot, root.id);
        if (safeSnapshot !== snapshot) navigationSnapshots.current.set(sequence, safeSnapshot);
      }
      const liveNavigation = captureNavigationRef.current();
      if (liveNavigation.page === "library" && liveNavigation.selectedRootId === root.id) {
        setBrowseData(null);
        setDetail(null);
        setCurrentNode(null);
      }
      if (liveNavigation.page === "search" && liveNavigation.searchRootId === root.id) {
        setSearchRootId(null);
      }
      const next = await loadRoots();
      await loadAllResources();
      if (!next.length && navigationGeneration.current === deletionGeneration) setPage("all");
      toast(t("app.rootRemoved"), "success");
    },
  });

  const rootAction = async (action: LibraryRootAction, root: LibraryRoot) => {
    switch (action) {
      case "rename": setRenameRoot(root); break;
      case "explorer": try { await api.openRootInExplorer(root.id); } catch (error) { toast(errorMessage(error), "error"); } break;
      case "scan": await startScan(root.id); break;
    }
  };

  const runConfirm = async () => {
    if (!confirm) return;
    setDialogBusy(true);
    try { await confirm.run(); setConfirm(null); }
    catch (error) { if (isStaleWorkError(error)) { await refreshCurrentAndAllResources(); setConfirm(null); } toast(errorMessage(error), "error"); }
    finally { setDialogBusy(false); }
  };

  const handleBangumiBound = useCallback(async (binding: MetadataBinding) => {
    if (bangumiNode?.workTarget) await refreshCurrentAndAllResources();
    patchNodeEverywhere(binding.nodeId, (node) => nodeWithBinding(node, binding));
    toast(binding.coverDownloadError ? t("app.bindingSavedCoverFailed", { error: t("error.coverFailed") }) : t("app.bindingSaved"), binding.coverDownloadError ? "info" : "success");
  }, [bangumiNode, patchNodeEverywhere, refreshCurrentAndAllResources, t, toast]);

  const selectedRoot = useMemo(() => roots.find((root) => root.id === selectedRootId) ?? null, [roots, selectedRootId]);
  const ActiveDetailPage=detail?.node.mediaKind==='COMIC'||detail?.node.mediaKind==='EBOOK'?ComicDetailPage:WorkDetailPage;
  const openComic=(book:ComicBook)=>{rememberNavigation(captureNavigation());setReaderBookId(book.id);};
  const allProjectCount = (page === "all" && allGrouping === "works" ? allResources?.works.length : allResources?.totalCount)
    ?? roots.reduce((sum, root) => sum + (root.nodeCount ?? 0), 0);
  const projectCount = page === "all" || (page === "search" && searchRootId == null)
    ? allProjectCount
    : page === "recent"
      ? recentlyWatched?.length ?? 0
      : page === "favorites"
        ? selectedFavoriteFolderId != null
          ? favoriteNodes?.length ?? 0
          : favoriteFolders?.reduce((sum, folder) => sum + folder.itemCount, 0) ?? 0
    : selectedRoot?.nodeCount ?? 0;
  const showOnboarding = initialized && desktopAvailable && !rootsLoading && roots.length === 0;

  const updateUi = <>
    {updateCheckResult?.update && !updateDialogOpen && !updateBannerDismissed && <UpdateBanner update={updateCheckResult.update} onOpen={() => { setUpdateDialogOpen(true); setUpdateBannerDismissed(true); }} onDismiss={() => { updateBannerDismissedRef.current = true; setUpdateBannerDismissed(true); }} />}
    <UpdateDialog open={updateDialogOpen && !bootstrap?.updateRecoveryNotice} checkResult={updateCheckResult} downloadStatus={updateDownloadStatus} failureAction={updateFailureAction} confirmInstallRequest={updateInstallRequest} onClose={closeUpdateDialog} onDownload={(version) => void downloadUpdate(version)} onInstall={(version) => void installDownloadedUpdate(version)} onRetry={retryUpdate} />
    <UpdateRecoveryDialog notice={bootstrap?.updateRecoveryNotice ?? null} busy={updateRecoveryBusy} error={updateRecoveryError} onAcknowledge={(notice) => void acknowledgeUpdateRecoveryNotice(notice)} />
  </>;
  const rootModeUi = <LibraryRecognitionModeDialog path={pendingRootAdd?.path ?? null} busy={rootAddBusy} onChoose={(mode,kind) => void confirmRootRecognitionMode(mode,kind)} onClose={() => { if (!rootAddBusy) setPendingRootAdd(null); }} />;

  if (showOnboarding) return <><OnboardingPage onComplete={finishOnboarding} onError={(message) => toast(message, "error")} />{rootModeUi}{updateUi}<ToastStack toasts={toasts} onDismiss={(id) => setToasts((items) => items.filter((item) => item.id !== id))} /></>;
  if(readerBookId!==null)return <><ComicReaderPage key={readerBookId} bookId={readerBookId} onBack={()=>{goBack();void loadAllResources();}} onProgress={()=>{void loadRecentlyWatched();}}/><ToastStack toasts={toasts} onDismiss={id=>setToasts(items=>items.filter(item=>item.id!==id))}/></>;

  return (
    <div className="app-shell">
      <Sidebar page={page} roots={roots} selectedRootId={selectedRootId} loading={rootsLoading} onNavigate={navigate} onSelectRoot={navigateRootSection} onAddRoot={() => void addRoot()} onRootMenu={(event, root) => setRootContext({ root, x: event.clientX, y: event.clientY })} projectCount={projectCount} />
      <main className="main-content">
        {!desktopAvailable && <div className="web-preview-notice"><Icon name="info" />{t("app.previewNotice")}</div>}
        {scan && !scan.background && (scan.status === "RUNNING" || scan.status === "CANCELLING") && <ScanBanner progress={scan} mediaKind={roots.find(root=>root.id===scan.rootId)?.mediaKind} onCancel={() => void cancelScan()} />}
        <div className={`content-scroll${page === "settings" ? " is-settings" : ""}`} ref={contentScrollRef}>
          {page === "all" && <AllResourcesPage mediaKind={allMediaKind} onMediaKind={setAllMediaKind} grouping={allGrouping} data={allResources} loading={allResourcesLoading} viewMode={viewMode} onViewMode={setViewMode} filter={allFilter} onFilter={setAllFilter} tagFilterId={allTagFilterId} onTagFilter={setAllTagFilterId} sort={allSort} onSort={changeAllSort} onOpenNode={openNode} onMenu={openNodeMenu} onBangumi={requestBangumi} onRetryCover={retryCover} onScan={() => void startScan()} onAddRoot={() => void addRoot()} onSearch={(query) => openSearch(query, null)} coverRevision={coverRevision} editMode={editMode} selectedNodeIds={selectedNodeIds} matchBusy={matchBusy || favoriteBusy || Boolean(scan)} onEditMode={changeEditMode} onToggleSelection={toggleNodeSelection} onSelectAll={selectNodeIds} onClearSelection={clearNodeSelection} onBatchTags={() => setBatchTagsOpen(true)} onBatchFavorites={() => setFavoriteAssignmentNodeIds([...selectedNodeIds])} onBatchMenu={openBatchMenu} onMatch={(nodeIds, rematch) => void matchExisting(nodeIds, rematch)} />}
          {page === "recent" && <RecentlyWatchedPage entries={recentlyWatched} loading={recentlyWatchedLoading} viewMode={viewMode} onViewMode={setViewMode} onOpenNode={(node) => {const bookId=recentlyWatched?.find(e=>e.node.id===node.id)?.comicBookId;if(bookId)openComic({id:bookId} as ComicBook);else void openNode(node);}} onMenu={(event, node) => setContext({ node, x: event.clientX, y: event.clientY })} onBangumi={requestBangumi} onRetryCover={(node, failed) => void retryCover(node, failed)} coverRevision={coverRevision} />}
          {page === "favorites" && <FavoritesPage folders={favoriteFolders} nodes={favoriteNodes} selectedFolderId={selectedFavoriteFolderId} loading={favoritesLoading} viewMode={viewMode} onViewMode={setViewMode} filter={favoriteFilter} onFilter={setFavoriteFilter} sort={favoriteSort} onSort={changeFavoriteSort} onOpenFolder={(folderId) => void openFavoriteFolder(folderId)} onBack={goBack} onCreate={() => setFavoriteFolderDialog("new")} onRename={setFavoriteFolderDialog} onDelete={deleteFavoriteFolder} onOpenNode={openNode} onMenu={openNodeMenu} onBangumi={requestBangumi} onRetryCover={retryCover} coverRevision={coverRevision} editMode={editMode} selectedNodeIds={selectedNodeIds} busy={favoriteBusy || matchBusy || Boolean(scan)} onEditMode={changeEditMode} onToggleSelection={toggleNodeSelection} onSelectAll={selectNodeIds} onClearSelection={clearNodeSelection} onBatchTags={() => setBatchTagsOpen(true)} onBatchFavorites={() => setFavoriteAssignmentNodeIds([...selectedNodeIds])} onBatchMenu={openBatchMenu} onRematch={() => void matchExisting([...selectedNodeIds], true)} onRemoveSelected={() => void removeSelectedFromFavorite()} />}
          {page === "library" && !selectedRoot && <EmptyState eyebrow={t("app.emptyEyebrow")} title={t("app.emptyTitle")} description={desktopAvailable ? t("app.emptyDesktop") : t("app.emptyWeb")} action={<button className="button primary" disabled={!desktopAvailable} onClick={() => void addRoot()} type="button"><Icon name="plus" />{t("app.addMediaDirectory")}</button>} />}
          {page === "library" && selectedRoot && !detail && <BrowsePage onReadComic={openComic} data={browseData} currentNode={currentNode} loading={contentLoading} viewMode={viewMode} onViewMode={setViewMode} filter={browseFilter} onFilter={setBrowseFilter} tagFilterId={browseTagFilterId} onTagFilter={setBrowseTagFilterId} sort={browseSort} onSort={changeBrowseSort} onRoot={goRoot} onBreadcrumb={(id) => void openBreadcrumb(id)} onOpenNode={openNode} onMenu={openNodeMenu} onBangumi={requestBangumi} onRetryCover={retryCover} onPlay={(file) => void play(file)} onRevealMedia={(file) => void revealMedia(file)} onOpenResource={(file) => void openResource(file)} onRevealResource={(file) => void revealResource(file)} onScan={() => void startScan(selectedRoot.id, currentNode?.id)} onAddRoot={() => void addRoot()} onSearch={(query) => openSearch(query, selectedRoot.id)} coverRevision={coverRevision} editMode={editMode} selectedNodeIds={selectedNodeIds} matchBusy={matchBusy || favoriteBusy || Boolean(scan)} onEditMode={changeEditMode} onToggleSelection={toggleNodeSelection} onSelectAll={selectNodeIds} onClearSelection={clearNodeSelection} onBatchTags={() => setBatchTagsOpen(true)} onBatchFavorites={() => setFavoriteAssignmentNodeIds([...selectedNodeIds])} onBatchMenu={openBatchMenu} onMatch={(nodeIds, rematch) => void matchExisting(nodeIds, rematch)} />}
          {page === "library" && selectedRoot && detail && <ActiveDetailPage onReadComic={openComic} onOpenBangumi={() => { void api.openBangumiSubject(detail.node.id).catch(error => toast(errorMessage(error), "error")); }} detail={detail} loading={contentLoading} rootLabel={selectedRoot.displayName} onRoot={goRoot} onBreadcrumb={(id) => void openBreadcrumb(id)} onBack={goBack} onBangumi={() => requestBangumi(detail.node)} onRetryCover={(failed) => void retryCover(detail.node, failed)} onRetryCoverNode={(node, failed) => void retryCover(node, failed)} onClearBangumi={() => void nodeAction("clear-bangumi", detail.node)} onReveal={() => void nodeAction("explorer", detail.node)} onPlay={(file) => void play(file)} onRevealMedia={(file) => void revealMedia(file)} onOpenResource={(file) => void openResource(file)} onRevealResource={(file) => void revealResource(file)} onOpenChild={(node) => void openNode(node)} onBangumiNode={requestBangumi} onMenu={(event, node) => setContext({ node, x: event.clientX, y: event.clientY })} coverRevision={coverRevision} />}
          {page === "search" && <SearchPage initialQuery={searchQuery} rootId={searchRootId} roots={roots} onRootChange={setSearchRootId} onQueryChange={setSearchQuery} onOpen={(hit: SearchHit) => void openNode(hit.node)} onError={(message) => toast(message, "error")} onResultsReady={applyPendingScroll} coverRevision={coverRevision} />}
          {page === "settings" && <SettingsPage onIgnoreScanWarnings={root=>{void api.setLibraryScanWarningsIgnored(root.id,!root.scanHealth?.warningsIgnored).then(()=>loadRoots()).catch(e=>toast(errorMessage(e),'error'));}} onHiddenNodes={() => setHiddenNodesOpen(true)} roots={roots} bootstrap={bootstrap} onAddRoot={() => void addRoot()} onRemoveRoot={removeRoot} onScanRoot={(root) => void startScan(root.id)} onAppearanceChange={applySettingsAppearance} onPersistenceFailure={handleSettingsPersistenceFailure} updateDownloadStatus={updateDownloadStatus} onCheckForUpdate={() => { if (updateCheckResult?.update && updateDownloadStatus.phase !== "CHECKING" && updateDownloadStatus.phase !== "DOWNLOADING" && updateDownloadStatus.phase !== "APPLYING") { setUpdateDialogOpen(true); setUpdateInstallRequest(0); } else { void checkForUpdate(true); } }} onError={(message) => toast(message, "error")} onSuccess={(message) => toast(message, "success")} />}
        </div>
        <footer className="app-footer"><span>{bootstrap ? `${t("brand.name")} ${bootstrap.version}` : t("brand.name")}</span><span className="footer-separator" /><span><Icon name="shield" />{t("common.readOnly")}</span><span className="footer-separator" /><span>{t("app.projectCount", { count: number(projectCount) })}</span>{(page === "library" || (page === "search" && searchRootId != null)) && selectedRoot && <><span className="footer-separator" /><span title={selectedRoot.path}>{selectedRoot.displayName}</span></>}</footer>
      </main>

      {context && <ContextMenu node={context.node} x={context.x} y={context.y} onAction={(action, node) => void nodeAction(action, node)} onClose={() => setContext(null)} />}
      {batchContext && selectedNodeIds.size > 0 && <BatchContextMenu count={selectedNodeIds.size} x={batchContext.x} y={batchContext.y} onAction={batchAction} onClose={() => setBatchContext(null)} />}
      {rootContext && <LibraryRootContextMenu root={rootContext.root} x={rootContext.x} y={rootContext.y} onAction={(action, root) => void rootAction(action, root)} onClose={() => setRootContext(null)} />}
      <TagManagerDialog node={tagNode} onClose={() => setTagNode(null)} onChanged={async () => { const rootId = tagNode?.libraryRootId; await Promise.all([refreshCurrentAndAllResources(), loadRoots(rootId)]); }} />
      {hiddenNodesOpen && <HiddenNodesDialog roots={roots} refreshKey={allResources} onClose={() => setHiddenNodesOpen(false)} onRestored={async () => { await Promise.all([refreshCurrentAndAllResources(), loadRoots(selectedRootId), loadRecentlyWatched()]); }} />}
      <BatchTagDialog nodeIds={batchTagsOpen ? [...selectedNodeIds] : []} onClose={() => setBatchTagsOpen(false)} onApplied={finishBatchMutation} />
      <FavoriteAssignmentDialog nodeIds={favoriteAssignmentNodeIds} onClose={() => setFavoriteAssignmentNodeIds([])} onApplied={handleFavoriteApplied} onFoldersChanged={loadFavoriteFolders} />
      <FavoriteFolderDialog folder={favoriteFolderDialog} busy={dialogBusy} onClose={() => setFavoriteFolderDialog(null)} onSave={(name) => void saveFavoriteFolder(name)} />
      {sourceChoice && <SourceChoiceDialog sources={sourceChoice.sources} onChoose={source => { const action = sourceChoice.action; setSourceChoice(null); void nodeAction(action, source); }} onClose={() => setSourceChoice(null)} />}
      <BangumiModal node={bangumiNode} onClose={() => setBangumiNode(null)} onBound={handleBangumiBound} onStale={async () => { await refreshCurrentAndAllResources(); toast(t("works.changed"), "info"); }} />
      <RenameDialog node={renameNode} busy={dialogBusy} onClose={() => setRenameNode(null)} onSave={(name) => { if (!renameNode) return; setDialogBusy(true); void api.renameNode(renameNode.id, name).then(() => refreshCurrentAndAllResources()).then(() => { setRenameNode(null); toast(t("app.displayNameUpdated"), "success"); }).catch((error) => toast(errorMessage(error), "error")).finally(() => setDialogBusy(false)); }} />
      <LibraryRootRenameDialog root={renameRoot} busy={dialogBusy} onClose={() => setRenameRoot(null)} onSave={(name) => { if (!renameRoot) return; setDialogBusy(true); void api.renameRoot(renameRoot.id, name).then(() => Promise.all([loadRoots(renameRoot.id), refreshCurrent()])).then(() => { setRenameRoot(null); toast(t("app.rootNameUpdated"), "success"); }).catch((error) => toast(errorMessage(error), "error")).finally(() => setDialogBusy(false)); }} />
      <ConfirmDialog open={Boolean(confirm)} title={confirm?.title ?? ""} description={confirm?.description ?? ""} confirmLabel={confirm?.confirmLabel} destructive={confirm?.destructive} busy={dialogBusy} onConfirm={() => void runConfirm()} onClose={() => !dialogBusy && setConfirm(null)} />
      {rootModeUi}
      {updateUi}
      <ToastStack toasts={toasts} onDismiss={(id) => setToasts((items) => items.filter((item) => item.id !== id))} />
    </div>
  );
}

export default App;

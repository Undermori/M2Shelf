// @vitest-environment jsdom
import { StrictMode } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings, MediaNode, ScanProgress } from "./types/media";

const mocks = vi.hoisted(() => ({
  chooseDirectory: vi.fn(),
  chooseCoverCacheDirectory: vi.fn(),
  api: {
    bootstrap: vi.fn(), getSettings: vi.fn(), listRoots: vi.fn(),
    getCollectionSortPreferences: vi.fn(), updateCollectionSortPreference: vi.fn(), scanStatus: vi.fn(), allResources: vi.fn(),
    listRecentlyWatched: vi.fn(), listFavoriteFolders: vi.fn(), showMainWindow: vi.fn(),onMpvPlayback:vi.fn(),
    testMpv: vi.fn(), tmdbCredentialStatus: vi.fn(), addRoot: vi.fn(), startScan: vi.fn(), nodeDetail: vi.fn(), cacheStats: vi.fn(), posterCacheStatus: vi.fn(), clearCoverCache: vi.fn(), updateSettings: vi.fn(),
    bindWorkBangumi: vi.fn(), bindBangumi: vi.fn(), clearWorkBangumi: vi.fn(), retryWorkBangumiCover: vi.fn(), bangumiPrefill: vi.fn(), searchBangumi: vi.fn(), renameNode: vi.fn(), playMedia: vi.fn(), openMediaInExplorer: vi.fn(),
    listHiddenNodes: vi.fn(), resetNodeType: vi.fn(), search: vi.fn(), browse: vi.fn(), syncPendingBangumiAliases: vi.fn(), openBangumiSubject: vi.fn(), openExternalUrl: vi.fn(),
    getBookCatalogue:vi.fn(),revealComicBook:vi.fn(),collectionRestoreNotice:vi.fn(),
  },
  progress: new Set<(value: ScanProgress) => void>(),
  finished: new Set<(value: ScanProgress) => void>(),
}));

vi.mock("./lib/api", async (importOriginal) => ({
  ...await importOriginal<typeof import("./lib/api")>(),
  desktopAvailable: true,
  api: mocks.api,
  chooseDirectory: mocks.chooseDirectory,
  chooseCoverCacheDirectory: mocks.chooseCoverCacheDirectory,
  onScanProgress: async (callback: (value: ScanProgress) => void) => {
    mocks.progress.add(callback); return () => mocks.progress.delete(callback);
  },
  onScanFinished: async (callback: (value: ScanProgress) => void) => {
    mocks.finished.add(callback); return () => mocks.finished.delete(callback);
  },
}));
vi.mock("./hooks/useCoverDataUrl", () => ({ useCoverDataUrl: () => ({
  coverUrl: null, coverCacheKey: "", coverFailed: false, coverLoading: false,
}) }));

import App from "./App";
import { I18nProvider } from "./lib/i18n";
import { AppSettingsProvider } from "./lib/settingsStore";

function node(id: number, title: string): MediaNode {
  return {
    id, libraryRootId: 1, parentNodeId: 100, absolutePath: `X:/Fixtures/${title}`,
    folderName: title, displayName: title, nodeType: "WORK", manualTypeOverride: false,
    coverSource: "PLACEHOLDER", coverCachePath: null, directVideoCount: 1,
    totalVideoCount: 1, childMediaBranchCount: 0, createdAt: "2026-01-01",
    updatedAt: "2026-01-01", lastSeenAt: "2026-01-01", userTags: [],
  };
}
const sources = [node(1, "Release A"), node(2, "Release B")];
const work = { ...sources[0], displayName: "Example Work", totalVideoCount: 2 };
const workTarget = { sourceNodeIds: sources.map(source => source.id), snapshot: "fixture-snapshot" };
const catalogue = {
  nodes: [{ ...node(100, "Archive"), nodeType: "CONTAINER" as const }],
  totalCount: 1, works: [{ node: work, sources, target: workTarget }],
};
const root = { id: 1, path: "X:/Fixtures", displayName: "Test Library", createdAt: "2026-01-01", lastScanAt: null, recognitionMode: "FOLDER" };
let settings: AppSettings;
const progress: ScanProgress = {
  background: true, libraryChanged: true,
  scanId: "startup-test", rootId: 1, currentPath: "X:/Fixtures", foldersScanned: 3,
  videosFound: 3, errors: 0, status: "COMPLETED", phase: "SCANNING",
  autoMatchCurrent: 0, autoMatchTotal: 0, autoMatchMatched: 0, autoMatchPending: 0,
  autoMatchUnmatched: 0, autoMatchErrors: 0,
};
const mount = (_caption = false) => render(<StrictMode><I18nProvider><AppSettingsProvider><App /></AppSettingsProvider></I18nProvider></StrictMode>);

beforeEach(() => {
  vi.resetAllMocks();
  vi.stubGlobal("requestIdleCallback", vi.fn());
  vi.stubGlobal("cancelIdleCallback", vi.fn());
  mocks.api.collectionRestoreNotice.mockResolvedValue(null);
  mocks.api.testMpv.mockResolvedValue({ok:true,version:"mpv v0.41.0",message:""});
  mocks.api.tmdbCredentialStatus.mockResolvedValue({source:"DEFAULT",defaultAvailable:true,personalConfigured:false,check:"UNCHECKED"});
  mocks.progress.clear(); mocks.finished.clear();
  window.history.replaceState(null, "", "/");
  window.matchMedia = vi.fn().mockReturnValue({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() });
  Element.prototype.scrollTo = vi.fn();
  Element.prototype.scrollIntoView = vi.fn();
  vi.stubGlobal("IntersectionObserver", class { observe() {} unobserve() {} disconnect() {} });
  settings = {
    mpvPath: null, defaultViewMode: "GRID", videoExtensions: [".mkv"],
    bangumiSearchEnabled: false, autoCheckUpdates: false, autoScanOnStartup: true,
    allResourcesFlattened: true,
    language: "zh-CN", theme: "light", coverCacheDirectory: "X:/AppCache",
  };
  mocks.api.getSettings.mockImplementation(async () => settings);
  mocks.api.updateSettings.mockImplementation(async (next: AppSettings) => { settings = next; return next; });
  mocks.api.bootstrap.mockResolvedValue({ name: "M²Shelf", version: "0.5.11", updateRecoveryNotice: null });
  mocks.api.listRoots.mockResolvedValue([root]);
  mocks.api.search.mockResolvedValue([]);
  mocks.api.bangumiPrefill.mockResolvedValue({ originalName: "Example Work", extractedName: "Example Work", candidates: [] });
  mocks.api.playMedia.mockResolvedValue(undefined);
  mocks.api.openMediaInExplorer.mockResolvedValue(undefined);
  mocks.api.syncPendingBangumiAliases.mockResolvedValue(false);
  mocks.api.browse.mockResolvedValue({ root, currentNode: null, nodes: sources, breadcrumbs: [], mediaFiles: [], resourceFiles: [] });
  mocks.api.getCollectionSortPreferences.mockResolvedValue({ all: "title-asc", browse: "title-asc", favorites: "title-asc" });
  mocks.api.updateCollectionSortPreference.mockImplementation(async (_scope, sort) => sort);
  mocks.api.scanStatus.mockResolvedValue(null);
  mocks.api.onMpvPlayback.mockResolvedValue(()=>{});
  mocks.api.allResources.mockResolvedValue(catalogue);
  mocks.api.listRecentlyWatched.mockResolvedValue([]);
  mocks.api.listFavoriteFolders.mockResolvedValue([]);
  mocks.api.listHiddenNodes.mockResolvedValue([]);
  mocks.api.resetNodeType.mockImplementation(async (id: number) => node(id, "Restored Work"));
  mocks.api.showMainWindow.mockResolvedValue(undefined);
  mocks.api.startScan.mockResolvedValue({ scanId: "startup-test" });
  mocks.api.cacheStats.mockResolvedValue({ fileCount: 0, totalBytes: 0, cacheDirectory: "X:/AppCache" });
  mocks.api.posterCacheStatus.mockResolvedValue({phase: "IDLE", processed: 0, total: 0, failed: 0});
  mocks.api.clearCoverCache.mockResolvedValue({ fileCount: 0, totalBytes: 0, cacheDirectory: "X:/AppCache" });
  mocks.api.nodeDetail.mockImplementation(async (_id: number, workView: boolean) => ({
    node: { ...work, workView, workTarget: workView ? workTarget : undefined }, workTarget: workView ? workTarget : null, binding: null, children: [], resourceFiles: [], breadcrumbs: [],
    workSources: workView ? sources : null,
    mediaFiles: sources.map((source, index) => ({ id: index + 1, nodeId: source.id,
      fileName: `Episode 0${index + 1}.mkv`, absolutePath: `${source.absolutePath}/01.mkv`,
      extension: ".mkv", fileSize: 100, modifiedAt: "2026-01-01", lastSeenAt: "2026-01-01",
      durationMs: null, width: null, height: null, codec: null })),
  }));
});
afterEach(() => { cleanup(); vi.useRealTimers(); vi.unstubAllGlobals(); });

describe("startup refresh and work browsing", () => {
  it('keeps local filters on their collection when Enter confirms text, while explicit global search still works',async()=>{
    settings.autoScanOnStartup=false;settings.allResourcesFlattened=false;
    mount();await screen.findByText(/^Archive/);
    const allInput=document.querySelector('.all-resources-page .search-field input')!;
    fireEvent.change(allInput,{target:{value:'Archive'}});
    fireEvent.keyDown(allInput,{key:'Enter',code:'Enter'});
    expect(document.querySelector('.all-resources-page')).not.toBeNull();
    expect(document.querySelector('.search-page')).toBeNull();expect(mocks.api.search).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button',{name:/Test Library/}));
    await waitFor(()=>expect(document.querySelector('.library-toolbar .search-field input')).not.toBeNull());
    const libraryInput=document.querySelector('.library-toolbar .search-field input')!;
    fireEvent.change(libraryInput,{target:{value:'Release A'}});
    fireEvent.keyDown(libraryInput,{key:'Enter',code:'Enter'});
    fireEvent.keyDown(libraryInput,{key:'Enter',code:'Enter',isComposing:true});
    expect(document.querySelector('.library-toolbar')).not.toBeNull();expect(document.querySelector('.search-page')).toBeNull();
    expect(mocks.api.search).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button',{name:'搜索'}));
    await waitFor(()=>expect(document.querySelector('.search-page')).not.toBeNull());
    const search=screen.getByRole('textbox',{name:'搜索'});fireEvent.change(search,{target:{value:'Release A'}});
    fireEvent.submit(search.closest('form')!);
    await waitFor(()=>expect(mocks.api.search).toHaveBeenCalledWith('Release A',undefined));
  });
  it('opens cached Settings without a loading frame',async()=>{
    settings.autoScanOnStartup=false;settings.allResourcesFlattened=false;
    mount();await screen.findByText(/^Archive/);
    fireEvent.click(screen.getByRole('button',{name:'设置'}));
    expect(document.querySelector('.settings-page')).not.toBeNull();
    expect(document.querySelector('.settings-page .loading-state')).toBeNull();
  });

  it('keeps both display modes ready without making a new catalogue request',async()=>{
    settings.autoScanOnStartup=false;settings.allResourcesFlattened=false;
    mount();await screen.findByText(/^Archive/);
    mocks.api.allResources.mockClear();
    fireEvent.click(screen.getByRole('button',{name:'设置'}));
    fireEvent.click(screen.getByRole('checkbox',{name:'全部资源穿透显示'}));
    await waitFor(()=>expect(settings.allResourcesFlattened).toBe(true));
    fireEvent.click(screen.getByRole('button',{name:'全部资源'}));
    expect(screen.getByText('Example Work')).toBeDefined();
    expect(document.querySelector('.all-resources-page .loading-state')).toBeNull();
    expect(mocks.api.allResources).not.toHaveBeenCalled();expect(screen.queryByText(/^Archive/)).toBeNull();
  });

  it('does not reread collections for player connection lifecycle events',async()=>{
    let listener!:(event:import('./types/episodes').MpvPlaybackEvent)=>void;
    mocks.api.onMpvPlayback.mockImplementation(async(callback)=>{listener=callback;return()=>{};});
    settings.autoScanOnStartup=false;mount();await screen.findByText('Example Work');
    await waitFor(()=>expect(listener).toBeDefined());
    mocks.api.allResources.mockClear();mocks.api.listRecentlyWatched.mockClear();
    await act(async()=>{for(const state of ['STARTED','CONNECTED','ENDED'] as const)listener({sessionId:900,state});await new Promise(resolve=>setTimeout(resolve,230));});
    expect(mocks.api.allResources).not.toHaveBeenCalled();expect(mocks.api.listRecentlyWatched).not.toHaveBeenCalled();
    await act(async()=>{listener({sessionId:901,state:'FILE',fileId:1,ownerNodeId:1});await new Promise(resolve=>setTimeout(resolve,230));});
    expect(mocks.api.allResources).toHaveBeenCalledOnce();expect(mocks.api.listRecentlyWatched).toHaveBeenCalledOnce();
    await act(async()=>{listener({sessionId:901,state:'FINISHED',fileId:1,ownerNodeId:1});listener({sessionId:901,state:'ENDED'});await new Promise(resolve=>setTimeout(resolve,230));});
    expect(mocks.api.allResources).toHaveBeenCalledTimes(2);expect(mocks.api.listRecentlyWatched).toHaveBeenCalledTimes(2);
  });
  it.each(["another work", "Settings"])("rejects an old detail completion after navigating to %s", async (destination) => {
    settings.autoScanOnStartup = false;
    const second = node(3, "Second Work");
    mocks.api.allResources.mockResolvedValue({ ...catalogue, works: [
      ...catalogue.works,
      { node: second, sources: [second], target: { sourceNodeIds: [3], snapshot: "second" } },
    ] });
    const detailFor = (value: MediaNode) => ({ node: value, children: [], mediaFiles: [], resourceFiles: [], breadcrumbs: [], binding: null });
    let finishFirst!: (value: ReturnType<typeof detailFor>) => void;
    mocks.api.nodeDetail.mockImplementation((id: number) => id === work.id
      ? new Promise(resolve => { finishFirst = resolve; })
      : Promise.resolve(detailFor(second)));
    mount();
    fireEvent.click((await screen.findByText("Example Work")).closest("button")!);
    await waitFor(() => expect(mocks.api.nodeDetail).toHaveBeenCalledTimes(1));
    if (destination === "Settings") {
      fireEvent.click(screen.getByRole("button", { name: "设置" }));
      await screen.findByRole("checkbox", { name: "全部资源穿透显示" });
    } else {
      fireEvent.click(screen.getByText("Second Work").closest("button")!);
      await screen.findByRole("heading", { name: "Second Work", level: 1 });
    }
    await act(async () => finishFirst(detailFor(work)));
    if (destination === "Settings") expect(document.querySelector(".settings-page")).not.toBeNull();
    else expect(screen.getByRole("heading", { name: "Second Work", level: 1 })).toBeTruthy();
    expect(document.querySelector(".detail-page h1")?.textContent).not.toBe("Example Work");
  });

  it.each((["zh-CN", "en-US", "ja-JP", "ko-KR"] as const).flatMap(language => (["light", "dark"] as const).map(theme => [language, theme] as const)))("shows ready content once under StrictMode (%s, %s)", async (language, theme) => {
    settings.autoScanOnStartup = false;
    settings.theme = theme;
    settings.language = language;
    let show!: () => void;
    mocks.api.showMainWindow.mockImplementation(() => new Promise<void>(resolve => { show = resolve; }));
    mount();
    await waitFor(() => expect(mocks.api.showMainWindow).toHaveBeenCalledTimes(1));
    expect(document.documentElement.dataset.theme).toBe(theme);
    expect(document.documentElement.lang).toBe(language);
    expect(mocks.api.allResources).toHaveBeenCalledTimes(1);
    expect(document.querySelector(".all-resources-page .media-card")).not.toBeNull();
    expect(document.querySelector(".root-item")).not.toBeNull();
    expect(document.querySelector(".loading-state")).toBeNull();
    expect(mocks.api.listRecentlyWatched).not.toHaveBeenCalled();
    await act(async () => show());
    await screen.findByText("Example Work");
    expect(mocks.api.allResources).toHaveBeenCalledTimes(1);
  });

  it("keeps the actual page visible until a cold Recently opened request is ready", async () => {
    settings.autoScanOnStartup = false;
    let finishHistory!: (entries: { node: MediaNode; watchedAt: string }[]) => void;
    mocks.api.listRecentlyWatched.mockImplementation(() => new Promise(resolve => { finishHistory = resolve; }));
    mount();
    await screen.findByText("Example Work");
    expect(mocks.api.listRecentlyWatched).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "最近打开" }));
    await waitFor(() => expect(mocks.api.listRecentlyWatched).toHaveBeenCalledTimes(1));
    expect(document.querySelector(".all-resources-page .media-card")).not.toBeNull();
    expect(document.querySelector(".loading-state")).toBeNull();
    await act(async () => finishHistory([{ node: node(3, "History Work"), watchedAt: "2026-01-01" }]));
    await screen.findByText("History Work");
    expect(document.querySelector(".recently-watched-page")).not.toBeNull();
  });

  it("prepares recent records only after display, reuses the ready snapshot, and preserves route cancellation", async () => {
    settings.autoScanOnStartup = false;
    let idle!: IdleRequestCallback;
    vi.stubGlobal("requestIdleCallback", vi.fn((callback: IdleRequestCallback) => {idle=callback; return 1;}));
    const entries=[{node:node(3,"History Work"),watchedAt:"2026-01-01"}];
    let finish!: (value: typeof entries) => void;
    mocks.api.listRecentlyWatched.mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));
    mount();
    await waitFor(()=>expect(mocks.api.showMainWindow).toHaveBeenCalledTimes(1));
    expect(mocks.api.listRecentlyWatched).not.toHaveBeenCalled();
    await waitFor(()=>expect(idle).toBeTypeOf("function"));
    await act(async()=>idle({didTimeout:false,timeRemaining:()=>40}));
    expect(mocks.api.listRecentlyWatched).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button",{name:"最近打开"}));
    expect(document.querySelector(".all-resources-page .media-card")).not.toBeNull();
    fireEvent.click(screen.getByRole("button",{name:"设置"}));
    await screen.findByRole("checkbox",{name:"全部资源穿透显示"});
    await act(async()=>finish(entries));
    expect(document.querySelector(".settings-page")).not.toBeNull();
    fireEvent.click(screen.getByRole("button",{name:"最近打开"}));
    await screen.findByText("History Work");
    expect(mocks.api.listRecentlyWatched).toHaveBeenCalledTimes(1);
    expect(document.querySelector(".loading-state")).toBeNull();
    fireEvent.click(screen.getByRole("button",{name:"全部资源"}));
    await screen.findByText("Example Work");
    fireEvent.click(screen.getByRole("button",{name:"最近打开"}));
    await screen.findByText("History Work");
    expect(mocks.api.listRecentlyWatched).toHaveBeenCalledTimes(1);
  });

  it("revalidates a prepared snapshot after manual progress changes without flashing old entries",async()=>{
    settings.autoScanOnStartup=false;
    mocks.api.listRecentlyWatched.mockResolvedValue([{node:node(3,"History Work"),watchedAt:"2026-01-01"}]);
    mount();await screen.findByText("Example Work");
    fireEvent.click(screen.getByRole("button",{name:"最近打开"}));await screen.findByText("History Work");
    fireEvent.click(screen.getByRole("button",{name:"全部资源"}));await screen.findByText("Example Work");
    let finish!:(value:unknown[])=>void;
    mocks.api.listRecentlyWatched.mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));
    act(()=>window.dispatchEvent(new Event('m2shelf-progress-changed')));
    fireEvent.click(screen.getByRole("button",{name:"最近打开"}));
    await waitFor(()=>expect(mocks.api.listRecentlyWatched).toHaveBeenCalledTimes(2));
    expect(document.querySelector(".all-resources-page .media-card")).not.toBeNull();
    expect(screen.queryByText("History Work")).toBeNull();
    expect(document.querySelector(".loading-state")).toBeNull();
    await act(async()=>finish([{node:node(4,"Newly watched"),watchedAt:"2026-01-02"}]));
    await screen.findByText("Newly watched");
  });

  it("keeps the window hidden and delays automatic scanning until the catalogue is ready", async () => {
    let finishCatalogue!: (value: typeof catalogue) => void;
    mocks.api.allResources.mockImplementation(() => new Promise(resolve => { finishCatalogue = resolve; }));
    mount();
    await waitFor(() => expect(mocks.api.allResources).toHaveBeenCalledTimes(1));
    expect(mocks.api.showMainWindow).not.toHaveBeenCalled();
    expect(mocks.api.startScan).not.toHaveBeenCalled();
    await act(async () => finishCatalogue(catalogue));
    await waitFor(() => expect(mocks.api.showMainWindow).toHaveBeenCalledTimes(1));
    expect(document.querySelector(".all-resources-page .media-card")).not.toBeNull();
    expect(document.querySelector(".loading-state")).toBeNull();
    await waitFor(() => expect(mocks.api.startScan).toHaveBeenCalledTimes(1));
  });

  it("waits for the library sidebar as well as the catalogue before displaying the window", async () => {
    settings.autoScanOnStartup = false;
    let finishRoots!: (value: typeof root[]) => void;
    mocks.api.listRoots.mockImplementation(() => new Promise(resolve => { finishRoots = resolve; }));
    mount();
    await screen.findByText("Example Work");
    expect(mocks.api.showMainWindow).not.toHaveBeenCalled();
    expect(mocks.api.startScan).not.toHaveBeenCalled();
    await act(async () => finishRoots([root]));
    await waitFor(() => expect(mocks.api.showMainWindow).toHaveBeenCalledTimes(1));
    expect(document.querySelector(".root-item")).not.toBeNull();
  });

  it("keeps a newer sort choice when library bootstrap finishes later", async () => {
    settings.autoScanOnStartup = false;
    let finishRoots!: (value: typeof root[]) => void;
    mocks.api.listRoots.mockImplementation(() => new Promise(resolve => { finishRoots = resolve; }));
    mount();
    await screen.findByText("Example Work");
    chooseOption(screen.getByRole("combobox", { name: "排序方式" }), "modified");
    await waitFor(() => expect(mocks.api.updateCollectionSortPreference).toHaveBeenCalledWith("all", "modified-asc"));
    await act(async () => finishRoots([root]));
    expect(screen.getByRole("combobox", { name: "排序方式" }).textContent).toBe("更新时间");
    expect(document.querySelector(".file-modified-time")).not.toBeNull();
  });

  it("does not repeatedly retry a failed initial catalogue in the background", async () => {
    settings.autoScanOnStartup = false;
    mocks.api.allResources.mockRejectedValue(Object.assign(new Error("fixture index unavailable"), { name: "M2ShelfError" }));
    mount();
    await screen.findByText("fixture index unavailable");
    await waitFor(() => expect(mocks.api.showMainWindow).toHaveBeenCalledTimes(1));
    await act(async () => undefined);
    expect(mocks.api.allResources).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    await screen.findByRole("checkbox", { name: "全部资源穿透显示" });
    expect(mocks.api.allResources).toHaveBeenCalledTimes(1);
  });

  it('opens a smart series from All Resources with original book identities and returns to All',async()=>{
    settings.autoScanOnStartup=false;settings.allResourcesFlattened=false;
    const bookRoot={...root,mediaKind:'EBOOK' as const,bookOrganizationStrategy:'SMART_MIXED' as const};
    const books=[1,2].map(id=>({id,nodeId:100,sourceKind:'ZIP_ARCHIVE' as const,displayName:`Volume ${id}.epub`,pageCount:2,revision:'v1',modifiedAt:'2026-01-01',documentFormat:'EPUB' as const,indexError:null,progress:null}));
    const logical={status:'READY',revision:1,groups:[{id:'series-1',title:'The series',kind:'SERIES' as const,relativePath:'The series',books,coverNode:null}],directories:[],fallbackBooks:[],directoryNodes:[]};
    mocks.api.listRoots.mockResolvedValue([bookRoot]);mocks.api.getBookCatalogue.mockResolvedValue(logical);
    mocks.api.allResources.mockResolvedValue({nodes:[],works:[],comicNodes:[],totalCount:1,bookLibraries:[{root:bookRoot,catalogue:logical}]});
    mocks.api.browse.mockResolvedValue({root:bookRoot,nodes:[],mediaFiles:[],comicBooks:[],resourceFiles:[],breadcrumbs:[]});
    mount();fireEvent.click((await screen.findByText('The series')).closest('button')!);
    await screen.findByText('Volume 1.epub',{selector:'.file-name strong'});await screen.findByText('Volume 2.epub',{selector:'.file-name strong'});
    expect(mocks.api.nodeDetail).not.toHaveBeenCalled();
    expect(window.history.state.smartMixed.destination.group).toBe('series-1');
    fireEvent.click(screen.getByRole('button',{name:/^返回$/}));
    await waitFor(()=>expect(document.querySelector('.all-resources-page')).toBeTruthy());
    expect(document.querySelectorAll('.media-card')).toHaveLength(1);
  });
  it('uses one real menu bar for existing view and navigation callbacks',async()=>{
    settings.autoScanOnStartup=false;mount();await screen.findByText('Example Work');
    expect(document.querySelectorAll('.window-titlebar')).toHaveLength(1);
    expect(screen.getAllByRole('menubar')).toHaveLength(1);
    fireEvent.click(screen.getByRole('menuitem',{name:'显示'}));
    fireEvent.click(screen.getByRole('menuitemcheckbox',{name:'列表'}));
    fireEvent.click(screen.getByRole('menuitem',{name:'显示'}));
    expect(screen.getByRole('menuitemcheckbox',{name:'列表'}).getAttribute('aria-checked')).toBe('true');
    fireEvent.keyDown(screen.getByRole('menuitemcheckbox',{name:'列表'}),{key:'Escape'});
    expect(screen.queryByRole('menu')).toBeNull();
    fireEvent.click(screen.getByRole('menuitem',{name:'转至'}));fireEvent.click(screen.getByRole('menuitem',{name:'设置'}));
    await screen.findByText('关于');expect(document.querySelector('.settings-page')).toBeTruthy();
    fireEvent.click(screen.getByRole('menuitem',{name:'帮助'}));fireEvent.click(screen.getByRole('menuitem',{name:'关于 M²Shelf'}));
    await waitFor(()=>expect(Element.prototype.scrollIntoView).toHaveBeenCalled());
  });
  it("queues scanning a new ebook library until the existing scan finishes", async () => {
    mocks.chooseDirectory.mockResolvedValue('X:/New Books');
    const added={...root,id:2,path:'X:/New Books',displayName:'New Books',mediaKind:'EBOOK',recognitionMode:'VIDEO_FILE'};
    mocks.api.addRoot.mockImplementation(async()=>{mocks.api.listRoots.mockResolvedValue([root,added]);return added;});
    mount();await screen.findByText('Example Work');
    await waitFor(()=>expect(mocks.api.startScan).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole('button',{name:'添加资源目录'}));
    fireEvent.click(await screen.findByRole('button',{name:'电子书'}));
    fireEvent.click(await screen.findByRole('button',{name:/按单个文件识别/}));
    fireEvent.click(screen.getByRole('button',{name:'创建资源库'}));
    await waitFor(()=>expect(mocks.api.addRoot).toHaveBeenCalledWith('X:/New Books','VIDEO_FILE','EBOOK',false,'LEGACY'));
    expect(mocks.api.startScan).toHaveBeenCalledTimes(1);
    await act(async()=>{for(const callback of mocks.finished)callback(progress);});
    await waitFor(()=>expect(mocks.api.startScan).toHaveBeenCalledWith(2,undefined,false));
    expect(mocks.api.startScan).toHaveBeenCalledTimes(2);
  });
  it("retains a newly added library when the native worker is briefly busy after completion", async () => {
    settings.autoScanOnStartup = false;
    mocks.chooseDirectory.mockResolvedValue('X:/New Books');
    mocks.api.startScan.mockRejectedValueOnce(new Error('SCAN_WORKER_BUSY')).mockResolvedValueOnce({scanId:'new-books'});
    mocks.api.addRoot.mockResolvedValue({...root,id:2,path:'X:/New Books',mediaKind:'EBOOK',recognitionMode:'VIDEO_FILE'});
    mount();await screen.findByText('Example Work');
    fireEvent.click(screen.getByRole('button',{name:'添加资源目录'}));
    fireEvent.click(await screen.findByRole('button',{name:'电子书'}));
    fireEvent.click(await screen.findByRole('button',{name:/按单个文件识别/}));
    fireEvent.click(screen.getByRole('button',{name:'创建资源库'}));
    await waitFor(()=>expect(mocks.api.startScan).toHaveBeenCalledTimes(2),{timeout:2000});
    expect(mocks.api.startScan.mock.calls.every(call=>call[0]===2)).toBe(true);
  });
  it("shows a transient save receipt only after a successful edit, not on initial load", async () => {
    settings.autoScanOnStartup = false;
    mount();
    await screen.findByText("Example Work");
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const toggle = await screen.findByRole("checkbox", { name: "全部资源穿透显示" });
    expect(document.querySelector(".settings-savebar")).toBeNull();
    const receipt = document.querySelector(".settings-save-status")!;
    expect(receipt.textContent).toBe("");
    vi.useFakeTimers();
    await act(async () => { fireEvent.click(toggle); });
    expect(receipt.textContent).toBe("所有修改已自动保存");
    await act(async () => { vi.advanceTimersByTime(3000); });
    expect(receipt.textContent).toBe("所有修改已自动保存");
    // A new edit cancels the previous receipt's timeout.
    await act(async () => { fireEvent.click(toggle); });
    await act(async () => { vi.advanceTimersByTime(400); });
    expect(receipt.textContent).toBe("所有修改已自动保存");
    await act(async () => { vi.advanceTimersByTime(2800); });
    expect(receipt.classList.contains("is-visible")).toBe(false);
    await act(async () => { vi.advanceTimersByTime(180); });
    expect(receipt.textContent).toBe("");
    expect(mocks.api.updateSettings).toHaveBeenCalledTimes(2);
  });

  it("keeps saving feedback until IPC completes and never reports a failed save as successful", async () => {
    settings.autoScanOnStartup = false;
    let rejectSave!: (reason: Error) => void;
    mocks.api.updateSettings.mockImplementation(() => new Promise((_resolve, reject) => { rejectSave = reject; }));
    mount();
    await screen.findByText("Example Work");
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const toggle = await screen.findByRole("checkbox", { name: "全部资源穿透显示" });
    fireEvent.click(toggle);
    const receipt = document.querySelector(".settings-save-status")!;
    expect(receipt.textContent).toBe("正在自动保存…");
    vi.useFakeTimers();
    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(receipt.textContent).toBe("正在自动保存…");
    await act(async () => { rejectSave(new Error("fixture save failure")); });
    expect(receipt.textContent).toBe("");
    expect((toggle as HTMLInputElement).checked).toBe(true);
    expect(screen.queryByText("所有修改已自动保存")).toBeNull();
  });

  it.each([
    ["zh-CN", "全部资源穿透显示"],
    ["en-US", "Flatten All Resources"],
    ["ja-JP", "すべてのリソースをフラット表示"],
    ["ko-KR", "전체 리소스 펼쳐 보기"],
  ].flatMap(([language, label]) => (["light", "dark"] as const).map(theme => [language, label, theme] as const)))("localizes the flatten toggle in %s with the shared switch style (%s, %s)", async (language, label, theme) => {
    settings.autoScanOnStartup = false;
    settings.theme = theme;
    mount();
    await screen.findByText("Example Work");
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const languageSelect = await within(document.querySelector('.window-titlebar') as HTMLElement).findByRole("combobox", { name: "界面语言" });
    await waitFor(() => expect((languageSelect as HTMLSelectElement).disabled).toBe(false));
    chooseOption(languageSelect, language);
    const toggle = await screen.findByRole("checkbox", { name: label });
    expect(toggle.closest("label")?.className).toBe("switch-field settings-switch-row");
    expect((toggle as HTMLInputElement).checked).toBe(true);
    expect(document.documentElement.dataset.theme).toBe(theme);
  });
  it("keeps original folder cards when flattening is off, saves the toggle, and ignores old grouping snapshots", async () => {
    settings.autoScanOnStartup = false;
    settings.allResourcesFlattened = false;
    mount();
    await screen.findByText(/^Archive/);
    expect(screen.queryByText("Example Work")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const toggle = await screen.findByRole("checkbox", { name: "全部资源穿透显示" });
    await waitFor(() => expect((toggle as HTMLInputElement).disabled).toBe(false));
    expect((toggle as HTMLInputElement).checked).toBe(false);
    fireEvent.click(toggle);
    await waitFor(() => expect(settings.allResourcesFlattened).toBe(true));
    fireEvent.click(screen.getByRole("button", { name: "全部资源" }));
    await screen.findByText("Example Work");
    expect(screen.queryByText(/^Archive/)).toBeNull();
    expect(mocks.api.startScan).not.toHaveBeenCalled();
    cleanup();
    mount();
    await screen.findByText("Example Work");
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const savedToggle = await screen.findByRole("checkbox", { name: "全部资源穿透显示" });
    await waitFor(() => expect((savedToggle as HTMLInputElement).disabled).toBe(false));
    expect((savedToggle as HTMLInputElement).checked).toBe(true);
    fireEvent.click(savedToggle);
    await waitFor(() => expect(settings.allResourcesFlattened).toBe(false));
    fireEvent.click(screen.getByRole("button", { name: "全部资源" }));
    await screen.findByText(/^Archive/);
    cleanup();
    mount();
    await screen.findByText(/^Archive/);
  });

  it("rolls back failed flattening persistence after leaving Settings", async () => {
    settings.autoScanOnStartup = false;
    settings.allResourcesFlattened = false;
    let rejectSave!: (reason: Error) => void;
    mocks.api.updateSettings.mockImplementation(() => new Promise((_resolve, reject) => { rejectSave = reject; }));
    mount();
    await screen.findByText(/^Archive/);
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const toggle = await screen.findByRole("checkbox", { name: "全部资源穿透显示" });
    await waitFor(() => expect((toggle as HTMLInputElement).disabled).toBe(false));
    fireEvent.click(toggle);
    fireEvent.click(screen.getByRole("button", { name: "全部资源" }));
    await screen.findByText("Example Work");
    await act(async () => rejectSave(new Error("fixture save failure")));
    await screen.findByText(/^Archive/);
    expect(settings.allResourcesFlattened).toBe(false);
  });
  it.each([
    ["zh-CN", "关注作者", "特别感谢", "原作者", "功能改进与维护"],
    ["en-US", "Follow the author", "Special thanks", "Original author", "Feature improvements and maintenance"],
    ["ja-JP", "作者をフォロー", "スペシャルサンクス", "原作者", "機能改善・メンテナンス"],
    ["ko-KR", "제작자 팔로우", "특별 감사", "원작자", "기능 개선 및 유지보수"],
  ].flatMap(([language, follow, thanks, original, contributor]) => (["light", "dark"] as const).map(theme => [language, follow, thanks, original, contributor, theme] as const)))("shows compact author links and thanks in %s (%s, %s, %s, %s, %s)", async (language, follow, thanks, original, contributor, theme) => {
    settings.autoScanOnStartup = false;
    settings.theme = theme;
    const websiteUrl = "https://space.bilibili.com/2903441";
    const xUrl = "https://x.com/f_undermori";
    mocks.api.bootstrap.mockResolvedValue({ name: "M²Shelf", version: "0.5.11", websiteUrl, xUrl, updateRecoveryNotice: null });
    mocks.api.openExternalUrl.mockResolvedValue(undefined);
    mount();
    await screen.findByText("Example Work");
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    await screen.findByText("关于");
    const languageSelect = within(document.querySelector('.settings-page')!).getByRole("combobox", { name: "界面语言" }) as HTMLSelectElement;
    await waitFor(() => expect(languageSelect.disabled).toBe(false));
    chooseOption(languageSelect, language);
    const section = document.querySelector(".about-section") as HTMLElement;
    const about = within(section);
    expect(await about.findByText(follow)).toBeTruthy();
    const credit = section.querySelector(".about-credit") as HTMLElement;
    expect(credit.querySelector("dt")?.textContent).toBe(thanks);
    expect(credit.querySelector("dd")?.textContent).toBe("Juvenile_A");
    expect(section.querySelector(".about-thanks")).toBeNull();
    expect(about.queryByText(original)).toBeNull();
    expect(about.queryByText(contributor)).toBeNull();
    expect(about.queryByText("Created by")).toBeNull();
    expect(section.querySelector(".created-by")).toBeNull();
    expect(section.querySelectorAll(".about-grid > div")).toHaveLength(5);
    expect(about.getAllByText("Juvenile_A")).toHaveLength(1);
    const links = within(section.querySelector(".about-author-links") as HTMLElement).getAllByRole("button");
    expect(links).toHaveLength(1);
    expect(links[0].textContent).toContain("Bilibili");
    expect(section.textContent).not.toContain("Undermori · X");
    fireEvent.click(links[0]);
    expect(mocks.api.openExternalUrl.mock.calls).toEqual([[websiteUrl]]);
    expect(document.documentElement.dataset.theme).toBe(theme);
  });

  it("lets the backend select damaged sources when no poster decode failure was reported", async () => {
    settings.autoScanOnStartup = false;
    const binding = { id: 1, nodeId: work.id, provider: "BANGUMI", providerSubjectId: 42, providerSubjectType: 2, providerTitle: "Bound", providerAliases: [], coverCachePath: null, coverDownloadError: "fixture" };
    mocks.api.allResources.mockResolvedValue({ ...catalogue, works: [{ node: { ...work, binding }, sources, target: workTarget }] });
    mocks.api.retryWorkBangumiCover.mockResolvedValue({ ...binding, coverDownloadError: null });
    mount();
    await screen.findByText("Bound");
    fireEvent.click(document.querySelector(".media-card .quick-bind.is-retry")!);
    await waitFor(() => expect(mocks.api.retryWorkBangumiCover).toHaveBeenCalledWith(workTarget, []));
  });

  it("opens the current binding in the browser through its original node ID", async () => {
    settings.autoScanOnStartup = false;
    const binding = { id:1,nodeId:work.id,provider:"BANGUMI",providerSubjectId:174584,providerSubjectType:2,providerTitle:"Bound Work",providerAliases:[],coverCachePath:null };
    mocks.api.nodeDetail.mockResolvedValue({ node:{...work,binding},binding,mediaFiles:[],resourceFiles:[],children:[],breadcrumbs:[],workSources:sources });
    mocks.api.openBangumiSubject.mockResolvedValue(undefined);
    mount();
    await screen.findByText("Example Work");
    fireEvent.click(document.querySelector(".media-card-open")!);
    fireEvent.click(await screen.findByRole("button", {name:"在 Bangumi 打开 ↗"}));
    expect(mocks.api.openBangumiSubject).toHaveBeenCalledWith(work.id);
  });

  it("silently backfills aliases at startup and only refreshes changed metadata", async () => {
    settings.autoScanOnStartup = false;
    let finish!: (changed: boolean) => void;
    mocks.api.syncPendingBangumiAliases.mockImplementationOnce(() => new Promise<boolean>(resolve => { finish = resolve; }));
    mount();
    await screen.findByText("Example Work");
    await waitFor(() => expect(mocks.api.syncPendingBangumiAliases).toHaveBeenCalledTimes(1));
    mocks.api.allResources.mockClear();
    await act(async () => finish(false));
    expect(mocks.api.allResources).not.toHaveBeenCalled();
    expect(screen.queryByText("同步别称")).toBeNull();
    expect(document.querySelector(".toast")).toBeNull();
    cleanup();
    mocks.api.syncPendingBangumiAliases.mockResolvedValueOnce(true);
    mocks.api.allResources.mockClear();
    mount();
    await waitFor(() => expect(mocks.api.allResources).toHaveBeenCalledTimes(2));
    expect(document.querySelector(".toast")).toBeNull();
  });
  it("searches every library from the sidebar and exposes scope changes without retaining stale results", async () => {
    settings.autoScanOnStartup = false;
    const watching = { ...root, id: 2, displayName: "Watching" };
    const flip = { ...node(45, "轻拍翻转小魔女 / Flip Flappers"), libraryRootId: 2 };
    mocks.api.listRoots.mockResolvedValue([root, watching]);
    mocks.api.search.mockImplementation(async (_query, scope) => scope === 1 ? [] : [{ kind: "NODE", node: flip, mediaFile: null }]);
    mount();
    await screen.findByRole("heading", { name: "全部资源" });
    // Settings retains the selected library, which previously leaked into sidebar search.
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    expect(document.querySelector(".content-scroll.is-settings")).not.toBeNull();
    fireEvent.click(within(document.querySelector(".sidebar")!).getByRole("button", { name: "搜索" }));
    const query = document.querySelector(".search-toolbar input")!;
    expect(document.querySelector(".content-scroll.is-settings")).toBeNull();
    expect(query.closest(".page-title-row")).toBeNull();
    expect(query.closest("form")?.previousElementSibling?.className).toBe("page-title-row");
    fireEvent.change(query, { target: { value: "轻拍" } });
    await screen.findByText(flip.displayName);
    expect(mocks.api.search).toHaveBeenLastCalledWith("轻拍", undefined);
    chooseOption(screen.getByRole("combobox", { name: "搜索范围" }), "1");
    await waitFor(() => expect(mocks.api.search).toHaveBeenLastCalledWith("轻拍", 1));
    await waitFor(() => expect(screen.queryByText(flip.displayName)).toBeNull());
    fireEvent.click(within(document.querySelector(".sidebar")!).getByRole("button", { name: "搜索" }));
    await screen.findByText(flip.displayName);
    fireEvent.change(query, { target: { value: "flip" } });
    await waitFor(() => expect(mocks.api.search).toHaveBeenLastCalledWith("flip", undefined));
    chooseOption(screen.getByRole("combobox", { name: "搜索范围" }), "1");
    await waitFor(() => expect(screen.queryByText(flip.displayName)).toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(within(document.querySelector(".sidebar")!).getByRole("button", { name: "搜索" }));
    await screen.findByText(flip.displayName);
    expect(mocks.api.search).toHaveBeenLastCalledWith("flip", undefined);
  });

  it("sorts by source-file update time, shows dates only for that sort, and remembers the choice", async () => {
    settings.autoScanOnStartup = false;
    const items = [
      { ...node(1, "Older Work"), latestFileModifiedAt: "2025-01-02T00:00:00Z", updatedAt: "2099-01-01" },
      { ...node(2, "Newer Work"), latestFileModifiedAt: "2026-09-12T08:30:00Z", updatedAt: "2000-01-01" },
      node(3, "Unknown Work"),
    ];
    mocks.api.allResources.mockResolvedValue({ ...catalogue, works: items.map((item) => ({ node: item, sources: [item] })) });
    mount();
    await screen.findByText("Newer Work");
    expect(document.querySelector(".file-modified-time")).toBeNull();
    const sort = screen.getByRole("combobox", { name: "排序方式" });
    chooseOption(sort, "modified");
    fireEvent.click(screen.getByRole("button", { name: "正序" }));
    const titles = () => [...document.querySelectorAll(".media-card-copy strong")].map((el) => el.textContent);
    expect(titles()).toEqual(["Newer Work", "Older Work", "Unknown Work"]);
    expect(document.querySelectorAll("time.file-modified-time")).toHaveLength(2);
    expect(screen.getByText("最后更新时间未知")).toBeTruthy();
    await waitFor(() => expect(mocks.api.updateCollectionSortPreference).toHaveBeenCalledWith("all", "modified-desc"));
    fireEvent.click(screen.getByRole("button", { name: "倒序" }));
    expect(titles()).toEqual(["Older Work", "Newer Work", "Unknown Work"]);
    fireEvent.click(screen.getByTitle("列表"));
    expect(document.querySelectorAll(".media-card-list time.file-modified-time")).toHaveLength(2);
    await waitFor(() => expect(mocks.api.updateCollectionSortPreference).toHaveBeenCalledWith("all", "modified-asc"));
    chooseOption(sort, "title");
    expect(document.querySelector(".file-modified-time")).toBeNull();
    await act(async () => undefined);
    cleanup();
    mocks.api.getCollectionSortPreferences.mockResolvedValue({ all: "modified-desc", browse: "title-asc", favorites: "title-asc" });
    mount();
    await screen.findByText("Newer Work");
    // Catalogue rows may settle before persisted sort restoration; wait for the complete state.
    await waitFor(() => {
      expect(titles()).toEqual(["Newer Work", "Older Work", "Unknown Work"]);
      expect(document.querySelectorAll("time.file-modified-time")).toHaveLength(2);
    });
  });

  it("opens hidden entries from settings, filters by path, and restores the selected original node", async () => {
    settings.autoScanOnStartup = false;
    const hidden = [node(8, "Hidden Show"), node(9, "Another Show")].map((item) => ({ ...item, nodeType: "IGNORED" as const }));
    mocks.api.listHiddenNodes.mockImplementation(async () => [...hidden]);
    mocks.api.resetNodeType.mockImplementation(async (id: number) => {
      hidden.splice(hidden.findIndex((item) => item.id === id), 1);
      mocks.api.allResources.mockResolvedValue({ ...catalogue, works: [...catalogue.works, { node: node(id, "Hidden Show"), sources: [node(id, "Hidden Show")] }] });
      return node(id, "Hidden Show");
    });
    mount();
    await screen.findByRole("heading", { name: "全部资源" });
    expect(within(document.querySelector(".sidebar")!).queryByRole("button", { name: "已隐藏条目" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const entry = await screen.findByRole("button", { name: "已隐藏条目" });
    entry.focus();
    fireEvent.click(entry);
    const dialog = within(screen.getByRole("dialog", { name: "已隐藏条目" }));
    await dialog.findByText("Hidden Show");
    fireEvent.change(dialog.getByRole("textbox"), { target: { value: "X:/Fixtures/Hidden" } });
    expect(dialog.queryByText("Another Show")).toBeNull();
    mocks.api.listRecentlyWatched.mockClear();
    fireEvent.click(dialog.getByRole("button", { name: "恢复 Hidden Show" }));
    await dialog.findByText("已恢复「Hidden Show」。");
    expect(mocks.api.resetNodeType).toHaveBeenCalledWith(8);
    await waitFor(() => expect(dialog.queryByRole("button", { name: "恢复 Hidden Show" })).toBeNull());
    fireEvent.change(dialog.getByRole("textbox"), { target: { value: "" } });
    await dialog.findByText("Another Show");
    fireEvent.keyDown(dialog.getByRole("textbox"), { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(entry);
    fireEvent.click(document.querySelector(".brand")!);
    await screen.findByText("Hidden Show");
    expect(mocks.api.listRecentlyWatched).toHaveBeenCalledTimes(1);
  });

  it("supports retry after a hidden list failure and retains a row when restoration fails", async () => {
    settings.autoScanOnStartup = false;
    mocks.api.listHiddenNodes.mockRejectedValue(new Error("Load failed"));
    mount();
    await screen.findByRole("heading", { name: "全部资源" });
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(await screen.findByRole("button", { name: "已隐藏条目" }));
    const dialog = within(screen.getByRole("dialog"));
    await dialog.findByText("Load failed");
    expect(dialog.queryByText("没有已隐藏的条目。")).toBeNull();
    mocks.api.listHiddenNodes.mockResolvedValue([{ ...node(8, "Hidden Show"), nodeType: "IGNORED" }]);
    fireEvent.click(dialog.getByRole("button", { name: "重试" }));
    await dialog.findByText("Hidden Show");
    mocks.api.resetNodeType.mockRejectedValueOnce(new Error("Restore failed"));
    fireEvent.click(dialog.getByRole("button", { name: "恢复 Hidden Show" }));
    await dialog.findByText("Restore failed");
    expect(dialog.getByRole("button", { name: "恢复 Hidden Show" })).toBeTruthy();
    mocks.api.listHiddenNodes.mockResolvedValue([]);
    fireEvent.click(dialog.getByRole("button", { name: "恢复 Hidden Show" }));
    await dialog.findByText("没有已隐藏的条目。");
    expect(mocks.api.resetNodeType).toHaveBeenCalledTimes(2);
  });

  it("does not resurrect a restored hidden item from an older list response", async () => {
    settings.autoScanOnStartup = false;
    const hidden = { ...node(8, "Hidden Show"), nodeType: "IGNORED" as const };
    mocks.api.listHiddenNodes.mockResolvedValue([hidden]);
    mount();
    await screen.findByRole("heading", { name: "全部资源" });
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(await screen.findByRole("button", { name: "已隐藏条目" }));
    const dialog = within(screen.getByRole("dialog"));
    await dialog.findByText("Hidden Show");
    let finishOldRequest!: (nodes: MediaNode[]) => void;
    mocks.api.listHiddenNodes.mockImplementationOnce(() => new Promise<MediaNode[]>((resolve) => { finishOldRequest = resolve; }));
    fireEvent.click(dialog.getByRole("button", { name: "刷新列表" }));
    mocks.api.listHiddenNodes.mockResolvedValue([]);
    fireEvent.click(dialog.getByRole("button", { name: "恢复 Hidden Show" }));
    await dialog.findByText("已恢复「Hidden Show」。");
    await act(async () => finishOldRequest([hidden]));
    expect(dialog.queryByRole("button", { name: "恢复 Hidden Show" })).toBeNull();
    await dialog.findByText("没有已隐藏的条目。");
  });

  it("scans once under StrictMode and refreshes the catalogue on completion", async () => {
    mount();
    await waitFor(() => expect(mocks.api.startScan).toHaveBeenCalledTimes(1));
    expect(mocks.api.startScan).toHaveBeenCalledWith(undefined, undefined, true);
    expect(document.querySelector(".scan-banner")).toBeNull();
    mocks.api.allResources.mockResolvedValue({ ...catalogue, works: [
      { node: { ...work, totalVideoCount: 3 }, sources }, { node: node(3, "New Work"), sources: [node(3, "New Work")] },
    ] });
    await act(async () => { mocks.finished.forEach((callback) => callback(progress)); });
    await screen.findByText("New Work");
    expect(document.querySelector(".toast")).toBeNull();
    expect(mocks.api.startScan).toHaveBeenCalledTimes(1);
  });

  it.each([false, true])("points to background cover progress after a completed manual scan/match (%s)", async (matchOnly) => {
    settings.autoScanOnStartup = false;
    mount(); await screen.findByText("Example Work");
    await act(async () => { mocks.finished.forEach(callback => callback({ ...progress, background: false, phase: matchOnly ? "AUTO_MATCHING" : "SCANNING", foldersScanned: matchOnly ? 0 : 1, videosFound: matchOnly ? 0 : 1 })); });
    expect(await screen.findByText(/将自动在后台准备适配当前资源库的封面尺寸/)).toBeTruthy();
  });

  it.each(["COMPLETED", "FAILED"] as const)("keeps unchanged content in place and stays silent on background %s", async (status) => {
    mount();
    await waitFor(() => expect(mocks.api.startScan).toHaveBeenCalledTimes(1));
    await screen.findByText("Example Work");
    const card = document.querySelector(".media-card");
    mocks.api.allResources.mockClear();
    mocks.api.listRoots.mockClear();
    mocks.api.listRecentlyWatched.mockClear();
    await act(async () => { mocks.progress.forEach((callback) => callback({ ...progress, status: "RUNNING" })); });
    expect(document.querySelector(".scan-banner")).toBeNull();
    await act(async () => { mocks.finished.forEach((callback) => callback({ ...progress, status, libraryChanged: false })); });
    expect(mocks.api.allResources).not.toHaveBeenCalled();
    expect(mocks.api.listRoots).toHaveBeenCalledTimes(1);
    expect(mocks.api.listRecentlyWatched).not.toHaveBeenCalled();
    expect(document.querySelector(".media-card")).toBe(card);
    expect(document.querySelector(".toast")).toBeNull();
  });

  it("keeps manual scan progress and completion feedback visible", async () => {
    settings.autoScanOnStartup = false;
    mount();
    await screen.findByText("Example Work");
    await act(async () => { mocks.progress.forEach((callback) => callback({ ...progress, background: false, status: "RUNNING" })); });
    expect(document.querySelector(".scan-banner")).not.toBeNull();
    mocks.api.allResources.mockClear();
    await act(async () => { mocks.finished.forEach((callback) => callback({ ...progress, background: false })); });
    expect(document.querySelector(".scan-banner")).toBeNull();
    expect(mocks.api.allResources).toHaveBeenCalled();
    expect(document.querySelector(".toast")).not.toBeNull();
  });

  it.each(["disabled", "empty", "already scanning"])("does not auto-start when %s", async (reason) => {
    if (reason === "disabled") settings.autoScanOnStartup = false;
    if (reason === "empty") mocks.api.listRoots.mockResolvedValue([]);
    if (reason === "already scanning") mocks.api.scanStatus.mockResolvedValue({ ...progress, status: "RUNNING" });
    mount();
    await waitFor(() => expect(mocks.api.showMainWindow).toHaveBeenCalled());
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 20)); });
    expect(mocks.api.startScan).not.toHaveBeenCalled();
  });

  it("does not resurrect a scan that completes before the start response", async () => {
    mocks.api.startScan.mockImplementation(async () => {
      mocks.finished.forEach((callback) => callback(progress));
      return { scanId: progress.scanId };
    });
    mount();
    await waitFor(() => expect(mocks.api.startScan).toHaveBeenCalledTimes(1));
    await act(async () => undefined);
    expect(document.querySelector(".scan-banner")).toBeNull();
  });

  it("shows editable settings while cache statistics are still pending", async () => {
    settings.autoScanOnStartup = false;
    mocks.api.cacheStats.mockReturnValue(new Promise(() => {}));
    mocks.api.posterCacheStatus.mockReturnValue(new Promise(() => {}));
    mount(); await screen.findByText("Example Work");
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const toggle = await screen.findByRole("checkbox", { name: "启动时自动更新资源库" });
    expect((toggle as HTMLInputElement).disabled).toBe(false);
    fireEvent.click(toggle);
    await waitFor(() => expect(settings.autoScanOnStartup).toBe(true));
    expect(document.querySelector(".settings-page")).not.toBeNull();
  });

  it("shows actual background cover progress and stops polling after navigation", async () => {
    settings.autoScanOnStartup = false;
    let phase = "RUNNING";
    mocks.api.posterCacheStatus.mockImplementation(async () => ({phase, processed: phase === "RUNNING" ? 2 : 5, total: 5, failed: 1}));
    mount(); await screen.findByText("Example Work");
    fireEvent.click(screen.getByRole("button", {name: "设置"}));
    expect(await screen.findByText("已处理 2 / 5 个封面")).toBeTruthy();
    expect(screen.getByRole("progressbar", {name: "适配封面"}).getAttribute("value")).toBe("2");
    const initialStatsCalls = mocks.api.cacheStats.mock.calls.length;
    phase = "COMPLETED";
    await screen.findByText("本轮处理完成", {}, {timeout: 2500});
    expect(screen.getByText("1 个未能生成")).toBeTruthy();
    expect(screen.getByRole("progressbar", {name: "适配封面"}).getAttribute("value")).toBe("5");
    await waitFor(() => expect(mocks.api.cacheStats).toHaveBeenCalledTimes(initialStatsCalls + 1));
    fireEvent.click(screen.getByRole("button", {name: "全部资源"}));
    const calls = mocks.api.posterCacheStatus.mock.calls.length;
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 1100)); });
    expect(mocks.api.posterCacheStatus).toHaveBeenCalledTimes(calls);
  });

  it("does not restore old statistics after the cache is cleared", async () => {
    settings.autoScanOnStartup = false;
    let finish!: (value: {fileCount: number; totalBytes: number; cacheDirectory: string}) => void;
    mocks.api.cacheStats.mockReturnValue(new Promise(resolve => {finish = resolve;}));
    mount(); await screen.findByText("Example Work");
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(await screen.findByRole("button", { name: "清理" }));
    await waitFor(() => expect(mocks.api.clearCoverCache).toHaveBeenCalled());
    await act(async () => finish({fileCount: 999, totalBytes: 999, cacheDirectory: "X:/AppCache"}));
    expect(document.querySelector(".maintenance-grid article p")?.textContent).toContain("0");
    expect(document.querySelector(".maintenance-grid article p")?.textContent).not.toContain("999");
  });

  it("keeps autosave responsive during cache refresh and discards statistics from the old directory", async () => {
    settings.autoScanOnStartup = false;
    type Stats = {fileCount: number; totalBytes: number; cacheDirectory: string};
    let finishOld!: (value: Stats) => void, finishNew!: (value: Stats) => void;
    const oldStats = new Promise<Stats>(resolve => {finishOld = resolve;});
    const newStats = new Promise<Stats>(resolve => {finishNew = resolve;});
    mocks.api.cacheStats.mockImplementation(() => settings.coverCacheDirectory === "X:/NewCache" ? newStats : oldStats);
    mocks.chooseCoverCacheDirectory.mockResolvedValue("X:/NewCache");
    mount(); await screen.findByText("Example Work");
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    await screen.findByRole("checkbox", { name: "启动时自动更新资源库" });
    fireEvent.click(document.querySelector(".cache-directory-field button")!);
    await waitFor(() => expect(settings.coverCacheDirectory).toBe("X:/NewCache"));
    fireEvent.click(screen.getByRole("checkbox", { name: "启动时自动更新资源库" }));
    await waitFor(() => expect(settings.autoScanOnStartup).toBe(true));
    await act(async () => finishNew({fileCount: 7, totalBytes: 700, cacheDirectory: "X:/NewCache"}));
    await act(async () => finishOld({fileCount: 999, totalBytes: 999, cacheDirectory: "X:/AppCache"}));
    expect(document.querySelector(".maintenance-grid article p")?.textContent).toContain("7");
    expect(document.querySelector(".maintenance-grid article p")?.textContent).not.toContain("999");
  });

  it("persists the startup-scan toggle and respects it on the next launch", async () => {
    mount();
    await waitFor(() => expect(mocks.api.startScan).toHaveBeenCalledTimes(1));
    await act(async () => { mocks.finished.forEach((callback) => callback(progress)); });
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const toggle = await screen.findByRole("checkbox", { name: "启动时自动更新资源库" });
    await waitFor(() => expect((toggle as HTMLInputElement).disabled).toBe(false));
    expect((toggle as HTMLInputElement).checked).toBe(true);
    fireEvent.click(toggle);
    await waitFor(() => expect(settings.autoScanOnStartup).toBe(false));
    cleanup();
    mount();
    await screen.findByRole("heading", { name: "全部资源" });
    await act(async () => undefined);
    expect(mocks.api.startScan).toHaveBeenCalledTimes(1);
  });

  it("opens aggregated videos, restores works, and offers folder and source editing views", async () => {
    settings.autoScanOnStartup = false;
    mount();
    await screen.findByRole("heading", { name: "全部资源" });
    expect(document.querySelectorAll(".media-card")).toHaveLength(1);
    fireEvent.click(screen.getByText("Example Work").closest("button")!);
    await screen.findByText("Episode 02.mkv");
    expect(mocks.api.nodeDetail).toHaveBeenCalledWith(1, true);
    expect(document.querySelector(".work-sources")).not.toBeNull();
    fireEvent.click(document.querySelector(".back-button")!);
    await screen.findByRole("heading", { name: "全部资源" });
    fireEvent.click(screen.getByRole("button", { name: /编辑模式/ }));
    expect(document.querySelectorAll(".media-card")).toHaveLength(2);
    expect(screen.getByText("Release B")).toBeTruthy();
  });
  it("flattens nested same-name videos and sends their original IDs to playback and reveal", async () => {
    settings.autoScanOnStartup=false;
    const file={id:901,nodeId:51,fileName:"01.mkv",absolutePath:"X:/Fixtures/Release B/SPs/01.mkv",extension:".mkv",fileSize:100,modifiedAt:"2026-01-01",lastSeenAt:"2026-01-01",durationMs:null,width:null,height:null,codec:null};
    mocks.api.nodeDetail.mockResolvedValue({node:{...work,workView:true,workTarget},workTarget,binding:null,children:[],resourceFiles:[],breadcrumbs:[],workSources:sources,mediaFiles:[],nestedMediaFiles:[{file,sourceNodeId:2,sourceName:"Release B",relativeDirectory:"SPs"}]});
    mount();await screen.findByText("Example Work");fireEvent.click(document.querySelector(".media-card-open")!);
    const title=await screen.findByText("01.mkv");expect(screen.getByText(/Release B \/ SPs/)).toBeTruthy();
    fireEvent.doubleClick(title.closest(".media-file-row")!);await waitFor(()=>expect(mocks.api.playMedia).toHaveBeenCalledWith(901));
    fireEvent.click(screen.getByRole("button",{name:/在资源管理器中显示 .*01.mkv/}));await waitFor(()=>expect(mocks.api.openMediaInExplorer).toHaveBeenCalledWith(901));
  });

  it("binds all aggregated sources with the current snapshot",async()=>{
    settings.autoScanOnStartup=false;
    const subject={subjectId:42,subjectType:2,title:"Example Subject",titleCn:null,titleEn:null,titleJa:null,titleKo:null,matchAliases:[],date:null,imageUrl:null,summary:null};
    mocks.api.searchBangumi.mockResolvedValue([subject]);
    mocks.api.bindWorkBangumi.mockResolvedValue({id:1,nodeId:1,provider:"BANGUMI",providerSubjectId:42,providerSubjectType:2,providerTitle:"Example Subject",providerAliases:[],coverCachePath:null});
    mount();await screen.findByText("Example Work");fireEvent.contextMenu(document.querySelector(".media-card")!);
    fireEvent.click(screen.getByRole("button",{name:"从 Bangumi 搜索并添加"}));
    const dialog=within(await screen.findByRole("dialog"));fireEvent.click(dialog.getByRole("button",{name:"搜索"}));
    fireEvent.click(await dialog.findByRole("button",{name:"选择"}));
    await waitFor(()=>expect(mocks.api.bindWorkBangumi).toHaveBeenCalledWith(workTarget,subject));
    expect(mocks.api.bindBangumi).not.toHaveBeenCalled();
  });

  it("requires source selection before renaming an aggregated card",async()=>{
    settings.autoScanOnStartup=false;mocks.api.renameNode.mockResolvedValue(sources[1]);
    mount();await screen.findByText("Example Work");fireEvent.contextMenu(document.querySelector(".media-card")!);
    fireEvent.click(screen.getByRole("button",{name:"修改显示名称"}));
    const chooser=within(await screen.findByRole("dialog",{name:"选择资源来源"}));
    fireEvent.click(chooser.getByRole("button",{name:/Release B/}));
    fireEvent.change(screen.getByRole("textbox",{name:"显示名称"}),{target:{value:"My release"}});
    fireEvent.click(screen.getByRole("button",{name:"保存名称"}));
    await waitFor(()=>expect(mocks.api.renameNode).toHaveBeenCalledWith(2,"My release"));
  });

  it("shows the saved name of a bound card after refresh and when opening its detail",async()=>{
    settings.autoScanOnStartup=false;
    const bound={...sources[0],binding:{nodeId:1,provider:'BANGUMI' as const,providerSubjectId:42,providerSubjectType:2 as const,providerTitle:'Official title',providerTitleCn:'官方标题',providerTitleEn:null,providerTitleJa:null,providerTitleKo:null,providerDate:null,providerImageUrl:null,boundAt:'2026-01-01',updatedAt:'2026-01-01',coverCachePath:null,coverDownloadError:null}};
    const result=(value:MediaNode)=>({nodes:[value],totalCount:1,works:[{node:value,sources:[value]}]});
    mocks.api.allResources.mockResolvedValue(result(bound));
    mocks.api.nodeDetail.mockResolvedValue({node:bound,binding:bound.binding,mediaFiles:[],resourceFiles:[],children:[],breadcrumbs:[]});
    mocks.api.renameNode.mockImplementation(async(_id,name)=>{
      const updated={...bound,displayName:name,manualDisplayName:true};
      mocks.api.allResources.mockResolvedValue(result(updated));
      mocks.api.nodeDetail.mockResolvedValue({node:updated,binding:bound.binding,mediaFiles:[],resourceFiles:[],children:[],breadcrumbs:[]});
      return updated;
    });
    mount();await screen.findByText('官方标题');
    fireEvent.contextMenu(document.querySelector('.media-card')!);
    fireEvent.click(screen.getByRole('button',{name:'修改显示名称'}));
    expect(screen.queryByText('仅修改 M²Shelf')).toBeNull();
    fireEvent.change(screen.getByRole('textbox',{name:'显示名称'}),{target:{value:'My preferred title'}});
    fireEvent.click(screen.getByRole('button',{name:'保存名称'}));
    await waitFor(()=>expect(document.querySelector('.media-card-copy > strong')?.textContent).toBe('My preferred title'));
    fireEvent.click(document.querySelector('.media-card-open')!);
    await waitFor(()=>expect(document.querySelector('.detail-copy h1')?.textContent).toBe('My preferred title'));
    expect(screen.getByText('官方标题')).toBeTruthy();
  });

  it("applies a confirmed series rename before revalidation finishes and preserves it across grid/list and detail",async()=>{
    settings.autoScanOnStartup=false;settings.allResourcesFlattened=false;
    const series={...sources[0],nodeType:'CONTAINER' as const,mediaKind:'LIVE_ACTION' as const,displayName:'Source (2015)',manualDisplayName:false,tmdbBinding:{active:true,coverCachePath:null,coverError:null,movie:{mediaType:'TV' as const,id:22,title:'Official TV Show',originalTitle:'Original Show',releaseDate:'2015-01-01',overview:'',posterPath:null}}};
    const result=(node:MediaNode)=>({nodes:[node],totalCount:1,works:[]});
    mocks.api.allResources.mockResolvedValue(result(series));
    let finish!:(value:ReturnType<typeof result>)=>void;
    mocks.api.renameNode.mockImplementation(async(_id,name)=>{
      const updated={...series,displayName:name,manualDisplayName:true};
      mocks.api.allResources.mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));
      mocks.api.nodeDetail.mockResolvedValue({node:updated,binding:null,mediaFiles:[],resourceFiles:[],children:[],breadcrumbs:[]});
      return updated;
    });
    mount();await screen.findByText('Official TV Show系列');
    const card=document.querySelector('.media-card');fireEvent.contextMenu(card!);fireEvent.click(screen.getByRole('button',{name:'修改显示名称'}));
    expect((screen.getByRole('textbox',{name:'显示名称'}) as HTMLInputElement).value).toBe('Official TV Show系列');
    fireEvent.change(screen.getByRole('textbox',{name:'显示名称'}),{target:{value:'My TV Series (2015)'}});fireEvent.click(screen.getByRole('button',{name:'保存名称'}));
    await waitFor(()=>expect(document.querySelector('.media-card-copy > strong')?.textContent).toBe('My TV Series (2015)'));
    expect(document.querySelector('.media-card')).toBe(card);expect(document.querySelector('.loading-state')).toBeNull();
    await act(async()=>finish(result({...series,displayName:'My TV Series (2015)',manualDisplayName:true})));
    await waitFor(()=>expect(screen.queryByRole('dialog',{name:'修改显示名称'})).toBeNull());
    fireEvent.click(document.querySelector('.media-card-open')!);
    await waitFor(()=>expect(document.querySelector('.detail-copy h1')?.textContent).toBe('My TV Series (2015)'));
  });

  it("shows durable scan failure details without replacing the unchanged work card",async()=>{
    mount();await screen.findByText("Example Work");await waitFor(()=>expect(mocks.api.startScan).toHaveBeenCalledTimes(1));
    const card=document.querySelector(".media-card");
    mocks.api.listRoots.mockResolvedValue([{...root,scanHealth:{lastAutoAttemptAt:"2026-01-01",lastSuccessAt:null,outcome:"PARTIAL",errorCount:1,detail:"Synthetic subdirectory permission failure"}}]);
    await act(async()=>{mocks.finished.forEach(callback=>callback({...progress,libraryChanged:false}));});
    await waitFor(()=>expect(document.querySelector(".root-scan-warning")).not.toBeNull());expect(document.querySelector(".media-card")).toBe(card);
    fireEvent.click(document.querySelector(".root-scan-warning")!);
    expect(await screen.findByText("Synthetic subdirectory permission failure")).toBeTruthy();
  });

  it("returns to the library when the open work disappears during a refresh", async () => {
    mount();
    await screen.findByText("Example Work");
    fireEvent.click(document.querySelector(".media-card-open")!);
    await screen.findByText("Episode 01.mkv");
    mocks.api.nodeDetail.mockRejectedValue(new Error("NODE_NOT_VISIBLE: hidden"));
    await act(async () => {
      mocks.finished.forEach(callback => callback({ ...progress, scanId: "after-hide" }));
    });
    await waitFor(() => expect(mocks.api.browse).toHaveBeenCalledWith(1, null));
    await waitFor(() => expect(document.querySelector(".work-detail-page")).toBeNull());
    expect(screen.queryByText("Episode 01.mkv")).toBeNull();
  });

});


function chooseOption(control: HTMLElement, value: string) {
 const option=Array.from(control.parentElement!.querySelector('select')!.options).find(option=>option.value===value)!;
 fireEvent.click(control);fireEvent.click(screen.getByRole('option',{name:option.textContent!}));
}

it("shares caption and Settings preferences, keeps system changes live, and persists without another read", async () => {
  settings.autoScanOnStartup=false;
  const themeListeners=new Set<()=>void>();
  const media={matches:false,addEventListener:vi.fn((_event:string,callback:()=>void)=>themeListeners.add(callback)),removeEventListener:vi.fn((_event:string,callback:()=>void)=>themeListeners.delete(callback))};
  window.matchMedia=vi.fn().mockReturnValue(media);
  mount(true);await screen.findByText('Example Work');
  const caption=within(document.querySelector('.window-titlebar')!);
  chooseOption(caption.getByRole('combobox',{name:'主题'}),'dark');
  await waitFor(()=>expect(document.documentElement.dataset.theme).toBe('dark'));
  fireEvent.click(screen.getByRole('button',{name:'设置'}));
  await screen.findByRole('heading',{name:'设置'});
  const page=within(document.querySelector('.settings-page')!);
  expect(page.getByRole('combobox',{name:'主题'}).textContent).toContain('深色');
  chooseOption(page.getByRole('combobox',{name:'主题'}),'system');
  await waitFor(()=>expect(document.documentElement.dataset.themePreference).toBe('system'));
  await act(async()=>{media.matches=true;themeListeners.forEach(callback=>callback());});
  expect(document.documentElement.dataset.theme).toBe('dark');
  chooseOption(caption.getByRole('combobox',{name:'界面语言'}),'en-US');
  await screen.findByRole('heading',{name:'Settings'});
  expect(page.getByRole('combobox',{name:'Interface language'}).textContent).toContain('English');
  await waitFor(()=>expect(settings).toMatchObject({language:'en-US',theme:'system'}));
  expect(mocks.api.getSettings).toHaveBeenCalledTimes(1);
});

it("rolls back a failed caption edit after leaving Settings and keeps newer page edits in the queue", async () => {
  settings.autoScanOnStartup=false;
  mount(true);await screen.findByText('Example Work');
  let finish!: (settings:AppSettings)=>void;
  mocks.api.updateSettings.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
  chooseOption(within(document.querySelector('.window-titlebar')!).getByRole('combobox',{name:'主题'}),'dark');
  fireEvent.click(screen.getByRole('button',{name:'设置'}));
  const startup=await screen.findByRole('checkbox',{name:'启动时自动更新资源库'});fireEvent.click(startup);
  expect(mocks.api.updateSettings).toHaveBeenCalledTimes(1);
  const first=mocks.api.updateSettings.mock.calls[0][0];
  await act(async()=>finish(first));
  await waitFor(()=>expect(settings).toMatchObject({theme:'dark',autoScanOnStartup:true}));
  let reject!: (error:Error)=>void;
  mocks.api.updateSettings.mockImplementationOnce(()=>new Promise((_resolve,no)=>{reject=no;}));
  chooseOption(within(document.querySelector('.window-titlebar')!).getByRole('combobox',{name:'主题'}),'light');
  fireEvent.click(screen.getByRole('button',{name:'全部资源'}));
  expect(document.documentElement.dataset.theme).toBe('light');
  await act(async()=>reject(new Error('fixture save failed')));
  await waitFor(()=>expect(document.documentElement.dataset.theme).toBe('dark'));
  expect(document.querySelector('.toast.is-error, .toast-error, .toast.error') ?? document.querySelector('.toast')).not.toBeNull();
  expect(settings.autoScanOnStartup).toBe(true);
});

it("filters real Root kinds with tabs and retains legacy VIDEO eligibility", async () => {
  settings.autoScanOnStartup=false;settings.allResourcesFlattened=false;
  const kinds=['ANIMATION','LIVE_ACTION','COMIC','EBOOK','DOUJIN','ARTBOOK','VIDEO'] as const;
  const nodes=kinds.map((kind,i)=>({...node(i+200,'Kind '+kind),mediaKind:kind}));
  mocks.api.allResources.mockResolvedValue({...catalogue,nodes,totalCount:nodes.length});
  mount();await screen.findByText('Kind ANIMATION');
  expect(screen.getAllByRole('tab').map(tab=>tab.textContent)).toEqual(['全部','动画','真人影视','漫画','电子书','同人本','设定集']);
  expect(screen.queryByRole('combobox',{name:'资源类型'})).toBeNull();
  fireEvent.click(screen.getByRole('tab',{name:'动画'}));
  expect(screen.getByText('Kind ANIMATION')).toBeTruthy();expect(screen.getByText('Kind VIDEO')).toBeTruthy();expect(screen.queryByText('Kind LIVE_ACTION')).toBeNull();
  fireEvent.keyDown(screen.getByRole('tab',{name:'动画'}),{key:'ArrowRight'});
  expect(screen.getByRole('tab',{name:'真人影视'}).getAttribute('aria-selected')).toBe('true');
  expect(screen.getByText('Kind LIVE_ACTION')).toBeTruthy();expect(screen.getByText('Kind VIDEO')).toBeTruthy();
  fireEvent.click(screen.getByRole('tab',{name:'同人本'}));
  expect(document.querySelectorAll('.media-card')).toHaveLength(1);expect(screen.getByText('Kind DOUJIN')).toBeTruthy();
  fireEvent.click(screen.getByRole('tab',{name:'设定集'}));
  expect(document.querySelectorAll('.media-card')).toHaveLength(1);expect(screen.getByText('Kind ARTBOOK')).toBeTruthy();
});

it("does not let a late bootstrap index overwrite preferences edited while it was loading", async () => {
  settings.autoScanOnStartup=false;
  let finish!: (value:typeof catalogue)=>void;
  mocks.api.allResources.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
  mount();
  fireEvent.click(screen.getByRole('button',{name:'设置'}));
  const toggle=await screen.findByRole('checkbox',{name:'全部资源穿透显示'});
  fireEvent.click(toggle);await waitFor(()=>expect(settings.allResourcesFlattened).toBe(false));
  await act(async()=>finish(catalogue));
  fireEvent.click(screen.getByRole('button',{name:'全部资源'}));
  await screen.findByText('Archive');expect(screen.queryByText('Example Work')).toBeNull();
});

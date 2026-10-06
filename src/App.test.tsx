// @vitest-environment jsdom
import { StrictMode } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings, MediaNode, ScanProgress } from "./types/media";

const mocks = vi.hoisted(() => ({
  api: {
    bootstrap: vi.fn(), getSettings: vi.fn(), listRoots: vi.fn(),
    getCollectionSortPreferences: vi.fn(), updateCollectionSortPreference: vi.fn(), scanStatus: vi.fn(), allResources: vi.fn(),
    listRecentlyWatched: vi.fn(), listFavoriteFolders: vi.fn(), showMainWindow: vi.fn(),
    startScan: vi.fn(), nodeDetail: vi.fn(), cacheStats: vi.fn(), updateSettings: vi.fn(),
    bindWorkBangumi: vi.fn(), bindBangumi: vi.fn(), clearWorkBangumi: vi.fn(), retryWorkBangumiCover: vi.fn(), bangumiPrefill: vi.fn(), searchBangumi: vi.fn(), renameNode: vi.fn(), playMedia: vi.fn(), openMediaInExplorer: vi.fn(),
    listHiddenNodes: vi.fn(), resetNodeType: vi.fn(), search: vi.fn(), browse: vi.fn(), syncPendingBangumiAliases: vi.fn(), openBangumiSubject: vi.fn(),
  },
  progress: new Set<(value: ScanProgress) => void>(),
  finished: new Set<(value: ScanProgress) => void>(),
}));

vi.mock("./lib/api", async (importOriginal) => ({
  ...await importOriginal<typeof import("./lib/api")>(),
  desktopAvailable: true,
  api: mocks.api,
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
const mount = () => render(<StrictMode><I18nProvider><App /></I18nProvider></StrictMode>);

beforeEach(() => {
  vi.resetAllMocks();
  mocks.progress.clear(); mocks.finished.clear();
  window.history.replaceState(null, "", "/");
  window.matchMedia = vi.fn().mockReturnValue({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() });
  Element.prototype.scrollTo = vi.fn();
  vi.stubGlobal("IntersectionObserver", class { observe() {} unobserve() {} disconnect() {} });
  settings = {
    mpvPath: null, defaultViewMode: "GRID", videoExtensions: [".mkv"],
    bangumiSearchEnabled: false, autoCheckUpdates: false, autoScanOnStartup: true,
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
  mocks.api.allResources.mockResolvedValue(catalogue);
  mocks.api.listRecentlyWatched.mockResolvedValue([]);
  mocks.api.listFavoriteFolders.mockResolvedValue([]);
  mocks.api.listHiddenNodes.mockResolvedValue([]);
  mocks.api.resetNodeType.mockImplementation(async (id: number) => node(id, "Restored Work"));
  mocks.api.showMainWindow.mockResolvedValue(undefined);
  mocks.api.startScan.mockResolvedValue({ scanId: "startup-test" });
  mocks.api.cacheStats.mockResolvedValue({ fileCount: 0, totalBytes: 0, cacheDirectory: "X:/AppCache" });
  mocks.api.nodeDetail.mockImplementation(async (_id: number, workView: boolean) => ({
    node: { ...work, workView, workTarget: workView ? workTarget : undefined }, workTarget: workView ? workTarget : null, binding: null, children: [], resourceFiles: [], breadcrumbs: [],
    workSources: workView ? sources : null,
    mediaFiles: sources.map((source, index) => ({ id: index + 1, nodeId: source.id,
      fileName: `Episode 0${index + 1}.mkv`, absolutePath: `${source.absolutePath}/01.mkv`,
      extension: ".mkv", fileSize: 100, modifiedAt: "2026-01-01", lastSeenAt: "2026-01-01",
      durationMs: null, width: null, height: null, codec: null })),
  }));
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe("startup refresh and work browsing", () => {
  it.each([
    ["zh-CN", "原作者", "功能改进与维护"],
    ["en-US", "Original author", "Feature improvements and maintenance"],
    ["ja-JP", "原作者", "機能改善・メンテナンス"],
    ["ko-KR", "원작자", "기능 개선 및 유지보수"],
  ] as const)("shows both author roles in %s", async (language, original, contributor) => {
    settings.autoScanOnStartup = false;
    mount();
    await screen.findByText("Example Work");
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    await screen.findByText("关于");
    const languageSelect = screen.getByRole("combobox", { name: "界面语言" }) as HTMLSelectElement;
    await waitFor(() => expect(languageSelect.disabled).toBe(false));
    fireEvent.change(languageSelect, { target: { value: language } });
    const about = within(document.querySelector(".about-grid") as HTMLElement);
    expect(await about.findByText(original)).toBeTruthy();
    expect(about.getByText(contributor)).toBeTruthy();
    expect(about.getByText("森下Undermori")).toBeTruthy();
    expect(about.getByText("Juvenile_A")).toBeTruthy();
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
    await waitFor(() => expect(mocks.api.allResources.mock.calls.length).toBeGreaterThan(2));
    expect(document.querySelector(".toast")).toBeNull();
  });
  it("searches every library from the sidebar and exposes scope changes without retaining stale results", async () => {
    settings.autoScanOnStartup = false;
    const watching = { ...root, id: 2, displayName: "Watching" };
    const flip = { ...node(45, "轻拍翻转小魔女 / Flip Flappers"), libraryRootId: 2 };
    mocks.api.listRoots.mockResolvedValue([root, watching]);
    mocks.api.search.mockImplementation(async (_query, scope) => scope === 1 ? [] : [{ kind: "NODE", node: flip, mediaFile: null }]);
    mount();
    await screen.findByRole("heading", { name: "作品库" });
    // Settings retains the selected library, which previously leaked into sidebar search.
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(within(document.querySelector(".sidebar")!).getByRole("button", { name: "搜索" }));
    const query = document.querySelector(".search-toolbar input")!;
    fireEvent.change(query, { target: { value: "轻拍" } });
    await screen.findByText(flip.displayName);
    expect(mocks.api.search).toHaveBeenLastCalledWith("轻拍", undefined);
    const scope = screen.getByRole("combobox", { name: "搜索范围" });
    fireEvent.change(scope, { target: { value: "1" } });
    await waitFor(() => expect(mocks.api.search).toHaveBeenLastCalledWith("轻拍", 1));
    await waitFor(() => expect(screen.queryByText(flip.displayName)).toBeNull());
    fireEvent.click(within(document.querySelector(".sidebar")!).getByRole("button", { name: "搜索" }));
    await screen.findByText(flip.displayName);
    fireEvent.change(query, { target: { value: "flip" } });
    await waitFor(() => expect(mocks.api.search).toHaveBeenLastCalledWith("flip", undefined));
    fireEvent.change(scope, { target: { value: "1" } });
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
    fireEvent.change(sort, { target: { value: "modified" } });
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
    fireEvent.change(sort, { target: { value: "title" } });
    expect(document.querySelector(".file-modified-time")).toBeNull();
    await act(async () => undefined);
    cleanup();
    mocks.api.getCollectionSortPreferences.mockResolvedValue({ all: "modified-desc", browse: "title-asc", favorites: "title-asc" });
    mount();
    await screen.findByText("Newer Work");
    expect(titles()).toEqual(["Newer Work", "Older Work", "Unknown Work"]);
    expect(document.querySelectorAll("time.file-modified-time")).toHaveLength(2);
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
    await screen.findByRole("heading", { name: "作品库" });
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
    await screen.findByRole("heading", { name: "作品库" });
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
    await screen.findByRole("heading", { name: "作品库" });
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
    await screen.findByRole("heading", { name: "作品库" });
    await act(async () => undefined);
    expect(mocks.api.startScan).toHaveBeenCalledTimes(1);
  });

  it("opens aggregated videos, restores works, and offers folder and source editing views", async () => {
    settings.autoScanOnStartup = false;
    mount();
    await screen.findByRole("heading", { name: "作品库" });
    expect(document.querySelectorAll(".media-card")).toHaveLength(1);
    fireEvent.click(screen.getByText("Example Work").closest("button")!);
    await screen.findByText("Episode 02.mkv");
    expect(mocks.api.nodeDetail).toHaveBeenCalledWith(1, true);
    expect(document.querySelector(".work-sources")).not.toBeNull();
    fireEvent.click(document.querySelector(".back-button")!);
    await screen.findByRole("heading", { name: "作品库" });
    fireEvent.change(screen.getByRole("combobox", { name: "资源展示方式" }), { target: { value: "folders" } });
    await screen.findByText(/^Archive/);
    fireEvent.change(screen.getByRole("combobox", { name: "资源展示方式" }), { target: { value: "works" } });
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

// @vitest-environment jsdom
import {StrictMode, useState, type ReactNode} from 'react';
import {act, cleanup, fireEvent, render, screen, within} from '@testing-library/react';
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {SearchPage} from '../pages/SearchPage';
import {DetailVideoTabs} from '../components/DetailVideoTabs';
import {EpisodeWatchSummary, type EpisodeWatchController} from '../components/EpisodeWatchControl';
import {CommandPalette} from '../components/CommandPalette';
import {FeatureModal} from '../components/FeatureModal';
import {ConfirmDialog} from '../components/ConfirmDialog';
import {ContextMenu} from '../components/ContextMenu';
import {SourceContextMenu} from '../components/BookContextMenu';
import {RenameDialog} from '../components/RenameDialog';
import {LibraryRootRenameDialog} from '../components/LibraryRootRenameDialog';
import {TagManagerDialog} from '../components/TagManagerDialog';
import {HiddenNodesDialog} from '../components/HiddenNodesDialog';
import {LibraryRecognitionModeDialog} from '../components/LibraryRecognitionModeDialog';
import {ToastStack, type ToastMessage} from '../components/Toast';
import {SettingsPage} from '../pages/SettingsPage';
import {AppSettingsProvider, SettingsStore} from './settingsStore';
import {I18nProvider} from './i18n';
import {DEFAULT_VIDEO_EXTENSIONS} from './videoExtensions';
import type {AppSettings, LibraryRoot, MediaFile, MediaNode, NodeDetail, SearchHit} from '../types/media';
import type {SearchFilters} from '../types/incremental';

const modalMocks = vi.hoisted(() => ({tags: vi.fn(), hidden: vi.fn(), restore: vi.fn()}));
const mocks = vi.hoisted(() => ({search: vi.fn(), extensions: vi.fn()}));
vi.mock('./api', () => ({desktopAvailable: false, api: {listUserTags: modalMocks.tags, listHiddenNodes: modalMocks.hidden, resetNodeType: modalMocks.restore, search: mocks.search, searchExtensions: mocks.extensions}}));
vi.mock('../hooks/useCoverDataUrl', () => ({useCoverDataUrl: () => ({coverUrl: null})}));
vi.mock('../hooks/usePosterViewportLifecycle', () => ({usePosterViewportLifecycle: () => ({coverRequested: false, coverVisible: false})}));
vi.mock('../components/TmdbCredentialSettings', () => ({TmdbCredentialSettings: () => null}));
vi.mock('../components/TmdbDiagnostics', () => ({TmdbDiagnostics: () => null}));
vi.mock('../components/CollectionBackupSettings', () => ({CollectionBackupSettings: () => null}));

const node: MediaNode = {
  id: 10, libraryRootId: 1, parentNodeId: 1, absolutePath: 'X:/Synthetic/Show',
  folderName: 'Show', displayName: 'Show', nodeType: 'WORK', mediaKind: 'ANIMATION',
  manualTypeOverride: false, coverSource: 'PLACEHOLDER', coverCachePath: null,
  createdAt: '2026-11-15', updatedAt: '2026-11-15', lastSeenAt: '2026-11-15', userTags: [],
};
const hit = (title: string): SearchHit => ({kind: 'NODE', node: {...node, displayName: title, folderName: title}});
const file = (id: number, fileName: string): MediaFile => ({
  id, nodeId: node.id, fileName, absolutePath: 'X:/Synthetic/Show/' + fileName,
  extension: '.mkv', fileSize: 100, modifiedAt: '2026-11-15', lastSeenAt: '2026-11-15',
  durationMs: null, width: null, height: null, codec: null,
});
const files = [file(101, 'Show S01E01.mkv'), file(102, 'Show NCOP01.mkv'), file(103, 'Interview.mkv')];
const detail: NodeDetail = {
  node, children: [], mediaFiles: files.slice(0, 2), resourceFiles: [], breadcrumbs: [], binding: null,
  nestedMediaFiles: [{file: files[2], relativeDirectory: 'Extras', sourceNodeId: node.id, sourceName: 'Show'}],
  // MAIN for direct files and OTHER for nested files are deliberately omitted.
  videoSections: [{fileId: 102, section: 'SPS'}],
};
const watchFor = (latest = 102): EpisodeWatchController => ({
  data: {
    nodeId: node.id, workView: false, snapshot: 'synthetic-snapshot',
    rows: files.map(f => ({fileId: f.id, kind: 'FILE', season: null, episodes: [], watched: f.id === latest, partiallyWatched: false})),
    totalEpisodes: 0, watchedEpisodes: 0, totalFiles: 3, watchedFiles: 1, unresolvedRecords: 0,
    latestWatchedFileId: latest, latestWatchedFileName: files.find(f => f.id === latest)?.fileName,
  },
  busy: false, error: false, refresh: vi.fn(), mark: vi.fn(), clear: vi.fn(), playPreferred: vi.fn(), playAndMark: vi.fn(),
});
const mount = (children: ReactNode) => render(<I18nProvider>{children}</I18nProvider>);
const flush = async () => { await act(async () => { await Promise.resolve(); }); };
const advance = async (ms: number) => { await act(async () => { await vi.advanceTimersByTimeAsync(ms); }); };
const originalScrollIntoView = Element.prototype.scrollIntoView;

beforeEach(() => {
  vi.resetAllMocks();
  vi.useFakeTimers();
  modalMocks.tags.mockResolvedValue([]);
  modalMocks.hidden.mockResolvedValue([]);
  mocks.search.mockResolvedValue([]);
  mocks.extensions.mockResolvedValue(['.mkv', ' ', '', '\t', '.cbz']);
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => window.setTimeout(() => callback(performance.now()), 16));
  vi.stubGlobal('cancelAnimationFrame', (id: number) => window.clearTimeout(id));
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(() => {
  cleanup();
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  Element.prototype.scrollIntoView = originalScrollIntoView;
});

describe('Round 3 extension validation', () => {
  async function settingsPage(videoExtensions = ['.mkv']) {
    const settings: AppSettings = {mpvPath: null, playerMode: 'SYSTEM', defaultViewMode: 'GRID', videoExtensions, bangumiSearchEnabled: false, autoCheckUpdates: false, autoScanOnStartup: false, allResourcesFlattened: false, language: 'zh-CN', theme: 'light', coverCacheDirectory: 'X:/Synthetic/Cache'};
    const save = vi.fn(async (next: AppSettings) => next);
    const store = new SettingsStore({get: async () => settings, save});
    await store.load();
    mount(<AppSettingsProvider store={store}><SettingsPage roots={[]} bootstrap={null} onAddRoot={vi.fn()} onHiddenNodes={vi.fn()} onRemoveRoot={vi.fn()} onScanRoot={vi.fn()} updateDownloadStatus={{phase: 'IDLE', downloadedBytes: 0, canInstall: false}} onCheckForUpdate={vi.fn()} onError={vi.fn()} onSuccess={vi.fn()}/></AppSettingsProvider>);
    return {save, store, input: screen.getByRole('textbox', {name: '视频扩展名'}) as HTMLInputElement};
  }

  it('restores all eight defaults immediately after deleting .mkv and preserves other settings', async () => {
    const {input,save,store}=await settingsPage([...DEFAULT_VIDEO_EXTENSIONS]);
    const before=store.getSnapshot().settings;
    const restore=screen.getByRole('button',{name:'恢复默认'}) as HTMLButtonElement;
    expect(restore.disabled).toBe(true);
    fireEvent.click(screen.getByRole('button',{name:'.mkv'}));
    expect(store.getSnapshot().settings?.videoExtensions).not.toContain('.mkv');
    expect(restore.disabled).toBe(false);
    fireEvent.change(input,{target:{value:'bad extension'}});
    fireEvent.blur(input);
    expect(screen.getByRole('alert')).toBeTruthy();
    fireEvent.click(restore);
    expect(store.getSnapshot().settings).toEqual(before);
    expect(input.value).toBe('');
    expect(screen.queryByRole('alert')).toBeNull();
    expect(restore.disabled).toBe(true);
    await flush();
    expect(save).toHaveBeenLastCalledWith(before);
  });

  it('starts neutral, adds normalized valid values through Enter and clears the field', async () => {
    const {input, save, store} = await settingsPage();
    expect(input.getAttribute('aria-invalid')).toBe('false');
    expect(input.getAttribute('aria-describedby')).toBeNull();
    expect(input.getAttribute('autocorrect')).toBe('off');
    expect(input.getAttribute('autocapitalize')).toBe('none');
    expect(input.getAttribute('spellcheck')).toBe('false');
    fireEvent.blur(input);
    expect(screen.queryByRole('alert')).toBeNull();
    fireEvent.change(input, {target: {value: 'MP4'}});
    fireEvent.keyDown(input, {key: 'Enter', isComposing: true});
    expect(save).not.toHaveBeenCalled();
    fireEvent.keyDown(input, {key: 'Enter'});
    await flush();
    expect(store.getSnapshot().settings?.videoExtensions).toEqual(['.mkv', '.mp4']);
    expect(save).toHaveBeenCalledOnce();
    expect(input.value).toBe('');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it.each(['bad ext', ' mp4', '.m*p4', 'abcdefghijk', ''])('rejects %j on submission and clears error immediately when editing', async value => {
    const {input, save} = await settingsPage();
    fireEvent.change(input, {target: {value}});
    expect(screen.queryByRole('alert')).toBeNull();
    fireEvent.click(screen.getByRole('button', {name: '添加'}));
    expect(screen.getByRole('alert').textContent).toContain('1–10');
    expect(input.getAttribute('aria-invalid')).toBe('true');
    expect(input.getAttribute('aria-describedby')).toBe(screen.getByRole('alert').id);
    expect(document.activeElement).toBe(input);
    expect(save).not.toHaveBeenCalled();
    fireEvent.change(input, {target: {value: 'still invalid'}});
    expect(screen.queryByRole('alert')).toBeNull();
    expect(input.getAttribute('aria-invalid')).toBe('false');
    expect(input.getAttribute('aria-describedby')).toBeNull();
  });

  it('validates nonempty blur without saving and reports duplicate normalized extensions', async () => {
    const {input, save} = await settingsPage();
    fireEvent.change(input, {target: {value: '.MKV'}});
    fireEvent.blur(input);
    expect(screen.getByRole('alert').textContent).toBe('此视频扩展名已存在。');
    expect(save).not.toHaveBeenCalled();
    fireEvent.change(input, {target: {value: '.webm'}});
    fireEvent.blur(input);
    expect(screen.queryByRole('alert')).toBeNull();
    expect(save).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', {name: '添加'}));
    await flush();
    expect(save).toHaveBeenCalledOnce();
    expect(input.value).toBe('');
  });
});

describe('Round 3 toast timer ownership', () => {
  const first: ToastMessage = {id: 1, text: 'First notice', kind: 'info'};
  function ToastHarness() {
    const [toasts, setToasts] = useState([first]);
    return <><button onClick={() => setToasts(items => [...items, {id: 2, text: 'Second notice', kind: 'success'}])}>Add notice</button><ToastStack toasts={toasts} onDismiss={id => setToasts(items => items.filter(item => item.id !== id))}/></>;
  }
  it('pauses on pointer hover, then consumes only the remaining lifetime', async () => {
    mount(<ToastHarness/>);
    const region = screen.getByRole('region', {name: '通知'});
    await advance(2000);
    fireEvent.pointerEnter(region);
    await advance(10000);
    expect(screen.getByText('First notice')).toBeTruthy();
    fireEvent.pointerLeave(region);
    await advance(1499);
    expect(screen.getByText('First notice')).toBeTruthy();
    await advance(1);
    expect(screen.queryByText('First notice')).toBeNull();
  });
  it('requires both focus and pointer to leave, including toasts added while paused', async () => {
    mount(<ToastHarness/>);
    const region = screen.getByRole('region');
    await advance(1000);
    fireEvent.mouseEnter(region);
    act(() => screen.getByRole('button', {name: '关闭提示'}).focus());
    fireEvent.click(screen.getByRole('button', {name: 'Add notice'}));
    fireEvent.mouseLeave(region);
    await advance(10000);
    expect(screen.getByText('First notice')).toBeTruthy();
    expect(screen.getByText('Second notice')).toBeTruthy();
    act(() => screen.getByRole('button', {name: 'Add notice'}).focus());
    await advance(2500);
    expect(screen.queryByText('First notice')).toBeNull();
    expect(screen.getByText('Second notice')).toBeTruthy();
    await advance(1000);
    expect(screen.queryByText('Second notice')).toBeNull();
  });
  it('retains lifetime across rerenders and discards removed IDs and unmounted timers', async () => {
    const onDismiss = vi.fn();
    const view = mount(<StrictMode><ToastStack toasts={[first]} onDismiss={onDismiss}/></StrictMode>);
    await advance(3000);
    view.rerender(<I18nProvider><StrictMode><ToastStack toasts={[{...first}]} onDismiss={onDismiss}/></StrictMode></I18nProvider>);
    await advance(499);
    expect(onDismiss).not.toHaveBeenCalled();
    await advance(1);
    expect(onDismiss).toHaveBeenCalledExactlyOnceWith(1);
    view.rerender(<I18nProvider><StrictMode><ToastStack toasts={[{...first, id: 2}]} onDismiss={onDismiss}/></StrictMode></I18nProvider>);
    view.unmount();
    await advance(6000);
    expect(onDismiss).toHaveBeenCalledOnce();
  });
  it('manually dismisses the focused notice and returns focus to the prior control', async () => {
    mount(<ToastHarness/>);
    const origin = screen.getByRole('button', {name: 'Add notice'});
    act(() => origin.focus());
    const close = screen.getByRole('button', {name: '关闭提示'});
    act(() => close.focus());
    fireEvent.click(close);
    expect(screen.queryByText('First notice')).toBeNull();
    expect(document.activeElement).toBe(origin);
    await advance(6000);
    expect(screen.queryByText('First notice')).toBeNull();
  });
  it('resumes remaining notices after a focused warning is withdrawn externally', async () => {
    const onDismiss = vi.fn();
    const second: ToastMessage = {...first, id: 2, text: 'Retained notice'};
    const renderStack = (toasts: ToastMessage[]) => <I18nProvider><button>Outside</button><ToastStack toasts={toasts} onDismiss={onDismiss}/></I18nProvider>;
    const view = render(renderStack([first, second]));
    const outside = screen.getByRole('button', {name: 'Outside'});
    act(() => outside.focus());
    await advance(1000);
    act(() => screen.getAllByRole('button', {name: '关闭提示'})[0].focus());
    await advance(6000);
    expect(onDismiss).not.toHaveBeenCalled();
    view.rerender(renderStack([second]));
    expect(document.activeElement).toBe(outside);
    await advance(2500);
    expect(onDismiss).toHaveBeenCalledExactlyOnceWith(2);
  });
});

function ControlledSearch({filters = {}, onQuery = vi.fn()}: {filters?: SearchFilters; onQuery?: (value: string) => void}) {
  const [query, setQuery] = useState('');
  return <SearchPage initialQuery={query} filters={filters} rootId={7} onQueryChange={value => {onQuery(value); setQuery(value);}} onOpen={vi.fn()} onError={vi.fn()} coverRevision={0}/>;
}

describe('SearchPage query lifecycle', () => {
  it('debounces consecutive controlled input for 280 ms and sends only the final trimmed query', async () => {
    const onQuery = vi.fn();
    mount(<ControlledSearch onQuery={onQuery}/>);
    await flush();
    const input = screen.getByRole('textbox', {name: '搜索'});
    fireEvent.change(input, {target: {value: 'S'}});
    await advance(150);
    fireEvent.change(input, {target: {value: 'Show'}});
    await advance(150);
    fireEvent.change(input, {target: {value: '  Show final  '}});
    await advance(279);
    expect(mocks.search).not.toHaveBeenCalled();
    await advance(1);
    expect(mocks.search).toHaveBeenCalledExactlyOnceWith('Show final', 7);
    expect(onQuery.mock.calls.map(args => args[0])).toEqual(['S', 'Show', '  Show final  ']);
    await advance(600);
    expect(mocks.search).toHaveBeenCalledOnce();
  });

  it('clears the pending debounce immediately, resets query and restores input focus', async () => {
    const onQuery = vi.fn();
    mount(<ControlledSearch onQuery={onQuery}/>);
    await flush();
    const input = screen.getByRole('textbox', {name: '搜索'});
    fireEvent.change(input, {target: {value: 'discard'}});
    await advance(279);
    fireEvent.click(screen.getByRole('button', {name: '清空'}));
    expect((input as HTMLInputElement).value).toBe('');
    expect(document.activeElement).toBe(input);
    expect(onQuery).toHaveBeenLastCalledWith('');
    expect((screen.getByRole('button', {name: '搜索'}) as HTMLButtonElement).disabled).toBe(true);
    await advance(1000);
    expect(mocks.search).not.toHaveBeenCalled();
  });

  it('does not restore stale results after clearing an in-flight request', async () => {
    let resolve!: (rows: SearchHit[]) => void;
    mocks.search.mockImplementationOnce(() => new Promise<SearchHit[]>(done => {resolve = done;}));
    mount(<ControlledSearch/>);
    await flush();
    fireEvent.change(screen.getByRole('textbox', {name: '搜索'}), {target: {value: 'old'}});
    await advance(280);
    expect(mocks.search).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole('button', {name: '清空'}));
    await act(async () => {resolve([hit('Late old result')]);});
    expect(screen.queryByText('Late old result')).toBeNull();
    expect(document.querySelector('.search-results')).toBeNull();
  });

  it('searches empty scan queries with the complete scope and re-queries when the scope changes', async () => {
    const filters: SearchFilters = {scanRootId: 7, mediaKind: 'ANIMATION', binding: 'unbound', extension: '.mkv'};
    const props = {initialQuery: '', filters, rootId: 7, coverRevision: 0, onOpen: vi.fn(), onError: vi.fn()};
    const view = mount(<SearchPage {...props}/>);
    await flush();
    expect(mocks.search).toHaveBeenCalledExactlyOnceWith('', 7, filters);
    const next = {...filters, scanRootId: 8};
    view.rerender(<I18nProvider><SearchPage {...props} rootId={8} filters={next}/></I18nProvider>);
    await flush();
    expect(mocks.search).toHaveBeenLastCalledWith('', 8, next);
    expect(mocks.search).toHaveBeenCalledTimes(2);
  });

  it('submits the current query immediately and cancels the later debounce duplicate', async () => {
    mount(<ControlledSearch/>);
    await flush();
    fireEvent.change(screen.getByRole('textbox', {name: '搜索'}), {target: {value: 'submit now'}});
    fireEvent.submit(document.querySelector('form.search-controls')!);
    await flush();
    expect(mocks.search).toHaveBeenCalledExactlyOnceWith('submit now', 7);
    await advance(600);
    expect(mocks.search).toHaveBeenCalledOnce();
  });

  it('filters blank extension options and preserves public filter callbacks', async () => {
    const onFiltersChange = vi.fn(), onRootChange = vi.fn();
    mount(<SearchPage initialQuery="" rootId={7} filters={{binding: 'bound'}} onFiltersChange={onFiltersChange} onRootChange={onRootChange} onOpen={vi.fn()} onError={vi.fn()} coverRevision={0}/>);
    await flush();
    const extension = [...document.querySelectorAll('select')].at(-1)!;
    expect([...extension.options].map(option => option.value)).toEqual(['', '.mkv', '.cbz']);
    fireEvent.change(extension, {target: {value: '.cbz'}});
    expect(onFiltersChange).toHaveBeenCalledWith({binding: 'bound', extension: '.cbz'});
    fireEvent.click(document.querySelector('.search-clear-filters')!);
    expect(onFiltersChange).toHaveBeenLastCalledWith({});
    expect(onRootChange).toHaveBeenCalledExactlyOnceWith(null);
    expect(mocks.search).not.toHaveBeenCalled();
  });

  it('cancels pending work on unmount and ignores earlier scope responses', async () => {
    const resolves: Array<(rows: SearchHit[]) => void> = [];
    mocks.search.mockImplementation(() => new Promise<SearchHit[]>(done => {resolves.push(done);}));
    const props = {initialQuery: 'Show', rootId: 7, coverRevision: 0, onOpen: vi.fn(), onError: vi.fn()};
    const view = mount(<SearchPage {...props}/>);
    await flush();
    view.rerender(<I18nProvider><SearchPage {...props} rootId={8}/></I18nProvider>);
    await flush();
    await act(async () => {resolves[1]([hit('Current scope result')]);});
    await act(async () => {resolves[0]([hit('Stale scope result')]);});
    expect(screen.getByText('Current scope result')).toBeTruthy();
    expect(screen.queryByText('Stale scope result')).toBeNull();
    fireEvent.change(screen.getByRole('textbox', {name: '搜索'}), {target: {value: 'unmounted'}});
    view.unmount();
    await advance(600);
    expect(mocks.search).toHaveBeenCalledTimes(2);
  });
});

describe('watch summary and video tabs', () => {
  it.each([102, 103])('locates file %s across tabs, emits its real ID and clears highlight after 1600 ms', async latest => {
    const watch = watchFor(latest), locate = vi.fn();
    window.addEventListener('m2shelf-locate-video-file', locate);
    try {
      mount(<><EpisodeWatchSummary watch={watch}/><DetailVideoTabs cdsFiles={[]} cdsFolders={[]} onOpenResource={vi.fn()} onRevealResource={vi.fn()} onOpenChild={vi.fn()} onMenu={vi.fn()} detail={detail} watch={watch} onPlay={vi.fn()} onReveal={vi.fn()}/></>);
      expect(document.querySelector('.media-file-row')?.getAttribute('data-file-id')).toBe('101');
      fireEvent.click(document.querySelector('.episode-watch-position.is-clickable')!);
      expect(locate).toHaveBeenCalledOnce();
      expect((locate.mock.calls[0][0] as CustomEvent).detail).toEqual({fileId: latest});
      await advance(16);
      const row = document.querySelector('.media-file-row[data-file-id="' + latest + '"]')!;
      expect(row.classList.contains('is-located')).toBe(true);
      expect(document.querySelector('.media-file-row[data-file-id="101"]')).toBeNull();
      expect(Element.prototype.scrollIntoView).toHaveBeenCalledWith({behavior: 'smooth', block: 'center'});
      await advance(1599);
      expect(row.classList.contains('is-located')).toBe(true);
      await advance(1);
      expect(row.classList.contains('is-located')).toBe(false);
      expect(watch.mark).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener('m2shelf-locate-video-file', locate);
    }
  });

  it('keeps tab keyboard navigation inside its tablist and restores MAIN for a new detail', () => {
    const watch = watchFor(), onPlay = vi.fn(), onReveal = vi.fn(), globalKey = vi.fn();
    const view = mount(<DetailVideoTabs cdsFiles={[]} cdsFolders={[]} onOpenResource={vi.fn()} onRevealResource={vi.fn()} onOpenChild={vi.fn()} onMenu={vi.fn()} detail={detail} watch={watch} onPlay={onPlay} onReveal={onReveal}/>);
    window.addEventListener('keydown', globalKey);
    try {
      const tabs = screen.getAllByRole('tab');
      fireEvent.keyDown(tabs[0], {key: 'End'});
      expect(tabs[2].getAttribute('aria-selected')).toBe('true');
      expect(document.activeElement).toBe(tabs[2]);
      fireEvent.keyDown(tabs[2], {key: 'ArrowRight'});
      expect(tabs[0].getAttribute('aria-selected')).toBe('true');
      fireEvent.keyDown(tabs[0], {key: 'ArrowLeft'});
      expect(tabs[2].getAttribute('aria-selected')).toBe('true');
      expect(globalKey).not.toHaveBeenCalled();
      view.rerender(<I18nProvider><DetailVideoTabs cdsFiles={[]} cdsFolders={[]} onOpenResource={vi.fn()} onRevealResource={vi.fn()} onOpenChild={vi.fn()} onMenu={vi.fn()} detail={{...detail, node: {...node, id: 11}}} watch={watch} onPlay={onPlay} onReveal={onReveal}/></I18nProvider>);
      expect(screen.getAllByRole('tab')[0].getAttribute('aria-selected')).toBe('true');
      expect(document.querySelector('.media-file-row')?.getAttribute('data-file-id')).toBe('101');
      expect(onPlay).not.toHaveBeenCalled();
      expect(onReveal).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener('keydown', globalKey);
    }
  });

  it('uses real file IDs for row actions without bubbling a checkbox into playback', () => {
    const watch = watchFor(), onPlay = vi.fn(), onReveal = vi.fn();
    mount(<DetailVideoTabs cdsFiles={[]} cdsFolders={[]} onOpenResource={vi.fn()} onRevealResource={vi.fn()} onOpenChild={vi.fn()} onMenu={vi.fn()} detail={detail} watch={watch} onPlay={onPlay} onReveal={onReveal}/>);
    const row = document.querySelector('.media-file-row')!;
    const checkbox = within(row as HTMLElement).getByRole('checkbox');
    fireEvent.click(checkbox);
    fireEvent.doubleClick(checkbox);
    expect(watch.mark).toHaveBeenCalledExactlyOnceWith(101, true);
    expect(watch.playAndMark).not.toHaveBeenCalled();
    fireEvent.doubleClick(row);
    expect(watch.playAndMark).toHaveBeenCalledWith(101, expect.any(Function));
    fireEvent.click(within(row as HTMLElement).getByRole('button', {name: '点击播放 Show S01E01.mkv'}));
    expect(onPlay).toHaveBeenCalledExactlyOnceWith(files[0]);
    fireEvent.click(within(row as HTMLElement).getByRole('button', {name: '在资源管理器中显示 Show S01E01.mkv'}));
    expect(onReveal).toHaveBeenCalledExactlyOnceWith(files[0]);
  });

  it('exposes summary retry and clear callbacks but disables mutation while busy', () => {
    const watch = {...watchFor(), error: true};
    const view = mount(<EpisodeWatchSummary watch={watch}/>);
    fireEvent.click(screen.getByRole('button', {name: '重试'}));
    expect(watch.refresh).toHaveBeenCalledOnce();
    fireEvent.click(document.querySelector('.episode-watch-clear')!);
    expect(watch.clear).toHaveBeenCalledOnce();
    view.rerender(<I18nProvider><EpisodeWatchSummary watch={{...watch, busy: true}}/></I18nProvider>);
    fireEvent.click(document.querySelector('.episode-watch-clear')!);
    fireEvent.click(screen.getByRole('button', {name: '重试'}));
    expect(watch.clear).toHaveBeenCalledOnce();
    expect(watch.refresh).toHaveBeenCalledOnce();
  });
});

describe('public command shortcut boundary', () => {
  it.each(['input', 'textarea', 'editable', 'modal', 'ime', 'prevented', 'alt', 'meta', 'disabled'])('does not steal Ctrl+K in %s context', context => {
    mount(<CommandPalette enabled={context !== 'disabled'} actions={[]} onHit={vi.fn()}/>);
    const target = document.createElement(context === 'textarea' ? 'textarea' : context === 'input' ? 'input' : 'div');
    if (context === 'editable') target.setAttribute('contenteditable', 'true');
    if (context === 'modal') target.setAttribute('aria-modal', 'true');
    document.body.append(target);
    try {
      const event = new KeyboardEvent('keydown', {key: 'k', ctrlKey: true, altKey: context === 'alt', metaKey: context === 'meta', isComposing: context === 'ime', bubbles: true, cancelable: true});
      if (context === 'prevented') event.preventDefault();
      fireEvent(target, event);
      expect(screen.queryByRole('dialog')).toBeNull();
      expect(mocks.search).not.toHaveBeenCalled();
    } finally {target.remove();}
  });

  it('opens only when allowed, dispatches the existing action and closes on Escape', async () => {
    const run = vi.fn();
    mount(<><button type="button">Origin</button><CommandPalette enabled actions={[{id: 'all', label: '全部资源', run}]} onHit={vi.fn()}/></>);
    const origin = screen.getByRole('button', {name: 'Origin'});
    origin.focus();
    fireEvent.keyDown(origin, {key: 'k', ctrlKey: true});
    await advance(16);
    expect(screen.getByRole('dialog')).toBeTruthy();
    fireEvent.keyDown(screen.getByRole('combobox'), {key: 'Enter'});
    expect(run).toHaveBeenCalledOnce();
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(origin);
    fireEvent.keyDown(origin, {key: 'k', ctrlKey: true});
    await advance(16);
    fireEvent.keyDown(screen.getByRole('combobox'), {key: 'Escape'});
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(run).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(origin);
  });
});

describe('modal and context panel focus lifecycle', () => {
  function ModalHarness() {
    const [open, setOpen] = useState(false);
    return <><button type="button" onClick={() => setOpen(true)}>Open modal</button>{open && <FeatureModal title="Synthetic modal" onClose={() => setOpen(false)}><input autoFocus aria-label="First field"/><button type="button">Last action</button></FeatureModal>}</>;
  }

  it('captures the invoker before a descendant autofocus and restores on close', () => {
    mount(<ModalHarness/>);
    const origin = screen.getByRole('button', {name: 'Open modal'});
    origin.focus();
    fireEvent.click(origin);
    expect(document.activeElement).toBe(screen.getByRole('textbox', {name: 'First field'}));
    const last = screen.getByRole('button', {name: 'Last action'});
    last.focus();
    fireEvent.keyDown(last, {key: 'Tab'});
    expect(document.activeElement).toBe(screen.getByRole('button', {name: '关闭'}));
    fireEvent.keyDown(document.activeElement!, {key: 'Tab', shiftKey: true});
    expect(document.activeElement).toBe(last);
    fireEvent.keyDown(last, {key: 'Escape', isComposing: true});
    expect(screen.getByRole('dialog')).toBeTruthy();
    fireEvent.keyDown(last, {key: 'Escape'});
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(origin);
  });

  function ConfirmHarness({busy = false}: {busy?: boolean}) {
    const [open, setOpen] = useState(false);
    return <><button type="button" onClick={() => setOpen(true)}>Delete collection</button><ConfirmDialog open={open} title="Confirm deletion" description="Synthetic collection only" destructive busy={busy} onConfirm={vi.fn()} onClose={() => setOpen(false)}/></>;
  }

  it('focuses Cancel for destructive confirmation and ignores composing Escape', () => {
    mount(<ConfirmHarness/>);
    const origin = screen.getByRole('button', {name: 'Delete collection'});
    origin.focus();
    fireEvent.click(origin);
    const cancel = screen.getByRole('button', {name: '取消'});
    expect(document.activeElement).toBe(cancel);
    fireEvent.keyDown(cancel, {key: 'Escape', isComposing: true});
    expect(screen.getByRole('dialog')).toBeTruthy();
    fireEvent.keyDown(cancel, {key: 'Escape'});
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(origin);
    fireEvent.click(origin);
    fireEvent.click(screen.getByRole('button', {name: '取消'}));
    expect(document.activeElement).toBe(origin);
  });

  it('does not dismiss a busy confirmation through Escape, close, or backdrop', () => {
    mount(<ConfirmHarness busy/>);
    fireEvent.click(screen.getByRole('button', {name: 'Delete collection'}));
    fireEvent.keyDown(screen.getByRole('dialog'), {key: 'Escape'});
    fireEvent.click(screen.getByRole('button', {name: '关闭'}));
    fireEvent.mouseDown(document.querySelector('.modal-backdrop')!);
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole('dialog'));
  });

  it('closes only the top nested modal and restores each invoker without inert leakage', async () => {
    function NestedHarness() {
      const [outer, setOuter] = useState(false), [inner, setInner] = useState(false);
      return <><button onClick={() => setOuter(true)}>Open outer</button>{outer && <FeatureModal title="Outer" onClose={() => setOuter(false)}><button onClick={() => setInner(true)}>Open inner</button><ConfirmDialog open={inner} title="Inner" description="Nested confirmation" destructive onConfirm={vi.fn()} onClose={() => setInner(false)}/></FeatureModal>}</>;
    }
    const view = mount(<NestedHarness/>);
    const origin = screen.getByRole('button', {name: 'Open outer'});
    origin.focus(); fireEvent.click(origin);
    const innerOrigin = screen.getByRole('button', {name: 'Open inner'});
    innerOrigin.focus(); fireEvent.click(innerOrigin);
    await flush();
    const inner = screen.getByRole('dialog', {name: 'Inner'});
    expect(inner.closest('[inert]')).toBeNull();
    expect(screen.queryByRole('dialog', {name: 'Outer'})).toBeNull();
    fireEvent.keyDown(document.activeElement!, {key: 'Escape'});
    expect(screen.queryByRole('dialog', {name: 'Inner'})).toBeNull();
    expect(screen.getByRole('dialog', {name: 'Outer'})).toBeTruthy();
    expect(document.activeElement).toBe(innerOrigin);
    fireEvent.keyDown(innerOrigin, {key: 'Escape'});
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(origin);
    expect(view.container.hasAttribute('inert')).toBe(false);
  });

  it('clamps context actions to the viewport, supports arrows, and restores invoker', () => {
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function(this: HTMLElement) {
      return {x: 0, y: 0, top: 0, left: 0, width: this.classList.contains('context-menu') ? 260 : 0, height: this.classList.contains('context-menu') ? 500 : 0, right: 260, bottom: 500, toJSON: () => ({})};
    });
    function ContextHarness() {
      const [open, setOpen] = useState(false);
      return <><button type="button" onClick={() => setOpen(true)}>Actions</button>{open && <ContextMenu node={node} x={window.innerWidth - 1} y={window.innerHeight - 1} onAction={vi.fn()} onClose={() => setOpen(false)}/>}</>;
    }
    mount(<ContextHarness/>);
    const origin = screen.getByRole('button', {name: 'Actions'});
    origin.focus(); fireEvent.click(origin);
    const group = screen.getByRole('group', {name: 'Show'});
    expect(group.style.left).toBe(`${window.innerWidth - 268}px`);
    expect(group.style.top).toBe(`${window.innerHeight - 508}px`);
    expect(screen.queryByRole('menu')).toBeNull();
    const items = within(group).getAllByRole('button');
    expect(document.activeElement).toBe(items[0]);
    fireEvent.keyDown(items[0], {key: 'ArrowUp'});
    expect(document.activeElement).toBe(items.at(-1));
    fireEvent.keyDown(document.activeElement!, {key: 'Home'});
    expect(document.activeElement).toBe(items[0]);
    fireEvent.keyDown(items[0], {key: 'ArrowDown'});
    expect(document.activeElement).toBe(items[1]);
    fireEvent.keyDown(items[1], {key: 'Escape', isComposing: true});
    expect(screen.getByRole('group')).toBeTruthy();
    fireEvent.keyDown(items[1], {key: 'Escape'});
    expect(screen.queryByRole('group')).toBeNull();
    expect(document.activeElement).toBe(origin);
  });

  it('keeps source actions a plain group, closes when Tab reaches outside, and does not steal focus', () => {
    function SourceHarness() {
      const [open, setOpen] = useState(true);
      return <><button type="button">Outside</button>{open && <SourceContextMenu title="Book source" x={0} y={0} onOpen={vi.fn()} onClose={() => setOpen(false)}/>}</>;
    }
    mount(<SourceHarness/>);
    expect(screen.queryByRole('menu')).toBeNull();
    expect(screen.getByRole('group', {name: 'Book source'})).toBeTruthy();
    const outside = screen.getByRole('button', {name: 'Outside'});
    act(() => outside.focus());
    expect(screen.queryByRole('group')).toBeNull();
    expect(document.activeElement).toBe(outside);
  });
});


const modalNode = node;
const modalRoot = {id: 1, displayName: "Synthetic library", path: "X:/Synthetic"} as LibraryRoot;
const flushModal = flush;

function ModalLauncher({kind}: {kind: 'rename' | 'tags' | 'hidden'}) {
  const [open, setOpen] = useState(false);
  const close = () => setOpen(false);
  return <><main data-testid="modal-background"><button onClick={() => setOpen(true)}>First trigger</button><button onClick={() => setOpen(true)}>Second trigger</button></main>
    {kind === 'rename' && <RenameDialog node={open ? modalNode : null} busy={false} onClose={close} onSave={vi.fn()}/>}
    {kind === 'tags' && <TagManagerDialog node={open ? modalNode : null} onClose={close} onChanged={vi.fn()}/>}
    {kind === 'hidden' && open && <HiddenNodesDialog roots={[]} refreshKey={null} onClose={close} onRestored={vi.fn()}/>}
  </>;
}

describe('Round 3 modal focus integration', () => {
  it.each(['rename', 'tags', 'hidden'] as const)('%s isolates the background, traps focus and restores each actual opener', async kind => {
    render(<StrictMode><I18nProvider><ModalLauncher kind={kind}/></I18nProvider></StrictMode>);
    const background = screen.getByTestId('modal-background');
    for (const name of ['First trigger', 'Second trigger']) {
      const trigger = screen.getByRole('button', {name});
      trigger.focus();
      fireEvent.click(trigger);
      await flushModal();
      const dialog = screen.getByRole('dialog');
      expect(background.hasAttribute('inert')).toBe(true);
      expect(background.getAttribute('aria-hidden')).toBe('true');
      expect(document.activeElement).toBe(within(dialog).getByRole('textbox'));
      const controls = [...dialog.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled)')];
      controls.at(-1)!.focus();
      fireEvent.keyDown(window, {key: 'Tab'});
      expect(document.activeElement).toBe(controls[0]);
      fireEvent.keyDown(window, {key: 'Tab', shiftKey: true});
      expect(document.activeElement).toBe(controls.at(-1));
      fireEvent.keyDown(window, {key: 'Escape', isComposing: true});
      expect(screen.getByRole('dialog')).toBe(dialog);
      fireEvent.keyDown(window, {key: 'Escape'});
      expect(screen.queryByRole('dialog')).toBeNull();
      expect(background.hasAttribute('inert')).toBe(false);
      expect(background.hasAttribute('aria-hidden')).toBe(false);
      expect(document.activeElement).toBe(trigger);
    }
  });

  it.each(['node', 'root'] as const)('%s rename retains initial full selection and refuses dismissal or submission while busy', async kind => {
    const close = vi.fn(), save = vi.fn();
    const component = (busy: boolean) => kind === 'node'
      ? <RenameDialog node={modalNode} busy={busy} onClose={close} onSave={save}/>
      : <LibraryRootRenameDialog root={modalRoot} busy={busy} onClose={close} onSave={save}/>;
    const view = render(<I18nProvider>{component(false)}</I18nProvider>);
    await act(async () => {await vi.advanceTimersByTimeAsync(16);});
    const input = screen.getByRole('textbox') as HTMLInputElement;
    expect(document.activeElement).toBe(input);
    expect(input.selectionStart).toBe(0);
    expect(input.selectionEnd).toBe(input.value.length);
    expect(input.getAttribute('autocorrect')).toBe('off');
    expect(input.getAttribute('autocapitalize')).toBe('none');
    expect(input.getAttribute('spellcheck')).toBe('false');
    view.rerender(<I18nProvider>{component(true)}</I18nProvider>);
    fireEvent.keyDown(window, {key: 'Escape'});
    fireEvent.mouseDown(document.querySelector('.modal-backdrop')!);
    fireEvent.submit(screen.getByRole('dialog'));
    expect(close).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();
    expect(input.disabled).toBe(true);
  });

  it('does not steal focus back after the tag list finishes loading', async () => {
    let resolve!: (rows: []) => void;
    modalMocks.tags.mockReturnValue(new Promise<[]>(done => {resolve = done;}));
    render(<I18nProvider><TagManagerDialog node={modalNode} onClose={vi.fn()} onChanged={vi.fn()}/></I18nProvider>);
    const close = screen.getByRole('button', {name: '关闭'});
    close.focus();
    await act(async () => {resolve([]);});
    expect(document.activeElement).toBe(close);
  });

  it('keeps hidden-content restoration busy until the parent refresh settles', async () => {
    modalMocks.hidden.mockResolvedValue([modalNode]);
    modalMocks.restore.mockResolvedValue(undefined);
    let finish!: () => void;
    const restored = vi.fn(() => new Promise<void>(resolve => {finish = resolve;}));
    const close = vi.fn();
    render(<I18nProvider><HiddenNodesDialog roots={[]} refreshKey={null} onClose={close} onRestored={restored}/></I18nProvider>);
    await flushModal();
    fireEvent.click(document.querySelector('.hidden-node-row button')!);
    await flushModal();
    fireEvent.keyDown(window, {key: 'Escape'});
    fireEvent.pointerDown(document.querySelector('.modal-backdrop')!);
    expect(close).not.toHaveBeenCalled();
    await act(async () => {finish();});
    fireEvent.keyDown(window, {key: 'Escape'});
    expect(close).toHaveBeenCalledOnce();
  });

  it('releases a suspended recognition dialog and restores the trigger after reactivation', () => {
    const close = vi.fn();
    const props = {path: 'X:/Synthetic', busy: false, onChoose: vi.fn(), onClose: close};
    const trigger = document.createElement('button');
    document.body.append(trigger);
    try {
      trigger.focus();
      const view = render(<I18nProvider><LibraryRecognitionModeDialog {...props}/></I18nProvider>);
      expect(screen.getByRole('dialog').contains(document.activeElement)).toBe(true);
      view.rerender(<I18nProvider><LibraryRecognitionModeDialog {...props} suspended/></I18nProvider>);
      expect(screen.queryByRole('dialog')).toBeNull();
      expect(document.activeElement).toBe(trigger);
      view.rerender(<I18nProvider><LibraryRecognitionModeDialog {...props}/></I18nProvider>);
      expect(screen.getByRole('dialog').contains(document.activeElement)).toBe(true);
      view.unmount();
      expect(document.activeElement).toBe(trigger);
    } finally {trigger.remove();}
  });
});

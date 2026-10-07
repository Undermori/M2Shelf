// @vitest-environment jsdom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {cleanup, fireEvent, render, screen, within} from '@testing-library/react';
import {ComicDetailPage} from './ComicDetailPage';
import type {WorkDetailPageProps} from './WorkDetailPage';
import type {MediaNode, NodeDetail} from '../types/media';
import type {ComicBook} from '../types/comic';
import {I18nProvider} from '../lib/i18n';

const mocks = vi.hoisted(() => ({detail: vi.fn(), reveal: vi.fn()}));
vi.mock('../lib/api', () => ({api: {comicDetail: mocks.detail, revealComicBook: mocks.reveal}}));
vi.mock('../hooks/useCoverDataUrl', () => ({useCoverDataUrl: () => ({coverUrl: null, coverLoading: false, coverFailed: false, coverCacheKey: ''})}));
const node = (id: number, name: string, count: number): MediaNode => ({id, folderName: name, displayName: name, absolutePath: `X:/Fixture/${name}`, libraryRootId: 1, parentNodeId: 100, mediaKind: 'COMIC', nodeType: 'CONTAINER', manualTypeOverride: false, totalComicBookCount: count, directVideoCount: 0, childMediaBranchCount: 0, totalVideoCount: 0, coverSource: 'PLACEHOLDER', coverCachePath: null, createdAt: '2026-01-01', updatedAt: '2026-01-01', lastSeenAt: '2026-01-01', userTags: []});
const book = (id: number): ComicBook => ({id, nodeId: id + 100, revision: 'fixture', sourceKind: 'IMAGE_FOLDER', displayName: `Volume ${id}`, sourcePath: `X:/Fixture/Original/Volume ${id}`, pageCount: 200, modifiedAt: '2026-01-01', indexError: null, progress: null});
const books = Array.from({length: 14}, (_, i) => book(i + 1));
const continuation = node(2, 'Original re', 194);
const detail: NodeDetail = {node: node(1, 'Original', 208), comicBooks: books, children: [continuation], resourceFiles: [], mediaFiles: [], binding: null, breadcrumbs: []};
const props = (snapshot = detail): WorkDetailPageProps => ({detail: snapshot, loading: false, rootLabel: 'Comics', coverRevision: 0, onRoot: vi.fn(), onBack: vi.fn(), onBreadcrumb: vi.fn(), onBangumi: vi.fn(), onOpenBangumi: vi.fn(), onRetryCover: vi.fn(), onRetryCoverNode: vi.fn(), onClearBangumi: vi.fn(), onReveal: vi.fn(), onMenu: vi.fn(), onReadComic: vi.fn(), onPlay: vi.fn(), onRevealMedia: vi.fn(), onOpenChild: vi.fn(), onBangumiNode: vi.fn(), onOpenResource: vi.fn(), onRevealResource: vi.fn()});
beforeEach(() => {vi.resetAllMocks(); mocks.reveal.mockResolvedValue(undefined);});
afterEach(cleanup);

describe('complete indexed comic details', () => {
  it('uses the same 14 owned books for both counts and keeps the 194-item continuation navigable', () => {
    const actions = props();
    render(<I18nProvider><ComicDetailPage {...actions}/></I18nProvider>);
    expect(document.querySelector('.detail-stats')?.textContent).toBe('14 项可阅读内容');
    expect(document.querySelector('.comic-reading-section .section-heading')?.textContent).toContain('14 项');
    expect(document.querySelectorAll('.comic-book-row')).toHaveLength(14);
    expect(screen.queryByText('208 册')).toBeNull();
    expect(document.querySelector('.comic-resource-section')).toBeNull();
    const child = screen.getByText('Original re').closest('button')!;
    expect(within(child).getByText(/194 项可阅读内容/)).toBeTruthy();
    expect(screen.queryByText(/附属资源目录/)).toBeNull();
    fireEvent.click(child); expect(actions.onOpenChild).toHaveBeenCalledWith(continuation);
    fireEvent.doubleClick(screen.getByText('Volume 1').closest('.comic-book-row')!);
    expect(actions.onReadComic).toHaveBeenCalledWith(books[0]);
    expect(mocks.detail).not.toHaveBeenCalled();
  });

  it('shows remaining archive folders without duplicating expanded PDF directories', async () => {
    const attachment = node(3, 'ing_jpg', 0);
    const snapshot = {...detail, children: [attachment], comicBooks: books.map(b => ({...b, sourceKind: 'ZIP_ARCHIVE' as const, documentFormat: 'PDF' as const, sourcePath: `X:/Fixture/ing_pdf/Parts/${b.id}.pdf`}))};
    const actions = props(snapshot);
    render(<I18nProvider><ComicDetailPage {...actions}/></I18nProvider>);
    expect(document.querySelectorAll('.comic-resource-section .resource-row')).toHaveLength(1);
    fireEvent.click(screen.getByText('ing_jpg').closest('button')!);
    expect(actions.onOpenChild).toHaveBeenCalledWith(attachment);
    expect(screen.queryByText('ing_pdf')).toBeNull();
    expect(screen.queryByText('视频已在上方展示')).toBeNull();
    fireEvent.click(screen.getAllByRole('button', {name: '在资源管理器中显示'})[0]);
    await vi.waitFor(() => expect(mocks.reveal).toHaveBeenCalledWith(1));
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('renders a newer complete snapshot immediately without an old asynchronous book response', () => {
    const view = render(<I18nProvider><ComicDetailPage {...props()}/></I18nProvider>);
    view.rerender(<I18nProvider><ComicDetailPage {...props({...detail, node: continuation, children: [], comicBooks: [book(90)]})}/></I18nProvider>);
    expect(document.querySelector('.detail-stats')?.textContent).toBe('1 项可阅读内容');
    expect(screen.queryByText('Volume 1')).toBeNull();
    expect(screen.getByText('Volume 90')).toBeTruthy();
    expect(mocks.detail).not.toHaveBeenCalled();
  });
});

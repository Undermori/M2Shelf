// @vitest-environment jsdom
import {act, cleanup, render, renderHook, waitFor} from '@testing-library/react';
import {afterEach, expect, it, vi} from 'vitest';
import type {MediaNode} from '../types/media';
import {useCoverDataUrl} from './useCoverDataUrl';

const {getCoverDataUrl} = vi.hoisted(() => ({getCoverDataUrl: vi.fn()}));
vi.mock('../lib/api', () => ({desktopAvailable: true, api: {getCoverDataUrl}}));
afterEach(() => {cleanup(); getCoverDataUrl.mockReset();});

const bookNode = (id: number): MediaNode => ({
  id, libraryRootId: 1, parentNodeId: 1, mediaKind: 'DOUJIN', nodeType: 'WORK',
  displayName: 'Image collection', folderName: 'Image collection', absolutePath: 'X:/Fixtures/Images',
  manualTypeOverride: false, createdAt: '2026-01-01', updatedAt: '2026-01-01',
  lastSeenAt: '2026-01-01', latestFileModifiedAt: '2026-01-01', totalComicBookCount: 1,
  totalVideoCount: 0, userTags: [], binding: null, coverSource: 'PLACEHOLDER', coverCachePath: null,
});

it('uses a normal placeholder when no local image cover is available', async () => {
  getCoverDataUrl.mockResolvedValue(null);
  const {result} = renderHook(() => useCoverDataUrl(bookNode(9840), 0));
  await waitFor(() => expect(result.current.coverLoading).toBe(false));
  expect(result.current.coverUrl).toBeNull();
  expect(result.current.coverFailed).toBe(false);
});

it('refreshes an image cover after rescan and discards the previous late completion', async () => {
  let finishOld!: (value: string) => void;
  getCoverDataUrl.mockReturnValueOnce(new Promise<string>(resolve => {finishOld = resolve;}))
    .mockResolvedValueOnce('data:image/png;base64,new');
  const initial = bookNode(9841);
  const {result, rerender} = renderHook(({node}) => useCoverDataUrl(node, 0), {initialProps: {node: initial}});
  await waitFor(() => expect(getCoverDataUrl).toHaveBeenCalledTimes(1));
  rerender({node: {...initial, lastSeenAt: '2026-01-02'}});
  await waitFor(() => expect(result.current.coverUrl).toBe('data:image/png;base64,new'));
  await act(async () => {finishOld('data:image/png;base64,old'); await Promise.resolve();});
  expect(result.current.coverUrl).toBe('data:image/png;base64,new');
  expect(getCoverDataUrl).toHaveBeenCalledTimes(2);
});

it('still reports a missing expected cached cover', async () => {
  getCoverDataUrl.mockResolvedValue(null);
  const {result} = renderHook(() => useCoverDataUrl({...bookNode(9842), coverCachePath: 'X:/Fixtures/Cache/cover.png'}, 0));
  await waitFor(() => expect(result.current.coverLoading).toBe(false));
  expect(result.current.coverFailed).toBe(true);
});

it('requests the measured tier and retains the current preview while upgrading it', async () => {
  let resize!: ResizeObserverCallback;
  vi.stubGlobal('ResizeObserver', class {
    constructor(callback: ResizeObserverCallback) {resize=callback;}
    observe() {} unobserve() {} disconnect() {}
  });
  const geometry=vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockReturnValue({width:200} as DOMRect);
  let finish!: (url: string) => void;
  getCoverDataUrl.mockResolvedValueOnce('data:image/webp;base64,small')
    .mockReturnValueOnce(new Promise<string>(resolve => {finish=resolve;}));
  function Cover() {
    const {coverFrameRef,coverUrl}=useCoverDataUrl(bookNode(9843),0);
    return <div ref={coverFrameRef}>{coverUrl}</div>;
  }
  const view=render(<Cover/>);
  await waitFor(() => expect(view.container.textContent).toBe('data:image/webp;base64,small'));
  expect(getCoverDataUrl).toHaveBeenCalledWith(9843,256);
  act(() => resize([{target:view.container.firstChild,contentRect:{width:400}} as ResizeObserverEntry],{} as ResizeObserver));
  await waitFor(() => expect(getCoverDataUrl).toHaveBeenCalledWith(9843,512));
  expect(view.container.textContent).toBe('data:image/webp;base64,small');
  await act(async () => finish('data:image/webp;base64,large'));
  expect(view.container.textContent).toBe('data:image/webp;base64,large');
  view.unmount(); geometry.mockRestore(); vi.unstubAllGlobals();
});

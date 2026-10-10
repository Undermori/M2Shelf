// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { MediaNode } from "../types/media";
import { I18nProvider } from "../lib/i18n";
import { MediaCard } from "./MediaCard";

vi.mock("../hooks/useCoverDataUrl", () => ({ useCoverDataUrl: () => ({ coverUrl: "data:image/png;base64,fixture", coverCacheKey: "fixture", coverFailed: false, coverLoading: false }) }));
vi.mock("../hooks/usePosterViewportLifecycle", () => ({ usePosterViewportLifecycle: () => ({ coverRequested: true, coverVisible: true }) }));
vi.mock("./PosterImage", () => ({ PosterImage: ({ alt, src, onError }: { alt: string; src: string; onError: () => void }) => <img alt={alt} src={src} onError={onError} /> }));
afterEach(cleanup);

const node: MediaNode = {
  id: 12, displayName: "Fixture work", folderName: "Fixture work", nodeType: "WORK",
  libraryRootId: 1, parentNodeId: 100, absolutePath: "X:/Fixtures/Fixture work", manualTypeOverride: false,
  createdAt: "2026-01-01", updatedAt: "2026-01-01", lastSeenAt: "2026-01-01",
  totalVideoCount: 2, coverSource: "BANGUMI", coverCachePath: "X:/Fixtures/cover.png", userTags: [],
  binding: { id: 1, nodeId: 12, provider: "BANGUMI", providerSubjectId: 42, providerSubjectType: 2,
    providerTitle: "Fixture work", providerTitleCn: null, providerTitleEn: null, providerTitleJa: null,
    providerTitleKo: null, providerAliases: [], providerDate: null, providerImageUrl: null,
    boundAt: "2026-01-01", updatedAt: "2026-01-01", coverCachePath: "X:/Fixtures/cover.png",
    coverDownloadError: "another source needs repair" },
};

function mount(onRetryCover: (node: MediaNode, imageDecodeFailed?: boolean) => void) {
  return render(<I18nProvider><MediaCard node={node} viewMode="grid" coverRevision={0} onOpen={vi.fn()} onMenu={vi.fn()} onBangumi={vi.fn()} onRetryCover={onRetryCover} /></I18nProvider>);
}

it("does not report a valid representative poster as damaged merely because another source failed", () => {
  const retry = vi.fn();
  mount(retry);
  fireEvent.click(document.querySelector(".quick-bind.is-retry")!);
  expect(retry).toHaveBeenCalledWith(node, false);
});

it("reports its original source when the WebView cannot decode the cached image", () => {
  const retry = vi.fn();
  mount(retry);
  fireEvent.error(screen.getByRole("img"));
  fireEvent.click(document.querySelector(".quick-bind.is-retry")!);
  expect(retry).toHaveBeenCalledWith(node, true);
});

it("keeps quantity and a bounded tag summary in one row, without an empty tag spacer", () => {
  const props = {viewMode: 'grid' as const, coverRevision: 0, onOpen: vi.fn(), onMenu: vi.fn(), onBangumi: vi.fn(), onRetryCover: vi.fn()};
  const result = render(<I18nProvider><MediaCard {...props} node={node}/></I18nProvider>);
  expect(result.container.querySelector('.media-card-tags')).toBeNull();
  expect(result.container.querySelector('.cover-frame > .system-tag')).not.toBeNull();
  expect(result.container.querySelector('.media-card-meta .system-tag')).toBeNull();
  const tagged = {...node, userTags: [{id: 1, name: 'Favorite', createdAt: '2026-01-01', updatedAt: '2026-01-01'}]};
  result.rerender(<I18nProvider><MediaCard {...props} node={tagged}/></I18nProvider>);
  const slot = result.container.querySelector('.media-card-tags')!;
  expect(slot.parentElement?.className).toBe('media-card-meta');
  expect(slot.previousElementSibling?.tagName).toBe('SMALL');
  expect(slot.textContent).toBe('Favorite');
  const tags = ['A very long tag'.repeat(12), 'Second', 'Third'].map((name, id) => ({id, name, createdAt:'2026-01-01', updatedAt:'2026-01-01'}));
  result.rerender(<I18nProvider><MediaCard {...props} node={{...node, userTags:tags}}/></I18nProvider>);
  expect(result.container.querySelectorAll('.user-tag-pill')).toHaveLength(1);
  expect(result.container.querySelector('.media-card-tag-overflow')?.textContent).toBe('+2');
  expect(result.container.querySelector('.media-card-tags')?.getAttribute('title')).toContain('Second · Third');
  fireEvent.click(result.container.querySelector('.user-tag-pill')!);
  expect(props.onOpen).toHaveBeenCalledTimes(1);
  result.rerender(<I18nProvider><MediaCard {...props} node={node}/></I18nProvider>);
  expect(result.container.querySelector('.media-card-tags')).toBeNull();
  expect(result.container.querySelector('.media-card-meta > small')?.textContent).toBeTruthy();
});

it("keeps list badges and tag summary; clicking tags in edit mode selects the original Node", () => {
  const onOpen=vi.fn(), onSelect=vi.fn();
  const tagged={...node,userTags:[{id:1,name:'One',createdAt:'',updatedAt:''},{id:2,name:'Two',createdAt:'',updatedAt:''}]};
  const result=render(<I18nProvider><MediaCard node={tagged} viewMode="list" editMode onOpen={onOpen} onSelect={onSelect} onMenu={vi.fn()} onBangumi={vi.fn()} onRetryCover={vi.fn()} coverRevision={0}/></I18nProvider>);
  expect(result.container.querySelector('.media-card-meta .system-tag')).not.toBeNull();
  fireEvent.click(result.container.querySelector('.media-card-tags')!);
  expect(onSelect).toHaveBeenCalledWith(tagged);
  expect(onOpen).not.toHaveBeenCalled();
});

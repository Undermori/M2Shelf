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

import {Select} from '../components/Select';
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { SearchHit, LibraryRoot } from "../types/media";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { LoadingState } from "../components/LoadingState";
import { PosterImage } from "../components/PosterImage";
import { api } from "../lib/api";
import { compactPath, errorMessage, formatBytes, nodeDisplayTitle, mediaBadge } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { useCoverDataUrl } from "../hooks/useCoverDataUrl";
import { usePosterViewportLifecycle } from "../hooks/usePosterViewportLifecycle";

interface SearchPageProps {
  roots?: LibraryRoot[];
  onRootChange?: (rootId: number | null) => void;
  initialQuery?: string;
  rootId?: number | null;
  onQueryChange?: (query: string) => void;
  onOpen: (hit: SearchHit) => void;
  onError: (message: string) => void;
  onResultsReady?: () => void;
  coverRevision: number;
}

function SearchResult({ hit, index, coverRevision, onOpen }: { hit: SearchHit; index: number; coverRevision: number; onOpen: (hit: SearchHit) => void }) {
  const { t } = useI18n();
  const coverNode=hit.comicBook && (hit.node.parentNodeId===null || (hit.node.totalComicBookCount??0)!==1) ? undefined : hit.node;
  const resultRef = useRef<HTMLButtonElement>(null);
  const hasCachedCover = Boolean(hit.comicBook || hit.node.coverCachePath || hit.node.binding?.coverCachePath);
  const { coverRequested, coverVisible } = usePosterViewportLifecycle(
    resultRef,
    `${hit.node.id}:${hit.comicBook?.id ?? hit.mediaFile?.id ?? ""}`,
    {
      activationMarginPx: 800,
      retentionEnabled: hasCachedCover,
      retentionMarginPx: 1_400,
    },
  );
  const { coverFrameRef, coverCacheKey, coverUrl, coverFailed: coverReadFailed } = useCoverDataUrl(coverNode, coverRevision, coverRequested,coverNode?undefined:hit.comicBook??undefined);
  const [imageFailed, setImageFailed] = useState(false);
  const title = hit.comicBook?.displayName ?? hit.mediaFile?.fileName ?? nodeDisplayTitle(hit.node);
  const nodeTitle = nodeDisplayTitle(hit.node);
  const coverFailed = coverReadFailed || imageFailed;
  const placeholderIcon = hit.node.nodeType === "CONTAINER" ? "folder" : hit.node.nodeType === "MIXED" ? "archive" : "work";

  useEffect(() => setImageFailed(false), [coverUrl]);

  return (
    <button className="search-hit" key={`${hit.kind}-${hit.comicBook?.id ?? hit.mediaFile?.id ?? hit.node.id}-${index}`} onClick={() => onOpen(hit)} ref={resultRef} type="button">
      <span ref={coverFrameRef} className={`search-hit-cover ${coverUrl && !coverFailed ? "has-cover" : "is-placeholder"} ${coverFailed ? "is-failed" : ""}`}>
        {coverUrl && !coverFailed
          ? <PosterImage active={coverVisible} alt={t("card.coverAlt", { title: nodeTitle })} cacheKey={coverCacheKey} onError={() => setImageFailed(true)} src={coverUrl} />
          : <Icon name={placeholderIcon} />}
      </span>
      <span className="search-hit-copy"><small>{hit.comicBook?t(hit.node.mediaKind==='EBOOK'?'ebook.files':hit.node.mediaKind==='ARTBOOK'?'artbook.files':'comic.files'):hit.kind === "MEDIA_FILE" ? t("search.videoFile") : mediaBadge(hit.node)}</small><strong>{title}</strong><em>{compactPath(hit.comicBook?.sourcePath ?? hit.mediaFile?.absolutePath ?? hit.node.absolutePath, 96)}</em></span>
      {hit.mediaFile && <span className="search-hit-meta">{formatBytes(hit.mediaFile.fileSize)}<small>{hit.mediaFile.extension.replace(/^\./, "").toUpperCase()}</small></span>}
      <Icon className="search-hit-chevron" name="chevron" />
    </button>
  );
}

export function SearchPage({ initialQuery = "", rootId, roots = [], onRootChange, onQueryChange, onOpen, onError, onResultsReady, coverRevision }: SearchPageProps) {
  const { t } = useI18n();
  const [query, setQuery] = useState(initialQuery);
  const [results, setResults] = useState<SearchHit[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const searchRequest = useRef(0);

  const search = async (value: string) => {
    const request = ++searchRequest.current;
    const keyword = value.trim();
    if (!keyword) { setResults([]); setSearched(false); setLoading(false); return; }
    setLoading(true);
    setSearched(true);
    try {
      const next = await api.search(keyword, rootId ?? undefined);
      if (request === searchRequest.current) setResults(next);
    }
    catch (error) { if (request === searchRequest.current) onError(errorMessage(error)); }
    finally { if (request === searchRequest.current) setLoading(false); }
  };

  useEffect(() => {
    setQuery(initialQuery);
    if (initialQuery.trim()) void search(initialQuery);
    else {
      searchRequest.current += 1;
      setResults([]);
      setSearched(false);
      setLoading(false);
    }
    inputRef.current?.focus();
  // Navigation may retain a query while its Library Root scope changes or is removed.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialQuery, rootId]);

  useLayoutEffect(() => {
    const waitingForInitialSearch = Boolean(initialQuery.trim()) && !searched;
    if (!loading && !waitingForInitialSearch) onResultsReady?.();
  }, [initialQuery, loading, onResultsReady, results, searched]);

  return (
    <section className="search-page">
      <header className="page-toolbar search-toolbar">
        <div className="toolbar-topline"><span className="all-resources-location"><Icon name="search" />{t("search.eyebrow")}</span></div>
        <div className="page-title-row">
          <div><h1>{t("common.search")}</h1><p>{t("search.description")}</p></div>
        </div>
        <form className="search-controls" onSubmit={event => { event.preventDefault(); void search(query); }}>
            <label className="search-field"><Icon name="search" /><input aria-label={t("common.search")} ref={inputRef} onChange={event => { setQuery(event.target.value); onQueryChange?.(event.target.value); }} value={query} /></label>
            <label className="sort-field"><Select aria-label={t("search.scope")} value={rootId ?? "all"} onChange={event => onRootChange?.(event.target.value === "all" ? null : Number(event.target.value))}><option value="all">{t("search.allLibraries")}</option>{roots.map(root => <option key={root.id} value={root.id}>{root.displayName}</option>)}</Select></label>
            <button className="button primary" disabled={!query.trim() || loading} type="submit">{t("common.search")}</button>
        </form>
      </header>
      <div className="search-content">
        {loading && <LoadingState label={t("search.loading")} />}
        {!loading && !searched && <EmptyState icon="search" title={t("search.promptTitle")} />}
        {!loading && searched && results.length === 0 && <EmptyState icon="info" title={t("search.noneTitle")} description={t("search.noneDescription")} />}
        {!loading && results.length > 0 && <><div className="search-result-heading"><strong>{t("search.resultCount", { count: results.length })}</strong><span>{rootId ? t("search.currentLibrary") : t("search.allLibraries")}</span></div><div className="search-results">{results.map((hit, index) => <SearchResult coverRevision={coverRevision} hit={hit} index={index} key={`${hit.kind}-${hit.mediaFile?.id ?? hit.node.id}-${index}`} onOpen={onOpen} />)}</div></>}
      </div>
    </section>
  );
}

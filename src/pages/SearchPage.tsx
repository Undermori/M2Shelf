import {isBookKind} from "../types/media";
import type {SearchFilters} from "../types/incremental";
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
  filters?:SearchFilters;
  onFiltersChange?:(filters:SearchFilters)=>void;
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
  const { coverFrameRef, coverCacheKey, coverUrl, coverFailed: coverReadFailed, onCoverError } = useCoverDataUrl(coverNode, coverRevision, coverRequested,coverNode?undefined:hit.comicBook??undefined);
  const [imageFailed, setImageFailed] = useState(false);
  const title = hit.comicBook?.displayName ?? hit.mediaFile?.fileName ?? nodeDisplayTitle(hit.node);
  const nodeTitle = nodeDisplayTitle(hit.node);
  const coverFailed = coverReadFailed || imageFailed;
  const placeholderIcon = hit.node.nodeType === "CONTAINER" ? "folder" : hit.node.nodeType === "MIXED" ? "archive" : hit.comicBook || isBookKind(hit.node.mediaKind) ? "book" : "work";

  useEffect(() => setImageFailed(false), [coverUrl]);

  return (
    <button className="search-hit" key={`${hit.kind}-${hit.comicBook?.id ?? hit.mediaFile?.id ?? hit.node.id}-${index}`} onClick={() => onOpen(hit)} ref={resultRef} type="button">
      <span ref={coverFrameRef} className={`search-hit-cover ${coverUrl && !coverFailed ? "has-cover" : "is-placeholder"} ${coverFailed ? "is-failed" : ""}`}>
        {coverUrl && !coverFailed
          ? <PosterImage active={coverVisible} alt={t("card.coverAlt", { title: nodeTitle })} cacheKey={coverCacheKey} onError={() => {setImageFailed(true);onCoverError?.();}} src={coverUrl} />
          : <Icon name={placeholderIcon} />}
      </span>
      <span className="search-hit-copy"><small>{hit.comicBook?t(hit.node.mediaKind==='EBOOK'?'ebook.files':hit.node.mediaKind==='ARTBOOK'?'artbook.files':'comic.files'):hit.kind === "MEDIA_FILE" ? t("search.videoFile") : mediaBadge(hit.node)}</small><strong>{title}</strong>{hit.noteExcerpt&&<span className="note-excerpt">{hit.noteExcerpt}</span>}<em>{compactPath(hit.comicBook?.sourcePath ?? hit.mediaFile?.absolutePath ?? hit.node.absolutePath, 96)}</em></span>
      {hit.mediaFile && <span className="search-hit-meta">{formatBytes(hit.mediaFile.fileSize)}<small>{hit.mediaFile.extension.replace(/^\./, "").toUpperCase()}</small></span>}
      <Icon className="search-hit-chevron" name="chevron" />
    </button>
  );
}

export function SearchPage({ filters={}, onFiltersChange, initialQuery = "", rootId, roots = [], onRootChange, onQueryChange, onOpen, onError, onResultsReady, coverRevision }: SearchPageProps) {
  const { t } = useI18n();
  const [extensions,setExtensions]=useState<string[]>([]);
  const filtersKey=JSON.stringify(filters);
  useEffect(()=>{let active=true;void Promise.resolve().then(()=>api.searchExtensions(rootId??undefined)).then(rows=>{if(active)setExtensions(rows.filter(extension=>extension.trim().length>0));}).catch(()=>{if(active)setExtensions([]);});return()=>{active=false;};},[rootId]);
  const changeFilter=(key:keyof SearchFilters,value:string)=>onFiltersChange?.({...filters,[key]:value||undefined});
  const [query, setQuery] = useState(initialQuery);
  const [results, setResults] = useState<SearchHit[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const searchRequest = useRef(0);

  const search = async (value: string) => {
    const request = ++searchRequest.current;
    const keyword = value.trim();
    if (!keyword && !filters.scanRootId) { setResults([]); setSearched(false); setLoading(false); return; }
    setLoading(true);
    setSearched(true);
    try {
      const next = await (Object.values(filters).some(v=>v!==undefined)?api.search(keyword, rootId ?? undefined, filters):api.search(keyword, rootId ?? undefined));
      if (request === searchRequest.current) setResults(next);
    }
    catch (error) { if (request === searchRequest.current) onError(errorMessage(error)); }
    finally { if (request === searchRequest.current) setLoading(false); }
  };

  const updateQuery = (value: string) => {
    setQuery(value);
    onQueryChange?.(value);
  };

  const clearQuery = () => {
    updateQuery("");
    if (!filters.scanRootId) void search("");
    inputRef.current?.focus();
  };

  useEffect(() => {
    setQuery(initialQuery);
    if (initialQuery.trim() || filters.scanRootId) void search(initialQuery);
    else {
      searchRequest.current += 1;
      setResults([]);
      setSearched(false);
      setLoading(false);
    }
    inputRef.current?.focus();
  // Navigation may retain a query while its Library Root scope changes or is removed.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialQuery, rootId, filtersKey]);

  useLayoutEffect(() => {
    const waitingForInitialSearch = Boolean(initialQuery.trim() || filters.scanRootId) && !searched;
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
            <div className="search-field">
              <Icon name="search" />
              <input aria-label={t("common.search")} ref={inputRef} onChange={event => updateQuery(event.target.value)} value={query} />
              {query && <button aria-label={t("common.clear")} onClick={clearQuery} type="button"><Icon name="close" /></button>}
            </div>
            {(rootId!=null||Object.values(filters).some(value=>value!==undefined))&&<button className="button ghost search-clear-filters" type="button" onClick={()=>{onFiltersChange?.({});onRootChange?.(null);}}>{t('filters.clear')}</button>}
            <button className="button primary" disabled={(!query.trim()&&!filters.scanRootId) || loading} type="submit">{t("common.search")}</button>
        </form>
        <div className="search-extra-filters search-five-filters">
          <Select aria-label={t("search.scope")} value={rootId ?? "all"} onChange={event => onRootChange?.(event.target.value === "all" ? null : Number(event.target.value))}><option value="all">{t("search.allLibraries")}</option>{roots.map(root => <option key={root.id} value={root.id}>{root.displayName}</option>)}</Select>
          <Select aria-label={t('filters.media')} value={filters.mediaKind??''} onChange={e=>changeFilter('mediaKind',e.target.value)}><option value="">{t('filters.media')}</option>{(['ANIMATION','LIVE_ACTION','COMIC','EBOOK','DOUJIN','ARTBOOK'] as const).map(kind=><option key={kind} value={kind}>{t(({ANIMATION:'comic.animation',LIVE_ACTION:'library.liveAction',COMIC:'comic.name',EBOOK:'ebook.name',DOUJIN:'doujin.name',ARTBOOK:'artbook.name'} as const)[kind])}</option>)}</Select>
          <Select aria-label={t('filters.binding')} value={filters.binding??''} onChange={e=>changeFilter('binding',e.target.value)}><option value="">{t('filters.binding')}</option><option value="bound">{t('filters.bound')}</option><option value="unbound">{t('filters.unbound')}</option></Select>
          <Select aria-label={t('state.filter')} value={filters.collectionStatus??''} onChange={e=>changeFilter('collectionStatus',e.target.value)}><option value="">{t('state.all')}</option><option value="UNSET">{t('state.unset')}</option>{(['WANT','IN_PROGRESS','COMPLETED','PAUSED','DROPPED'] as const).map(state=><option key={state} value={state}>{t(state==='WANT'?'state.want':`state.${state}`)}</option>)}</Select>
          <Select aria-label={t('filters.extension')} value={filters.extension??''} onChange={e=>changeFilter('extension',e.target.value)}><option value="">{t('filters.extension')}</option>{extensions.map(ext=><option key={ext} value={ext}>{ext}</option>)}</Select>
        </div>
        {filters.scanRootId&&<div className="search-added-context" role="status">{t('delta.newResults')}</div>}

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

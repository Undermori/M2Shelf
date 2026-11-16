import {searchKnowledge, type KnowledgeResults} from '../lib/knowledgeSearch';
import {openStudio} from '../lib/openStudio';
import {HighlightMatch, matchExcerpt} from '../components/CommandPalette';
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
  sourceFilter?:string;onSourceFilterChange?:(source:string)=>void;
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

function SearchResult({ hit, index, query, coverRevision, onOpen }: { hit: SearchHit; index: number; query: string; coverRevision: number; onOpen: (hit: SearchHit) => void }) {
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
    <div className="knowledge-result"><button className="search-hit" key={`${hit.kind}-${hit.comicBook?.id ?? hit.mediaFile?.id ?? hit.node.id}-${index}`} onClick={() => onOpen(hit)} ref={resultRef} type="button">
      <span ref={coverFrameRef} className={`search-hit-cover ${coverUrl && !coverFailed ? "has-cover" : "is-placeholder"} ${coverFailed ? "is-failed" : ""}`}>
        {coverUrl && !coverFailed
          ? <PosterImage active={coverVisible} alt={t("card.coverAlt", { title: nodeTitle })} cacheKey={coverCacheKey} onError={() => {setImageFailed(true);onCoverError?.();}} src={coverUrl} />
          : <Icon name={placeholderIcon} />}
      </span>
      <span className="search-hit-copy"><small>{hit.comicBook?t(hit.node.mediaKind==='EBOOK'?'ebook.files':hit.node.mediaKind==='ARTBOOK'?'artbook.files':'comic.files'):hit.kind === "MEDIA_FILE" ? t("search.videoFile") : mediaBadge(hit.node)}</small><strong>{title}</strong>{hit.noteExcerpt&&<span className="note-excerpt"><HighlightMatch text={matchExcerpt(hit.noteExcerpt,query)} query={query}/></span>}<em>{compactPath(hit.comicBook?.sourcePath ?? hit.mediaFile?.absolutePath ?? hit.node.absolutePath, 96)}</em></span>
      {hit.mediaFile && <span className="search-hit-meta">{formatBytes(hit.mediaFile.fileSize)}<small>{hit.mediaFile.extension.replace(/^\./, "").toUpperCase()}</small></span>}
      <Icon className="search-hit-chevron" name="chevron" />
    </button>{hit.noteExcerpt&&<button type="button" className="button ghost" onClick={()=>openStudio({view:'notes',target:{kind:hit.comicBook?'BOOK':'NODE',id:hit.comicBook?.id??hit.node.id},nodeId:hit.node.id})}>{t('knowledge.edit')}</button>}</div>
  );
}

export function SearchPage({ sourceFilter,onSourceFilterChange,filters={}, onFiltersChange, initialQuery = "", rootId, roots = [], onRootChange, onQueryChange, onOpen, onError, onResultsReady, coverRevision }: SearchPageProps) {
  const { t } = useI18n();
  const [extensions,setExtensions]=useState<string[]>([]);
  const filtersKey=JSON.stringify(filters);
  useEffect(()=>{let active=true;void Promise.resolve().then(()=>api.searchExtensions(rootId??undefined)).then(rows=>{if(active)setExtensions(rows.filter(extension=>extension.trim().length>0));}).catch(()=>{if(active)setExtensions([]);});return()=>{active=false;};},[rootId]);
  const changeFilter=(key:keyof SearchFilters,value:string)=>onFiltersChange?.({...filters,[key]:value||undefined});
  const [query, setQuery] = useState(initialQuery);
  const [results, setResults] = useState<SearchHit[]>([]);
  const [knowledge,setKnowledge]=useState<KnowledgeResults>({works:[],notes:[],screenshots:[],failed:false});
  const [localSource,setLocalSource]=useState('');
  const source=sourceFilter??localSource;
  const setSource=(next:string)=>{setLocalSource(next);onSourceFilterChange?.(next);};
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const searchRequest = useRef(0);
  const debounceTimer = useRef<number | undefined>(undefined);
  const lastTypedQuery = useRef<string | null>(null);
  const prevScope = useRef({ rootId, filtersKey });

  useEffect(() => () => {window.clearTimeout(debounceTimer.current);searchRequest.current++;}, []);

  const search = async (value: string) => {
    window.clearTimeout(debounceTimer.current);
    const request = ++searchRequest.current;
    const keyword = value.trim();
    setKnowledge({works:[],notes:[],screenshots:[],failed:false});
    if (!keyword && !filters.scanRootId) { setResults([]); setSearched(false); setLoading(false); return; }
    setLoading(true);
    setResults([]);
    setSearched(true);
    try {
      const extra = !Object.values(filters).some(v=>v!==undefined)?searchKnowledge(keyword,false,rootId??undefined):Promise.resolve({works:[],notes:[],screenshots:[],failed:false});
      const next = await (Object.values(filters).some(v=>v!==undefined)?api.search(keyword, rootId ?? undefined, filters):api.search(keyword, rootId ?? undefined));
      const supplementary=await extra;
      if (request === searchRequest.current) {setResults(next);setKnowledge(supplementary);}
    }
    catch (error) { if (request === searchRequest.current) onError(errorMessage(error)); }
    finally { if (request === searchRequest.current) setLoading(false); }
  };

  const updateQuery = (value: string) => {
    searchRequest.current++;
    lastTypedQuery.current = value;
    setQuery(value);
    onQueryChange?.(value);
    window.clearTimeout(debounceTimer.current);
    if (!value.trim() && !filters.scanRootId) {
      void search("");
      return;
    }
    debounceTimer.current = window.setTimeout(() => {
      void search(value);
    }, 280);
  };

  const clearQuery = () => {
    window.clearTimeout(debounceTimer.current);
    lastTypedQuery.current = null;
    setQuery("");
    onQueryChange?.("");
    void search("");
    inputRef.current?.focus();
  };

  useEffect(() => {
    const scopeChanged = prevScope.current.rootId !== rootId || prevScope.current.filtersKey !== filtersKey;
    prevScope.current = { rootId, filtersKey };
    if (!scopeChanged && lastTypedQuery.current !== null && initialQuery === lastTypedQuery.current) {
      return;
    }
    lastTypedQuery.current = null;
    window.clearTimeout(debounceTimer.current);
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

  const visibleWorks = source === '' || source === 'works' ? results : [];
  const visibleNotes = source === '' || source === 'notes' ? knowledge.notes : [];
  const visibleScreenshots = source === '' || source === 'screenshots' ? knowledge.screenshots : [];
  const visibleCount = visibleWorks.length + visibleNotes.length + visibleScreenshots.length;

  return (
    <section className="search-page">
      <header className="page-toolbar search-toolbar">
        <div className="page-title-row">
          <div><h1>{t("common.search")}</h1></div>
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
<Select aria-label={t('knowledge.all')} value={source} onChange={event=>setSource(event.target.value)}><option value="">{t('knowledge.all')}</option><option value="works">{t('knowledge.work')}</option><option value="notes">{t('knowledge.note')}</option><option value="screenshots">{t('knowledge.screenshot')}</option></Select>
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
        {!loading && searched && <div className="search-result-heading"><strong role="status">{t("search.resultCount", { count: visibleCount })}</strong><span>{rootId ? t("search.currentLibrary") : t("search.allLibraries")}</span></div>}
        {!loading && searched && visibleCount === 0 && <EmptyState icon="info" title={t("search.noneTitle")} description={t("search.noneDescription")} />}
        {!loading && visibleWorks.length > 0 && <div className="search-results">{visibleWorks.map((hit, index) => <SearchResult coverRevision={coverRevision} hit={hit} index={index} query={query} key={`${hit.kind}-${hit.comicBook?.id ?? hit.mediaFile?.id ?? hit.node.id}-${index}`} onOpen={onOpen} />)}</div>}
        {!loading&&knowledge.failed&&<p role="status">{t('knowledge.searchFailed')}</p>}
        {!loading&&<div className="knowledge-results">
          {visibleNotes.map(note=><button type="button" key={`note:${note.target.kind}:${note.target.id}`} onClick={()=>openStudio({view:'notes',target:note.target,nodeId:note.nodeId})}><span className="knowledge-source">{t('knowledge.note')}</span><span className="knowledge-entry"><strong><HighlightMatch text={note.title} query={query}/></strong><small><HighlightMatch text={matchExcerpt(note.body,query)} query={query}/></small></span></button>)}
          {visibleScreenshots.map(shot=><button type="button" key={`shot:${shot.id}`} onClick={()=>openStudio({view:'gallery',screenshotId:shot.id})}><span className="knowledge-source">{t('knowledge.screenshot')}</span><span className="knowledge-entry"><strong><HighlightMatch text={shot.fileName} query={query}/></strong><small><HighlightMatch text={matchExcerpt([shot.note,...shot.tags].join(' · '),query)} query={query}/></small></span></button>)}
        </div>}
      </div>
    </section>
  );
}

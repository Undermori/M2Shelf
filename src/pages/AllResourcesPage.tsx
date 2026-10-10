import { CollectionSortControl } from "../components/CollectionSortControl";
import { useMemo, useState, useEffect } from "react";
import type { AllResourcesResult, CollectionSort, MediaNode, ViewMode } from "../types/media";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { LoadingState } from "../components/LoadingState";
import { PosterGrid } from "../components/PosterGrid";
import { SelectionToolbar } from "../components/SelectionToolbar";
import { TagFilter } from "../components/TagFilter";
import { compareMediaNodes, nodeMatchesQuery, nodeDisplayTitle, naturalCompare, compareFileModifiedTimes } from "../lib/format";
import { allCollectionEntries, type BookCollectionEntry } from '../lib/bookCollection';
import { MediaCard } from '../components/MediaCard';
import { BookPosterCard } from '../components/BookPosterCard';
import type {ComicBook} from '../types/comic';
import {BookContextMenu} from '../components/BookContextMenu';
import { useI18n } from "../lib/i18n";

interface AllResourcesPageProps {
  onOpenBookEntry:(entry:BookCollectionEntry)=>void;
  onRevealBook:(book:ComicBook)=>void;
  mediaKind?:'ALL'|'ANIMATION'|'LIVE_ACTION'|'COMIC'|'EBOOK'|'DOUJIN'|'ARTBOOK';onMediaKind?:(kind:'ALL'|'ANIMATION'|'LIVE_ACTION'|'COMIC'|'EBOOK'|'DOUJIN'|'ARTBOOK')=>void;
  grouping: "works" | "folders";
  data: AllResourcesResult | null;
  loading: boolean;
  viewMode: ViewMode;
  onViewMode: (mode: ViewMode) => void;
  filter: string;
  onFilter: (value: string) => void;
  tagFilterId: number | null;
  onTagFilter: (tagId: number | null) => void;
  sort: CollectionSort;
  onSort: (value: CollectionSort) => void;
  onOpenNode: (node: MediaNode) => void;
  onMenu: (event: React.MouseEvent, node: MediaNode) => void;
  onBangumi: (node: MediaNode) => void;
  onRetryCover: (node: MediaNode, imageDecodeFailed?: boolean) => void;
  onScan: () => void;
  onAddRoot: () => void;
  onSearch: (query: string) => void;
  coverRevision: number;
  editMode: boolean;
  selectedNodeIds: ReadonlySet<number>;
  matchBusy: boolean;
  onEditMode: (active: boolean) => void;
  onToggleSelection: (node: MediaNode) => void;
  onSelectAll: (nodeIds: number[]) => void;
  onClearSelection: () => void;
  onBatchTags: () => void;
  onBatchFavorites: () => void;
  onBatchMenu: (event: React.MouseEvent<HTMLButtonElement>) => void;
  onMatch: (nodeIds: number[] | null, rematchExisting: boolean) => void;
}

export function AllResourcesPage({ onOpenBookEntry,onRevealBook,mediaKind='ALL',onMediaKind,grouping, data, loading, viewMode, onViewMode, filter, onFilter, tagFilterId, onTagFilter, sort, onSort, onOpenNode, onMenu, onBangumi, onRetryCover, onScan, onAddRoot, onSearch, coverRevision, editMode, selectedNodeIds, matchBusy, onEditMode, onToggleSelection, onSelectAll, onClearSelection, onBatchTags, onBatchFavorites, onBatchMenu, onMatch }: AllResourcesPageProps) {
  const { language, number, t } = useI18n();
  const [bookMenu,setBookMenu]=useState<{x:number;y:number;book:ComicBook}|null>(null);
  useEffect(()=>setBookMenu(null),[data,grouping,editMode]);
  const sourceEntries=useMemo(()=>allCollectionEntries(data,grouping,editMode),[data,grouping,editMode]);
  const sourceNodes=useMemo(()=>sourceEntries.flatMap(entry=>entry.node?[entry.node]:[]),[sourceEntries]);
  const effectiveTagFilterId = tagFilterId != null && sourceNodes.some((node) => node.userTags.some((tag) => tag.id === tagFilterId))
    ? tagFilterId
    : null;
  const entries = useMemo(() => {
    const query = filter.trim().toLocaleLowerCase();
    const name=(entry:typeof sourceEntries[number])=>entry.node?nodeDisplayTitle(entry.node,language):entry.bookEntry?.name??'';
    const stamp=(entry:typeof sourceEntries[number])=>sort.startsWith('added-')?entry.node?.createdAt??entry.bookEntry?.root.createdAt:sort.startsWith('watched-')?entry.node?.lastWatchedAt??entry.bookEntry?.group?.books.map(b=>b.progress?.lastReadAt??'').sort().at(-1):entry.node?.latestFileModifiedAt??entry.bookEntry?.group?.books.map(b=>b.modifiedAt).sort().at(-1);
    return [...sourceEntries]
      .filter(({node,bookEntry})=>mediaKind==='ALL'||(bookEntry?.root.mediaKind??node?.mediaKind??'VIDEO')===mediaKind||node?.mediaKind==='VIDEO'&&(mediaKind==='ANIMATION'?node.binding?.providerSubjectType!==6:mediaKind==='LIVE_ACTION'&&node.binding?.providerSubjectType!==2))
      .filter(({node,bookEntry})=>!query||!!node&&nodeMatchesQuery(node,query,language)||bookEntry?.name.toLocaleLowerCase().includes(query)||bookEntry?.group?.books.some(book=>`${book.displayName} ${book.sourcePath??''}`.toLocaleLowerCase().includes(query))||node?.workView&&data?.works.find(work=>work.node.id===node.id)?.sources.some(source=>nodeMatchesQuery(source,query,language)))
      .filter(({node})=>effectiveTagFilterId==null||node?.userTags.some(tag=>tag.id===effectiveTagFilterId))
      .sort((a,b)=>a.node&&b.node?compareMediaNodes(a.node,b.node,sort,language):(sort.startsWith('title-')?(sort.endsWith('-desc')?-1:1)*naturalCompare(name(a),name(b),language):compareFileModifiedTimes(stamp(a),stamp(b),sort))||naturalCompare(name(a),name(b),language)||a.key.localeCompare(b.key));
  }, [mediaKind,data?.works, sourceEntries, effectiveTagFilterId, filter, language, sort]);
  const nodes=entries.flatMap(entry=>entry.node?[entry.node]:[]);
  const hasActiveFilter = Boolean(filter.trim()) || effectiveTagFilterId != null || mediaKind!=='ALL';

  if (loading && !data) return <LoadingState label={t("all.loading")} />;
  if (!data) return <EmptyState eyebrow={t("all.eyebrow")} title={t("all.emptyTitle")} description={t("all.emptyDescription")} action={<button className="button primary" onClick={onAddRoot} type="button"><Icon name="plus" />{t("app.addMediaDirectory")}</button>} />;

  return (
    <section className="browse-page all-resources-page">
      {(data.recognitionWarnings?.length ?? 0) > 0 && <div className="preview-banner"><Icon name="warning" /><span>{t("works.recognitionWarning")} {data.recognitionWarnings?.map(node => <button type="button" className="button ghost" key={node.id} onClick={() => onOpenNode(node)}>{node.folderName}</button>)}</span></div>}
      <header className="page-toolbar collection-toolbar all-resources-toolbar">
        <div className="collection-heading">
          <div className="collection-title"><h1>{t("all.title")}</h1><span className="collection-count" role="status">{t("app.projectCount", { count: number(entries.length) })}{hasActiveFilter && <span> / {number(sourceEntries.length)}</span>}</span></div>
          <div className="toolbar-actions">
            <button aria-pressed={editMode} className={`button secondary edit-mode-button ${editMode ? "is-active" : ""}`} onClick={() => onEditMode(!editMode)} type="button"><Icon name="edit" />{editMode ? t("selection.exit") : t("selection.editMode")}</button>
            <button className="button secondary scan-button" disabled={matchBusy || (editMode && selectedNodeIds.size === 0)} onClick={() => onMatch(editMode ? [...selectedNodeIds] : null, editMode)} type="button"><Icon name="bangumi" />{matchBusy ? t("selection.matching") : editMode ? t("selection.rematchSelectedCount", { count: number(selectedNodeIds.size) }) : t("all.matchExisting")}</button>
            <button className="button secondary scan-button" disabled={matchBusy} onClick={onScan} type="button"><Icon name="refresh" />{t("all.scanAndMatch")}</button>
          </div>
        </div>
        <div className="media-kind-tabs" role="tablist" aria-label={t("comic.kind")}>
          {([['ALL', 'all.typeAll'], ['ANIMATION', 'comic.animation'], ['LIVE_ACTION', 'all.typeLiveAction'], ['COMIC', 'comic.name'], ['EBOOK', 'ebook.name'], ['DOUJIN', 'doujin.name'], ['ARTBOOK', 'artbook.name']] as const).map(([kind, label], index, items) => <button key={kind} type="button" role="tab" aria-selected={mediaKind === kind} tabIndex={mediaKind === kind ? 0 : -1} onClick={() => onMediaKind?.(kind)} onKeyDown={event => {
            const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : event.key === 'ArrowRight' ? (index + 1) % items.length : event.key === 'ArrowLeft' ? (index + items.length - 1) % items.length : -1;
            if (next < 0) return;
            event.preventDefault(); onMediaKind?.(items[next][0]);
            (event.currentTarget.parentElement?.children[next] as HTMLElement)?.focus();
          }}>{t(label)}</button>)}
        </div>
          <div className="browse-controls collection-filters all-resources-filters">
            <label className="search-field"><Icon name="search" /><input aria-label={t("all.filterAria")} onChange={(event) => onFilter(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && filter.trim()) onSearch(filter.trim()); }} placeholder={t("all.filterPlaceholder")} value={filter} />{filter && <button aria-label={t("all.clearFilter")} onClick={() => onFilter("")} type="button"><Icon name="close" /></button>}</label>
            <TagFilter nodes={sourceNodes} value={tagFilterId} onChange={onTagFilter} />
            <CollectionSortControl value={sort} onChange={onSort} />
            <div className="view-toggle" aria-label={t("common.displayMode")}><button className={viewMode === "grid" ? "is-active" : ""} onClick={() => onViewMode("grid")} title={t("common.grid")} type="button"><Icon name="grid" /></button><button className={viewMode === "list" ? "is-active" : ""} onClick={() => onViewMode("list")} title={t("common.list")} type="button"><Icon name="list" /></button></div>
          </div>
      </header>
      <div className="page-content">
        {editMode && <SelectionToolbar selectedCount={selectedNodeIds.size} visibleCount={nodes.length} busy={matchBusy} onSelectAll={() => onSelectAll(nodes.map((node) => node.id))} onClear={onClearSelection} onTags={onBatchTags} onFavorites={onBatchFavorites} onMore={onBatchMenu} onRematch={() => onMatch([...selectedNodeIds], true)} onExit={() => onEditMode(false)} />}
        {entries.length > 0 && <PosterGrid nodes={[]} viewMode={viewMode} onOpen={onOpenNode} onMenu={onMenu} onBangumi={onBangumi} onRetryCover={onRetryCover} coverRevision={coverRevision}>
          {entries.map(({key,node,bookEntry})=>node?<MediaCard key={key} showModifiedTime={sort.startsWith('modified-')} node={node} viewMode={viewMode} onOpen={()=>bookEntry?onOpenBookEntry(bookEntry):onOpenNode(node)} onMenu={onMenu} onBangumi={onBangumi} onRetryCover={onRetryCover} coverRevision={coverRevision} editMode={editMode} selected={selectedNodeIds.has(node.id)} onSelect={onToggleSelection}/>:bookEntry?.group&&<BookPosterCard key={key} book={bookEntry.group.books[0]} title={bookEntry.name} count={bookEntry.group.books.length} series={bookEntry.group.kind==='SERIES'} root={bookEntry.root} viewMode={viewMode} revision={coverRevision} onOpen={()=>onOpenBookEntry(bookEntry)} onMenu={event=>{event.preventDefault();setBookMenu({x:Math.max(8,Math.min(event.clientX,window.innerWidth-245)),y:Math.max(8,Math.min(event.clientY,window.innerHeight-100)),book:bookEntry.group!.books[0]});}} editMode={editMode}/>)}
        </PosterGrid>}
        {entries.length === 0 && <EmptyState compact icon={hasActiveFilter ? "search" : "folder"} title={hasActiveFilter ? t("all.noMatch") : t("all.temporarilyEmpty")} description={hasActiveFilter ? t("all.noMatchDescription") : t("all.emptyScanDescription")} />}
        {bookMenu&&<BookContextMenu {...bookMenu} onClose={()=>setBookMenu(null)} onReveal={onRevealBook}/>}
      </div>
    </section>
  );
}

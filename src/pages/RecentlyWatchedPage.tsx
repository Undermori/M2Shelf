import { useMemo,useState } from "react";
import type { MediaNode, RecentlyWatchedEntry, ViewMode } from "../types/media";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { LoadingState } from "../components/LoadingState";
import { PosterGrid } from "../components/PosterGrid";
import { useI18n } from "../lib/i18n";
import {MediaCard} from '../components/MediaCard';
import {BookPosterCard} from '../components/BookPosterCard';
import {BookContextMenu} from '../components/BookContextMenu';
import type {ComicBook} from '../types/comic';
import type {CollectionTarget} from '../types/collection';
import {SelectionToolbar} from '../components/SelectionToolbar';
import {ConfirmDialog} from '../components/ConfirmDialog';
import {errorMessage} from '../lib/format';

interface RecentlyWatchedPageProps {
  onReadBook:(book:ComicBook)=>void;
  onRevealBook:(book:ComicBook)=>void;
  entries: RecentlyWatchedEntry[] | null;
  loading: boolean;
  viewMode: ViewMode;
  onViewMode: (mode: ViewMode) => void;
  onOpenNode: (node: MediaNode) => void;
  onMenu: (event: React.MouseEvent, node: MediaNode) => void;
  onBangumi: (node: MediaNode) => void;
  onRetryCover: (node: MediaNode, imageDecodeFailed?: boolean) => void;
  coverRevision: number;
  onRemoveRecent?:(targets:CollectionTarget[])=>Promise<void>;
  onClearRecent?:()=>Promise<void>;
  onError?:(message:string)=>void;
}

export function RecentlyWatchedPage({ onReadBook,onRevealBook,entries, loading, viewMode, onViewMode, onOpenNode, onMenu, onBangumi, onRetryCover, coverRevision,onRemoveRecent,onClearRecent,onError }: RecentlyWatchedPageProps) {
  const { number, t } = useI18n();
  const nodes = useMemo(() => entries?.map((entry) => entry.node) ?? [], [entries]);
  const [bookMenu,setBookMenu]=useState<{x:number;y:number;book:ComicBook}|null>(null);
  const [edit,setEdit]=useState(false),[selected,setSelected]=useState<Set<string>>(new Set()),[busy,setBusy]=useState(false),[confirmClear,setConfirmClear]=useState(false);
  const target=(entry:RecentlyWatchedEntry):CollectionTarget=>entry.comicBook?{kind:'BOOK',id:entry.comicBook.id}:{kind:'NODE',id:entry.node.id};
  const key=(entry:RecentlyWatchedEntry)=>`${target(entry).kind}:${target(entry).id}`;
  const toggle=(entry:RecentlyWatchedEntry)=>setSelected(previous=>{const next=new Set(previous);if(next.has(key(entry)))next.delete(key(entry));else next.add(key(entry));return next;});
  const chosen=(entries??[]).filter(entry=>selected.has(key(entry)));
  const run=async(action:()=>Promise<void>)=>{setBusy(true);try{await action();setSelected(new Set());setConfirmClear(false);}catch(error){onError?.(errorMessage(error));}finally{setBusy(false);}};

  if (loading && !entries) return <LoadingState label={t("recent.loading")} />;

  return (
    <section className="browse-page recently-watched-page">
      <header className="page-toolbar">
        <div className="toolbar-topline">
          <span className="all-resources-location"><Icon name="clock" />{t('comic.recentTitle')}</span>
        </div>
        <div className="page-title-row">
          <div className="page-heading-title"><h1>{t('comic.recentTitle')}</h1><span className="collection-count">{t("recent.count", { count: number(nodes.length) })}</span></div>
          <div className="browse-controls">
            {onRemoveRecent&&<button className="button secondary" disabled={busy} type="button" aria-pressed={edit} onClick={()=>{setEdit(v=>!v);setSelected(new Set());}}><Icon name="edit"/>{t(edit?'selection.exit':'selection.editMode')}</button>}
            {onClearRecent&&<button className="button secondary" disabled={busy||!entries?.length} type="button" onClick={()=>setConfirmClear(true)}>{t('recent.clear')}</button>}
            <div className="view-toggle" aria-label={t("common.displayMode")}><button className={viewMode === "grid" ? "is-active" : ""} onClick={() => onViewMode("grid")} title={t("common.grid")} type="button"><Icon name="grid" /></button><button className={viewMode === "list" ? "is-active" : ""} onClick={() => onViewMode("list")} title={t("common.list")} type="button"><Icon name="list" /></button></div>
          </div>
        </div>
      </header>
      {edit&&<SelectionToolbar selectedCount={chosen.length} visibleCount={entries?.length??0} busy={busy} onSelectAll={()=>setSelected(new Set(entries?.map(key)))} onClear={()=>setSelected(new Set())} onRemoveSelected={()=>void run(()=>onRemoveRecent!(chosen.map(target)))} onExit={()=>{setEdit(false);setSelected(new Set());}}/>}
      <div className="page-content">
        {nodes.length > 0 && <><PosterGrid nodes={[]} viewMode={viewMode} onOpen={onOpenNode} onMenu={onMenu} onBangumi={onBangumi} onRetryCover={onRetryCover} coverRevision={coverRevision}>
          {entries?.map(entry=>entry.comicBook?<BookPosterCard editMode={edit} selected={selected.has(key(entry))} onSelect={()=>toggle(entry)} key={`book-${entry.comicBook.id}`} book={entry.comicBook} watchedAt={entry.watchedAt} node={entry.node.parentNodeId!==null&&entry.node.totalComicBookCount===1?entry.node:undefined} title={entry.comicBook.displayName} count={1} series={false} root={entry.node} viewMode={viewMode} revision={coverRevision} onOpen={()=>onReadBook(entry.comicBook!)} onMenu={event=>{event.preventDefault();setBookMenu({x:event.clientX,y:event.clientY,book:entry.comicBook!});}}/>:<MediaCard editMode={edit} selected={selected.has(key(entry))} onSelect={()=>toggle(entry)} key={`node-${entry.node.id}`} node={entry.node} viewMode={viewMode} onOpen={onOpenNode} onMenu={onMenu} onBangumi={onBangumi} onRetryCover={onRetryCover} coverRevision={coverRevision} watchedAt={entry.watchedAt}/>)}
        </PosterGrid></>}
        {bookMenu&&<BookContextMenu {...bookMenu} onClose={()=>setBookMenu(null)} onReveal={onRevealBook} onRemoveRecent={onRemoveRecent?()=>void run(()=>onRemoveRecent([{kind:'BOOK',id:bookMenu.book.id}])):undefined}/>}
        {!loading && nodes.length === 0 && <EmptyState compact icon="clock" title={t('comic.recentEmpty')} description={t('comic.recentHelp')} />}
      </div>
      <ConfirmDialog open={confirmClear} title={t('recent.clearTitle')} description={t('recent.clearHelp')} busy={busy} onClose={()=>!busy&&setConfirmClear(false)} onConfirm={()=>void run(()=>onClearRecent!())}/>
    </section>
  );
}

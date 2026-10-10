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
}

export function RecentlyWatchedPage({ onReadBook,onRevealBook,entries, loading, viewMode, onViewMode, onOpenNode, onMenu, onBangumi, onRetryCover, coverRevision }: RecentlyWatchedPageProps) {
  const { number, t } = useI18n();
  const nodes = useMemo(() => entries?.map((entry) => entry.node) ?? [], [entries]);
  const [bookMenu,setBookMenu]=useState<{x:number;y:number;book:ComicBook}|null>(null);

  if (loading && !entries) return <LoadingState label={t("recent.loading")} />;

  return (
    <section className="browse-page recently-watched-page">
      <header className="page-toolbar">
        <div className="toolbar-topline">
          <span className="all-resources-location"><Icon name="clock" />{t('comic.recentTitle')}</span>
        </div>
        <div className="page-title-row">
          <div><h1>{t('comic.recentTitle')}</h1></div>
          <div className="browse-controls">
            <div className="view-toggle" aria-label={t("common.displayMode")}><button className={viewMode === "grid" ? "is-active" : ""} onClick={() => onViewMode("grid")} title={t("common.grid")} type="button"><Icon name="grid" /></button><button className={viewMode === "list" ? "is-active" : ""} onClick={() => onViewMode("list")} title={t("common.list")} type="button"><Icon name="list" /></button></div>
          </div>
        </div>
      </header>
      <div className="page-content">
        {nodes.length > 0 && <><div className="all-resources-summary"><strong>{t("recent.count", { count: number(nodes.length) })}</strong></div><PosterGrid nodes={[]} viewMode={viewMode} onOpen={onOpenNode} onMenu={onMenu} onBangumi={onBangumi} onRetryCover={onRetryCover} coverRevision={coverRevision}>
          {entries?.map(entry=>entry.comicBook?<BookPosterCard key={`book-${entry.comicBook.id}`} book={entry.comicBook} watchedAt={entry.watchedAt} node={entry.node.parentNodeId!==null&&entry.node.totalComicBookCount===1?entry.node:undefined} title={entry.comicBook.displayName} count={1} series={false} root={entry.node} viewMode={viewMode} revision={coverRevision} onOpen={()=>onReadBook(entry.comicBook!)} onMenu={event=>{event.preventDefault();setBookMenu({x:event.clientX,y:event.clientY,book:entry.comicBook!});}}/>:<MediaCard key={`node-${entry.node.id}`} node={entry.node} viewMode={viewMode} onOpen={onOpenNode} onMenu={onMenu} onBangumi={onBangumi} onRetryCover={onRetryCover} coverRevision={coverRevision} watchedAt={entry.watchedAt}/>)}
        </PosterGrid></>}
        {bookMenu&&<BookContextMenu {...bookMenu} onClose={()=>setBookMenu(null)} onReveal={onRevealBook}/>}
        {!loading && nodes.length === 0 && <EmptyState compact icon="clock" title={t('comic.recentEmpty')} description={t('comic.recentHelp')} />}
      </div>
    </section>
  );
}

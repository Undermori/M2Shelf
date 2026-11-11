import {bookCardProgress,cardProgressLabel} from '../lib/cardProgress';
import {useCollection} from '../lib/collectionState';
import {memo,useRef,useState,useEffect} from 'react';
import type {ComicBook} from '../types/comic';
import type {LibraryRoot,MediaNode,ViewMode} from '../types/media';
import {useCoverDataUrl} from '../hooks/useCoverDataUrl';
import {usePosterViewportLifecycle} from '../hooks/usePosterViewportLifecycle';
import {useI18n} from '../lib/i18n';
import {PosterImage} from './PosterImage';
import {Icon} from './Icon';
import {formatDate} from '../lib/format';

/** Typed book source; uses the same request gate, persistent tiers and image renderer as MediaCard. */
export const BookPosterCover=memo(function BookPosterCover({book,node,revision,detail=false,title}:{book:ComicBook;node?:MediaNode|null;revision:number;detail?:boolean;title:string}) {
 const {t}=useI18n();const ref=useRef<HTMLSpanElement>(null);
 const {coverRequested,coverVisible,visibleRef}=usePosterViewportLifecycle(ref,node?.id??`book-${book.id}`,{activationMarginPx:3000,retentionMarginPx:4200,retentionEnabled:true});
 const cover=useCoverDataUrl(node,revision,detail||coverRequested,node?undefined:book,detail?undefined:visibleRef);
 const [failed,setFailed]=useState(false);useEffect(()=>setFailed(false),[cover.coverUrl]);
 return <span ref={ref} className="book-poster-source"><span ref={cover.coverFrameRef} className="book-poster-image">{cover.coverUrl&&!failed?<PosterImage active={detail||coverVisible} alt={t('card.coverAlt',{title})} cacheKey={cover.coverCacheKey} src={cover.coverUrl} onError={()=>{setFailed(true);cover.onCoverError?.();}}/>:<span className="cover-placeholder"><span className="cover-art"><Icon name="book"/></span><small>{t('card.noCover')}</small></span>}</span></span>;
});
function BookSummary({sources,count}:{sources:ComicBook[];count:number}) {
 const {t}=useI18n();const collection=useCollection();
 const status=sources.length&&sources.every(b=>collection?.get({kind:'BOOK',id:b.id})==='COMPLETED')?'COMPLETED':undefined;
 const label=cardProgressLabel(t,true,bookCardProgress(sources),status);
 return <small className="card-summary" title={label||undefined}>{label&&<>{label}<span aria-hidden="true"> · </span></>}{t('comic.books',{count})}</small>;
}
export function BookPosterCard({book,books,node,title,count,series,root,viewMode,revision,onOpen,onMenu,editMode=false,selected=false,onSelect,watchedAt}:{book:ComicBook;books?:ComicBook[];node?:MediaNode;title:string;count:number;series:boolean;root:Pick<LibraryRoot,'mediaKind'>;viewMode:ViewMode;revision:number;onOpen:()=>void;onMenu:(event:React.MouseEvent)=>void;editMode?:boolean;selected?:boolean;onSelect?:()=>void;watchedAt?:string}) {
 const {t}=useI18n();const sources=books??(count===1?[book]:[]);const badge=t('comic.badge',{media:t(root.mediaKind==='EBOOK'?'ebook.name':root.mediaKind==='DOUJIN'?'doujin.name':root.mediaKind==='ARTBOOK'?'artbook.name':'comic.name'),structure:t(series?'card.systemSeries':'card.systemWork')});
 return <article className={`media-card media-card-${viewMode}${watchedAt?' has-watch-time':''}${editMode?' is-edit-mode':''}${selected?' is-selected':''}`} onContextMenu={event=>{event.preventDefault();onMenu(event);}}>
  <button className="media-card-open" type="button" disabled={editMode&&!onSelect} aria-pressed={editMode?selected:undefined} onClick={()=>editMode?onSelect?.():onOpen()}>
   <span className="cover-frame"><BookPosterCover book={book} node={node} revision={revision} title={title}/>{viewMode==='grid'&&<span className="type-pill system-tag">{badge}</span>}{editMode&&<span aria-hidden="true" className="selection-indicator"><Icon name={selected?'check':'plus'}/></span>}</span>
   <span className="media-card-copy"><strong title={title}>{title}</strong>{watchedAt&&<time className="media-card-watch-time" dateTime={watchedAt}>{t('comic.openedAt',{time:formatDate(watchedAt)})}</time>}<span className="media-card-meta">{viewMode==='list'&&<span className="type-pill system-tag">{badge}</span>}<BookSummary sources={sources} count={count}/></span></span>
  </button>
  {!editMode&&<button className="card-menu" aria-label={t('card.moreActions',{title})} type="button" onClick={onMenu}><Icon name="more"/></button>}
 </article>;
}

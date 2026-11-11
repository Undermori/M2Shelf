import {useEffect,useLayoutEffect,useMemo,useRef,useState,useSyncExternalStore} from 'react';
import {Icon} from './Icon';
import {PageThumbnailCache,THUMBNAIL_LIMITS,type ThumbnailSource} from '../lib/pageThumbnails';
import '../styles/page-thumbnails.css';

export interface PageThumbnailLabels {title:string;close:string;page:(number:number)=>string;loading:string;failed:string;retry:string;current:string}
export interface ReaderPageThumbnailsProps {identity:string;count:number;currentPage:number;currentRange?:readonly number[];source:ThumbnailSource;onJump:(index:number)=>void;onClose:()=>void;labels:PageThumbnailLabels}
const ROW_HEIGHT=210;
export function ReaderPageThumbnails({identity,count,currentPage,currentRange,source,onJump,onClose,labels}:ReaderPageThumbnailsProps){
 const cache=useMemo(()=>new PageThumbnailCache(identity,count,source),[identity,count,source]);
 const version=useSyncExternalStore(cache.subscribe,cache.snapshot,cache.snapshot);void version;
 const viewport=useRef<HTMLDivElement>(null),panel=useRef<HTMLElement>(null);const [size,setSize]=useState({width:280,height:420});const [top,setTop]=useState(Math.max(0,Math.floor(currentPage/2)*ROW_HEIGHT-100));
 const columns=size.width>=236?2:1;const rows=Math.ceil(count/columns),startRow=Math.max(0,Math.floor(top/ROW_HEIGHT)-1),endRow=Math.min(rows-1,Math.ceil((top+size.height)/ROW_HEIGHT)+1);
 const shown=Array.from({length:Math.max(0,Math.min(THUMBNAIL_LIMITS.demand,(endRow-startRow+1)*columns,count-startRow*columns))},(_,offset)=>startRow*columns+offset);const shownKey=shown.join(',');
 const selected=new Set(currentRange??[currentPage]);const stats=cache.stats;
 useEffect(()=>()=>cache.dispose(),[cache]);
 useLayoutEffect(()=>{const element=viewport.current;if(!element)return;const measure=()=>setSize({width:element.clientWidth||280,height:element.clientHeight||420});measure();if(typeof ResizeObserver==='undefined')return;const observer=new ResizeObserver(measure);observer.observe(element);return()=>observer.disconnect();},[]);
 useLayoutEffect(()=>{const element=viewport.current;if(!element)return;const row=Math.floor(Math.max(0,currentPage)/columns),target=row*ROW_HEIGHT;const next=target<element.scrollTop||target+ROW_HEIGHT>element.scrollTop+size.height?Math.max(0,target-Math.floor(size.height/3)):element.scrollTop;element.scrollTop=next;setTop(next);},[identity,currentPage,columns,size.height]);
 useEffect(()=>{const ordered=[...shown].sort((a,b)=>Math.abs(a-currentPage)-Math.abs(b-currentPage));cache.demand(ordered);for(const index of ordered)void cache.load(index);},[cache,shownKey,currentPage]);
 const focusPage=(index:number)=>{const next=Math.max(0,Math.min(count-1,index)),row=Math.floor(next/columns);if(viewport.current){viewport.current.scrollTop=Math.max(0,row*ROW_HEIGHT-size.height/3);setTop(viewport.current.scrollTop);}requestAnimationFrame(()=>panel.current?.querySelector<HTMLButtonElement>(`[data-thumbnail-index="${next}"]`)?.focus({preventScroll:true}));};
 return <aside ref={panel} className="reader-page-thumbnails" aria-label={labels.title} data-thumbnail-identity={identity} data-thumbnail-entries={stats.entries} data-thumbnail-bytes={stats.bytes} data-thumbnail-active={stats.active} data-thumbnail-queued={stats.queued} data-thumbnail-requests={stats.requests} data-thumbnail-peak-active={stats.peakActive} data-thumbnail-peak-bytes={stats.peakBytes} data-thumbnail-peak-entries={stats.peakEntries} data-thumbnail-failed={stats.failed} data-thumbnail-completed={stats.completed} onKeyDown={event=>{if(event.key==='Escape'){event.preventDefault();event.stopPropagation();onClose();return;}const button=(event.target as HTMLElement).closest<HTMLElement>('[data-thumbnail-index]');if(!button)return;const index=Number(button.dataset.thumbnailIndex);const step=event.key==='ArrowRight'?1:event.key==='ArrowLeft'?-1:event.key==='ArrowDown'?columns:event.key==='ArrowUp'?-columns:0;if(step||event.key==='Home'||event.key==='End'){event.preventDefault();event.stopPropagation();focusPage(event.key==='Home'?0:event.key==='End'?count-1:index+step);}}}>
  <div className="reader-page-thumbnails-heading"><strong>{labels.title}</strong><button className="icon-button" type="button" aria-label={labels.close} onClick={onClose}><Icon name="close"/></button></div>
  <div ref={viewport} className="reader-page-thumbnails-scroll" onScroll={event=>setTop(event.currentTarget.scrollTop)}>
   <div className="reader-page-thumbnails-space" style={{height:rows*ROW_HEIGHT}}>
    {shown.map(index=>{const entry=cache.peek(index);return <div className={`reader-thumbnail-cell${selected.has(index)?' is-current':''}`} key={index} style={{top:Math.floor(index/columns)*ROW_HEIGHT,left:`${index%columns/columns*100}%`,width:`${100/columns}%`}}>
     <button className="reader-thumbnail" data-thumbnail-index={index} aria-current={index===currentPage?'page':undefined} aria-label={`${labels.page(index+1)}${selected.has(index)?` · ${labels.current}`:''}`} onClick={()=>onJump(index)} type="button"><span className="reader-thumbnail-art">{entry?.status==='ready'?<img src={entry.url} alt="" width={entry.width} height={entry.height} onError={()=>cache.fail(index)}/>:<span className="reader-thumbnail-placeholder"><Icon name={entry?.status==='error'?'image':'file'}/><span>{entry?.status==='error'?labels.failed:labels.loading}</span></span>}</span><span className="reader-thumbnail-number">{labels.page(index+1)}</span></button>
     {entry?.status==='error'&&<button className="reader-thumbnail-retry" type="button" aria-label={`${labels.retry} · ${labels.page(index+1)}`} title={labels.retry} onClick={()=>void cache.retry(index)}><Icon name="refresh"/></button>}
    </div>;})}
   </div>
  </div>
 </aside>;
}

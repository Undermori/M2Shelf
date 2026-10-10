import {Select} from '../components/Select';
import {BookmarkSelect} from '../components/BookmarkSelect';
import {useCallback,useEffect,useLayoutEffect,useMemo,useRef,useState} from 'react';
import type {ComicOpenResult,ComicReaderSettings} from '../types/comic';
import {defaultComicReaderSettings} from '../types/comic';
import {api} from '../lib/api';
import {ComicPageCache,comicSpreads,readerKeyStep} from '../lib/comicReader';
import {useI18n} from '../lib/i18n';
import {errorMessage} from '../lib/format';
import {Icon} from '../components/Icon';
import {DocumentReaderPage} from './DocumentReaderPage';
import type {ReaderMenuActions} from '../types/readerMenu';

export function ComicReaderPage({bookId,initialPage,onBack,onProgress,onMenuActions}:{bookId:number;initialPage?:number;onBack:()=>void;onProgress:()=>void;onMenuActions?:(actions:ReaderMenuActions|null)=>void}){
 const {t}=useI18n();const [opened,setOpened]=useState<ComicOpenResult|null>(null);const [page,setPage]=useState(0);
 const [settings,setSettings]=useState<ComicReaderSettings>(defaultComicReaderSettings);const [fit,setFit]=useState<'PAGE'|'WIDTH'|'NATIVE'>('PAGE');const [zoom,setZoom]=useState(1);const [background,setBackground]=useState('GRAY');
 const [revision,setRevision]=useState(0);const [error,setError]=useState('');const [panel,setPanel]=useState<'SETTINGS'|null>(null);const [bookmarks,setBookmarks]=useState<number[]>([]);const [bookmarkBusy,setBookmarkBusy]=useState(false);
 const [viewport,setViewport]=useState({width:900,height:640,top:0});const [retry,setRetry]=useState(0);
 const viewportRef=useRef<HTMLDivElement>(null);const rootRef=useRef<HTMLElement>(null);const cacheRef=useRef<ComicPageCache|null>(null);const generation=useRef(0);const desiredScroll=useRef<number|null>(null);const anchor=useRef({index:0,offset:0});
 const visiblePages=useRef(new Set<number>());
 const pendingProgress=useRef<number|null>(null);const saveTimer=useRef<number|null>(null);const saveChain=useRef<Promise<void>>(Promise.resolve());const alive=useRef(true);const progressCallback=useRef(onProgress);progressCallback.current=onProgress;
 const flush=useCallback(()=>{
   if(saveTimer.current!==null){window.clearTimeout(saveTimer.current);saveTimer.current=null;}
   const value=pendingProgress.current;if(value===null)return saveChain.current;pendingProgress.current=null;
   // Serialize progress writes; a delayed earlier write cannot overwrite a later position.
   saveChain.current=saveChain.current.catch(()=>undefined).then(()=>api.updateComicProgress(bookId,value,opened?.book.revision)).then(()=>progressCallback.current()).catch(e=>{if(alive.current)setError(errorMessage(e));});
   return saveChain.current;
 },[bookId,opened?.book.revision]);
 useEffect(()=>{alive.current=true;return()=>{alive.current=false;void flush();};},[flush]);
 useEffect(()=>{
   const token=++generation.current;let active=true;setOpened(null);setError('');
   void Promise.all([api.openComicBook(bookId),api.getSettings()]).then(([result,app])=>{
     if(!active||token!==generation.current)return;
     const start=Math.max(0,Math.min(result.pages.length-1,initialPage??result.book.progress?.lastPageIndex??0));
     setOpened(result);setPage(start);setBookmarks(result.bookmarks);setSettings(app.comicReader??defaultComicReaderSettings);desiredScroll.current=start;anchor.current={index:start,offset:0};
     if(!result.book.documentFormat)cacheRef.current=new ComicPageCache(bookId,result.pages,()=>{if(active&&token===generation.current)setRevision(v=>v+1);},(id,index)=>api.readComicPage(id,index,result.book.revision));setRevision(v=>v+1);
   }).catch(e=>{if(active)setError(errorMessage(e));});
   return()=>{active=false;++generation.current;cacheRef.current?.dispose();cacheRef.current=null;};
 },[bookId,initialPage,retry]);
 useEffect(()=>{const element=viewportRef.current;if(!element)return;const update=()=>setViewport(v=>({...v,width:element.clientWidth||900,height:element.clientHeight||640}));update();if(typeof ResizeObserver==='undefined')return;const observer=new ResizeObserver(update);observer.observe(element);return()=>observer.disconnect();},[opened]);
 const count=opened?.pages.length??0;const cache=cacheRef.current;const paged=settings.mode==='PAGED';
 const spreads=useMemo(()=>comicSpreads(count,settings.layout==='DOUBLE',settings.widePageAlone,cache?.sizes??new Map()),[count,settings.layout,settings.widePageAlone,cache,revision]);
 const spreadIndex=Math.max(0,spreads.findIndex(s=>s.includes(page)));const currentSpread=spreads[spreadIndex]??[page];
 const offsets=useMemo(()=>{
   const result=[0];for(let i=0;i<count;i++){
     const size=cache?.sizes.get(i);const natural=fit==='NATIVE'&&size?size.width:viewport.width;
     const width=Math.max(1,(fit==='PAGE'&&settings.mode!=='WEBTOON'?Math.min(natural,viewport.height*(size?size.width/size.height:0.67)):natural)*zoom);
     const height=width*(size?size.height/size.width:1.5)+(settings.mode==='WEBTOON'?0:16);result.push(result[i]+height);
   }return result;
 },[cache,count,revision,viewport.width,viewport.height,fit,zoom,settings.mode]);
 const atOffset=useCallback((value:number)=>{let lo=0,hi=count;while(lo<hi){const mid=Math.floor((lo+hi)/2);if(offsets[mid+1]<=value)lo=mid+1;else hi=mid;}return Math.min(Math.max(0,count-1),lo);},[count,offsets]);
 const visibleStart=atOffset(viewport.top);const visibleEnd=atOffset(viewport.top+viewport.height);
 const shownStart=Math.max(0,visibleStart-2);
 const shown=paged?currentSpread:Array.from({length:Math.max(0,Math.min(count-1,visibleEnd+2,shownStart+11)-shownStart+1)},(_,i)=>shownStart+i);
 const shownKey=shown.join(',');
 useEffect(()=>{
   if(!cache||!count)return;
   const protectedPages=new Set(paged?currentSpread:shown.filter(i=>i>=visibleStart&&i<=visibleEnd));visiblePages.current=protectedPages;cache.retain(protectedPages);
   const preheat=paged?Array.from({length:Math.min(count-1,currentSpread[currentSpread.length-1]+3)-Math.max(0,page-2)+1},(_,i)=>Math.max(0,page-2)+i):shown;
   const token=generation.current;
   const demand=[...new Set([...shown,...preheat])];cache.demand(new Set(demand));
   // Demand pages precede speculative neighbors in the two-request queue.
   for(const index of demand)void cache.load(index).catch(e=>{if(token===generation.current&&visiblePages.current.has(index))setError(String(e).includes('LIMIT')?t('comic.limit'):e instanceof Error&&e.name==='M2ShelfError'?errorMessage(e):t('comic.error'));});
 // Cache callbacks change revision; loading depends only on the requested window, not its completions.
 // eslint-disable-next-line react-hooks/exhaustive-deps
 },[cache,count,page,shownKey,paged,visibleStart,visibleEnd,t]);
 useLayoutEffect(()=>{
   if(paged||!count||!viewportRef.current)return;
   const target=desiredScroll.current;
   if(target!==null){viewportRef.current.scrollTop=offsets[target];desiredScroll.current=null;anchor.current={index:target,offset:0};setViewport(v=>({...v,top:offsets[target]}));}
   else{const next=offsets[anchor.current.index]+anchor.current.offset;if(Math.abs(viewportRef.current.scrollTop-next)>1){viewportRef.current.scrollTop=next;setViewport(v=>({...v,top:next}));}}
 },[offsets,paged,count]);
 useEffect(()=>{
   const index=paged?page:atOffset(viewport.top+Math.min(120,viewport.height*0.25));
   if(!cache?.peek(index))return;
   const frame=requestAnimationFrame(()=>{pendingProgress.current=index;if(saveTimer.current===null)saveTimer.current=window.setTimeout(()=>void flush(),700);});
   return()=>cancelAnimationFrame(frame);
 },[cache,page,paged,revision,viewport.top,viewport.height,atOffset,flush]);
 const jump=useCallback((index:number)=>{setError('');const target=Math.max(0,Math.min(count-1,index));setPage(target);if(!paged){desiredScroll.current=target;const top=offsets[target];if(viewportRef.current)viewportRef.current.scrollTop=top;anchor.current={index:target,offset:0};setViewport(v=>({...v,top}));}},[count,offsets,paged]);
 const turn=useCallback(async(delta:number)=>{
   if(!paged){viewportRef.current?.scrollBy({top:delta*viewport.height*0.85,behavior:'smooth'});return;}
   if(!cache)return;const token=generation.current;
   try{await Promise.all([cache.load(page),cache.load(page+1)]);if(token!==generation.current)return;const current=comicSpreads(count,settings.layout==='DOUBLE',settings.widePageAlone,cache.sizes);const i=Math.max(0,current.findIndex(s=>s.includes(page)));jump(current[Math.max(0,Math.min(current.length-1,i+delta))][0]);}catch(e){if(token===generation.current)setError(errorMessage(e));}
 },[paged,viewport.height,cache,page,count,settings.layout,settings.widePageAlone,jump]);
 const toggleBookmark=useCallback(async()=>{if(bookmarkBusy||!cache?.peek(page))return;setBookmarkBusy(true);const token=generation.current;try{const result=await (bookmarks.includes(page)?api.removeComicBookmark(bookId,page,opened?.book.revision):api.addComicBookmark(bookId,page,opened?.book.revision));if(token===generation.current)setBookmarks(result);}catch(e){if(token===generation.current)setError(errorMessage(e));}finally{if(token===generation.current)setBookmarkBusy(false);}},[bookId,bookmarks,page,bookmarkBusy,cache]);
 const fullscreen=useCallback(async()=>{try{if(document.fullscreenElement)await document.exitFullscreen();else await rootRef.current?.requestFullscreen();}catch(e){setError(errorMessage(e));}},[]);
 const exit=useCallback(async()=>{await flush();if(document.fullscreenElement)await document.exitFullscreen().catch(()=>undefined);onBack();},[flush,onBack]);
 useEffect(()=>{if(!opened||opened.book.documentFormat)return;onMenuActions?.({previous:()=>void turn(-1),next:()=>void turn(1),fullscreen:()=>void fullscreen(),bookmark:()=>void toggleBookmark(),canPrevious:!!count&&(!paged||page>0),canNext:!!count&&(!paged||spreadIndex<spreads.length-1),canBookmark:!bookmarkBusy&&!!cache?.peek(page)});},[opened,onMenuActions,turn,fullscreen,toggleBookmark,count,paged,page,spreadIndex,spreads.length,bookmarkBusy,cache,revision]);
 useEffect(()=>()=>onMenuActions?.(null),[onMenuActions]);
 useEffect(()=>{const key=(event:KeyboardEvent)=>{
   if(opened?.book.documentFormat)return;
   if(event.target instanceof HTMLElement&&event.target.closest('[role=menu]'))return;
   if(event.target instanceof HTMLElement&&(event.target.isContentEditable||event.target.closest('input,textarea,select,[role=combobox],[role=listbox]')))return;
   if(event.altKey||event.ctrlKey||event.metaKey)return;
   const step=readerKeyStep(event.key,settings.direction==='RTL');if(step){event.preventDefault();void turn(step);return;}
   if(event.key==='Home'||event.key==='End'){event.preventDefault();jump(event.key==='Home'?0:count-1);}
   else if(event.key.toLowerCase()==='f'){event.preventDefault();void fullscreen();}
   else if(event.key.toLowerCase()==='b'){event.preventDefault();void toggleBookmark();}
   else if(event.key==='Escape'){event.preventDefault();event.stopImmediatePropagation();if(document.fullscreenElement)void document.exitFullscreen();else if(panel)setPanel(null);else void exit();}
 };window.addEventListener('keydown',key,true);return()=>window.removeEventListener('keydown',key,true);},[count,exit,fullscreen,jump,panel,settings.direction,toggleBookmark,turn,opened?.book.documentFormat]);
 const touchStart=useRef<{x:number;y:number}|null>(null);
 const visual=paged&&settings.direction==='RTL'?[...shown].reverse():shown;
 const pageFrames=visual.map(index=>{
   const size=cache?.sizes.get(index)??{width:600,height:900};
   const scale=(fit==='NATIVE'?1:fit==='WIDTH'?viewport.width/Math.max(1,visual.length)/size.width:Math.min(viewport.height/size.height,viewport.width/Math.max(1,visual.length)/size.width))*zoom;
   return {index,width:size.width*scale,height:size.height*scale};
 });
 const settingSelect=(label:'comic.direction'|'comic.layout'|'comic.mode',field:'direction'|'layout'|'mode',options:[string,'comic.rtl'|'comic.ltr'|'comic.single'|'comic.double'|'comic.paged'|'comic.scroll'|'comic.webtoon'][])=> <label><span>{t(label)}</span><Select value={settings[field]} onChange={e=>{if(field==='mode')desiredScroll.current=page;setSettings(v=>({...v,[field]:e.target.value}));}}>{options.map(([value,key])=><option value={value} key={value}>{t(key)}</option>)}</Select></label>;
 if(opened?.book.documentFormat)return <DocumentReaderPage opened={opened} initialPage={initialPage} onBack={onBack} onProgress={onProgress} onMenuActions={onMenuActions}/>;
 return <section className={`comic-reader controls-visible reader-bg-${background.toLowerCase()}`} aria-label={opened?.book.displayName??t('comic.loading')} ref={rootRef}>
 <header className="reader-topbar"><button className="button secondary" onClick={()=>void exit()} type="button"><Icon name="arrow-left"/>{t('comic.back')}</button><strong>{opened?.book.displayName??t('comic.loading')}</strong><button className="icon-button" type="button" onClick={()=>setPanel(panel==='SETTINGS'?null:'SETTINGS')} title={t('comic.controls')} aria-label={t('comic.controls')}><Icon name="settings"/></button><button className="button secondary" type="button" onClick={()=>void fullscreen()}>{t('comic.fullscreen')}</button></header>
 <div className={`reader-viewport ${paged?'is-paged':'is-scroll'} fit-${fit.toLowerCase()}`} ref={viewportRef} tabIndex={0} onScroll={e=>{const top=e.currentTarget.scrollTop;anchor.current={index:atOffset(top),offset:top-offsets[atOffset(top)]};setViewport(v=>({...v,top}));if(!paged)setPage(atOffset(top+Math.min(120,viewport.height*0.25)));}} onTouchStart={e=>{const p=e.touches[0];touchStart.current={x:p.clientX,y:p.clientY};}} onTouchEnd={e=>{const start=touchStart.current;touchStart.current=null;if(!paged||!start)return;const p=e.changedTouches[0],dx=p.clientX-start.x,dy=p.clientY-start.y;if(Math.abs(dx)>60&&Math.abs(dx)>Math.abs(dy)*1.5)void turn((dx<0?1:-1)*(settings.direction==='RTL'?-1:1));}}>
 {!opened&&!error&&<p className="reader-loading">{t('comic.loading')}</p>}
 {paged?<div className={`reader-spread spread-${visual.length}`} style={{width:Math.max(viewport.width,pageFrames.reduce((sum,frame)=>sum+frame.width,0)),height:Math.max(viewport.height,...pageFrames.map(frame=>frame.height))}}>{pageFrames.map(({index,width,height})=>{const entry=cache?.peek(index);return <figure key={index} style={{width,height}}>{entry?<img src={entry.url} alt={t('comic.progress',{page:index+1,count})} style={{width,height}}/>:<span>{t('comic.loading')}</span>}</figure>;})}</div>:<div className={`reader-scroll-pages ${settings.mode==='WEBTOON'?'is-webtoon':''}`} style={{height:offsets[count]??0,minWidth:fit==='NATIVE'?Math.max(viewport.width,...shown.map(i=>(cache?.sizes.get(i)?.width??0)*zoom)):viewport.width*zoom}}>{shown.map(index=>{const entry=cache?.peek(index);return <figure key={index} style={{top:offsets[index],height:offsets[index+1]-offsets[index]}}>{entry&&<img src={entry.url} alt={t('comic.progress',{page:index+1,count})} style={{height:'100%',width:fit==='NATIVE'?entry.width*zoom:undefined}}/>}</figure>;})}</div>}
 </div>
 {error&&<div className="reader-error" role="alert"><p>{error}</p><button className="button secondary" onClick={()=>{void flush();setRetry(v=>v+1);}} type="button">{t('comic.retry')}</button></div>}
 <footer className="reader-bottombar"><button className="button secondary" type="button" disabled={!count||page===0} onClick={()=>void turn(-1)}>{t('comic.previous')}</button><span>{t('comic.progress',{page:count?page+1:0,count})}</span><input aria-label={t('comic.jump')} type="number" min={1} max={count||1} value={count?page+1:1} onChange={e=>{const value=Number(e.target.value);if(value>=1&&value<=count)jump(value-1);}}/><button className="button secondary" type="button" disabled={!count||page>=count-1} onClick={()=>void turn(1)}>{t('comic.next')}</button><button className="icon-button" aria-label={t(bookmarks.includes(page)?'comic.removeBookmark':'comic.addBookmark')} type="button" aria-pressed={bookmarks.includes(page)} disabled={bookmarkBusy||!cache?.peek(page)} onClick={()=>void toggleBookmark()}><Icon name="bookmark"/></button><BookmarkSelect bookmarks={bookmarks} count={count} onJump={jump}/></footer>
 {panel&&<aside className="reader-panel" aria-label={t('comic.controls')}><div className="reader-panel-heading"><strong>{t('comic.controls')}</strong><button className="icon-button" aria-label={t('comic.close')} onClick={()=>setPanel(null)} type="button"><Icon name="close"/></button></div>
 <>
 {settingSelect('comic.direction','direction',[['RTL','comic.rtl'],['LTR','comic.ltr']])}{settingSelect('comic.layout','layout',[['SINGLE','comic.single'],['DOUBLE','comic.double']])}{settingSelect('comic.mode','mode',[['PAGED','comic.paged'],['SCROLL','comic.scroll'],['WEBTOON','comic.webtoon']])}
 <label><span>{t('comic.fit')}</span><Select value={fit} onChange={e=>setFit(e.target.value as typeof fit)}><option value="PAGE">{t('comic.fitPage')}</option><option value="WIDTH">{t('comic.fitWidth')}</option><option value="NATIVE">{t('comic.native')}</option></Select></label>
 <label><span>{t('comic.zoom')}</span><input type="range" min={0.25} max={3} step={0.05} value={zoom} onChange={e=>setZoom(Number(e.target.value))}/></label><button className="button secondary" type="button" onClick={()=>setZoom(1)}>{t('comic.reset')}</button>
 <label><span>{t('comic.background')}</span><Select value={background} onChange={e=>setBackground(e.target.value)}><option value="BLACK">{t('comic.black')}</option><option value="GRAY">{t('comic.gray')}</option><option value="WHITE">{t('comic.white')}</option></Select></label><label className="switch-field settings-switch-row"><span>{t('comic.wideAlone')}</span><input type="checkbox" checked={settings.widePageAlone} onChange={e=>setSettings(s=>({...s,widePageAlone:e.target.checked}))}/><i/></label>
 </></aside>}
 </section>;
}

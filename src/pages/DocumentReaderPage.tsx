import {BookmarkSelect} from '../components/BookmarkSelect';
import {EpubContent,isLocalBookImage,type EpubBlock} from '../components/EpubContent';
import {useCallback,useEffect,useRef,useState} from 'react';
import type {PDFDocumentProxy,RenderTask} from 'pdfjs-dist';
import type {ComicOpenResult} from '../types/comic';
import {api} from '../lib/api';
import {errorMessage} from '../lib/format';
import {useI18n} from '../lib/i18n';
import {Icon} from '../components/Icon';
type Block=EpubBlock;

/** PDF pages and safe EPUB chapter blocks share application-owned progress and bookmarks. */
export function DocumentReaderPage({opened,initialPage,onBack,onProgress}:{opened:ComicOpenResult;initialPage?:number;onBack:()=>void;onProgress:()=>void}){
 const {t}=useI18n();const {book}=opened;const count=book.pageCount;const epub=book.documentFormat==='EPUB';
 const documentError=useCallback((value:unknown)=>value instanceof Error&&value.name==='M2ShelfError'?errorMessage(value):t('comic.error'),[t]);
 const [page,setPage]=useState(Math.max(0,Math.min(count-1,initialPage??book.progress?.lastPageIndex??0)));
 const [zoom,setZoom]=useState(1);const [fit,setFit]=useState<'PAGE'|'WIDTH'|'NATIVE'>('PAGE');const [panel,setPanel]=useState<'SETTINGS'|null>(null);const [background,setBackground]=useState('GRAY');const [error,setError]=useState('');const [readyPage,setReadyPage]=useState<number|null>(null);const [pdfReady,setPdfReady]=useState(false);
 const [blocks,setBlocks]=useState<Block[]>([]);const [bookmarks,setBookmarks]=useState(opened.bookmarks);const [bookmarkBusy,setBookmarkBusy]=useState(false);
 const canvas=useRef<HTMLCanvasElement>(null);const root=useRef<HTMLElement>(null);const viewport=useRef<HTMLDivElement>(null);const pdf=useRef<PDFDocumentProxy|null>(null);const renderTask=useRef<RenderTask|null>(null);const renderChain=useRef(Promise.resolve());
 const [size,setSize]=useState({width:900,height:600});const generation=useRef(0);const pendingProgress=useRef<number|null>(null);const timer=useRef<number|null>(null);const saveChain=useRef(Promise.resolve());const alive=useRef(true);const onProgressRef=useRef(onProgress);onProgressRef.current=onProgress;
 const renderWidth=epub?0:size.width;const renderHeight=epub?0:size.height;
 const pdfZoom=epub?1:zoom;const pdfFit=epub?'PAGE':fit;const chapterReads=useRef(Promise.resolve());
 const flush=useCallback(()=>{if(timer.current!==null){clearTimeout(timer.current);timer.current=null;}const index=pendingProgress.current;pendingProgress.current=null;if(index!==null)saveChain.current=saveChain.current.catch(()=>undefined).then(()=>api.updateComicProgress(book.id,index,book.revision)).then(()=>onProgressRef.current()).catch(e=>{if(alive.current)setError(errorMessage(e));});return saveChain.current;},[book.id,book.revision]);
 useEffect(()=>{alive.current=true;return()=>{alive.current=false;void flush();};},[flush]);
 useEffect(()=>{if(!viewport.current)return;const element=viewport.current;const update=()=>setSize({width:element.clientWidth||900,height:element.clientHeight||600});update();const observer=new ResizeObserver(update);observer.observe(element);return()=>observer.disconnect();},[]);
 useEffect(()=>{
  if(epub)return;let active=true;let destroy:(()=>Promise<void>)|undefined;
  void (async()=>{
   const module=await import('pdfjs-dist');const {default:workerUrl}=await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
   module.GlobalWorkerOptions.workerSrc=workerUrl;
   let range:InstanceType<typeof module.PDFDataRangeTransport>|undefined;let data:Uint8Array|undefined;
   if(book.sourceSize){
    const length=book.sourceSize;const initial=new Uint8Array(await api.readPdfRange(book.id,0,Math.min(length,65536),book.revision));if(!active)return;
    class NativePdfTransport extends module.PDFDataRangeTransport {
     private stopped=false;private chain=Promise.resolve();
     override requestDataRange(begin:number,end:number){
      this.chain=this.chain.then(async()=>{for(let offset=begin;offset<end&&!this.stopped&&active;){const next=Math.min(end,offset+2*1024*1024);const bytes=new Uint8Array(await api.readPdfRange(book.id,offset,next,book.revision));if(!this.stopped&&active)this.onDataRange(offset,bytes);offset=next;}}).catch(e=>{if(!this.stopped&&active){setError(documentError(e));void destroy?.();}});
     }
     override abort(){this.stopped=true;}
    }
    range=new NativePdfTransport(length,initial,true);
   }else{data=new Uint8Array(await api.readBookDocument(book.id,0,book.revision));if(!active)return;}
   const loading=module.getDocument({data,range,rangeChunkSize:256*1024,useSystemFonts:true,disableAutoFetch:true,disableStream:true,maxImageSize:40_000_000,canvasMaxAreaInBytes:64*1024*1024,cMapUrl:new URL('/pdf-resources/cmaps/',window.location.href).href,cMapPacked:true,wasmUrl:new URL('/pdf-resources/wasm/',window.location.href).href});
   destroy=()=>loading.destroy();const document=await loading.promise;
   if(!active){await loading.destroy();return;}
   if(document.numPages!==count)throw new Error('COMIC_PAGE_CHANGED');pdf.current=document;setPdfReady(true);
  })().catch(e=>{if(active)setError(documentError(e));});
  return()=>{active=false;renderTask.current?.cancel();pdf.current=null;void destroy?.();};
 },[book.id,book.revision,book.sourceSize,count,epub,documentError]);
 useEffect(()=>{
  const token=++generation.current;let active=true;setReadyPage(null);viewport.current?.scrollTo?.({top:0});
  if(epub){setError('');setBlocks([]);chapterReads.current=chapterReads.current.catch(()=>undefined).then(async()=>{if(!active)return;const bytes=await api.readBookDocument(book.id,page,book.revision);if(!active)return;
   const chapter:Block[]=JSON.parse(new TextDecoder().decode(bytes));
   await Promise.all(chapter.filter((b):b is Extract<Block,{kind:'image'}>=>b.kind==='image'&&isLocalBookImage(b.data_url)).map(async b=>{const image=new Image();image.src=b.data_url;await image.decode();}));
   if(active&&token===generation.current){setBlocks(chapter);setReadyPage(page);}
  }).catch(e=>{if(active)setError(documentError(e));});return()=>{active=false;};}
  if(!pdfReady||!pdf.current)return;
  setError('');
  renderTask.current?.cancel();
  renderChain.current=renderChain.current.catch(()=>undefined).then(async()=>{
   const document=pdf.current;if(!active||!document||!canvas.current)return;
   const source=await document.getPage(page+1);if(!active){source.cleanup();return;}
   const natural=source.getViewport({scale:1});const dpr=Math.min(2,window.devicePixelRatio||1);
   const availableWidth=Math.max(1,size.width-48);const availableHeight=Math.max(1,size.height-48);
   const fitted=fit==='NATIVE'?1:fit==='WIDTH'?availableWidth/natural.width:Math.min(availableWidth/natural.width,availableHeight/natural.height);
   const scale=Math.min(fitted*zoom,Math.sqrt(16_000_000/(natural.width*natural.height))/dpr);
   const view=source.getViewport({scale:scale*dpr});const target=canvas.current;const context=target.getContext('2d');if(!context)throw new Error(t('comic.error'));
   target.width=Math.ceil(view.width);target.height=Math.ceil(view.height);target.style.width=`${view.width/dpr}px`;target.style.height=`${view.height/dpr}px`;
   try{const task=source.render({canvas:target,canvasContext:context,viewport:view});renderTask.current=task;await task.promise;if(active&&token===generation.current)setReadyPage(page);}finally{source.cleanup();}
  }).catch(e=>{if(active&&e?.name!=='RenderingCancelledException')setError(documentError(e));});
  return()=>{active=false;renderTask.current?.cancel();};
 },[book.id,book.revision,epub,page,pdfReady,renderWidth,renderHeight,pdfFit,pdfZoom,t,documentError]);
 useEffect(()=>{if(readyPage!==page)return;const frame=requestAnimationFrame(()=>{pendingProgress.current=page;if(timer.current===null)timer.current=window.setTimeout(()=>void flush(),700);});return()=>cancelAnimationFrame(frame);},[page,readyPage,flush]);
 const jump=useCallback((index:number)=>setPage(Math.max(0,Math.min(count-1,index))),[count]);
 const fullscreen=useCallback(async()=>{try{if(document.fullscreenElement)await document.exitFullscreen();else await root.current?.requestFullscreen();}catch(e){setError(errorMessage(e));}},[]);
 const exit=useCallback(async()=>{await flush();if(document.fullscreenElement)await document.exitFullscreen().catch(()=>undefined);onBack();},[flush,onBack]);
 const bookmark=useCallback(async()=>{if(bookmarkBusy||readyPage!==page)return;setBookmarkBusy(true);try{setBookmarks(await (bookmarks.includes(page)?api.removeComicBookmark(book.id,page,book.revision):api.addComicBookmark(book.id,page,book.revision)));}catch(e){setError(errorMessage(e));}finally{setBookmarkBusy(false);}},[book.id,book.revision,page,readyPage,bookmarks,bookmarkBusy]);
 useEffect(()=>{const handler=(e:KeyboardEvent)=>{if(e.altKey||e.ctrlKey||e.metaKey||e.target instanceof HTMLElement&&e.target.closest('input,select,textarea'))return;if(['ArrowRight','PageDown',' '].includes(e.key)){e.preventDefault();jump(page+1);}else if(['ArrowLeft','PageUp'].includes(e.key)){e.preventDefault();jump(page-1);}else if(e.key==='Home')jump(0);else if(e.key==='End')jump(count-1);else if(e.key.toLowerCase()==='f')void fullscreen();else if(e.key.toLowerCase()==='b')void bookmark();else if(e.key==='Escape'){e.preventDefault();e.stopImmediatePropagation();if(document.fullscreenElement)void document.exitFullscreen();else void exit();}};window.addEventListener('keydown',handler,true);return()=>window.removeEventListener('keydown',handler,true);},[page,count,jump,fullscreen,bookmark,exit]);
 const progress=t(epub?'ebook.chapterProgress':'comic.progress',{page:page+1,count});
 return <section ref={root} className={`comic-reader document-reader controls-visible reader-bg-${background.toLowerCase()}`} aria-label={book.displayName}>
  <header className="reader-topbar"><button className="button secondary" onClick={()=>void exit()} type="button"><Icon name="arrow-left"/>{t('comic.back')}</button><strong>{book.displayName}</strong><button className="icon-button" type="button" aria-label={t('comic.controls')} aria-expanded={panel==='SETTINGS'} onClick={()=>setPanel(panel==='SETTINGS'?null:'SETTINGS')}><Icon name="settings"/></button><button className="button secondary" onClick={()=>void fullscreen()} type="button">{t('comic.fullscreen')}</button></header>
  <div ref={viewport} className={`reader-viewport document-viewport${epub?' is-epub':''}`}>
   {epub?<EpubContent blocks={blocks} zoom={zoom} height={size.height}/>:<canvas ref={canvas} aria-label={progress} role="img"/>}
   {readyPage!==page&&!error&&<p className="reader-loading">{t('comic.loading')}</p>}
  </div>
  {error&&<div className="reader-error" role="alert"><p>{error}</p></div>}
  <footer className="reader-bottombar"><button className="button secondary" type="button" disabled={page===0} onClick={()=>jump(page-1)}>{t(epub?'ebook.previousChapter':'comic.previous')}</button><span>{progress}</span><input aria-label={t(epub?'ebook.jumpChapter':'comic.jump')} type="number" min={1} max={count} value={page+1} onChange={e=>{const n=Number(e.target.value);if(n>=1&&n<=count)jump(n-1);}}/><button className="button secondary" type="button" disabled={page===count-1} onClick={()=>jump(page+1)}>{t(epub?'ebook.nextChapter':'comic.next')}</button><button className="icon-button" type="button" disabled={bookmarkBusy||readyPage!==page} aria-pressed={bookmarks.includes(page)} aria-label={t(bookmarks.includes(page)?'comic.removeBookmark':'comic.addBookmark')} onClick={()=>void bookmark()}><Icon name="bookmark"/></button><BookmarkSelect bookmarks={bookmarks} count={count} chapters={epub} onJump={jump}/></footer>
  {panel&&<aside className="reader-panel" aria-label={t('comic.controls')}><div className="reader-panel-heading"><strong>{t('comic.controls')}</strong><button className="icon-button" type="button" aria-label={t('comic.close')} onClick={()=>setPanel(null)}><Icon name="close"/></button></div><>
   {!epub&&<label><span>{t('comic.fit')}</span><select aria-label={t('comic.fit')} value={fit} onChange={e=>{setFit(e.target.value as typeof fit);setZoom(1);}}><option value="PAGE">{t('comic.fitPage')}</option><option value="WIDTH">{t('comic.fitWidth')}</option><option value="NATIVE">{t('comic.native')}</option></select></label>}
   <label><span>{t('comic.zoom')}</span><input type="range" aria-label={t('comic.zoom')} min={0.25} max={3} step={0.05} value={zoom} onChange={e=>setZoom(Number(e.target.value))}/><output>{Math.round(zoom*100)}%</output></label><button className="button secondary" type="button" onClick={()=>setZoom(1)}>{t('comic.reset')}</button>
   <label><span>{t('comic.background')}</span><select aria-label={t('comic.background')} value={background} onChange={e=>setBackground(e.target.value)}><option value="BLACK">{t('comic.black')}</option><option value="GRAY">{t('comic.gray')}</option><option value="WHITE">{t('comic.white')}</option></select></label>
  </></aside>}
 </section>;
}

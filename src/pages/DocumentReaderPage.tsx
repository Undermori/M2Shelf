import {TextReaderControls} from '../components/TextReaderControls';
import {defaultTextReaderSettings, type TextReaderSettings} from '../types/textReader';
import {Select} from '../components/Select';
import {BookmarkSelect} from '../components/BookmarkSelect';
import {EpubContent,isLocalBookImage,type EpubBlock} from '../components/EpubContent';
import {useCallback,useEffect,useLayoutEffect,useRef,useState} from 'react';
import type {PDFDocumentProxy,RenderTask} from 'pdfjs-dist';
import type {ComicOpenResult,TextPosition} from '../types/comic';
import {captureTextPosition,positionRect,friendlyChapterTitle} from '../lib/textPosition';
import {api} from '../lib/api';
import {errorMessage} from '../lib/format';
import {useI18n} from '../lib/i18n';
import {Icon} from '../components/Icon';
import type {ReaderMenuActions} from '../types/readerMenu';
type Block=EpubBlock;

/** PDF pages and safe EPUB chapter blocks share application-owned progress and bookmarks. */
export function DocumentReaderPage({opened,initialPage,onBack,onProgress,onMenuActions}:{opened:ComicOpenResult;initialPage?:number;onBack:()=>void;onProgress:()=>void;onMenuActions?:(actions:ReaderMenuActions|null)=>void}){
 const {t}=useI18n();const {book}=opened;const count=book.pageCount;const epub=book.documentFormat!=='PDF';const chapters=epub&&book.documentFormat!=='TXT';
 const documentError=useCallback((value:unknown)=>value instanceof Error&&value.name==='M2ShelfError'?errorMessage(value):t('comic.error'),[t]);
 const [page,setPage]=useState(Math.max(0,Math.min(count-1,initialPage??book.progress?.lastPageIndex??0)));
 const [zoom,setZoom]=useState(1);const [fit,setFit]=useState<'PAGE'|'WIDTH'|'NATIVE'>('PAGE');const [panel,setPanel]=useState<'SETTINGS'|null>(null);const [background,setBackground]=useState('GRAY');const [error,setError]=useState('');const [readyPage,setReadyPage]=useState<number|null>(null);const [pdfReady,setPdfReady]=useState(false);
 const [textSettings,setTextSettings]=useState<TextReaderSettings>({...defaultTextReaderSettings});const [textSettingsReady,setTextSettingsReady]=useState(false);const [screen,setScreen]=useState(0);const [screens,setScreens]=useState(1);const textFrame=useRef<HTMLDivElement>(null);const textSaveChain=useRef(Promise.resolve());const textSaved=useRef('');const textLatest=useRef(textSettings);textLatest.current=textSettings;
 const saveText=useCallback(()=>{if(!epub||!textSettingsReady)return textSaveChain.current;const current=textLatest.current;const key=JSON.stringify(current);if(key===textSaved.current)return textSaveChain.current;textSaveChain.current=textSaveChain.current.catch(()=>undefined).then(()=>api.updateTextReaderSettings(current)).then(()=>{textSaved.current=key;}).catch(e=>{if(alive.current)setError(errorMessage(e));});return textSaveChain.current;},[epub,textSettingsReady]);
 useEffect(()=>{if(!epub)return;let active=true;void Promise.resolve().then(()=>api.getTextReaderSettings()).then(saved=>{if(active){if(saved){textSaved.current=JSON.stringify(saved);setTextSettings(saved);}setTextSettingsReady(true);}}).catch(()=>{if(active)setTextSettingsReady(true);});return()=>{active=false;};},[epub]);
 useEffect(()=>{if(!textSettingsReady)return;const timer=window.setTimeout(()=>void saveText(),500);return()=>{clearTimeout(timer);};},[textSettings,textSettingsReady,saveText]);
 useEffect(()=>()=>{void saveText();},[saveText]);
 const [imageLayout,setImageLayout]=useState(0);
 const [tocOpen,setTocOpen]=useState(false);
 const [verticalBounds,setVerticalBounds]=useState({start:true,end:false});
 const position=useRef<TextPosition>(book.progress?.textPosition??{blockIndex:0,characterOffset:0});
 const intent=useRef<{page:number;position?:TextPosition;end?:boolean;fragment?:string}>({page:initialPage??book.progress?.lastPageIndex??0,position:initialPage===undefined||initialPage===book.progress?.lastPageIndex?book.progress?.textPosition??undefined:opened.bookmarkPositions?.[initialPage]});
 const bookmarkPositions=useRef({...opened.bookmarkPositions});
 const onImageLayout=useCallback(()=>setImageLayout(value=>value+1),[]);
 const [blocks,setBlocks]=useState<Block[]>([]);const [bookmarks,setBookmarks]=useState(opened.bookmarks);const [bookmarkBusy,setBookmarkBusy]=useState(false);
 const canvas=useRef<HTMLCanvasElement>(null);const root=useRef<HTMLElement>(null);const viewport=useRef<HTMLDivElement>(null);const pdf=useRef<PDFDocumentProxy|null>(null);const renderTask=useRef<RenderTask|null>(null);const renderChain=useRef(Promise.resolve());
 const [size,setSize]=useState({width:900,height:600});const generation=useRef(0);const pendingProgress=useRef<{page:number;position?:TextPosition}|null>(null);const timer=useRef<number|null>(null);const saveChain=useRef(Promise.resolve());const alive=useRef(true);const onProgressRef=useRef(onProgress);onProgressRef.current=onProgress;
 const renderWidth=epub?0:size.width;const renderHeight=epub?0:size.height;
 const pdfZoom=epub?1:zoom;const pdfFit=epub?'PAGE':fit;const chapterReads=useRef(Promise.resolve());
 const flush=useCallback(()=>{if(timer.current!==null){clearTimeout(timer.current);timer.current=null;}const current=pendingProgress.current;pendingProgress.current=null;if(current!==null)saveChain.current=saveChain.current.catch(()=>undefined).then(()=>current.position?api.updateComicProgress(book.id,current.page,book.revision,current.position):api.updateComicProgress(book.id,current.page,book.revision)).then(()=>onProgressRef.current()).catch(e=>{if(alive.current)setError(errorMessage(e));});return saveChain.current;},[book.id,book.revision]);
 const remember=useCallback(()=>{if(readyPage!==page)return;if(epub&&viewport.current){const v=viewport.current;const next={start:v.scrollTop<=2,end:v.scrollTop>=v.scrollHeight-v.clientHeight-2};setVerticalBounds(previous=>previous.start===next.start&&previous.end===next.end?previous:next);}const next=epub&&textFrame.current&&viewport.current?captureTextPosition(textFrame.current,viewport.current,textLatest.current.mode==='PAGED'):null;if(next)position.current=next;pendingProgress.current={page,position:epub?{...position.current}:undefined};if(timer.current===null)timer.current=window.setTimeout(()=>void flush(),700);},[epub,page,readyPage,flush]);
 const changeTextSettings=useCallback((next:TextReaderSettings)=>{remember();setTextSettings(next);},[remember]);
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
  const token=++generation.current;let active=true;setReadyPage(null);setScreen(0);viewport.current?.scrollTo?.({top:0});
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
 useEffect(()=>{if(readyPage!==page||epub)return;pendingProgress.current={page};if(timer.current===null)timer.current=window.setTimeout(()=>void flush(),700);},[page,readyPage,flush,epub]);
 const jump=useCallback((index:number,anchor?:TextPosition,fragment?:string,end=false)=>{const target=Math.max(0,Math.min(count-1,index));intent.current={page:target,position:anchor,fragment,end};position.current=anchor??{blockIndex:0,characterOffset:0};if(target===page)setImageLayout(v=>v+1);else setPage(target);},[count,page]);
 const readingWidth=Math.max(100,Math.min(textSettings.maxWidth,size.width-2*textSettings.horizontalMargin));const readingHeight=Math.max(100,size.height-2*textSettings.verticalMargin);
 useLayoutEffect(()=>{if(!epub||readyPage!==page||!textSettingsReady)return;
  const frame=requestAnimationFrame(()=>{const element=textFrame.current;const scrollport=viewport.current;if(!element||!scrollport)return;
   const paged=textSettings.mode==='PAGED',stride=element.clientWidth+48;
   const total=paged?Math.max(1,Math.ceil((element.scrollWidth+48)/stride)):1;setScreens(total);
   const target=intent.current.page===page?intent.current:null;
   let anchor=target?.position??position.current;
   if(target?.fragment){const marker=Array.from(element.querySelectorAll<HTMLElement>('[data-book-anchor]')).find(n=>n.dataset.bookAnchor===target.fragment);if(marker)anchor={blockIndex:Number(marker.dataset.bookBlock),characterOffset:0};}
   const rect=positionRect(element,anchor);
   if(paged){const wanted=target?.end?total-1:rect?Math.floor((rect.left-element.getBoundingClientRect().left+element.scrollLeft+1)/stride):0;const next=Math.max(0,Math.min(total-1,wanted));element.scrollLeft=next*stride;setScreen(next);}
   else{element.scrollLeft=0;setScreen(0);if(target?.end)scrollport.scrollTop=scrollport.scrollHeight;else if(rect)scrollport.scrollTop=Math.max(0,scrollport.scrollTop+rect.top-scrollport.getBoundingClientRect().top-textSettings.verticalMargin);}
   intent.current={page:-1};remember();
  });return()=>cancelAnimationFrame(frame);
 },[epub,blocks,textSettings,readingWidth,readingHeight,imageLayout,readyPage,page,textSettingsReady,remember]);
 useEffect(()=>{if(!epub)return;const element=viewport.current;if(!element)return;let frame=0;const scroll=()=>{cancelAnimationFrame(frame);frame=requestAnimationFrame(remember);};element.addEventListener('scroll',scroll,{passive:true});return()=>{cancelAnimationFrame(frame);element.removeEventListener('scroll',scroll);};},[epub,remember]);
 const step=useCallback((direction:number)=>{if(readyPage!==page)return;if(epub&&textFrame.current&&viewport.current){
  const element=textFrame.current,scrollport=viewport.current;
  if(textSettings.mode==='PAGED'){const stride=element.clientWidth+48;const current=Math.round(element.scrollLeft/stride),next=current+direction;if(next>=0&&next<screens){element.scrollLeft=next*stride;setScreen(next);remember();return;}}
  else{const max=Math.max(0,scrollport.scrollHeight-scrollport.clientHeight);if(direction>0&&scrollport.scrollTop<max-2||direction<0&&scrollport.scrollTop>2){scrollport.scrollTop=Math.max(0,Math.min(max,scrollport.scrollTop+direction*scrollport.clientHeight));remember();return;}}
 }if(page+direction>=0&&page+direction<count)jump(page+direction,undefined,undefined,direction<0);},[epub,textSettings.mode,screens,jump,page,count,readyPage,remember]);
 const palette=textSettings.theme==='CUSTOM'?[textSettings.backgroundColor,textSettings.textColor]:textSettings.theme==='PAPER'?['#ffffff','#252529']:textSettings.theme==='SEPIA'?['#f5eedf','#302b25']:textSettings.theme==='NIGHT'?['#202124','#deded8']:['var(--surface-app)','var(--text-normal)'];

 const fullscreen=useCallback(async()=>{remember();try{if(document.fullscreenElement)await document.exitFullscreen();else await root.current?.requestFullscreen();}catch(e){setError(errorMessage(e));}},[remember]);
 const exit=useCallback(async()=>{remember();await Promise.all([flush(),saveText()]);if(document.fullscreenElement)await document.exitFullscreen().catch(()=>undefined);onBack();},[remember,flush,saveText,onBack]);
 const bookmark=useCallback(async()=>{if(bookmarkBusy||readyPage!==page)return;remember();setBookmarkBusy(true);try{const saved=epub?{...position.current}:undefined;setBookmarks(await (bookmarks.includes(page)?api.removeComicBookmark(book.id,page,book.revision):saved?api.addComicBookmark(book.id,page,book.revision,saved):api.addComicBookmark(book.id,page,book.revision)));if(saved)bookmarkPositions.current[page]=saved;}catch(e){setError(errorMessage(e));}finally{setBookmarkBusy(false);}},[book.id,book.revision,page,readyPage,bookmarks,bookmarkBusy,remember,epub]);
 useEffect(()=>{onMenuActions?.({previous:()=>step(-1),next:()=>step(1),fullscreen:()=>void fullscreen(),bookmark:()=>void bookmark(),canPrevious:readyPage===page&&!(page===0&&screen===0&&(textSettings.mode==='PAGED'||verticalBounds.start)),canNext:readyPage===page&&!(page===count-1&&(textSettings.mode==='PAGED'?screen===screens-1:verticalBounds.end)),canBookmark:!bookmarkBusy&&readyPage===page});},[onMenuActions,step,fullscreen,bookmark,readyPage,page,screen,screens,textSettings.mode,verticalBounds.start,verticalBounds.end,count,bookmarkBusy]);
 useEffect(()=>()=>onMenuActions?.(null),[onMenuActions]);
 useEffect(()=>{const handler=(e:KeyboardEvent)=>{if(e.altKey||e.ctrlKey||e.metaKey||e.target instanceof HTMLElement&&e.target.closest('input,select,textarea,[role=combobox],[role=listbox],[role=menu]'))return;if(e.target instanceof HTMLElement&&e.target.closest('button,a,[role=button]')&&[' ','Enter'].includes(e.key))return;if(['ArrowRight','PageDown',' '].includes(e.key)){e.preventDefault();step(1);}else if(['ArrowLeft','PageUp'].includes(e.key)){e.preventDefault();step(-1);}else if(e.key==='Home')jump(0);else if(e.key==='End')jump(count-1);else if(e.key.toLowerCase()==='f')void fullscreen();else if(e.key.toLowerCase()==='b')void bookmark();else if(e.key==='Escape'){e.preventDefault();e.stopImmediatePropagation();if(document.fullscreenElement)void document.exitFullscreen();else if(tocOpen)setTocOpen(false);else if(panel)setPanel(null);else void exit();}};window.addEventListener('keydown',handler,true);return()=>window.removeEventListener('keydown',handler,true);},[page,count,jump,step,fullscreen,bookmark,exit,tocOpen,panel]);
 const progress=t(chapters?'ebook.chapterProgress':'comic.progress',{page:page+1,count});
 const textFilter=[textSettings.brightness,textSettings.contrast,textSettings.saturation].every(value=>value===100)&&textSettings.sepia===0&&textSettings.hue===0&&!textSettings.negative?'none':`brightness(${textSettings.brightness}%) contrast(${textSettings.contrast}%) saturate(${textSettings.saturation}%) sepia(${textSettings.sepia}%) hue-rotate(${textSettings.hue}deg)${textSettings.negative?' invert(1)':''}`;
 return <section ref={root} className={`comic-reader document-reader controls-visible reader-bg-${background.toLowerCase()}`} aria-label={book.displayName}>
  <header className="reader-topbar"><button className="button secondary" onClick={()=>void exit()} type="button"><Icon name="arrow-left"/>{t('comic.back')}</button><strong>{book.displayName}</strong><button className="icon-button" type="button" aria-label={t('comic.controls')} aria-expanded={panel==='SETTINGS'} onClick={()=>setPanel(panel==='SETTINGS'?null:'SETTINGS')}><Icon name="settings"/></button><button className="button secondary" onClick={()=>void fullscreen()} type="button">{t('comic.fullscreen')}</button></header>
  <div ref={viewport} className={`reader-viewport document-viewport${epub?` is-epub${textSettings.mode==='PAGED'?' is-paged':''}`:''}`} style={epub?{filter:textFilter,padding:`${textSettings.verticalMargin}px ${textSettings.horizontalMargin}px`, '--reading-bg':palette[0], '--reading-text':palette[1]} as React.CSSProperties:undefined}>
   {epub?<div ref={textFrame} className="text-page-frame" style={{width:readingWidth,height:textSettings.mode==='PAGED'?readingHeight:undefined}}><EpubContent blocks={blocks} zoom={zoom} height={readingHeight} settings={textSettings} columnWidth={readingWidth} source={{bookId:book.id,chapter:page,revision:book.revision}} onLayout={onImageLayout}/></div>:<canvas ref={canvas} aria-label={progress} role="img"/>}
   {readyPage!==page&&!error&&<p className="reader-loading">{t('comic.loading')}</p>}
  </div>
  {error&&<div className="reader-error" role="alert"><p>{error}</p></div>}
  {epub?<footer className="reader-bottombar text-reader-navigation">
   <div className="reader-nav-start"><button className="button secondary" type="button" disabled={readyPage!==page||page===0&&screen===0&&(textSettings.mode==='PAGED'||verticalBounds.start)} onClick={()=>step(-1)}>{t('comic.previous')}</button></div>
   <span className="reader-nav-status" role="status">{t(chapters?'reader.chapterStatus':'reader.segmentStatus',{page:page+1,count})}</span>
   <div className="reader-nav-end"><button className="button secondary" type="button" disabled={readyPage!==page||page===count-1&&(textSettings.mode==='PAGED'?screen===screens-1:verticalBounds.end)} onClick={()=>step(1)}>{t('comic.next')}</button><button className="button secondary" type="button" aria-expanded={tocOpen} aria-controls="reader-contents" onClick={()=>setTocOpen(v=>!v)}>{t('book.toc')}</button><button className="icon-button" type="button" disabled={bookmarkBusy||readyPage!==page} aria-pressed={bookmarks.includes(page)} aria-label={t(bookmarks.includes(page)?'comic.removeBookmark':'comic.addBookmark')} onClick={()=>void bookmark()}><Icon name="bookmark"/></button></div>
  </footer>:<footer className="reader-bottombar"><button className="button secondary" type="button" disabled={page===0&&screen===0} onClick={()=>step(-1)}>{t('comic.previous')}</button><span>{progress}{epub&&screens>1&&<> · {t('text.screenProgress',{page:screen+1,count:screens})}</>}</span><input aria-label={t(chapters?'ebook.jumpChapter':'comic.jump')} type="number" min={1} max={count} value={page+1} onChange={e=>{const n=Number(e.target.value);if(n>=1&&n<=count)jump(n-1);}}/><button className="button secondary" type="button" disabled={page===count-1&&screen===screens-1} onClick={()=>step(1)}>{t('comic.next')}</button><button className="icon-button" type="button" disabled={bookmarkBusy||readyPage!==page} aria-pressed={bookmarks.includes(page)} aria-label={t(bookmarks.includes(page)?'comic.removeBookmark':'comic.addBookmark')} onClick={()=>void bookmark()}><Icon name="bookmark"/></button>{chapters&&<Select className="reader-toc" aria-label={t('book.toc')} value={page} onChange={event=>jump(Number(event.target.value))}>{opened.pages.map(item=><option key={item.pageIndex} value={item.pageIndex}>{item.pageIndex+1}. {item.pageName}</option>)}</Select>}<BookmarkSelect bookmarks={bookmarks} count={count} chapters={chapters} onJump={jump}/></footer>}
  {epub&&tocOpen&&<aside className="reader-toc-panel" id="reader-contents" aria-label={t('book.toc')}>
   <div className="reader-panel-heading"><strong>{t('book.toc')}</strong><button className="icon-button" type="button" aria-label={t('common.close')} onClick={()=>setTocOpen(false)}><Icon name="close"/></button></div>
   <Select className="reader-toc" aria-label={t('book.toc')} value={`page-${page}`} onChange={event=>{const value=event.target.value;if(value.startsWith('nav-')){const entry=opened.navigation?.[Number(value.slice(4))];if(entry)jump(entry.pageIndex,undefined,entry.fragment??undefined);}else jump(Number(value.slice(5)));setTocOpen(false);}}>
    {opened.pages.map(item=><option key={`page-${item.pageIndex}`} value={`page-${item.pageIndex}`}>{friendlyChapterTitle(item.pageName,t('reader.chapterName',{page:item.pageIndex+1}))}</option>)}
    {opened.navigation?.filter(item=>item.fragment).map((item)=><option key={`nav-${opened.navigation!.indexOf(item)}`} value={`nav-${opened.navigation!.indexOf(item)}`}>{item.title}</option>)}
   </Select>
   <label className="reader-location-input"><span>{t(chapters?'ebook.jumpChapter':'comic.jump')}</span><input aria-label={t(chapters?'ebook.jumpChapter':'comic.jump')} type="number" min={1} max={count} value={page+1} onChange={e=>{const value=Number(e.target.value);if(value>=1&&value<=count)jump(value-1);}}/></label>
   <BookmarkSelect bookmarks={bookmarks} count={count} chapters={chapters} onJump={index=>{jump(index,bookmarkPositions.current[index]);setTocOpen(false);}}/>
  </aside>}

  {panel&&<aside className="reader-panel" aria-label={t('comic.controls')}><div className="reader-panel-heading"><strong>{t('comic.controls')}</strong><button className="icon-button" type="button" aria-label={t('comic.close')} onClick={()=>setPanel(null)}><Icon name="close"/></button></div><>
   {epub&&<TextReaderControls value={textSettings} onChange={changeTextSettings}/>}
   {!epub&&<label><span>{t('comic.fit')}</span><Select aria-label={t('comic.fit')} value={fit} onChange={e=>{setFit(e.target.value as typeof fit);setZoom(1);}}><option value="PAGE">{t('comic.fitPage')}</option><option value="WIDTH">{t('comic.fitWidth')}</option><option value="NATIVE">{t('comic.native')}</option></Select></label>}
   {!epub&&<><label><span>{t('comic.zoom')}</span><input type="range" aria-label={t('comic.zoom')} min={0.25} max={3} step={0.05} value={zoom} onChange={e=>setZoom(Number(e.target.value))}/><output>{Math.round(zoom*100)}%</output></label><button className="button secondary" type="button" onClick={()=>setZoom(1)}>{t('comic.reset')}</button>
   <label><span>{t('comic.background')}</span><Select aria-label={t('comic.background')} value={background} onChange={e=>setBackground(e.target.value)}><option value="BLACK">{t('comic.black')}</option><option value="GRAY">{t('comic.gray')}</option><option value="WHITE">{t('comic.white')}</option></Select></label>
  </>}</></aside>}
 </section>;
}

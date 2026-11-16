import {useEffect, useId, useLayoutEffect, useRef, useState} from 'react';
import {createPortal} from 'react-dom';
import {notifyToast} from '../lib/toast';
import {useWorkbenchRaster,workbenchThumbCache} from '../lib/workbenchThumbCache';
import {useNativeFileDrag} from '../lib/useNativeFileDrag';
import {api, chooseScreenshotDirectories} from '../lib/api';
import {errorMessage, nodeDisplayTitle} from '../lib/format';
import {useI18n} from '../lib/i18n';
import {useModalFocus} from '../hooks/useModalFocus';
import type {CollectionTarget} from '../types/collection';
import type {StudioTargetRequest} from '../pages/OrganizePage';
import {targetKey, type ScreenshotBatchAction, type ScreenshotFolder, type ScreenshotImage, type ScreenshotSort, type ScreenshotSession, type WorkNoteEntry} from '../types/workbench';
import {round8GalleryCopy} from '../lib/round8GalleryCopy';
import {workspaceCopy} from '../lib/workspaceCopy';
import {floatingPanelPosition} from '../lib/floatingPanel';
import '../styles/gallery-round8.css';
import {Icon} from './Icon';
import {Select} from './Select';
import {EmptyState} from './EmptyState';
import {ConfirmDialog} from './ConfirmDialog';

type OpenTarget = (target: CollectionTarget, nodeId?: number) => Promise<void>;
/** AVIF uses the WebView decoder; only the bounded 768px raster enters the thumbnail grid. */
async function screenshotBlob(bytes:ArrayBuffer,image:ScreenshotImage,thumbnail:boolean):Promise<Blob> {
  const header=new Uint8Array(bytes,0,Math.min(bytes.byteLength,12));
  const nativeRaster=header[0]===0x89&&header[1]===0x50||header[0]===0xff&&header[1]===0xd8;
  if(!thumbnail||image.mime!=='image/avif'||nativeRaster)return new Blob([bytes]);
  if(typeof createImageBitmap!=='function'||!image.width||!image.height)throw new Error('SCREENSHOT_FORMAT');
  const bitmap=await createImageBitmap(new Blob([bytes],{type:'image/avif'}));
  try {
    if(!bitmap.width||!bitmap.height||bitmap.width*bitmap.height>20_000_000)throw new Error('SCREENSHOT_LIMIT');
    // Container dimensions can include auxiliary AVIF items; preserve the decoded frame's aspect.
    const scale=Math.min(1,768/Math.max(bitmap.width,bitmap.height));
    const width=Math.max(1,Math.round(bitmap.width*scale)),height=Math.max(1,Math.round(bitmap.height*scale));
    if(typeof OffscreenCanvas!=='undefined'){
      const canvas=new OffscreenCanvas(width,height),context=canvas.getContext('2d');if(!context)throw new Error('SCREENSHOT_FORMAT');
      context.drawImage(bitmap,0,0,width,height);return await canvas.convertToBlob({type:'image/png'});
    }
    const canvas=document.createElement('canvas');canvas.width=width;canvas.height=height;
    const context=canvas.getContext('2d');if(!context)throw new Error('SCREENSHOT_FORMAT');context.drawImage(bitmap,0,0,width,height);
    return await new Promise<Blob>((resolve,reject)=>canvas.toBlob(blob=>blob?resolve(blob):reject(new Error('SCREENSHOT_FORMAT')),'image/png'));
  } finally {bitmap.close();}
}

/** Workbench rasters share four bounded reads and retain reusable thumbnail leases. */
export function ScreenshotRaster({image,thumbnail=true}:{image:ScreenshotImage;thumbnail?:boolean}) {
  const {t}=useI18n();const frame=useRef<HTMLDivElement>(null),[visible,setVisible]=useState(!thumbnail||Boolean(workbenchThumbCache.peek(JSON.stringify(['screenshot',image.folderId,image.id,image.revision,'thumb'])))),[ready,setReady]=useState(false);
  const requestKey=JSON.stringify(['screenshot',image.folderId,image.id,image.revision,thumbnail?'thumb':'full']);
  const raster=useWorkbenchRaster(requestKey,visible,async()=>screenshotBlob(await api.readScreenshot(image.id,image.revision,thumbnail),image,thumbnail),thumbnail);
  useEffect(()=>{if(!thumbnail||!frame.current)return;if(typeof IntersectionObserver==='undefined'){setVisible(true);return;}const observer=new IntersectionObserver(entries=>setVisible(entries.some(entry=>entry.isIntersecting)),{rootMargin:'480px'});observer.observe(frame.current);return()=>observer.disconnect();},[thumbnail]);
  useEffect(()=>setReady(false),[requestKey]);
  return <div ref={frame} data-ready={Boolean(raster.url)&&ready&&!raster.failed} className={`studio-raster${thumbnail?'':' studio-raster-full'}`}>{raster.url&&!raster.failed?<img ref={raster.image} src={raster.url} alt={image.fileName} draggable={false} decoding="async" onLoad={()=>setReady(true)} onError={raster.fail}/>:<Icon name="image"/>}{raster.failed&&<span role="status">{t('studio.imageUnavailable')}</span>}</div>;
}

function dismissWorkPicker(dialog:HTMLElement|null) {
  const trigger=dialog?.querySelector<HTMLButtonElement>('.studio-picker-trigger[aria-expanded="true"]');
  if(!trigger)return false;trigger.click();trigger.focus();return true;
}

export function WorkPicker({value,onChange,disabled=false,nullLabel,filter=false,valueNodeId}:{value:CollectionTarget|null;onChange:(target:CollectionTarget|null)=>void;disabled?:boolean;nullLabel?:string;filter?:boolean;valueNodeId?:number|null}) {
  const {t,language}=useI18n();const id=useId(),container=useRef<HTMLDivElement>(null),trigger=useRef<HTMLButtonElement>(null),panel=useRef<HTMLDivElement>(null),search=useRef<HTMLInputElement>(null);
  const [open,setOpen]=useState(false),[query,setQuery]=useState(''),[rows,setRows]=useState<WorkNoteEntry[]>([]),[failed,setFailed]=useState(false),[loading,setLoading]=useState(false);
  const [labels,setLabels]=useState<Record<string,string>>({}),[position,setPosition]=useState({left:0,top:0,width:320,maxHeight:360});const generation=useRef(0),labelLanguage=useRef(language);
  const emptyLabel=nullLabel??t('studio.noLink');
  useEffect(()=>{
    const changedLanguage=labelLanguage.current!==language;labelLanguage.current=language;if(changedLanguage)setLabels({});
    if(!value)return;let active=true;
    const lookup=async()=>{try{let title:string|null|undefined;if(value.kind==='NODE'){const detail=await api.nodeDetail(value.id);title=nodeDisplayTitle({...detail.node,binding:detail.binding},language);}else if(valueNodeId){title=(await api.comicDetail(valueNodeId)).find(book=>book.id===value.id)?.displayName;}if(active&&title)setLabels(current=>({...current,[targetKey(value)]:title}));}catch{/* A stale or unavailable association remains clearable. */}};
    if(changedLanguage||!labels[targetKey(value)])void lookup();return()=>{active=false;};
  },[targetKey(value),valueNodeId,language]);
  useEffect(()=>{if(!open&&!value)return;const token=++generation.current;setLoading(true);const timer=window.setTimeout(()=>{void api.listWorkNotes({notesOnly:false,rootId:null,mediaKind:null,query:query.trim(),offset:0,limit:60}).then(page=>{if(token===generation.current){setRows(page.items);setLabels(current=>Object.fromEntries([...Object.entries(current).filter(([key])=>key===targetKey(value)),...page.items.map(row=>[targetKey(row.target),row.title])]));setFailed(false);}}).catch(()=>{if(token===generation.current)setFailed(true);}).finally(()=>{if(token===generation.current)setLoading(false);});},query?180:0);return()=>{generation.current++;window.clearTimeout(timer);};},[open,query,language,targetKey(value)]);
  useLayoutEffect(()=>{
    if(!open)return;
    const update=()=>{const rect=trigger.current?.getBoundingClientRect();if(!rect)return;const next=floatingPanelPosition(rect,{width:360,height:Math.min(360,panel.current?.scrollHeight?panel.current.scrollHeight+2:360)},{width:window.innerWidth,height:window.innerHeight});setPosition(current=>Object.keys(next).every(key=>current[key as keyof typeof next]===next[key as keyof typeof next])?current:next);};
    update();if(panel.current&&typeof panel.current.showPopover==='function'&&!panel.current.matches(':popover-open'))panel.current.showPopover();search.current?.focus();
    const outside=(event:PointerEvent)=>{if(!container.current?.contains(event.target as Node))setOpen(false);};
    document.addEventListener('pointerdown',outside);window.addEventListener('resize',update);window.addEventListener('scroll',update,true);
    const observer=typeof ResizeObserver==='undefined'?null:new ResizeObserver(update);if(trigger.current)observer?.observe(trigger.current);if(panel.current)observer?.observe(panel.current);
    return()=>{observer?.disconnect();document.removeEventListener('pointerdown',outside);window.removeEventListener('resize',update);window.removeEventListener('scroll',update,true);};
  },[open]);
  useEffect(()=>{if(disabled)setOpen(false);},[disabled]);
  const choose=(target:CollectionTarget|null)=>{onChange(target);setOpen(false);setQuery('');trigger.current?.focus();};
  return <div ref={container} className={`studio-work-picker${filter?' studio-link-filter':''}`} onBlur={event=>{if(event.relatedTarget&&!container.current?.contains(event.relatedTarget as Node))setOpen(false);}} onKeyDown={event=>{
    if(event.nativeEvent.isComposing)return;
    if(event.key==='Escape'&&open){event.preventDefault();event.stopPropagation();setOpen(false);trigger.current?.focus();}
    if(open&&!event.altKey&&!event.ctrlKey&&!event.metaKey&&['ArrowDown','ArrowUp','Home','End'].includes(event.key)&&event.target!==search.current){const buttons=Array.from(panel.current?.querySelectorAll<HTMLButtonElement>('.studio-work-option')??[]),index=buttons.indexOf(event.target as HTMLButtonElement);const next=event.key==='Home'?0:event.key==='End'?buttons.length-1:index+(event.key==='ArrowDown'?1:-1);const destination=buttons[next];if(destination&&destination!==event.target){event.preventDefault();event.stopPropagation();destination.focus();destination.scrollIntoView?.({block:'nearest',inline:'nearest'});}}
  }}>
    <div className="studio-picker-control"><button ref={trigger} type="button" className="studio-picker-trigger" aria-label={filter?t('studio.filterWorks'):t('studio.linkWork')} aria-describedby={`${id}-value`} aria-expanded={open} aria-controls={open?id:undefined} disabled={disabled} title={value?labels[targetKey(value)]??t('studio.linkWork'):emptyLabel} onClick={()=>setOpen(current=>!current)}><span id={`${id}-value`}>{value?labels[targetKey(value)]??t('studio.linkWork'):emptyLabel}</span><Icon name={open?"chevron-up":"chevron-down"}/></button>{value&&<button type="button" className="studio-picker-clear" disabled={disabled} aria-label={filter?t('studio.clearWorkFilter'):t('studio.clearLink')} title={filter?t('studio.clearWorkFilter'):t('studio.clearLink')} onClick={()=>choose(null)}><Icon name="close"/></button>}</div>
    {open&&<div ref={panel} id={id} popover="auto" className="studio-work-popover" style={position} role="group" aria-label={t('studio.findWork')} onToggle={event=>{if(event.newState==='closed')setOpen(false);}}>
      <input ref={search} type="search" aria-label={t('studio.findWork')} placeholder={t('studio.findWork')} value={query} onChange={event=>setQuery(event.target.value)} onKeyDown={event=>{if(event.key==='ArrowDown'&&!event.nativeEvent.isComposing&&!event.altKey&&!event.ctrlKey&&!event.metaKey){const destination=panel.current?.querySelector<HTMLButtonElement>('.studio-work-option');if(destination){event.preventDefault();event.stopPropagation();destination.focus();destination.scrollIntoView?.({block:'nearest',inline:'nearest'});}}}}/>
      <div className="studio-work-options" aria-busy={loading}><button type="button" className="studio-work-option" aria-pressed={!value} onClick={()=>choose(null)}>{emptyLabel}</button>{rows.map(row=><button type="button" className="studio-work-option" key={targetKey(row.target)} aria-pressed={targetKey(row.target)===targetKey(value)} onClick={()=>choose(row.target)}><strong>{row.title}</strong><small>{row.libraryName}</small></button>)}{failed?<small role="alert">{t('studio.loadFailed')}</small>:loading?<small role="status">{t('comic.loading')}</small>:!rows.length&&<small>{t('studio.noResults')}</small>}</div>
    </div>}
  </div>;
}

export function ScreenshotStudio({busy,onError,onOpenTarget,onOpenNote,active=true,request=null,session,onSessionChange,sortPreference,onSortChange}:{busy:boolean;onError:(message:string)=>void;onOpenTarget:OpenTarget;onOpenNote?:(target:CollectionTarget,nodeId?:number)=>void;active?:boolean;request?:StudioTargetRequest|null;session?:ScreenshotSession;onSessionChange?:(session:ScreenshotSession)=>void;sortPreference?:{sort:ScreenshotSort;descending:boolean};onSortChange?:(sort:ScreenshotSort,descending:boolean)=>void}) {
  const {t,language}=useI18n();const copy=round8GalleryCopy[language],fileCopy=workspaceCopy[language];const userSorted=useRef(false);const [sort,setSort]=useState<ScreenshotSort>(sortPreference?.sort??session?.sort??'MODIFIED'),[descending,setDescending]=useState<boolean>(sortPreference?.descending??session?.descending??true);const [folders,setFolders]=useState<ScreenshotFolder[]>([]),[foldersReady,setFoldersReady]=useState(false),[folderId,setFolderId]=useState(session?.folderId??''),[query,setQuery]=useState(session?.query??''),[target,setTarget]=useState<CollectionTarget|null>(session?.target??null),[stars,setStars]=useState(session?.stars??false),[density,setDensity]=useState(session?.density??'comfortable');
  const [tag,setTag]=useState<string|null>(session?.tag??null),[selecting,setSelecting]=useState(false),[selection,setSelection]=useState<Set<number>>(new Set()),[batchTags,setBatchTags]=useState(''),[batchTarget,setBatchTarget]=useState<CollectionTarget|null>(null),[paging,setPaging]=useState(false);
  const anchor=useRef<number|null>(null),pageLock=useRef(false),skipLoad=useRef(false),silentRefresh=useRef(false),starringIds=useRef(new Set<number>());
  const [images,setImages]=useState<ScreenshotImage[]>([]),[total,setTotal]=useState(0),[offset,setOffset]=useState(session?.offset??0),[loading,setLoading]=useState(true),[failed,setFailed]=useState(false),[action,setAction]=useState(false),[refresh,setRefresh]=useState(0);
  const [folderEditor,setFolderEditor]=useState<ScreenshotFolder|null>(null),[folderManager,setFolderManager]=useState(false),[remove,setRemove]=useState<ScreenshotFolder|null>(null),[activeImage,setActiveImage]=useState<number|null>(null);
  const generation=useRef(0),mounted=useRef(true),actionLock=useRef(false),errorRef=useRef(onError);errorRef.current=onError;
  useEffect(()=>{if(!sortPreference||userSorted.current)return;setSort(sortPreference.sort);setDescending(sortPreference.descending);},[sortPreference?.sort,sortPreference?.descending]);
  const applySort=(nextSort:ScreenshotSort,nextDescending:boolean)=>{userSorted.current=true;setSort(nextSort);setDescending(nextDescending);setOffset(0);onSortChange?.(nextSort,nextDescending);};
  const sessionChange=useRef(onSessionChange);sessionChange.current=onSessionChange;
  const sessionJson=JSON.stringify({folderId,query,target,stars,density,sort,descending,tag,offset} satisfies ScreenshotSession);
  const externalJson=JSON.stringify(session?{folderId:session.folderId,query:session.query,target:session.target,stars:session.stars,density:session.density,sort:session.sort,descending:session.descending,tag:session.tag,offset:session.offset}:null),externalSeen=useRef(externalJson),restoring=useRef<string|null>(null),published=useRef('');
  useLayoutEffect(()=>{
    if(externalSeen.current===externalJson)return;externalSeen.current=externalJson;
    if(!session||externalJson===sessionJson)return;
    restoring.current=externalJson;generation.current++;setActiveImage(null);
    setFolderId(session.folderId);setQuery(session.query);setTarget(session.target);setStars(session.stars);setDensity(session.density);if(!sortPreference){setSort(session.sort);setDescending(session.descending);}setTag(session.tag);setOffset(session.offset);
  },[externalJson,sortPreference]);
  useEffect(()=>{
    if(restoring.current&&restoring.current!==sessionJson)return;restoring.current=null;
    if(sessionJson===published.current)return;published.current=sessionJson;
    if(sessionJson!==externalJson)sessionChange.current?.(JSON.parse(sessionJson) as ScreenshotSession);
  },[sessionJson,externalJson]);
  useEffect(()=>{if(!request)return;let live=true;setTarget(request.target);setFolderId('');setQuery('');setStars(false);setTag(null);setOffset(0);setActiveImage(null);if(request.screenshotId){void api.locateScreenshot(request.screenshotId,sort,descending).then(({image,offset})=>{if(live){setTarget(null);setFolderId(String(image.folderId));setOffset(offset);setActiveImage(image.id);}}).catch(error=>{if(live)errorRef.current(errorMessage(error));});}return()=>{live=false;};},[request]);
  useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;generation.current++;};},[]);
  useEffect(()=>{let active=true;void api.screenshotFolders().then(items=>{if(active){setFolders(items);setFoldersReady(true);}}).catch(error=>{if(active){setFoldersReady(true);errorRef.current(errorMessage(error));}});return()=>{active=false;};},[refresh]);
  useEffect(()=>{setSelection(new Set());anchor.current=null;},[folderId,query,targetKey(target),stars,tag,offset,sort,descending]);
  useEffect(()=>{if(skipLoad.current){skipLoad.current=false;return;}const quiet=silentRefresh.current;silentRefresh.current=false;const token=++generation.current;if(!quiet)setLoading(true);setFailed(false);const timer=window.setTimeout(()=>{void api.screenshots({sort,descending,folderId:folderId?Number(folderId):null,query:query.trim(),target,starredOnly:stars,offset,limit:48,...(tag?{tag}:{})}).then(page=>{if(token===generation.current){setImages(page.items);setTotal(page.total);setSelection(current=>new Set([...current].filter(id=>page.items.some(image=>image.id===id))));if(offset>=page.total&&offset>0){setOffset(Math.max(0,Math.floor((page.total-1)/48)*48));}}}).catch(error=>{if(token===generation.current){setFailed(true);errorRef.current(errorMessage(error));}}).finally(()=>{if(token===generation.current)setLoading(false);});},query?180:0);return()=>{generation.current++;window.clearTimeout(timer);};},[folderId,query,targetKey(target),stars,offset,refresh,tag,sort,descending]);
  const run=async(task:()=>Promise<void>)=>{if(actionLock.current||busy)return;actionLock.current=true;setAction(true);try{await task();}catch(error){if(mounted.current)onError(errorMessage(error));}finally{actionLock.current=false;if(mounted.current)setAction(false);}};
  const processFolders=async(tasks:Array<()=>Promise<ScreenshotFolder>>,resetOffset=false)=>{
    let completed=0,matched=0,limited=false;const failures:unknown[]=[];
    try{for(const task of tasks){try{const folder=await task();matched+=folder?.autoMatch?.matchedCount??0;limited=limited||Boolean(folder?.autoMatch?.limited);completed++;}catch(error){failures.push(error);}}}
    finally{if(mounted.current&&completed){if(resetOffset)setOffset(0);setRefresh(value=>value+1);const text=workspaceCopy[language];notifyToast('info',[matched?`${text.autoMatched.replace('{count}',String(matched))} · ${text.autoPartial}`:text.autoNone,limited?text.autoLimited:''].filter(Boolean).join(' '));}}
    if(mounted.current&&failures.length)onError(`${t('studio.folderFailures',{count:failures.length})} ${errorMessage(failures[0])}`);
  };
  const importFolders=()=>run(async()=>{const paths=await chooseScreenshotDirectories();await processFolders(paths.map(path=>()=>api.importScreenshotFolder(path)),true);});
  const refreshFolders=()=>run(()=>processFolders(folders.filter(item=>!folderId||item.id===Number(folderId)).map(folder=>()=>api.refreshScreenshotFolder(folder.id))));
  const currentFolder=folders.find(folder=>folder.id===Number(folderId));const locked=busy||action;
  const openImage=activeImage===null?null:images.find(image=>image.id===activeImage)??null;
  const saveImage=(image:ScreenshotImage)=>{if(!mounted.current)return;generation.current++;setLoading(false);setImages(current=>current.map(item=>item.id===image.id?image:item));};
  const toggleStar=async(image:ScreenshotImage)=>{if(busy||starringIds.current.has(image.id))return;starringIds.current.add(image.id);try{const token=generation.current;const next=await api.updateScreenshot({...image,starred:!image.starred});if(!mounted.current)return;if(token!==generation.current){setRefresh(value=>value+1);return;}saveImage(next);if(stars&&!next.starred){setImages(current=>current.filter(item=>item.id!==next.id));setTotal(current=>Math.max(0,current-1));}}catch(error){if(mounted.current)onError(errorMessage(error));}finally{starringIds.current.delete(image.id);}};
  const selectImage=(id:number,range=false)=>{const previous=anchor.current;setSelection(current=>{const next=new Set(current);const start=images.findIndex(image=>image.id===previous),end=images.findIndex(image=>image.id===id);if(range&&start>=0){for(const image of images.slice(Math.min(start,end),Math.max(start,end)+1))next.add(image.id);}else if(next.has(id))next.delete(id);else next.add(id);return next;});anchor.current=id;};
  const filterTag=(value:string)=>{setTag(current=>current===value?null:value);setOffset(0);setActiveImage(null);};
  const applyBatch=(batchAction:ScreenshotBatchAction)=>run(async()=>{const selected=images.filter(image=>selection.has(image.id));if(!selected.length)return;try{const updated=await api.batchUpdateScreenshots(selected,batchAction);if(mounted.current){generation.current++;setImages(current=>current.map(item=>updated.find(next=>next.id===item.id)??item));notifyToast('success',`${updated.length} ${copy.changed}`);setRefresh(value=>value+1);}}catch(error){if(mounted.current){setSelection(new Set());setRefresh(value=>value+1);}throw error;}});
  const drag=useNativeFileDrag<ScreenshotImage>({active,disabled:busy||activeImage!==null,onDrag:async image=>{if(busy)return;try{const selected=selecting&&selection.has(image.id)?images.filter(item=>selection.has(item.id)):[image];await api.dragScreenshots(selected.map(({id,revision})=>({id,revision})));}catch(error){if(mounted.current)onError(errorMessage(error));}}});
  const autoScan=useRef({running:false,lastStarted:0});
  const latestScan=useRef({active,busy,loading,action,folders,folderId,query,target,stars,offset,tag,sort,descending,activeImage,folderEditor,folderManager,remove,selecting,selection,images,total});
  latestScan.current={active,busy,loading,action,folders,folderId,query,target,stars,offset,tag,sort,descending,activeImage,folderEditor,folderManager,remove,selecting,selection,images,total};
  const scanPending=useRef(false);
  useEffect(()=>{
    if(!active)return;
    let live=true,timer=0;
    const attempt=async()=>{
      const state=latestScan.current;
      if(!live||!state.active||document.visibilityState==='hidden')return;
      if(state.busy||state.loading||state.action||actionLock.current||state.activeImage!==null||state.folderEditor||state.folderManager||state.remove||state.selecting||document.querySelector('[aria-modal="true"],dialog[open]')){timer=window.setTimeout(attempt,600);return;}
      if(!scanPending.current||!state.folders.length)return;
      if(autoScan.current.running){timer=window.setTimeout(attempt,300);return;}
      const wait=2500-(Date.now()-autoScan.current.lastStarted);if(wait>0){timer=window.setTimeout(attempt,wait);return;}
      scanPending.current=false;autoScan.current={running:true,lastStarted:Date.now()};
      const token=generation.current;let matched=0,completed=0;
      try{
        for(const folder of state.folders){if(!live||!latestScan.current.active)break;try{const result=await api.refreshScreenshotFolder(folder.id);matched+=result.autoMatch?.matchedCount??0;completed++;}catch{/* Background refresh keeps the last valid index and never interrupts editing. */}}
        const now=latestScan.current;
        if(!live||!completed||token!==generation.current||now.action||now.activeImage!==null||now.folderEditor||now.folderManager||now.remove||now.selecting)return;
        const [page,updatedFolders]=await Promise.all([api.screenshots({sort:state.sort,descending:state.descending,folderId:state.folderId?Number(state.folderId):null,query:state.query.trim(),target:state.target,starredOnly:state.stars,offset:state.offset,limit:48,...(state.tag?{tag:state.tag}:{})}),api.screenshotFolders()]);
        if(live&&token===generation.current&&!actionLock.current&&!latestScan.current.busy&&latestScan.current.activeImage===null&&!latestScan.current.folderEditor&&!latestScan.current.folderManager&&!latestScan.current.remove&&!latestScan.current.selecting){
          const current=latestScan.current;
          if(page.total!==current.total||JSON.stringify(page.items)!==JSON.stringify(current.images)){setImages(page.items);setTotal(page.total);}
          if(JSON.stringify(updatedFolders)!==JSON.stringify(current.folders))setFolders(updatedFolders);
          if(matched)notifyToast('info',workspaceCopy[language].autoMatched.replace('{count}',String(matched)));
        }
      }catch{/* A failed quiet list read preserves the displayed index; the next foreground event retries. */}
      finally{autoScan.current.running=false;}
    };
    const schedule=()=>{scanPending.current=true;window.clearTimeout(timer);timer=window.setTimeout(attempt,300);};
    schedule();window.addEventListener('focus',schedule);document.addEventListener('visibilitychange',schedule);
    return()=>{live=false;window.clearTimeout(timer);window.removeEventListener('focus',schedule);document.removeEventListener('visibilitychange',schedule);};
  },[active,folders.length,language]);
  const parsedBatchTags=[...new Set(batchTags.split(/[,，]/).map(value=>value.trim()).filter(Boolean))];
  const invalidBatchTags=parsedBatchTags.length>20||parsedBatchTags.some(value=>[...value].length>80)||parsedBatchTags.reduce((sum,value)=>sum+[...value].length,0)>1000;
  const moveImage=async(direction:-1|1)=>{if(pageLock.current||loading||locked||!openImage)return;const index=images.indexOf(openImage),next=images[index+direction];if(next){setActiveImage(next.id);return;}const nextOffset=offset+direction*48;if(nextOffset<0||nextOffset>=total)return;pageLock.current=true;setPaging(true);const token=++generation.current;try{const page=await api.screenshots({sort,descending,folderId:folderId?Number(folderId):null,query:query.trim(),target,starredOnly:stars,offset:nextOffset,limit:48,...(tag?{tag}:{})});if(mounted.current&&token===generation.current){if(!page.items.length){setTotal(page.total);return;}skipLoad.current=true;setImages(page.items);setTotal(page.total);setOffset(nextOffset);setActiveImage(direction===1?page.items[0].id:page.items[page.items.length-1].id);}}catch(error){if(mounted.current)onError(errorMessage(error));}finally{pageLock.current=false;if(mounted.current)setPaging(false);}};
  return <div className="screenshot-studio">
    <div className="studio-gallery-controls">
    <div className="studio-gallery-toolbar"><Select aria-label={t('studio.allFolders')} value={folderId} onChange={e=>{setFolderId(e.target.value);setOffset(0);}}><option value="">{t('studio.allFolders')}</option>{folders.map(folder=><option value={folder.id} key={folder.id}>{folder.alias} ({folder.imageCount})</option>)}</Select><div className="studio-toolbar-actions"><button type="button" className="button secondary studio-compact-action" aria-label={t('studio.manageFolders')} title={t('studio.manageFolders')} disabled={locked||(foldersReady&&!folders.length)} onClick={()=>{if(!folders.length)return;if(currentFolder)setFolderEditor(currentFolder);else setFolderManager(true);}}><Icon name="edit"/><span>{t('studio.manageFolders')}</span></button><button type="button" className="button secondary studio-compact-action" aria-label={t('studio.refresh')} title={t('studio.refresh')} disabled={locked||(foldersReady&&!folders.length)} onClick={()=>{if(folders.length)void refreshFolders();}}><Icon name="refresh"/><span>{t('studio.refresh')}</span></button><button type="button" className="button primary" disabled={locked} onClick={()=>void importFolders()}><Icon name="plus"/>{t('studio.import')}</button></div></div>
    <div className="studio-gallery-filters"><label className="search-field studio-search"><Icon name="search"/><input type="search" aria-label={t('studio.searchImages')} placeholder={t('studio.searchImages')} value={query} onChange={e=>{setQuery(e.target.value);setOffset(0);}}/></label><WorkPicker filter value={target} valueNodeId={request&&targetKey(request.target)===targetKey(target)?request.nodeId:undefined} nullLabel={t('studio.allLinked')} onChange={value=>{setTarget(value);setOffset(0);}}/><button type="button" className="button secondary studio-star-toggle" aria-pressed={stars} onClick={()=>{setStars(value=>!value);setOffset(0);}}><Icon name="star"/>{t('studio.starredOnly')}</button><div className="workspace-sort"><Select aria-label={t('common.sort')} value={sort} disabled={locked} onChange={event=>applySort(event.target.value as ScreenshotSort,descending)}>{(['MODIFIED','NAME','SIZE'] as const).map(key=><option key={key} value={key}>{fileCopy[{MODIFIED:'modified',NAME:'name',SIZE:'size'}[key] as 'modified'|'name'|'size']}</option>)}</Select><button type="button" className="icon-button" disabled={locked} aria-label={fileCopy[descending?'sortDescending':'sortAscending']} title={fileCopy[descending?'sortDescending':'sortAscending']} onClick={()=>applySort(sort,!descending)}><span aria-hidden="true">{descending?'↓':'↑'}</span></button></div><Select aria-label={t('studio.density')} value={density} onChange={e=>setDensity(e.target.value)}><option value="comfortable">{t('studio.comfortable')}</option><option value="compact">{t('studio.compact')}</option></Select></div>
    </div>
    
    {currentFolder&&<div className="studio-folder-strip"><div><strong>{currentFolder.alias}</strong><span title={currentFolder.path}>{currentFolder.note||currentFolder.path}</span></div><button type="button" className="button ghost" disabled={locked} onClick={()=>setFolderEditor(currentFolder)}><Icon name="edit"/>{t('studio.folderSettings')}</button><button type="button" className="icon-button" aria-label={t('studio.removeFolder')} disabled={locked} onClick={()=>setRemove(currentFolder)}><Icon name="trash"/></button></div>}
    <div className="studio-result-heading"><span>{t('studio.showing',{count:images.length,total})}</span><div className="studio-gallery-actions">{tag&&<button type="button" className="studio-active-tag" title={copy.filterTag} onClick={()=>filterTag(tag)}>{tag} ×</button>}{loading&&<span role="status">{t('comic.loading')}</span>}<button type="button" className="button secondary" aria-pressed={selecting} disabled={locked} onClick={()=>{setSelecting(value=>!value);setSelection(new Set());anchor.current=null;}}>{selecting?copy.done:copy.organize}</button></div></div>
    {selecting&&<div className="studio-batch-bar" aria-label={copy.organize}>
      <strong aria-live="polite">{selection.size} {copy.selected}</strong>
      <button type="button" className="button ghost" disabled={locked||loading||!images.length} onClick={()=>setSelection(new Set(images.map(image=>image.id)))}>{copy.selectPage}</button>
      <button type="button" className="button ghost" disabled={locked||!selection.size} onClick={()=>{setSelection(new Set());anchor.current=null;}}>{copy.clear}</button>
      <button type="button" className="button secondary" disabled={locked||loading||!selection.size} onClick={()=>void applyBatch({kind:'star',starred:true})}>★ {t('studio.star')}</button>
      <button type="button" className="button secondary" disabled={locked||loading||!selection.size} onClick={()=>void applyBatch({kind:'star',starred:false})}>☆ {t('studio.unstar')}</button>
      <div className="studio-batch-tags"><input aria-label={copy.tagInput} placeholder={copy.tagInput} value={batchTags} maxLength={2048} aria-invalid={invalidBatchTags||undefined} disabled={locked} onChange={event=>setBatchTags(event.target.value)}/><button type="button" className="button secondary" disabled={locked||loading||!selection.size||!parsedBatchTags.length||invalidBatchTags} onClick={()=>void applyBatch({kind:'addTags',tags:parsedBatchTags})}>{copy.addTags}</button><button type="button" className="button secondary" disabled={locked||loading||!selection.size||!parsedBatchTags.length||invalidBatchTags} onClick={()=>void applyBatch({kind:'removeTags',tags:parsedBatchTags})}>{copy.removeTags}</button></div>
      {invalidBatchTags&&<small className="studio-tag-error" role="alert">{t('studio.tagsLimit')}</small>}
      <div className="studio-batch-link"><WorkPicker value={batchTarget} onChange={setBatchTarget} disabled={locked} nullLabel={t('studio.inheritFolder')}/><button type="button" className="button secondary" aria-label={copy.link} disabled={locked||loading||!selection.size} onClick={()=>void applyBatch({kind:'link',target:batchTarget})}>{copy.applyLink}</button></div>
      <small className="studio-batch-hint">{copy.batchHint}</small>
    </div>}
    {failed?<EmptyState icon="info" title={t('studio.loadFailed')} action={<button type="button" className="button secondary" onClick={()=>setRefresh(value=>value+1)}>{t('studio.refresh')}</button>}/>:!images.length&&!loading?<EmptyState icon="image" title={t(folders.length?'studio.noResults':'studio.emptyGallery')} description={!folders.length?t('studio.emptyGalleryHint'):undefined} action={!folders.length?<button type="button" className="button primary" disabled={locked} onClick={()=>void importFolders()}>{t('studio.import')}</button>:undefined}/>:<div className={`studio-gallery-grid is-${density}`} aria-busy={loading}>{images.map(image=><article {...drag.bind(image)} className="studio-image-card" data-selected={selecting&&selection.has(image.id)} key={image.id}><button data-native-drag-surface type="button" className="studio-image-open" disabled={loading||busy} aria-label={image.fileName} aria-pressed={selecting?selection.has(image.id):undefined} onClick={event=>selecting?selectImage(image.id,event.shiftKey):setActiveImage(image.id)}>{selecting&&<span className="studio-image-selection" data-checked={selection.has(image.id)} aria-hidden="true">{selection.has(image.id)&&<Icon name="check"/>}</span>}{active&&<ScreenshotRaster image={image}/>}</button><div className="studio-image-caption"><strong title={image.fileName}>{image.fileName}</strong><button className="studio-card-star" type="button" aria-label={`${t(image.starred?'studio.unstar':'studio.star')} · ${image.fileName}`} aria-pressed={image.starred} title={t(image.starred?'studio.unstar':'studio.star')} disabled={busy} onClick={event=>{event.stopPropagation();void toggleStar(image);}}><Icon name="star"/></button></div><div className="studio-image-tags">{image.tags.slice(0,3).map(value=><button type="button" key={value} aria-pressed={tag===value} title={copy.filterTag} disabled={busy} onClick={()=>filterTag(value)}>{value}</button>)}{image.tags.length>3&&<small title={image.tags.slice(3).join(' · ')}>+{image.tags.length-3}</small>}</div><small className="studio-image-summary" title={image.note||image.tags.join(' · ')}>{image.note||image.tags.join(' · ')||(image.target?(image.targetTitle||t('studio.linkWork')):'\u00a0')}</small>{(image.target??folders.find(folder=>folder.id===image.folderId)?.target)&&<div className="studio-card-links"><button type="button" className="button ghost" disabled={busy} onClick={()=>void onOpenTarget((image.target??folders.find(folder=>folder.id===image.folderId)?.target)!,image.nodeId??folders.find(folder=>folder.id===image.folderId)?.nodeId??undefined).catch(error=>onError(errorMessage(error)))}><Icon name="external"/>{t('studio.openWork')}</button>{onOpenNote&&<button type="button" className="button ghost" disabled={busy} onClick={()=>onOpenNote((image.target??folders.find(folder=>folder.id===image.folderId)?.target)!,image.nodeId??folders.find(folder=>folder.id===image.folderId)?.nodeId??undefined)}><Icon name="edit"/>{t('studio.viewNote')}</button>}</div>}</article>)}</div>}
    {(offset>0||offset+images.length<total)&&<div className="studio-pagination"><button type="button" className="button secondary" aria-label={t('studio.previous')} disabled={loading||offset===0} onClick={()=>setOffset(value=>Math.max(0,value-48))}><Icon name="arrow-left"/></button><button type="button" className="button secondary" disabled={loading||offset+images.length>=total} onClick={()=>setOffset(value=>value+48)}>{t('studio.more')}</button></div>}
    {active&&folderManager&&<FolderManager folders={folders} busy={locked} onClose={()=>setFolderManager(false)} onEdit={folder=>{setFolderManager(false);setFolderEditor(folder);}}/>}
    {active&&folderEditor&&<FolderEditor folder={folderEditor} busy={locked} onClose={()=>setFolderEditor(null)} onRefresh={()=>run(()=>processFolders([()=>api.refreshScreenshotFolder(folderEditor.id)]))} onRemove={()=>{setRemove(folderEditor);setFolderEditor(null);}} onSave={(alias,note,link)=>run(async()=>{await api.updateScreenshotFolder(folderEditor.id,alias,note,link);if(mounted.current){setFolderEditor(null);setRefresh(value=>value+1);}})}/>}
    <ConfirmDialog open={active&&Boolean(remove)} title={t('studio.removeTitle',{name:remove?.alias??''})} description={t('studio.removeHint')} confirmLabel={t('studio.removeFolder')} destructive busy={locked} onClose={()=>setRemove(null)} onConfirm={()=>void run(async()=>{await api.removeScreenshotFolder(remove!.id);if(mounted.current){setRemove(null);setFolderId('');setOffset(0);setRefresh(value=>value+1);}})}/>
    {active&&openImage&&<ScreenshotLightbox image={openImage} position={offset+images.indexOf(openImage)+1} total={total} busy={busy||paging} paging={paging} onFilterTag={filterTag} onClose={()=>{setActiveImage(null);silentRefresh.current=true;setRefresh(value=>value+1);}} onPrevious={offset+images.indexOf(openImage)>0?()=>void moveImage(-1):undefined} onNext={offset+images.indexOf(openImage)+1<total?()=>void moveImage(1):undefined} onSaved={saveImage} onOpenTarget={onOpenTarget} onOpenNote={onOpenNote?((target,nodeId)=>{setActiveImage(null);onOpenNote(target,nodeId);}):undefined} onError={onError} linkedNodeId={openImage.nodeId??folders.find(folder=>folder.id===openImage.folderId)?.nodeId??undefined} folderTarget={folders.find(folder=>folder.id===openImage.folderId)?.target??null}/>}</div>;
}

function FolderManager({folders,busy,onClose,onEdit}:{folders:ScreenshotFolder[];busy:boolean;onClose:()=>void;onEdit:(folder:ScreenshotFolder)=>void}) {
  const {t}=useI18n();const id=useId(),backdrop=useRef<HTMLDivElement>(null),dialog=useRef<HTMLElement>(null);
  useModalFocus(backdrop,dialog,()=>{if(!dismissWorkPicker(dialog.current))onClose();},undefined,busy);
  return createPortal(<div ref={backdrop} className="modal-backdrop"><section ref={dialog} tabIndex={-1} role="dialog" aria-modal="true" aria-labelledby={id} className="studio-folder-dialog"><header><h2 id={id}>{t('studio.manageFolders')}</h2><button className="icon-button" type="button" disabled={busy} aria-label={t('common.close')} onClick={onClose}><Icon name="close"/></button></header><div className="studio-folder-manager-list">{folders.map(folder=><button type="button" className="studio-folder-manager-row" key={folder.id} disabled={busy} onClick={()=>onEdit(folder)}><Icon name="folder-open"/><span><strong>{folder.alias}</strong><small>{t('studio.folderCount',{count:folder.imageCount})}</small><small className="studio-folder-path">{folder.path}</small></span><Icon name="chevron"/></button>)}</div></section></div>,document.body);
}

function FolderEditor({folder,busy,onClose,onSave,onRefresh,onRemove}:{folder:ScreenshotFolder;busy:boolean;onClose:()=>void;onSave:(alias:string,note:string,target:CollectionTarget|null)=>Promise<void>;onRefresh:()=>Promise<void>;onRemove:()=>void}) {
  const {t}=useI18n();const id=useId(),backdrop=useRef<HTMLDivElement>(null),dialog=useRef<HTMLElement>(null);const [alias,setAlias]=useState(folder.alias),[note,setNote]=useState(folder.note),[target,setTarget]=useState(folder.target);
  useModalFocus(backdrop,dialog,()=>{if(!dismissWorkPicker(dialog.current))onClose();},undefined,busy);
  return createPortal(<div ref={backdrop} className="modal-backdrop"><section ref={dialog} tabIndex={-1} role="dialog" aria-modal="true" aria-labelledby={id} className="studio-folder-dialog"><header><h2 id={id}>{t('studio.folderSettings')}</h2><button className="icon-button" type="button" disabled={busy} aria-label={t('common.close')} onClick={onClose}><Icon name="close"/></button></header><label>{t('studio.folderName')}<input value={alias} maxLength={200} disabled={busy} onChange={e=>setAlias(e.target.value)}/></label><p className="studio-folder-path">{folder.path}</p><label>{t('studio.folderNote')}<textarea rows={4} maxLength={10000} value={note} disabled={busy} onChange={e=>setNote(e.target.value)}/></label><WorkPicker value={target} valueNodeId={folder.nodeId} onChange={setTarget} disabled={busy}/><div className="studio-folder-maintenance"><button className="button ghost" type="button" disabled={busy} onClick={()=>void onRefresh()}><Icon name="refresh"/>{t('studio.refresh')}</button><button className="button ghost" type="button" disabled={busy} onClick={onRemove}><Icon name="trash"/>{t('studio.removeFolder')}</button></div><div className="dialog-actions"><button className="button ghost" type="button" disabled={busy} onClick={onClose}>{t('common.cancel')}</button><button className="button primary" type="button" disabled={busy||!alias.trim()} onClick={()=>void onSave(alias.trim(),note,target)}>{t('note.save')}</button></div></section></div>,document.body);
}

type ScreenshotDraft = {image:ScreenshotImage;tags:string; baseline:ScreenshotImage; error:string|null};
// Failed drafts survive an exceptional view unmount. Successful saves remove their entry.
const screenshotDrafts=new Map<number,ScreenshotDraft>();
const parseScreenshotTags=(value:string)=>[...new Set(value.split(/[,，]/).map(tag=>tag.trim()).filter(Boolean))];
const invalidScreenshotTags=(tags:string[])=>tags.length>20||tags.some(tag=>[...tag].length>80)||tags.reduce((sum,tag)=>sum+[...tag].length,0)>1000;
const screenshotMetadata=(value:ScreenshotImage)=>JSON.stringify([value.note,value.tags,value.starred,value.ownTarget??null]);

function ScreenshotLightbox({image,position,total,busy,paging,onFilterTag,onClose,onPrevious,onNext,onSaved,onOpenTarget,onOpenNote,onError,folderTarget,linkedNodeId}:{image:ScreenshotImage;position:number;total:number;busy:boolean;paging:boolean;onFilterTag:(tag:string)=>void;onClose:()=>void;onPrevious?:()=>void;onNext?:()=>void;onSaved:(image:ScreenshotImage)=>void;onOpenTarget:OpenTarget;onOpenNote?:(target:CollectionTarget,nodeId?:number)=>void;onError:(message:string)=>void;folderTarget:CollectionTarget|null;linkedNodeId?:number}) {
  const {t,language}=useI18n(),copy=round8GalleryCopy[language];
  const id=useId(),backdrop=useRef<HTMLDivElement>(null),dialog=useRef<HTMLElement>(null);
  const [draft,setDraft]=useState(image),[tags,setTags]=useState(image.tags.join(', ')),[saving,setSaving]=useState(false),[saved,setSaved]=useState(false),[saveError,setSaveError]=useState<string|null>(null),[actual,setActual]=useState(false),[leaving,setLeaving]=useState(false);
  const current=useRef<ScreenshotDraft>({image,tags:image.tags.join(', '),baseline:image,error:null}),mounted=useRef(true),generation=useRef(0),timer=useRef(0),inflight=useRef<Promise<boolean>|null>(null),navigation=useRef(false);
  const callbacks=useRef({onSaved,onError});callbacks.current={onSaved,onError};
  const dirty=()=>{const value=current.current;return invalidScreenshotTags(parseScreenshotTags(value.tags))||screenshotMetadata({...value.image,tags:parseScreenshotTags(value.tags)})!==screenshotMetadata(value.baseline);};
  const clearTimer=()=>{window.clearTimeout(timer.current);timer.current=0;};
  const flush=():Promise<boolean>=>{
    clearTimer();
    if(inflight.current)return inflight.current;
    if(invalidScreenshotTags(parseScreenshotTags(current.current.tags)))return Promise.resolve(false);
    if(!dirty())return Promise.resolve(true);
    const token=generation.current;
    if(mounted.current){setSaving(true);setSaveError(null);}
    const task=(async()=>{
      try {
        while(token===generation.current&&dirty()){
          const record=current.current,parsed=parseScreenshotTags(record.tags);
          if(invalidScreenshotTags(parsed))return false;
          const sent={...record.image,revision:record.baseline.revision,tags:parsed};
          const next=await api.updateScreenshot(sent);
          if(token!==generation.current)return false;
          // Keep edits made while the write was in flight, adopting only the confirmed revision.
          const latest=current.current;
          latest.baseline=next;latest.error=null;
          latest.image={...next,note:latest.image.note,starred:latest.image.starred,ownTarget:latest.image.ownTarget,tags:parseScreenshotTags(latest.tags)};
          if(mounted.current)setDraft(latest.image);callbacks.current.onSaved(next);
          if(dirty())screenshotDrafts.set(next.id,{...latest});else screenshotDrafts.delete(next.id);
        }
        if(mounted.current&&token===generation.current){setSaved(true);setSaveError(null);}
        return true;
      }catch(error){
        if(token===generation.current){const text=errorMessage(error);current.current.error=text;screenshotDrafts.set(current.current.image.id,{...current.current});if(mounted.current){setSaveError(text);callbacks.current.onError(text);}else notifyToast('error',text);}
        return false;
      }finally{if(token===generation.current){inflight.current=null;if(mounted.current)setSaving(false);}}
    })();
    inflight.current=task;return task;
  };
  const flushRef=useRef(flush);flushRef.current=flush;
  useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;clearTimer();void flushRef.current();};},[]);
  useLayoutEffect(()=>{
    generation.current++;clearTimer();navigation.current=false;inflight.current=null;
    const retained=screenshotDrafts.get(image.id);
    current.current=retained??{image,tags:image.tags.join(', '),baseline:image,error:null};
    setDraft(current.current.image);setTags(current.current.tags);setSaveError(current.current.error);setSaved(false);setSaving(false);setLeaving(false);
  },[image.id]);
  const edit=(patch:Partial<ScreenshotImage>,tagText?:string,immediate=false)=>{
    const value=current.current;
    value.image={...value.image,...patch};if(tagText!==undefined)value.tags=tagText;
    value.error=null;setDraft(value.image);setTags(value.tags);setSaved(false);setSaveError(null);
    screenshotDrafts.set(value.image.id,{...value});clearTimer();
    if(immediate)void flush();else timer.current=window.setTimeout(()=>void flushRef.current(),450);
  };
  const navigate=(action:(next?:ScreenshotImage)=>void)=>{
    if(navigation.current||paging||busy)return;
    // Clean navigation stays synchronous, retaining keyboard repeat and focus semantics.
    if(!dirty()&&!inflight.current){action(current.current.baseline);return;}
    navigation.current=true;setLeaving(true);
    void flush().then(ok=>{if(ok&&mounted.current)action(current.current.baseline);}).finally(()=>{navigation.current=false;if(mounted.current)setLeaving(false);});
  };
  useModalFocus(backdrop,dialog,()=>{if(!dismissWorkPicker(dialog.current))navigate(onClose);},'[data-modal-initial-focus]',paging||leaving);
  useEffect(()=>{const key=(event:KeyboardEvent)=>{
    if(event.isComposing||event.defaultPrevented)return;
    const element=event.target instanceof HTMLElement?event.target:null;
    if(event.ctrlKey||event.metaKey||event.altKey||element?.isContentEditable||element?.closest('input,textarea,select,[role="combobox"],.studio-work-picker'))return;
    if(event.key==='ArrowLeft'&&onPrevious){event.preventDefault();navigate(onPrevious);}if(event.key==='ArrowRight'&&onNext){event.preventDefault();navigate(onNext);}
  };window.addEventListener('keydown',key);return()=>window.removeEventListener('keydown',key);});
  const parsedTags=parseScreenshotTags(tags),invalidTags=invalidScreenshotTags(parsedTags),locked=busy||leaving;
  const link=draft.ownTarget===undefined?image.target??folderTarget:draft.ownTarget??folderTarget;
  return createPortal(<div ref={backdrop} className="modal-backdrop studio-lightbox-backdrop"><section ref={dialog} className="studio-lightbox" tabIndex={-1} role="dialog" aria-modal="true" aria-labelledby={id} aria-busy={paging||leaving}>
    <header><div><h2 id={id}>{image.fileName}</h2><span>{t('studio.imagePosition',{current:position,total})} · {image.width} × {image.height}</span></div><button data-modal-initial-focus className="icon-button" type="button" aria-label={t('common.close')} disabled={leaving||paging} onClick={()=>navigate(onClose)}><Icon name="close"/></button></header>
    <div className="studio-lightbox-content"><div className={`studio-lightbox-stage${actual?' is-actual':''}`}><div className="studio-zoom-tools"><button type="button" className="button secondary" aria-pressed={!actual} onClick={()=>setActual(false)}>{copy.fit}</button><button type="button" className="button secondary" aria-pressed={actual} onClick={()=>setActual(true)}>{copy.actual}</button><small>{paging?t('comic.loading'):copy.viewHint}</small></div><ScreenshotRaster key={`${image.id}:${image.revision}`} image={image} thumbnail={false}/><div className="studio-image-nav"><button className="button secondary" type="button" aria-label={t('studio.previous')} disabled={!onPrevious||leaving||paging} onClick={()=>onPrevious&&navigate(onPrevious)}><Icon name="arrow-left"/>{t('studio.previous')}</button><button className="button secondary" type="button" aria-label={t('studio.next')} disabled={!onNext||leaving||paging} onClick={()=>onNext&&navigate(onNext)}>{t('studio.next')}<Icon name="chevron"/></button></div></div>
    <aside className="studio-image-inspector">
      <button type="button" className="button secondary studio-star-button" aria-pressed={draft.starred} disabled={locked} onClick={()=>edit({starred:!current.current.image.starred},undefined,true)}><Icon name="star"/>{t(draft.starred?'studio.unstar':'studio.star')}</button>
      <div className="studio-link-field"><span>{t('studio.linkWork')}</span><WorkPicker value={draft.ownTarget??null} valueNodeId={image.nodeId} nullLabel={folderTarget?t('studio.inheritFolder'):undefined} disabled={locked} onChange={target=>edit({ownTarget:target},undefined,true)}/></div>
      <label>{t('studio.imageNote')}<textarea rows={5} maxLength={10000} value={draft.note} disabled={locked} onChange={e=>edit({note:e.target.value})} onBlur={()=>void flush()}/></label>
      <label>{t('studio.tags')}<input value={tags} aria-invalid={invalidTags||undefined} aria-describedby={invalidTags?`${id}-tags-error`:undefined} maxLength={2048} disabled={locked} onChange={e=>edit({},e.target.value)} onBlur={()=>void flush()}/></label>{invalidTags&&<small id={`${id}-tags-error`} className="studio-tag-error" role="alert">{t('studio.tagsLimit')}</small>}
      {image.tags.length>0&&<div className="studio-image-tags">{image.tags.map(tag=><button type="button" key={tag} title={copy.filterTag} disabled={locked} onClick={()=>navigate(()=>onFilterTag(tag))}>{tag}</button>)}</div>}
      {saveError&&<div className="studio-save-error" role="alert"><p>{saveError}</p><button type="button" className="button secondary" disabled={saving||locked||invalidTags} onClick={()=>void flush()}>{t('common.retry')}</button></div>}
      <div className="studio-inspector-actions">{(saving||saved)&&<small role="status">{t(saving?'note.saving':'studio.saved')}</small>}<button type="button" className="button ghost" onClick={()=>void api.revealScreenshot(image.id,image.revision).catch(error=>onError(errorMessage(error)))}><Icon name="folder-open"/>{t('detail.openExplorer')}</button>{link&&<button type="button" className="button ghost" disabled={locked} onClick={()=>navigate(()=>{void onOpenTarget(link,linkedNodeId).then(onClose).catch(error=>onError(errorMessage(error)));})}><Icon name="external"/>{t('studio.openWork')}</button>}{link&&onOpenNote&&<button type="button" className="button secondary" disabled={locked} onClick={()=>navigate(next=>{const destination=next?(next.ownTarget===undefined?next.target??folderTarget:next.ownTarget??folderTarget):link;if(destination)onOpenNote(destination,destination.kind==='NODE'?destination.id:next?.nodeId??linkedNodeId);})}><Icon name="edit"/>{t('studio.viewNote')}</button>}</div>
    </aside></div></section></div>,document.body);
}

import {useEffect, useLayoutEffect, useId, useRef, useState, type KeyboardEvent} from 'react';
import {notifyToast} from '../lib/toast';
import {api, desktopAvailable} from '../lib/api';
import {noteMarkdown, saveMarkdown} from '../lib/noteMarkdown';
import {errorMessage, nodeDisplayTitle} from '../lib/format';
import {useI18n} from '../lib/i18n';
import type {LibraryRoot} from '../types/media';
import type {CollectionTarget} from '../types/collection';
import {targetKey, type WorkNoteEntry, type WorkNoteSort, type WorkbenchSortPreferences, type WorkbenchSortUpdate} from '../types/workbench';
import {Select} from '../components/Select';
import {EmptyState} from '../components/EmptyState';
import {Icon} from '../components/Icon';
import {StudioNoteEditor, flushPrivateNotes} from '../components/PrivateNote';
import {takeStudioLink} from '../lib/openStudio';
import {ScreenshotStudio} from '../components/ScreenshotStudio';
import {WorkspaceFiles} from '../components/WorkspaceFiles';
import {workspaceCopy} from '../lib/workspaceCopy';
import type {WorkspaceRequest,WorkbenchLocation} from '../types/workspace';
import '../styles/workbenchStudio.css';

export interface WorkbenchProps {
  active?:boolean; onContentReady?:()=>void; location?:WorkbenchLocation; onLocationChange?:(next:WorkbenchLocation,mode?:'push'|'replace'|'parent')=>void;
  roots: LibraryRoot[]; revision: number; refreshKey?: number; busy: boolean;
  sortPreferences?: WorkbenchSortPreferences;
  onSortPreferenceChange?: (update: WorkbenchSortUpdate) => void;
  onOpenTarget: (target: CollectionTarget, nodeId?: number) => Promise<void>;
  onError: (message: string) => void;
}
export function OrganizePage(props: WorkbenchProps) {
  const {t,language} = useI18n(); const id = useId();
  const tabs=['notes','gallery','files'] as const;
  type View=typeof tabs[number];
  const [view, setView] = useState<View>(props.location?.view??'notes');
  const active=props.active??true,activeRef=useRef(active);activeRef.current=active;
  const locationRef=useRef(props.location);locationRef.current=props.location;
  const [visitedFiles,setVisitedFiles]=useState(props.location?.view==='files'),[filesRequest,setFilesRequest]=useState<WorkspaceRequest|null>(null);
  const pageRef=useRef<HTMLElement>(null),viewRef=useRef<View>(props.location?.view??'notes'),scrollPositions=useRef<Record<View,number>>({notes:0,gallery:0,files:0});
  const [visitedGallery, setVisitedGallery] = useState(props.location?.view==='gallery');
  const [galleryRequest, setGalleryRequest] = useState<StudioTargetRequest | null>(null);
  const [noteRequest, setNoteRequest] = useState<StudioTargetRequest | null>(null);
  const requestSequence = useRef(0);
  const switchView = (next: View,notify=true) => {
    if(next===viewRef.current)return;
    const scroller=pageRef.current?.closest<HTMLElement>('.content-scroll');
    if(activeRef.current&&scroller)scrollPositions.current[viewRef.current]=scroller.scrollTop;
    if (next === 'gallery') setVisitedGallery(true);
    if(next==='files')setVisitedFiles(true);
    viewRef.current=next;
    setView(next);
    if(notify)props.onLocationChange?.({...locationRef.current,view:next,workspace:locationRef.current?.workspace??{folderId:null,relativeDir:''}},'replace');
  };
  useLayoutEffect(()=>{if(props.location)switchView(props.location.view,false);},[props.location?.view]);
  useLayoutEffect(()=>{if(!active)return;const scroller=pageRef.current?.closest<HTMLElement>('.content-scroll');if(scroller)scroller.scrollTop=scrollPositions.current[view];},[view]);
  const openGallery = (row: WorkNoteEntry) => {
    setGalleryRequest({target: row.target, nodeId: row.nodeId, title: row.title, sequence: ++requestSequence.current});
    switchView('gallery');
  };
  const openNote = (target: CollectionTarget, nodeId?: number) => {
    setNoteRequest({target, nodeId, sequence: ++requestSequence.current});
    switchView('notes');
  };
  useEffect(() => {
    const apply = () => {
      if(!activeRef.current)return;
      const link = takeStudioLink(); if (!link) return;
      if (link.view === 'notes') openNote(link.target, link.nodeId);
      else if(link.view==='files'){setFilesRequest({folderId:link.folderId,sequence:++requestSequence.current});switchView('files');}
      else {setGalleryRequest({target: null, screenshotId: link.screenshotId, sequence: ++requestSequence.current}); switchView('gallery');}
    };
    apply(); window.addEventListener('m2shelf-open-studio', apply);
    return () => window.removeEventListener('m2shelf-open-studio', apply);
  }, [active]);
  useEffect(() => {
    const shortcut = (event: globalThis.KeyboardEvent) => {
      if (!activeRef.current || event.defaultPrevented || event.isComposing || !event.ctrlKey || event.altKey || event.metaKey || !['1', '2','3'].includes(event.key)) return;
      // Modal edits own their keyboard scope, including unsaved screenshot drafts.
      if (document.querySelector('[aria-modal="true"], dialog[open]')) return;
      event.preventDefault();
      const next = tabs[Number(event.key)-1];
      switchView(next); document.getElementById(`${id}-${next}`)?.focus();
    };
    window.addEventListener('keydown', shortcut);
    return () => window.removeEventListener('keydown', shortcut);
  }, [id]);
  const switchKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey || event.nativeEvent.isComposing || !['ArrowLeft','ArrowRight','Home','End'].includes(event.key)) return;
    const next = event.key === 'Home' ? 'notes' : event.key === 'End' ? 'files' : tabs[(tabs.indexOf(view)+(event.key==='ArrowRight'?1:2))%3];
    if(next===view)return;
    const button=document.getElementById(`${id}-${next}`);if(!button)return;
    event.preventDefault();switchView(next);button.focus();button.scrollIntoView?.({block:'nearest',inline:'nearest'});
  };
  return <section ref={pageRef} className="studio-page">
    <header className="studio-heading"><h1>{t('studio.title')}</h1>
      <div className="studio-switch" data-view={view} role="tablist" aria-label={t('studio.views')}>{tabs.map((tab,index) => <button key={tab} id={`${id}-${tab}`} type="button" role="tab" aria-selected={view === tab} aria-controls={`${id}-${tab}-panel`} aria-keyshortcuts={`Control+${index+1}`} title={`${tab==='files'?workspaceCopy[language].title:t(`studio.${tab}`)} · Ctrl+${index+1}`} tabIndex={view === tab ? 0 : -1} onKeyDown={switchKey} onClick={() => switchView(tab)}><Icon name={tab === 'notes' ? 'edit' : tab==='gallery'?'image':'folder'}/>{tab==='files'?workspaceCopy[language].title:t(`studio.${tab}`)}</button>)}</div>
    </header>
    <div className="studio-panel" role="tabpanel" id={`${id}-notes-panel`} aria-labelledby={`${id}-notes`} hidden={view !== 'notes'}><WorkNotesStudio {...props} request={noteRequest} onOpenGallery={openGallery}/></div>
    <div className="studio-panel" role="tabpanel" id={`${id}-gallery-panel`} aria-labelledby={`${id}-gallery`} hidden={view !== 'gallery'}>{visitedGallery && <ScreenshotStudio session={props.location?.gallery} onSessionChange={gallery=>props.onLocationChange?.({...locationRef.current,view:viewRef.current,workspace:locationRef.current?.workspace??{folderId:null,relativeDir:''},gallery},'replace')} sortPreference={props.sortPreferences ? {sort:props.sortPreferences.gallerySort,descending:props.sortPreferences.galleryDescending} : undefined} onSortChange={(sort,descending)=>props.onSortPreferenceChange?.({scope:'gallery',sort,descending})} busy={props.busy} active={active&&view === 'gallery'} request={galleryRequest} onOpenNote={openNote} onError={props.onError} onOpenTarget={props.onOpenTarget}/>}</div>
    <div className="studio-panel" role="tabpanel" id={`${id}-files-panel`} aria-labelledby={`${id}-files`} hidden={view!=='files'}>{visitedFiles&&<WorkspaceFiles onContentReady={props.onContentReady} busy={props.busy} active={active&&view==='files'} request={filesRequest} location={props.location?.workspace} onLocationChange={(workspace,mode)=>props.onLocationChange?.({...locationRef.current,view:viewRef.current,workspace},mode)} sortPreference={props.sortPreferences ? {sort:props.sortPreferences.filesSort,descending:props.sortPreferences.filesDescending} : undefined} onSortChange={(sort,descending)=>props.onSortPreferenceChange?.({scope:'files',sort,descending})} onError={props.onError} onOpenTarget={props.onOpenTarget}/>}</div>
  </section>;
}

export interface StudioTargetRequest {target: CollectionTarget | null; nodeId?: number; title?: string; sequence: number; screenshotId?: number;}

function WorkNotesStudio({roots,revision,refreshKey,onError,onOpenTarget,request,onOpenGallery,sortPreferences,onSortPreferenceChange}: WorkbenchProps & {request:StudioTargetRequest|null;onOpenGallery:(row:WorkNoteEntry)=>void}) {
  const {t,language}=useI18n();
  const persistedSort = sortPreferences?.notesSort;
  const userSorted = useRef(false);
  const [sort,setSort]=useState<WorkNoteSort>(persistedSort ?? 'recent'), [exportBusy,setExportBusy]=useState(false);
  useEffect(()=>{if(persistedSort && !userSorted.current)setSort(persistedSort);},[persistedSort]);
  const exportAll=async()=>{
    setExportBusy(true);
    try {
      await flushPrivateNotes();
      let entries:WorkNoteEntry[]=[];
      if(desktopAvailable)entries=await api.exportWorkNoteEntries();
      else {
        for(let start=0;start<10000;start+=100){const page=await api.listWorkNotes({notesOnly:true,rootId:null,mediaKind:null,query:'',offset:start,limit:100,sort:'title'});entries.push(...page.items);if(page.total>10000)throw new Error('NOTE_LIMIT');if(start+page.items.length>=page.total)break;if(!page.items.length)throw new Error('NOTE_CONFLICT');}
      }
      if(await saveMarkdown(noteMarkdown(entries,language),'M2Shelf-notes',language))notifyToast('success',t('knowledge.exported'));
    } catch {notifyToast('error',t('knowledge.failed'));}finally{setExportBusy(false);}
  };
  const [notesOnly,setNotesOnly]=useState(true), [query,setQuery]=useState(''), [root,setRoot]=useState(''), [media,setMedia]=useState('');
  const [rows,setRows]=useState<WorkNoteEntry[]>([]), [total,setTotal]=useState(0), [selected,setSelected]=useState<WorkNoteEntry|null>(null);
  const [offset,setOffset]=useState(0), [refresh,setRefresh]=useState(0), [loading,setLoading]=useState(true), [failed,setFailed]=useState(false);
  const generation=useRef(0), errorRef=useRef(onError), listRef=useRef<HTMLDivElement>(null);errorRef.current=onError;
  const paperRef=useRef<HTMLElement>(null),linkedSelection=useRef<string|null>(null);
  const confirmed=useRef(new Map<string,{body:string;revision:number;updatedAt:string}>());
  useEffect(()=>{
    const token=++generation.current;setLoading(true);setFailed(false);
    const timer=window.setTimeout(()=>{void api.listWorkNotes({notesOnly,rootId:root?Number(root):null,mediaKind:media||null,query:query.trim(),offset,limit:60,sort}).then(page=>{
      if(token!==generation.current)return;
      const items=page.items.map(row=>{const latest=confirmed.current.get(targetKey(row.target));return latest&&latest.revision>row.revision?{...row,...latest}:row;});
      setRows(items);setTotal(page.total);
      setSelected(current=>items.find(row=>targetKey(row.target)===targetKey(current?.target??null))??(linkedSelection.current===targetKey(current?.target)?current:null)??items[0]??null);
    }).catch(error=>{if(token===generation.current){setFailed(true);errorRef.current(errorMessage(error));}}).finally(()=>{if(token===generation.current)setLoading(false);});},query?180:0);
    return()=>{generation.current++;window.clearTimeout(timer);};
  },[notesOnly,root,media,query,offset,refresh,revision,refreshKey,language,sort]);
  useEffect(()=>{
    if(!request?.target)return;
    const requestedTarget = request.target;
    let active=true;linkedSelection.current=targetKey(request.target);
    const focusEditor=()=>{
      if(paperRef.current?.closest('[hidden]'))return false;
      const editor=paperRef.current?.querySelector<HTMLTextAreaElement>('textarea');
      if(paperRef.current?.dataset.noteTarget===targetKey(request.target)&&editor&&!editor.disabled){editor.focus({preventScroll:true});return true;}
      return false;
    };
    const observer=new MutationObserver(()=>{if(focusEditor())observer.disconnect();});
    if(paperRef.current)observer.observe(paperRef.current,{childList:true,subtree:true,attributes:true,attributeFilter:['disabled','data-note-target']});
    void (async()=>{
      try{
        let row=rows.find(item=>targetKey(item.target)===targetKey(request.target));
        if(!row){
          const nodeId=requestedTarget.kind==='NODE'?requestedTarget.id:request.nodeId;
          if(!nodeId)throw new Error(t('studio.linkUnavailable'));
          const detail=await api.nodeDetail(nodeId);
          const library=roots.find(item=>item.id===detail.node.libraryRootId);
          let title=nodeDisplayTitle({...detail.node,binding:detail.binding},language);
          if(requestedTarget.kind==='BOOK'){
            const book=(await api.comicDetail(nodeId)).find(item=>item.id===requestedTarget.id);
            if(!book)throw new Error(t('studio.linkUnavailable'));
            title=book.displayName;
          }
          row={target:requestedTarget,nodeId,title,libraryId:detail.node.libraryRootId,libraryName:library?.displayName??'',mediaKind:library?.mediaKind??'VIDEO',body:'',revision:0,updatedAt:null};
        }
        if(active&&linkedSelection.current===targetKey(request.target)){setSelected(row);if(focusEditor())observer.disconnect();}
      }catch(error){if(active){observer.disconnect();errorRef.current(errorMessage(error));}}
    })();
    return()=>{active=false;observer.disconnect();};
  },[request,language]);
  useEffect(()=>{const saved=(event:Event)=>{
    const detail=(event as CustomEvent<{target:CollectionTarget;body:string;revision:number}>).detail;if(!detail)return;
    const latest={body:detail.body,revision:detail.revision,updatedAt:new Date().toISOString()};confirmed.current.set(targetKey(detail.target),latest);
    setRows(current=>current.map(row=>targetKey(row.target)===targetKey(detail.target)?{...row,...latest}:row));
    setSelected(current=>current&&targetKey(current.target)===targetKey(detail.target)?{...current,...latest}:current);
    setRefresh(value=>value+1);
  };window.addEventListener('m2shelf-note-saved',saved);return()=>window.removeEventListener('m2shelf-note-saved',saved);},[]);
  const filter=(setter:(value:string)=>void,value:string)=>{linkedSelection.current=null;setOffset(0);setter(value);};
  const moveSelection=(event:KeyboardEvent<HTMLButtonElement>,index:number)=>{
    if(event.defaultPrevented||event.ctrlKey||event.metaKey||event.altKey||event.shiftKey||event.nativeEvent.isComposing||!['ArrowUp','ArrowDown','Home','End'].includes(event.key))return;
    const next=event.key==='Home'?0:event.key==='End'?rows.length-1:Math.max(0,Math.min(rows.length-1,index+(event.key==='ArrowDown'?1:-1)));
    if(next===index||!rows[next])return;
    event.preventDefault();linkedSelection.current=null;setSelected(rows[next]);
    const button=listRef.current?.querySelectorAll<HTMLButtonElement>('.studio-note-row')[next];button?.focus();button?.scrollIntoView?.({block:'nearest',inline:'nearest'});
  };
  return <div className="notes-studio">
    <aside className="studio-note-browser">
      <div className="studio-note-scope">{[true,false].map(value=><button type="button" key={String(value)} aria-pressed={notesOnly===value} onClick={()=>{setOffset(0);setNotesOnly(value);}}>{t(value?'studio.savedNotes':'studio.allWorks')}</button>)}</div>
      <label className="search-field studio-search"><Icon name="search"/><input type="search" aria-label={t('studio.searchNotes')} placeholder={t('studio.searchNotes')} value={query} onChange={e=>filter(setQuery,e.target.value)}/></label>
      <div className="studio-note-filters"><Select aria-label={t('search.scope')} value={root} onChange={e=>filter(setRoot,e.target.value)}><option value="">{t('search.allLibraries')}</option>{roots.map(item=><option value={item.id} key={item.id}>{item.displayName}</option>)}</Select>
      <Select aria-label={t('filters.media')} value={media} onChange={e=>filter(setMedia,e.target.value)}><option value="">{t('filters.media')}</option>{(['ANIMATION','LIVE_ACTION','COMIC','EBOOK','DOUJIN','ARTBOOK'] as const).map(kind=><option key={kind} value={kind}>{t(({ANIMATION:'comic.animation',LIVE_ACTION:'library.liveAction',COMIC:'comic.name',EBOOK:'ebook.name',DOUJIN:'doujin.name',ARTBOOK:'artbook.name'} as const)[kind])}</option>)}</Select></div>
      <div className="studio-result-heading studio-note-sort"><Select aria-label={t('knowledge.sort')} value={sort} onChange={event=>{const next=event.target.value as WorkNoteSort;userSorted.current=true;setSort(next);setOffset(0);onSortPreferenceChange?.({scope:'notes',sort:next});}}><option value="recent">{t('knowledge.recent')}</option><option value="title">{t('knowledge.title')}</option></Select><span className="sr-only" role="status">{loading?t('comic.loading'):t('studio.showing',{count:rows.length,total})}</span><button type="button" className="icon-button" disabled={loading} aria-label={t('studio.refresh')} onClick={()=>setRefresh(value=>value+1)}><Icon name="refresh"/></button></div>
      <div ref={listRef} className="studio-note-list" role="listbox" aria-label={t('studio.notes')} aria-busy={loading}>
        {failed?<p role="alert">{t('studio.loadFailed')}</p>:!rows.length&&!loading?<div className="studio-list-empty"><Icon name="edit"/><strong>{t(notesOnly&&!query&&!root&&!media?'studio.emptyNotes':'studio.noResults')}</strong>{notesOnly&&<button className="button secondary" type="button" onClick={()=>{setNotesOnly(false);setQuery('');setRoot('');setMedia('');setOffset(0);}}>{t('studio.startWriting')}</button>}</div>:rows.map((row,index)=><button type="button" key={targetKey(row.target)} className="studio-note-row" role="option" aria-selected={targetKey(selected?.target??null)===targetKey(row.target)} tabIndex={targetKey(selected?.target??null)===targetKey(row.target)?0:-1} aria-pressed={targetKey(selected?.target??null)===targetKey(row.target)} onKeyDown={event=>moveSelection(event,index)} onClick={()=>{linkedSelection.current=null;setSelected(row);}}><strong>{row.title}</strong>{row.body&&<span>{row.body}</span>}<small>{row.libraryName}{row.updatedAt?` · ${new Intl.DateTimeFormat(language,{month:'short',day:'numeric'}).format(new Date(row.updatedAt))}`:''}</small></button>)}
      </div>
      <button className="button ghost studio-export-all" type="button" disabled={exportBusy} onClick={()=>void exportAll()}>{t('knowledge.exportAll')}</button>
      {(offset>0||offset+rows.length<total)&&<div className="studio-pagination"><button type="button" className="button ghost" aria-label={t('detail.back')} disabled={loading||offset===0} onClick={()=>setOffset(value=>Math.max(0,value-60))}><Icon name="arrow-left"/></button><button type="button" className="button ghost" disabled={loading||offset+rows.length>=total} onClick={()=>setOffset(value=>value+60)}>{t('studio.more')}</button></div>}
    </aside>
    <section ref={paperRef} data-note-target={targetKey(selected?.target)} className="studio-note-paper">{selected?<><header><div><span className="studio-kicker">{selected.libraryName}</span><h2>{selected.title}</h2></div><div className="studio-note-links"><button className="button secondary" type="button" onClick={()=>onOpenGallery(selected)}><Icon name="image"/>{t('studio.viewScreenshots')}</button><button className="button ghost" type="button" onClick={()=>void onOpenTarget(selected.target,selected.nodeId).catch(error=>onError(errorMessage(error)))}><Icon name="external"/>{t('studio.openWork')}</button></div></header><StudioNoteEditor key={targetKey(selected.target)} target={selected.target} entry={selected}/></>:<EmptyState icon="edit" title={t('studio.selectWork')}/>}</section>
  </div>;
}

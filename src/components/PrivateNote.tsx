import {notifyToast} from '../lib/toast';
import {useEffect,useState,useSyncExternalStore} from 'react';
import {api} from '../lib/api';
import {useI18n} from '../lib/i18n';
import type {CollectionTarget} from '../types/collection';
import {desktopAvailable} from '../lib/api';
import {getCurrentWindow} from '@tauri-apps/api/window';
import {Icon} from './Icon';
import {openStudio} from '../lib/openStudio';
import {noteMarkdown, saveMarkdown} from '../lib/noteMarkdown';
import type {WorkNoteEntry} from '../types/workbench';

type Draft={body:string;saved:string;revision:number;ready:boolean;busy:boolean;reloading:boolean;error:boolean};
type Session={target:CollectionTarget;value:Draft;listeners:Set<()=>void>;timer?:ReturnType<typeof setTimeout>;flight?:Promise<void>};
const sessions=new Map<string,Session>();
const emit=(s:Session,patch:Partial<Draft>)=>{s.value={...s.value,...patch};s.listeners.forEach(f=>f());};
function session(target:CollectionTarget){const key=`${target.kind}:${target.id}`;let s=sessions.get(key);if(!s){
 for(const [k,v] of sessions){if(sessions.size<128)break;if(!v.listeners.size&&!v.flight&&v.value.body===v.value.saved)sessions.delete(k);}
 s={target,value:{body:'',saved:'',revision:0,ready:false,busy:false,reloading:false,error:false},listeners:new Set()};sessions.set(key,s);
 }return s;}
async function load(s:Session){if(s.value.ready||s.flight)return;emit(s,{busy:true});s.flight=api.getNote(s.target).then(n=>emit(s,{body:n.body,saved:n.body,revision:n.revision,ready:true,error:false})).catch(()=>emit(s,{error:true})).finally(()=>{s.flight=undefined;emit(s,{busy:false});});await s.flight;}
async function reload(s:Session){
 clearTimeout(s.timer);if(s.flight)return;
 emit(s,{busy:true,reloading:true});
 s.flight=api.getNote(s.target).then(n=>emit(s,{body:n.body,saved:n.body,revision:n.revision,ready:true,error:false})).catch(()=>emit(s,{error:true})).finally(()=>{s.flight=undefined;emit(s,{busy:false,reloading:false});});await s.flight;
}
function changeNote(s:Session,body:string){if(!s.value.ready||s.value.reloading)return;emit(s,{body,error:false});clearTimeout(s.timer);s.timer=setTimeout(()=>void flush(s),650);}
async function flush(s:Session){clearTimeout(s.timer);if(s.flight){await s.flight;if(s.value.ready&&s.value.body!==s.value.saved&&!s.value.error)await flush(s);return;}
 if(!s.value.ready||s.value.body===s.value.saved)return;const body=s.value.body;emit(s,{busy:true,error:false});
 s.flight=api.saveNote(s.target,body,s.value.revision).then(n=>{emit(s,{saved:n.body,revision:n.revision});window.dispatchEvent(new CustomEvent('m2shelf-note-saved',{detail:{target:s.target,body:n.body,revision:n.revision}}));}).catch(()=>emit(s,{error:true})).finally(()=>{s.flight=undefined;emit(s,{busy:false});});await s.flight;
 if(!s.value.error&&s.value.body!==s.value.saved)await flush(s);
}
// Failed drafts survive route changes; an actual window reload warns before losing them.
if(typeof window!=='undefined')window.addEventListener('beforeunload',e=>{for(const s of sessions.values())if(s.value.ready&&s.value.body!==s.value.saved){void flush(s);e.preventDefault();break;}});
export function PrivateNotesLifecycle(){
 useEffect(()=>{if(!desktopAvailable)return;let disposed=false,allowed=false,busy=false;let stop:(()=>void)|undefined;let native;try{native=getCurrentWindow();}catch{return;}
 void native.onCloseRequested(async event=>{if(allowed||![...sessions.values()].some(s=>s.value.ready&&s.value.body!==s.value.saved))return;event.preventDefault();if(busy)return;busy=true;await Promise.all([...sessions.values()].map(flush));busy=false;if([...sessions.values()].some(s=>s.value.ready&&s.value.body!==s.value.saved)){window.dispatchEvent(new Event('m2shelf-note-save-failed'));return;}if(!disposed){allowed=true;void native.close();}}).then(f=>{if(disposed)f();else stop=f;}).catch(()=>undefined);return()=>{disposed=true;stop?.();};},[]);return null;
}
export function PrivateNote({target,detail=false,nodeId}:{target:CollectionTarget;detail?:boolean;nodeId?:number}){
 const{t}=useI18n();const [open,setOpen]=useState(false);const s=session(target);
 const v=useSyncExternalStore(f=>{s.listeners.add(f);return()=>{s.listeners.delete(f);};},()=>s.value);
 useEffect(()=>()=>{void flush(s);},[s]);
 const change=(body:string)=>changeNote(s,body);
 return <details className={`private-note${detail?' detail-private-note':''}`} open={open} onToggle={e=>{setOpen(e.currentTarget.open);if(e.currentTarget.open)void load(s);}} onDoubleClick={e=>e.stopPropagation()} onKeyDown={e=>e.stopPropagation()}>
  <summary>{detail&&<Icon name="edit"/>}<span>{t('note.title')}</span>{detail&&<Icon className="private-note-chevron" name="chevron"/>}</summary>{open&&<div className="private-note-editor">
   {v.ready&&<textarea aria-label={t('note.title')} maxLength={10000} rows={4} disabled={v.reloading} value={v.body} onChange={e=>change(e.target.value)}/>}
   <span role={v.error&&!v.reloading?'alert':'status'}>{t(v.reloading?'comic.loading':v.error?'note.failed':v.busy?'note.saving':!v.ready?'comic.loading':v.body!==v.saved?'note.editing':'note.saved')}</span>
   <div className="private-note-actions"><button className="button ghost" type="button" disabled={v.busy} onClick={()=>void(v.ready?flush(s):load(s))}>{t(v.error?'note.retry':'note.save')}</button>
   <button className="button ghost" type="button" disabled={!v.ready||v.busy||!v.body} onClick={()=>change('')}>{t('note.clear')}</button>{detail&&<button className="button secondary" type="button" onClick={()=>openStudio({view:'notes',target,nodeId})}><Icon name="edit"/>{t('knowledge.edit')}</button>}</div>
   {v.error&&<button className="button ghost" type="button" disabled={v.busy} onClick={()=>void reload(s)}>{t('note.reload')}</button>}
  </div>}
 </details>;
}

/** The studio and detail view share the same draft, CAS revision and close flush. */
export async function flushPrivateNotes() {
 await Promise.all([...sessions.values()].map(flush));
 if ([...sessions.values()].some(s=>s.value.ready && s.value.body!==s.value.saved)) throw new Error('NOTE_CONFLICT');
}
export function StudioNoteEditor({target,entry}:{target:CollectionTarget;entry?:WorkNoteEntry}) {
 const {t,language}=useI18n(); const s=session(target); const [exportBusy,setExportBusy]=useState(false);
 const v=useSyncExternalStore(f=>{s.listeners.add(f);return()=>{s.listeners.delete(f);};},()=>s.value);
 useEffect(()=>{void load(s);return()=>{void flush(s);};},[s]);
 const change=(body:string)=>changeNote(s,body);
 const exportNote=async(copy:boolean)=>{
  if(!entry||!v.ready)return; setExportBusy(true);
  try {
   const markdown=noteMarkdown([{...entry,body:v.body,updatedAt:v.body!==v.saved?new Date().toISOString():entry.updatedAt}],language);
   if(copy){await navigator.clipboard.writeText(markdown);notifyToast('success',t('knowledge.copied'));}
   else if(await saveMarkdown(markdown,entry.title,language))notifyToast('success',t('knowledge.exported'));
  }catch{notifyToast('error',t('knowledge.failed'));}finally{setExportBusy(false);}
 };
 return <div className="studio-note-editor" onKeyDown={event=>{
  if(!event.nativeEvent.isComposing&&(event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='s'){
   event.preventDefault();event.stopPropagation();if(!v.busy)void(v.ready?flush(s):load(s));
  }
 }}>
  <textarea aria-label={t('note.title')} maxLength={10000} rows={14} disabled={!v.ready||v.reloading} placeholder={t('studio.notePlaceholder')} value={v.body} onChange={e=>change(e.target.value)}/>
  <div className="studio-note-footer">
   <div className="studio-note-meta" title={t('studio.autosave')}><span role={v.error&&!v.reloading?'alert':'status'}>{t(v.reloading?'comic.loading':v.error?'studio.saveFailed':v.busy?'note.saving':!v.ready?'comic.loading':v.body!==v.saved?'studio.unsavedShort':'note.saved')}</span><span className="studio-note-count" title={t('studio.characters',{count:[...v.body].length})}>{t('studio.characterCount',{count:[...v.body].length})}</span></div>
   <div className="studio-note-actions"><button className="button ghost" type="button" disabled={!v.ready||v.busy||!v.body} onClick={()=>change('')}>{t('note.clear')}</button><button className="button secondary" type="button" title={`${t('note.save')} · Ctrl+S`} disabled={v.busy} onClick={()=>void(v.ready?flush(s):load(s))}>{t(v.error?'note.retry':'note.save')}</button></div>
  </div>
  {entry&&<div className="studio-export-actions"><button className="button ghost" type="button" disabled={!v.ready||exportBusy} onClick={()=>void exportNote(true)}>{t('knowledge.copy')}</button><button className="button ghost" type="button" disabled={!v.ready||exportBusy} onClick={()=>void exportNote(false)}>{t('knowledge.export')}</button></div>}
  {v.error&&<button className="button secondary" type="button" disabled={v.busy} onClick={()=>void reload(s)}>{t('note.reload')}</button>}
 </div>;
}

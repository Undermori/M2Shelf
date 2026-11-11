import {useEffect,useRef,useState} from 'react';
import {api} from '../lib/api';
import {errorMessage} from '../lib/format';
import {useI18n} from '../lib/i18n';
import type {TidyIssue} from '../types/incremental';
import type {LibraryRoot,MediaNode} from '../types/media';
import {MediaCard} from '../components/MediaCard';
import {Select} from '../components/Select';
import {EmptyState} from '../components/EmptyState';
import {LoadingState} from '../components/LoadingState';
import {Icon} from '../components/Icon';

export function OrganizePage({roots,revision,refreshKey,busy,onOpen,onMatch,onRetryMatch,onRetryCover,onReveal,onMenu,onError}:{roots:LibraryRoot[];revision:number;refreshKey?:number;busy:boolean;onOpen:(node:MediaNode)=>void;onMatch:(node:MediaNode)=>void;onRetryMatch:(node:MediaNode)=>Promise<unknown>;onRetryCover:(node:MediaNode)=>Promise<unknown>;onReveal:(node:MediaNode)=>Promise<unknown>;onMenu:(e:React.MouseEvent,node:MediaNode)=>void;onError:(message:string)=>void}) {
  const {t}=useI18n();
  const [rows,setRows]=useState<TidyIssue[]>([]),[loading,setLoading]=useState(true),[failed,setFailed]=useState(false),[action,setAction]=useState(false),[watchCover,setWatchCover]=useState(0);
  const [reason,setReason]=useState(''),[root,setRoot]=useState(''),[media,setMedia]=useState(''),[ignored,setIgnored]=useState(false),[query,setQuery]=useState('');
  const generation=useRef(0),errorRef=useRef(onError);errorRef.current=onError;
  const load=async()=>{const token=++generation.current;setLoading(true);setFailed(false);try{const next=await api.organizeIssues();if(token===generation.current)setRows(next);}catch(e){if(token===generation.current){setFailed(true);errorRef.current(errorMessage(e));}}finally{if(token===generation.current)setLoading(false);}};
  useEffect(()=>{void load();return()=>{generation.current++;};},[revision,refreshKey,busy]);
  const run=async(task:()=>Promise<unknown>,watch=false)=>{setAction(true);try{await task();await load();if(watch)setWatchCover(v=>v+1);}catch(e){errorRef.current(errorMessage(e));}finally{setAction(false);}};
  useEffect(()=>{
    if(!watchCover)return;let active=true;let timer:number|undefined;
    const poll=async()=>{try{const status=await api.posterCacheStatus();if(!active)return;if(status.phase==='QUEUED'||status.phase==='RUNNING')timer=window.setTimeout(()=>void poll(),1500);else void load();}catch{/* Existing retry feedback owns failures. */}};
    void poll();return()=>{active=false;if(timer!==undefined)window.clearTimeout(timer);};
  },[watchCover]);
  const filtered=rows.filter(row=>row.ignored===ignored&&(!reason||row.reason===reason)&&(!root||row.node.libraryRootId===Number(root))&&(!media||row.node.mediaKind===media)&&(!query||[row.node.displayName,row.node.folderName,row.node.binding?.providerTitle].some(s=>s?.toLowerCase().includes(query.toLowerCase()))));
  return <section className="organize-page"><header className="page-toolbar"><div className="page-title-row"><h1>{t('tidy.title')}</h1><span>{t('tidy.count',{count:filtered.length})}</span><button className="button secondary" type="button" disabled={loading} onClick={()=>void load()}>{t('tidy.refresh')}</button></div>
    <label className="search-field"><Icon name="search"/><input aria-label={t("common.search")} placeholder={t("common.search")} value={query} onChange={e=>setQuery(e.target.value)}/></label><div className="search-extra-filters"><Select aria-label={t('tidy.all')} value={reason} onChange={e=>setReason(e.target.value)}><option value="">{t('tidy.all')}</option>{[...new Set(rows.map(r=>r.reason))].map(r=><option value={r} key={r}>{t(`tidy.reason.${r}` as 'tidy.reason.cover')}</option>)}</Select>
    <Select aria-label={t('search.scope')} value={root} onChange={e=>setRoot(e.target.value)}><option value="">{t('search.allLibraries')}</option>{roots.map(r=><option key={r.id} value={r.id}>{r.displayName}</option>)}</Select>
    <Select aria-label={t('filters.media')} value={media} onChange={e=>setMedia(e.target.value)}><option value="">{t('filters.media')}</option>{(['ANIMATION','LIVE_ACTION','COMIC','EBOOK','DOUJIN','ARTBOOK'] as const).map(kind=><option key={kind} value={kind}>{t(({ANIMATION:'comic.animation',LIVE_ACTION:'library.liveAction',COMIC:'comic.name',EBOOK:'ebook.name',DOUJIN:'doujin.name',ARTBOOK:'artbook.name'} as const)[kind])}</option>)}</Select>
    <Select aria-label={t('tidy.ignored')} value={ignored?'ignored':'active'} onChange={e=>setIgnored(e.target.value==='ignored')}><option value="active">{t('tidy.active')} · {rows.filter(r=>!r.ignored).length}</option><option value="ignored">{t('tidy.ignored')} · {rows.filter(r=>r.ignored).length}</option></Select></div></header>
    <div className="page-content">{loading?<LoadingState/>:failed?<button className="button secondary" onClick={()=>void load()}>{t('tidy.refresh')}</button>:filtered.length===0?<EmptyState icon="check" title={t('tidy.empty')}/>:<div className="organize-issues">{filtered.map(issue=><div className="organize-issue" key={`${issue.node.id}:${issue.kind}`}>
      <MediaCard node={issue.node} viewMode="list" onOpen={onOpen} onMenu={onMenu} onBangumi={onMatch} onRetryCover={node=>void run(()=>onRetryCover(node),true)} coverRevision={revision}/>
      <div className="organize-issue-actions"><strong>{t(`tidy.reason.${issue.reason}` as 'tidy.reason.cover')}</strong>
      <button type="button" className="button ghost" onClick={()=>onOpen(issue.node)}>{t('tidy.open')}</button>
      {issue.kind!=='cover'&&<button type="button" disabled={busy||action} className="button secondary" onClick={()=>onMatch(issue.node)}>{t('tidy.match')}</button>}
      {issue.kind==='match'&&<button type="button" disabled={busy||action} className="button secondary" onClick={()=>void run(()=>onRetryMatch(issue.node))}>{t('tidy.retryMatch')}</button>}
      {issue.kind==='cover'&&(issue.node.binding||issue.node.tmdbBinding?.active||issue.node.coverCachePath||['COMIC','EBOOK','DOUJIN','ARTBOOK'].includes(issue.node.mediaKind??''))&&<button type="button" disabled={busy||action} className="button secondary" onClick={()=>void run(()=>onRetryCover(issue.node),true)}>{t('tidy.retryCover')}</button>}
      {issue.kind==='cover'&&!issue.node.binding&&!issue.node.tmdbBinding&&<button type="button" disabled={busy||action} className="button secondary" onClick={()=>onMatch(issue.node)}>{t('tidy.match')}</button>}
      <button type="button" className="button ghost" disabled={action} onClick={()=>void run(()=>onReveal(issue.node))}>{t('tidy.reveal')}</button>
      <button type="button" className="button ghost" disabled={action} onClick={()=>void run(()=>api.setIssueReminder(issue.node.id,issue.kind,!issue.ignored))}>{t(issue.ignored?'tidy.restore':'tidy.ignore')}</button></div>
    </div>)}</div>}</div>
  </section>;
}

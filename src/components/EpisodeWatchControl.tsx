import {useCallback,useEffect,useLayoutEffect,useMemo,useRef,useState} from 'react';
import {api} from '../lib/api';
import {useI18n} from '../lib/i18n';
import type {MediaFile,NodeDetail} from '../types/media';
import type {EpisodeWatchRow,EpisodeWatchSnapshot} from '../types/episodes';
import {Icon} from './Icon';
import {isCardProgressGenerationCurrent,updateCardProgress} from '../lib/cardProgress';

function confirmProgress(snapshot:EpisodeWatchSnapshot){
 const watched=(snapshot.watchedEpisodes??0)+(snapshot.watchedFiles??0),total=(snapshot.totalEpisodes??0)+(snapshot.totalFiles??0);
 updateCardProgress({nodeId:snapshot.nodeId,workView:snapshot.workView,watched,total,started:watched>0});
}

export function useEpisodeWatch(detail:NodeDetail|null){
 const acceptedInline=useRef<EpisodeWatchSnapshot|undefined>(undefined);
 const incoming=detail?.episodeWatch;
 const validInline=incoming&&incoming.nodeId===detail.node.id&&incoming.workView===(detail.node.workView===true)&&(detail.episodeWatchGeneration===undefined||isCardProgressGenerationCurrent(detail.episodeWatchGeneration))?incoming:undefined;
 const [storedData,setData]=useState<EpisodeWatchSnapshot|null>(validInline??null),[storedBusy,setBusy]=useState(false),[error,setError]=useState(false);
 const generation=useRef(0),mutating=useRef(false),target=useRef(detail),active=useRef(true),pendingRefresh=useRef(false);target.current=detail;
 const freshInline=validInline&&validInline!==acceptedInline.current&&!mutating.current;
 const data=freshInline?validInline:storedData,busy=freshInline?false:storedBusy;
 const latestData=useRef(data);latestData.current=data;
 const sourceKey=useMemo(()=>JSON.stringify(detail?[detail.node.id,detail.node.workView,detail.node.nodeType,detail.node.updatedAt,detail.binding?.updatedAt,detail.node.tmdbBinding,detail.workTarget,detail.mediaFiles,detail.nestedMediaFiles]:null),[detail]);
 const identity=detail?`${detail.node.id}:${detail.node.workView===true}`:'';
 const previousIdentity=useRef(identity);
 const refresh=useCallback(()=>{
  if(mutating.current){pendingRefresh.current=true;return;}
  const current=target.current,token=++generation.current;
  if(!current||typeof api.episodeWatchSnapshot!=='function'){setData(null);setBusy(false);setError(false);return;}
  setBusy(true);setError(false);
  void api.episodeWatchSnapshot(current.node.id,current.node.workView===true).then(next=>{if(token===generation.current){setData(next);confirmProgress(next);}}).catch(()=>{if(token===generation.current)setError(true);}).finally(()=>{if(token===generation.current)setBusy(false);});
 },[]);
 useEffect(()=>{active.current=true;const metadata=()=>refresh();window.addEventListener('m2shelf-metadata-changed',metadata);return()=>{active.current=false;generation.current++;window.removeEventListener('m2shelf-metadata-changed',metadata);};},[refresh]);
 useEffect(()=>{
  const playback=(event:Event)=>{const fileId=(event as CustomEvent<import('../types/episodes').MpvPlaybackEvent>).detail;if(fileId?.state!=='FILE'&&fileId?.state!=='FINISHED')return;const detail=target.current;if(detail&&(detail.mediaFiles.some(file=>file.id===fileId.fileId)||detail.nestedMediaFiles?.some(entry=>entry.file.id===fileId.fileId)||latestData.current?.rows.some(row=>row.fileId===fileId.fileId)))refresh();};
  window.addEventListener('m2shelf-playback-changed',playback);return()=>window.removeEventListener('m2shelf-playback-changed',playback);
 },[refresh]);
 useLayoutEffect(()=>{
  if(previousIdentity.current!==identity){previousIdentity.current=identity;generation.current++;latestData.current=null;setData(null);setError(false);}
  if(validInline){
   acceptedInline.current=validInline;
   if(mutating.current){pendingRefresh.current=true;return;}
   generation.current++;latestData.current=validInline;setData(validInline);setBusy(false);setError(false);return;
  }
  refresh();
 },[identity,sourceKey,incoming,refresh]);
 const mark=useCallback(async(fileId:number,watched:boolean)=>{
  const current=target.current;if(!data||!current||mutating.current||busy)return;
  const token=++generation.current;mutating.current=true;setBusy(true);setError(false);
  try{const next=await api.markEpisodeWatched(current.node.id,current.node.workView===true,fileId,watched,data.snapshot);window.dispatchEvent(new Event('m2shelf-progress-changed'));if(token===generation.current){setData(next);confirmProgress(next);}}
  catch{if(token===generation.current)setError(true);}
  finally{mutating.current=false;if(token===generation.current)setBusy(false);if(pendingRefresh.current&&active.current){pendingRefresh.current=false;refresh();}}
 },[data,busy,refresh]);
 const clear=useCallback(async()=>{
  const current=target.current,snapshot=latestData.current;if(!current||!snapshot||mutating.current||busy)return;
  const token=++generation.current;mutating.current=true;setBusy(true);setError(false);
  try{const next=await api.clearEpisodeWatch(current.node.id,current.node.workView===true,snapshot.snapshot);window.dispatchEvent(new Event('m2shelf-progress-changed'));if(token===generation.current){setData(next);confirmProgress(next);}}
  catch{if(token===generation.current)setError(true);}
  finally{mutating.current=false;if(token===generation.current)setBusy(false);if(pendingRefresh.current&&active.current){pendingRefresh.current=false;refresh();}}
 },[busy,refresh]);
 const playPreferred=useCallback(async(files:MediaFile[],fallback:MediaFile,play:(file:MediaFile)=>void|boolean|Promise<boolean>)=>{
  const current=target.current;if(!current)return;
  const token=generation.current;
  let snapshot=latestData.current;
  if(!snapshot&&typeof api.episodeWatchSnapshot==='function'){
   try{snapshot=await api.episodeWatchSnapshot(current.node.id,current.node.workView===true);if(token===generation.current)setData(snapshot);}catch{if(token===generation.current)setError(true);return;}
  }
  if(!active.current||target.current?.node.id!==current.node.id||target.current?.node.workView!==current.node.workView)return;
  const file=files.find(file=>file.id===snapshot?.latestWatchedFileId)??fallback;
  await play(file);
 },[]);
 const playAndMark=useCallback(async(fileId:number,play:()=>void|boolean|Promise<boolean>)=>{
  const current=target.current;if(!current||mutating.current)return;
  const token=++generation.current;mutating.current=true;setBusy(true);setError(false);
  try{
   let snapshot=latestData.current,readFailed=false;
   if(!snapshot||snapshot.nodeId!==current.node.id||snapshot.workView!==(current.node.workView===true)){
    try{snapshot=await api.episodeWatchSnapshot(current.node.id,current.node.workView===true);}catch{readFailed=true;}
   }
   if(token===generation.current&&snapshot)setData(snapshot);
   if(await play()!==true)return;
   if(readFailed){if(active.current)setError(true);return;}
   const row=snapshot?.rows.find(row=>row.fileId===fileId);
   if(!row)return;
   const next=await api.markEpisodeWatched(current.node.id,current.node.workView===true,fileId,true,snapshot!.snapshot);window.dispatchEvent(new Event('m2shelf-progress-changed'));
   if(token===generation.current){setData(next);confirmProgress(next);}
  }catch{
   if(active.current&&target.current?.node.id===current.node.id){
    try{const fresh=await api.episodeWatchSnapshot(current.node.id,current.node.workView===true);if(fresh.rows.some(row=>row.fileId===fileId&&row.watched)){if(token===generation.current)setData(fresh);return;}}catch{/* Keep the last confirmed UI on a genuine save failure. */}
    setError(true);
   }
  }
  finally{mutating.current=false;if(token===generation.current)setBusy(false);if(pendingRefresh.current&&active.current){pendingRefresh.current=false;refresh();}}
 },[refresh]);
 return {data,busy,error,refresh,mark,clear,playPreferred,playAndMark};
}
export type EpisodeWatchController=ReturnType<typeof useEpisodeWatch>;
export function EpisodeWatchControl({row,name,busy,onChange}:{row:EpisodeWatchRow;name:string;busy:boolean;onChange:(watched:boolean)=>void}){
 const {t}=useI18n();
 const label=t(row.partiallyWatched?'episodes.partial':row.watched?'episodes.markUnwatched':'episodes.markWatched',{name});
 return <button type="button" className={`episode-watch-toggle${row.watched?' is-watched':''}${row.partiallyWatched?' is-partial':''}`} role="checkbox" aria-checked={row.partiallyWatched?'mixed':row.watched} aria-label={label} title={row.kind==='FILE'?`${label} · ${t('episodes.fileLevel')}`:label} disabled={busy} onDoubleClick={event=>event.stopPropagation()} onMouseDown={event=>event.stopPropagation()} onClick={event=>{event.stopPropagation();onChange(!row.watched);}}><Icon name={row.partiallyWatched?'more':'check'}/></button>;
}
export function EpisodeWatchSummary({watch}:{watch:EpisodeWatchController}){
 const {t}=useI18n();const {data,error,busy,refresh,clear}=watch;
 if(!data&&!error)return null;
 if(data&&!data.totalEpisodes&&!data.totalFiles&&!error)return null;
 const watched=Boolean(data?.rows.some(row=>row.watched||row.partiallyWatched));
 return <div className="episode-watch-summary">
  {data&&<><div className="episode-watch-position" role="status"><Icon name="play"/><strong title={data.latestWatchedFileName??undefined}>{data.latestWatchedFileName?t('episodes.watchedThrough',{name:data.latestWatchedFileName}):t('episodes.notStarted')}</strong></div><button className="button ghost episode-watch-clear" type="button" disabled={busy||!watched} onClick={()=>void clear()}>{t('episodes.clear')}</button></>}
  {error&&<span className="episode-watch-error" role="alert">{t('episodes.error')} <button type="button" className="button ghost" disabled={busy} onClick={refresh}>{t('common.retry')}</button></span>}
  {Boolean(data?.unresolvedRecords)&&<small>{t('episodes.unresolved',{count:data!.unresolvedRecords})}</small>}
 </div>;
}

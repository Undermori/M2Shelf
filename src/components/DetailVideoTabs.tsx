import {useId,useMemo,useState} from 'react';
import type {MediaFile,NodeDetail} from '../types/media';
import type {EpisodeWatchController} from './EpisodeWatchControl';
import {MediaFileList} from './MediaFileList';
import {useI18n} from '../lib/i18n';
type Section='MAIN'|'SPS'|'OTHER';
const sections:Section[]=['MAIN','SPS','OTHER'];
export function DetailVideoTabs({detail,watch,onPlay,onReveal}:{detail:NodeDetail;watch:EpisodeWatchController;onPlay:(file:MediaFile)=>void|boolean|Promise<boolean>;onReveal:(file:MediaFile)=>void}) {
 const {t}=useI18n(),id=useId();const identity=`${detail.node.id}:${detail.node.workView===true}`;const [selection,setSelection]=useState<{identity:string;section:Section}>({identity,section:'MAIN'});const selected=selection.identity===identity?selection.section:'MAIN';const setSelected=(section:Section)=>setSelection({identity,section});
 const groups=useMemo(()=>{
 const classified=new Map(detail.videoSections?.map(e=>[e.fileId,e.section]));
 const sources=new Map(detail.workSources?.map(n=>[n.id,n.folderName]));
 const rows=[...detail.mediaFiles.map(file=>({file,directory:'',source:detail.workSources&&detail.workSources.length>1?sources.get(file.nodeId)??'':''})),...(detail.nestedMediaFiles??[]).map(e=>({file:e.file,directory:e.relativeDirectory,source:detail.workSources&&detail.workSources.length>1?e.sourceName:''}))];
 return sections.map(section=>({section,rows:rows.filter(row=>(classified.get(row.file.id)??(row.directory?'OTHER':'MAIN'))===section)})).filter(group=>group.rows.length>0);
 },[detail.mediaFiles,detail.nestedMediaFiles,detail.videoSections,detail.workSources]);
 const active=groups.find(g=>g.section===selected)??groups[0];
 const fileProps=useMemo(()=>({files:active?.rows.map(r=>r.file)??[],relativePaths:Object.fromEntries(active?.rows.map(r=>[r.file.id,[r.source,active.section==='SPS'?'':r.directory].filter(Boolean).join(' / ')])??[]),directorySortKeys:Object.fromEntries(active?.rows.map(r=>[r.file.id,r.directory])??[])}),[active]);
 if(!active)return null;
 const keys={MAIN:'video.main',SPS:'video.sps',OTHER:'video.other'} as const;
 return <section className="content-section detail-video-section"><div className="video-tabs-heading"><div className="media-kind-tabs video-tabs" role="tablist" aria-label={t('video.sections')} onKeyDown={e=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(e.key))return;e.preventDefault();e.stopPropagation();const i=groups.findIndex(g=>g===active),next=e.key==='Home'?0:e.key==='End'?groups.length-1:(i+(e.key==='ArrowRight'?1:-1)+groups.length)%groups.length;setSelected(groups[next].section);document.getElementById(`${id}-${groups[next].section}`)?.focus();}}>{groups.map(group=><button type="button" key={group.section} role="tab" id={`${id}-${group.section}`} aria-controls={`${id}-panel`} aria-selected={group===active} tabIndex={group===active?0:-1} onClick={()=>setSelected(group.section)}>{t(keys[group.section])}<span>{group.rows.length}</span></button>)}</div><small>{t('detail.doubleClickReady')}</small></div><div role="tabpanel" id={`${id}-panel`} aria-labelledby={`${id}-${active.section}`}><MediaFileList watch={watch} {...fileProps} onPlay={onPlay} onReveal={onReveal}/></div></section>;
}

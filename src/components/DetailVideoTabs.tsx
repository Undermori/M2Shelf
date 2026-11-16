import {useEffect,useId,useMemo,useState} from 'react';
import type {MediaFile,MediaNode,NodeDetail,ResourceFile,VideoSection} from '../types/media';
import type {EpisodeWatchController} from './EpisodeWatchControl';
import {MediaFileList} from './MediaFileList';
import {OtherResourceList} from './OtherResourceList';
import {useI18n} from '../lib/i18n';
import {firstUnwatched,locateUnwatchedCopy} from '../lib/firstUnwatched';
type Section=VideoSection;
const sections:Section[]=['MAIN','SPS','CDS','OTHER'];
interface DetailVideoTabsProps {
 detail:NodeDetail;
 watch:EpisodeWatchController;
 cdsFiles:ResourceFile[];
 cdsFolders:MediaNode[];
 spFiles?:ResourceFile[];
 spFolders?:MediaNode[];
 selectedSection?:Section;
 onSectionChange?:(section:Section)=>void;
 onPlay:(file:MediaFile)=>void|boolean|Promise<boolean>;
 onReveal:(file:MediaFile)=>void;
 onOpenResource:(file:ResourceFile)=>void;
 onRevealResource:(file:ResourceFile)=>void;
 onOpenChild:(node:MediaNode)=>void;
 onMenu:(event:React.MouseEvent,node:MediaNode)=>void;
}
export function DetailVideoTabs({detail,watch,cdsFiles,cdsFolders,spFiles=[],spFolders=[],selectedSection,onSectionChange,onPlay,onReveal,onOpenResource,onRevealResource,onOpenChild,onMenu}:DetailVideoTabsProps) {
 const {t,language}=useI18n(),id=useId();const identity=`${detail.node.id}:${detail.node.workView===true}`;const [selection,setSelection]=useState<{identity:string;section:Section}>({identity,section:'MAIN'});const selected=selectedSection??(selection.identity===identity?selection.section:'MAIN');const setSelected=(section:Section)=>{setSelection({identity,section});onSectionChange?.(section);};
 const groups=useMemo(()=>{
 const classified=new Map(detail.videoSections?.map(e=>[e.fileId,e.section]));
 const sources=new Map(detail.workSources?.map(n=>[n.id,n.folderName]));
 const rows=[...detail.mediaFiles.map(file=>({file,directory:'',source:detail.workSources&&detail.workSources.length>1?sources.get(file.nodeId)??'':''})),...(detail.nestedMediaFiles??[]).map(e=>({file:e.file,directory:e.relativeDirectory,source:detail.workSources&&detail.workSources.length>1?e.sourceName:''}))];
 return sections.map(section=>{
  const sectionRows=rows.filter(row=>(classified.get(row.file.id)??(row.directory?'OTHER':'MAIN'))===section);
  return {section,rows:sectionRows,count:sectionRows.length+(section==='CDS'?cdsFiles.length+cdsFolders.length:section==='SPS'?spFiles.length+spFolders.length:0)};
 }).filter(group=>group.count>0);
 },[detail.mediaFiles,detail.nestedMediaFiles,detail.videoSections,detail.workSources,cdsFiles.length,cdsFolders.length,spFiles.length,spFolders.length]);
 useEffect(()=>{
  let clearTimer:number|undefined;
  const locate=(event:Event)=>{
   const fileId=(event as CustomEvent<{fileId?:number}>).detail?.fileId;
   if(!fileId)return;
   const targetGroup=groups.find(group=>group.rows.some(row=>row.file.id===fileId));
   if(targetGroup&&targetGroup.section!==selected){setSelection({identity,section:targetGroup.section});onSectionChange?.(targetGroup.section);}
   window.requestAnimationFrame(()=>{
    const row=document.querySelector<HTMLElement>(`.media-file-row[data-file-id="${fileId}"]`);
    if(!row)return;
    row.scrollIntoView?.({behavior:window.matchMedia?.('(prefers-reduced-motion: reduce)').matches?'auto':'smooth',block:'center'});
    row.querySelector<HTMLButtonElement>('.file-actions button:not([role=checkbox])')?.focus({preventScroll:true});
    row.classList.add('is-located');
    window.clearTimeout(clearTimer);
    clearTimer=window.setTimeout(()=>row.classList.remove('is-located'),1600);
   });
  };
  window.addEventListener('m2shelf-locate-video-file',locate);
  return()=>{window.clearTimeout(clearTimer);window.removeEventListener('m2shelf-locate-video-file',locate);};
 },[groups,identity,selected]);
 const active=groups.find(g=>g.section===selected)??groups[0];
 const fileProps=useMemo(()=>({files:active?.rows.map(r=>r.file)??[],relativePaths:Object.fromEntries(active?.rows.map(r=>[r.file.id,[r.source,active.section==='SPS'?'':r.directory].filter(Boolean).join(' / ')])??[]),directorySortKeys:Object.fromEntries(active?.rows.map(r=>[r.file.id,r.directory])??[])}),[active]);
 const firstUnwatchedId=useMemo(()=>firstUnwatched(active?.rows??[],watch.data?.rows??[],language),[active,watch.data?.rows,language]);
 if(!active)return null;
 const keys={MAIN:'video.main',SPS:'video.sps',CDS:'video.cds',OTHER:'video.other'} as const;
 return <section className="content-section detail-video-section"><div className="video-tabs-heading"><div className="media-kind-tabs video-tabs" role="tablist" aria-label={t('video.sections')} onKeyDown={e=>{if(e.defaultPrevented||e.nativeEvent.isComposing||e.altKey||e.ctrlKey||e.metaKey||!['ArrowLeft','ArrowRight','Home','End'].includes(e.key))return;const i=groups.findIndex(g=>g===active),next=e.key==='Home'?0:e.key==='End'?groups.length-1:(i+(e.key==='ArrowRight'?1:-1)+groups.length)%groups.length;const target=document.getElementById(`${id}-${groups[next].section}`);if(next===i||!target)return;e.preventDefault();e.stopPropagation();setSelected(groups[next].section);target.focus();target.scrollIntoView?.({block:'nearest',inline:'nearest'});}}>{groups.map(group=><button type="button" key={group.section} role="tab" id={`${id}-${group.section}`} aria-controls={`${id}-panel`} aria-selected={group===active} tabIndex={group===active?0:-1} onClick={()=>setSelected(group.section)}>{t(keys[group.section])}<span>{group.count}</span></button>)}</div>{firstUnwatchedId!==undefined&&<button type="button" className="button ghost locate-unwatched" disabled={watch.busy} onClick={()=>window.dispatchEvent(new CustomEvent('m2shelf-locate-video-file',{detail:{fileId:firstUnwatchedId}}))}>{locateUnwatchedCopy[language]}</button>}{active.rows.length>0&&<small>{t('detail.doubleClickReady')}</small>}</div><div className="video-tabs-panel" key={active.section} role="tabpanel" id={`${id}-panel`} aria-labelledby={`${id}-${active.section}`}>
  {active.rows.length>0&&<MediaFileList watch={watch} {...fileProps} onPlay={onPlay} onReveal={onReveal}/>}
  {active.section==='SPS'&&(spFiles.length>0||spFolders.length>0)&&<OtherResourceList expandedFolderIds={detail.expandedFolderIds} files={spFiles} folders={spFolders} onOpenFile={onOpenResource} onRevealFile={onRevealResource} onOpenFolder={onOpenChild} onFolderMenu={onMenu}/>}
  {active.section==='CDS'&&(cdsFiles.length>0||cdsFolders.length>0)&&<OtherResourceList expandedFolderIds={detail.expandedFolderIds} files={cdsFiles} folders={cdsFolders} onOpenFile={onOpenResource} onRevealFile={onRevealResource} onOpenFolder={onOpenChild} onFolderMenu={onMenu}/>}
 </div></section>;
}

import {createContext,useCallback,useContext,useEffect,useMemo,useRef,useState,type ReactNode} from 'react';
import {api,desktopAvailable} from './api';
import {errorMessage} from './format';
import type {CollectionSnapshot,CollectionStatus,CollectionTarget} from '../types/collection';
import type {MediaNode} from '../types/media';
import {Select} from '../components/Select';
import {useI18n} from './i18n';

type Filter=''|'UNSET'|CollectionStatus;
const values=['WANT','IN_PROGRESS','COMPLETED','PAUSED','DROPPED'] as const;
const key=(target:CollectionTarget)=>`${target.kind}:${target.id}`;
interface CollectionContext {
 ready:boolean;failed:boolean;busy:boolean;refresh:()=>Promise<void>;
 get:(target:CollectionTarget)=>CollectionStatus|undefined;
 set:(target:CollectionTarget,value:CollectionStatus|null)=>Promise<void>;
 filters:Record<string,Filter>;filter:(scope:string,value:Filter)=>void;
 matches:(targets:CollectionTarget[],value:Filter)=>boolean;
 nodeTargets:(node:MediaNode)=>CollectionTarget[];
}
const Context=createContext<CollectionContext|null>(null);
export const useCollection=()=>useContext(Context);
export function CollectionStateProvider({children}:{children:ReactNode}) {
 const [snapshot,setSnapshot]=useState<CollectionSnapshot>({entries:[],bookMemberships:[],nodeMemberships:[]});const [ready,setReady]=useState(!desktopAvailable),[busy,setBusy]=useState(false),[filters,setFilters]=useState<Record<string,Filter>>({});
 const generation=useRef(0),mutating=useRef(false);const [failed,setFailed]=useState(false);
 const refresh=useCallback(async()=>{const token=++generation.current;if(!desktopAvailable)return;try{const next=await api.collectionStatuses();if(token===generation.current){setSnapshot(next);setReady(true);setFailed(false);}}catch{if(token===generation.current){setReady(false);setFailed(true);}}},[]);
 useEffect(()=>{void refresh();return()=>{generation.current++;};},[refresh]);
 const map=useMemo(()=>new Map(snapshot.entries.map(entry=>[key(entry.target),entry.state])),[snapshot]);
 const books=useMemo(()=>new Map(snapshot.bookMemberships.map(entry=>[entry.nodeId,entry.bookIds])),[snapshot]);
 const nodes=useMemo(()=>new Map(snapshot.nodeMemberships.map(entry=>[entry.nodeId,entry.nodeIds])),[snapshot]);
 const set=useCallback(async(target:CollectionTarget,value:CollectionStatus|null)=>{if(mutating.current)return;mutating.current=true;generation.current++;setBusy(true);try{await api.setCollectionStatus(target,value);await refresh();}finally{mutating.current=false;setBusy(false);}},[refresh]);
 const context=useMemo<CollectionContext>(()=>({ready,failed,busy,refresh,get:target=>map.get(key(target)),set,filters,filter:(scope,value)=>setFilters(previous=>({...previous,[scope]:value})),matches:(targets,value)=>!value||targets.some(target=>value==='UNSET'?!map.has(key(target)):map.get(key(target))===value),nodeTargets:node=>!node.workView&&map.has(key({kind:'NODE',id:node.id}))?[{kind:'NODE',id:node.id}]:['COMIC','EBOOK','DOUJIN','ARTBOOK'].includes(node.mediaKind??'')?(books.get(node.id)??[]).map(id=>({kind:'BOOK',id})):!node.workView?(nodes.get(node.id)??[]).map(id=>({kind:'NODE',id})):[]}),[ready,failed,busy,refresh,map,set,filters,books,nodes]);
 return <Context.Provider value={context}>{children}</Context.Provider>;
}
export function CollectionStatusControl({target,readable=false}:{target:CollectionTarget;readable?:boolean}) {
 const collection=useCollection();const {t}=useI18n();const [error,setError]=useState('');
 if(!collection||target.id<=0)return null;
 if(collection.failed)return <button type="button" className="button secondary" onClick={event=>{event.stopPropagation();void collection.refresh();}}>{t('state.label')} · {t('common.retry')}</button>;
 return <span className="collection-status-control" onDoubleClick={event=>event.stopPropagation()} onClick={event=>event.stopPropagation()}><Select aria-label={t('state.label')} disabled={!collection.ready||collection.busy} value={collection.get(target)??''} onChange={event=>{setError('');void collection.set(target,(event.target.value||null) as CollectionStatus|null).catch(e=>setError(errorMessage(e)));}}><option value="">{t('state.unset')}</option>{values.map(value=><option key={value} value={value}>{t(value==='WANT'?(readable||target.kind==='BOOK'?'state.wantRead':'state.wantWatch'):`state.${value}`)}</option>)}</Select>{error&&<small role="alert">{error}</small>}</span>;
}
export function CollectionStatusFilter({scope}:{scope:string}) {
 const collection=useCollection();const {t}=useI18n();if(!collection)return null;
 if(collection.failed)return <button type="button" className="button secondary" onClick={()=>void collection.refresh()}>{t('state.label')} · {t('common.retry')}</button>;
 return <Select className="collection-state-select" aria-label={t('state.filter')} disabled={!collection.ready} value={collection.filters[scope]??''} onChange={event=>collection.filter(scope,event.target.value as Filter)}><option value="">{t('state.all')}</option><option value="UNSET">{t('state.unset')}</option>{values.map(value=><option key={value} value={value}>{t(value==='WANT'?'state.want':`state.${value}`)}</option>)}</Select>;
}

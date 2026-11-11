import {useEffect,useRef,useState,useId} from 'react';
import type {MediaNode} from '../types/media';
import {useCollection,CollectionStatusFilter} from '../lib/collectionState';
import {useI18n} from '../lib/i18n';
import {TagFilter} from './TagFilter';
import {Icon} from './Icon';
export function CollectionFilters({nodes,value,onChange,scope}:{nodes:MediaNode[];value:number|null;onChange:(value:number|null)=>void;scope:string}) {
 const {t}=useI18n(),collection=useCollection(),id=useId();const [open,setOpen]=useState(false);const root=useRef<HTMLDivElement>(null),button=useRef<HTMLButtonElement>(null);
 const count=Number(value!==null&&nodes.some(n=>n.userTags.some(tag=>tag.id===value)))+Number(Boolean(collection?.filters[scope]));
 useEffect(()=>{if(!open)return;const outside=(e:PointerEvent)=>{if(!root.current?.contains(e.target as Node))setOpen(false);};const key=(e:KeyboardEvent)=>{if(e.isComposing)return;if(e.key==='Escape'&&!root.current?.querySelector('.select-trigger[aria-expanded="true"]')){e.preventDefault();e.stopPropagation();setOpen(false);button.current?.focus();}};document.addEventListener('pointerdown',outside);document.addEventListener('keydown',key,true);return()=>{document.removeEventListener('pointerdown',outside);document.removeEventListener('keydown',key,true);};},[open]);
 return <div className="collection-filter-disclosure" ref={root}><button ref={button} type="button" className={`button secondary collection-filter-button${count?' is-active':''}`} aria-expanded={open} aria-controls={open?id:undefined} onClick={()=>setOpen(v=>!v)}><Icon name="filter"/>{t('layout.filter')}{count>0&&<span className="filter-count">{count}</span>}</button>{open&&<div className="collection-filter-panel" id={id} role="group" aria-label={t('layout.filters')}><span className="filter-panel-label">{t('layout.tags')}</span><TagFilter nodes={nodes} value={value} onChange={onChange}/><span className="filter-panel-label">{t('state.label')}</span><CollectionStatusFilter scope={scope}/>{count>0&&<button className="button ghost" type="button" onClick={()=>{onChange(null);collection?.filter(scope,'');}}>{t('layout.clearFilters')}</button>}</div>}</div>;
}

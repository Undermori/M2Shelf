import {useEffect,useRef,useState} from 'react';
import {getCurrentWebviewWindow} from '@tauri-apps/api/webviewWindow';
import {api,chooseDirectory,desktopAvailable} from '../lib/api';
import {useI18n} from '../lib/i18n';
import {FeatureModal} from './FeatureModal';
export function LibraryDropImport({enabled,onFolders,onError}:{enabled:boolean;onFolders:(paths:string[])=>void;onError:(message:string)=>void}){
 const{t}=useI18n();const[over,setOver]=useState(false);const latest=useRef({enabled,onFolders,onError,epoch:0});latest.current={enabled,onFolders,onError,epoch:latest.current.epoch+(enabled!==latest.current.enabled?1:0)};
 useEffect(()=>{
  if(!desktopAvailable)return;
  let disposed=false;let stop:(()=>void)|undefined;let view;
  try{view=getCurrentWebviewWindow();}catch{return;}
  const available=()=>latest.current.enabled&&!document.querySelector('[aria-modal="true"],dialog[open]');
  void view.onDragDropEvent(event=>{
   if(!available()){setOver(false);return;}
   const e=event.payload;setOver(e.type==='enter'||e.type==='over');
   if(e.type!=='drop')return;
   const epoch=++latest.current.epoch;
   const current=()=>!disposed&&epoch===latest.current.epoch&&available();
   const unique=[...new Set(e.paths)].slice(0,20);
   void (async()=>{
    const accepted:string[]=[];
    for(const path of unique){
     if(!current())return;
     try{const actual=await api.validateLibraryPath(path);if(!accepted.some(p=>p.toLowerCase()===actual.toLowerCase()))accepted.push(actual);}
     catch{if(current())latest.current.onError(t('drop.invalid'));}
    }
    if(current()&&accepted.length)latest.current.onFolders(accepted);
   })();
  }).then(f=>{if(disposed)f();else stop=f;}).catch(()=>undefined);
  return()=>{disposed=true;stop?.();};
 },[t]);
 useEffect(()=>{if(!enabled)setOver(false);},[enabled]);
 return over&&enabled?<div className="library-drop-target" aria-live="polite">{t('drop.title')}</div>:null;
}
export function LibraryPathDialog({onChoose,onClose,initialPath=""}:{onChoose:(path:string)=>void;onClose:()=>void;initialPath?:string}){
 const{t}=useI18n();const[path,setPath]=useState(initialPath),[busy,setBusy]=useState(false),[error,setError]=useState('');
 const accept=async()=>{setBusy(true);setError('');try{onChoose(await api.validateLibraryPath(path.trim()));}catch{setError(t('drop.invalid'));}finally{setBusy(false);}};
 return <FeatureModal title={t('drop.choose')} onClose={()=>{if(!busy)onClose();}}><form onSubmit={e=>{e.preventDefault();void accept();}}><label>{t('drop.path')}<input autoFocus disabled={busy} value={path} onChange={e=>setPath(e.target.value)} aria-label={t('drop.path')}/></label>{error&&<p role="alert">{error}</p>}<div className="dialog-actions"><button type="button" className="button secondary" disabled={busy} onClick={()=>{void chooseDirectory().then(value=>{if(value)setPath(value);});}}>{t('drop.choose')}</button><button className="button primary" disabled={busy||!path.trim()} type="submit">{t('common.confirm')}</button></div></form></FeatureModal>;
}

import {useEffect,useRef,useState} from 'react';
import {open,save} from '@tauri-apps/plugin-dialog';
import {api,desktopAvailable} from '../lib/api';
import {useI18n} from '../lib/i18n';
import {useAppSettings} from '../lib/settingsStore';
import type {BackupPreview} from '../types/collection';
import {ConfirmDialog} from './ConfirmDialog';

export function CollectionBackupSettings() {
 const {t}=useI18n();const {flush,saving}=useAppSettings();const [busy,setBusy]=useState(false),[preview,setPreview]=useState<BackupPreview|null>(null),[message,setMessage]=useState('');
 const mounted=useRef(true),pending=useRef<string|null>(null),restarting=useRef(false);
 const failure=(error:unknown)=>{
  const code=String((error as {causeValue?:unknown})?.causeValue??error).match(/BACKUP_[A-Z_]+/)?.[0]??'BACKUP_IO';
  return t(`backup.${code}` as 'backup.BACKUP_IO');
 };
 useEffect(()=>{mounted.current=true;if(desktopAvailable)void api.collectionRestoreNotice().then(code=>{if(code&&mounted.current)setMessage(t(`backup.${code}` as 'backup.BACKUP_RESTORED'));}).catch(()=>{});return()=>{mounted.current=false;if(pending.current&&!restarting.current)void api.discardCollectionRestore(pending.current).catch(()=>{});};},[t]);
 const run=async(action:()=>Promise<void>)=>{setBusy(true);setMessage('');try{await flush();await action();}catch(error){if(mounted.current)setMessage(failure(error));}finally{if(mounted.current)setBusy(false);}};
 const exportBackup=()=>run(async()=>{
  const destination=await save({defaultPath:`M2Shelf-${new Date().toISOString().slice(0,10)}.m2backup`,filters:[{name:'M²Shelf',extensions:['m2backup']}]});
  if(destination){await api.exportCollectionBackup(destination);if(mounted.current)setMessage(t('backup.saved'));}
 });
 const importBackup=()=>run(async()=>{
  const source=await open({multiple:false,directory:false,filters:[{name:'M²Shelf',extensions:['m2backup']}]});
  if(typeof source==='string'){
   const result=await api.prepareCollectionRestore(source);
   if(!mounted.current){await api.discardCollectionRestore(result.token);return;}
   pending.current=result.token;setPreview(result);
  }
 });
 const cancel=()=>{if(busy)return;const token=pending.current;pending.current=null;setPreview(null);if(token)void api.discardCollectionRestore(token).catch(e=>setMessage(failure(e)));};
 return <div className="collection-backup-settings">
  <h3>{t('backup.title')}</h3><p>{t('backup.help')}</p>
  <div className="dialog-actions"><button className="button secondary" disabled={!desktopAvailable||saving||busy||!!preview} onClick={()=>void exportBackup()} type="button">{t(busy?'common.processing':'backup.export')}</button><button className="button secondary" disabled={!desktopAvailable||saving||busy||!!preview} onClick={()=>void importBackup()} type="button">{t('backup.import')}</button></div>
  <small>{t('backup.privacy')}</small>{message&&<p role="status">{message}</p>}
  <ConfirmDialog open={!!preview} title={t('backup.confirmTitle')} description={preview?t('backup.confirmHelp',{roots:preview.roots,covers:preview.manualCovers,offline:preview.offlineRoots}):''} confirmLabel={t('backup.confirm')} busy={busy} onClose={cancel} onConfirm={()=>{if(!preview)return;void run(async()=>{restarting.current=true;try{await api.applyCollectionRestore(preview.token);}catch(error){restarting.current=false;throw error;}});}}/>
 </div>;
}

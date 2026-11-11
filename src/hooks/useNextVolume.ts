import {useCallback,useEffect,useRef,useState} from 'react';
import {api} from '../lib/api';
import {useI18n} from '../lib/i18n';
import type {ComicBook} from '../types/comic';
import type {NextVolumeResult} from '../types/readerFeatures';

// A reached text ending is useful within this reading session, without inferring it from a chapter number.
const sessionEnds=new Set<string>();
export const knownBookEnd=(book:ComicBook)=>sessionEnds.has(`${book.id}:${book.revision}`)||
  (!['EPUB','TXT','MOBI','AZW3'].includes(book.documentFormat??'')&&book.pageCount>0&&book.progress?.lastPageIndex===book.pageCount-1);
export function useNextVolume(book:ComicBook|null,reachedEnd=false){
 const {t}=useI18n();const [result,setResult]=useState<NextVolumeResult|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState('');
 const generation=useRef(0),inFlight=useRef(false);
 useEffect(()=>{const token=++generation.current;setResult(null);setError('');setBusy(false);inFlight.current=false;
  if(book)void Promise.resolve().then(()=>api.nextVolume(book.id,book.revision)).then(value=>{if(token===generation.current)setResult(value);}).catch(()=>{if(token===generation.current)setResult(null);});
  return()=>{generation.current++;};
 },[book?.id,book?.revision]);
 useEffect(()=>{if(!book||!reachedEnd)return;const key=`${book.id}:${book.revision}`;sessionEnds.delete(key);sessionEnds.add(key);if(sessionEnds.size>128)sessionEnds.delete(sessionEnds.values().next().value!);},[book?.id,book?.revision,reachedEnd]);
 const advance=useCallback(async(flush:()=>Promise<boolean>,open:(book:ComicBook)=>void)=>{
  if(!book||!result?.next||inFlight.current)return;inFlight.current=true;setBusy(true);setError('');const token=generation.current;
  try{
   const fresh=await api.nextVolume(book.id,book.revision);
   if(fresh.snapshot!==result.snapshot||fresh.next?.id!==result.next.id)throw new Error('NEXT_VOLUME_CHANGED');
   const ready=await api.openComicBook(fresh.next.id);
   if(token!==generation.current)return;
   // A first document open can refresh its adapter revision. Validate that new index before saving.
   const prepared=await api.nextVolume(book.id,book.revision);
   if(prepared.next?.id!==ready.book.id||prepared.next.revision!==ready.book.revision)throw new Error('NEXT_VOLUME_CHANGED');
   if(!await flush()){setError(t('nextVolume.saveFailed'));return;}
   const confirmed=await api.confirmNextVolume(book.id,book.revision,ready.book.id,prepared.snapshot);
   if(confirmed.snapshot!==prepared.snapshot||confirmed.next?.id!==ready.book.id||confirmed.next.revision!==ready.book.revision)throw new Error('NEXT_VOLUME_CHANGED');
   if(token===generation.current)open(ready.book);
  }catch(reason){if(token===generation.current){setError(t(reason instanceof Error&&reason.message==='NEXT_VOLUME_CHANGED'?'nextVolume.changed':'nextVolume.failed'));void api.nextVolume(book.id,book.revision).then(value=>{if(token===generation.current)setResult(value);}).catch(()=>{});}}
  finally{if(token===generation.current){inFlight.current=false;setBusy(false);}}
 },[book?.id,book?.revision,result,t]);
 return {result,busy,error,advance};
}

import {useCallback,useEffect,useRef,useState} from 'react';
import {api} from '../lib/api';
import {useI18n} from '../lib/i18n';
import type {ComicBook} from '../types/comic';

/** Explicit visible-table order, not inferred series/volume ownership. No background enumeration. */
export function useReadingSequence(book:ComicBook|null,list?:readonly ComicBook[]){
 const {t}=useI18n();const [busy,setBusy]=useState(false),[error,setError]=useState('');const generation=useRef(0),working=useRef(false);
 const index=book?list?.findIndex(item=>item.id===book.id)??-1:-1;
 const hasContext=!!list&&index>=0;const next=hasContext?list![index+1]??null:null;
 useEffect(()=>{generation.current++;working.current=false;setBusy(false);setError('');return()=>{generation.current++;};},[book?.id,book?.revision,list]);
 const advance=useCallback(async(flush:()=>Promise<boolean>,open:(book:ComicBook)=>void)=>{
  if(!book||!next||working.current)return;working.current=true;const token=generation.current;setBusy(true);setError('');
  try{
   const ready=await api.openComicBook(next.id);if(token!==generation.current)return;
   if(next.sourcePath&&ready.book.sourcePath!==next.sourcePath)throw Error('READING_SEQUENCE_CHANGED');
   const saved=await flush();if(token!==generation.current)return;
   if(!saved){setError(t('nextVolume.saveFailed'));return;}
   const confirmed=await api.openComicBook(next.id);
   if(confirmed.book.revision!==ready.book.revision)throw Error('READING_SEQUENCE_CHANGED');
   if(token===generation.current)open(confirmed.book);
  }catch{if(token===generation.current)setError(t('nextVolume.failed'));}
  finally{if(token===generation.current){working.current=false;setBusy(false);}}
 },[book?.id,book?.revision,next,t]);
 return {hasContext,next,busy,error,advance};
}

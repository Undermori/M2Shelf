import {useCallback,useEffect,useMemo,useRef,useState,useSyncExternalStore,type RefObject} from 'react';
import type {MediaNode} from '../types/media';
import type {ComicBook} from '../types/comic';
import {desktopAvailable} from '../lib/api';
import {usePosterThumbnailSize} from './usePosterThumbnailSize';
import {posterResourceEpoch,posterResourceUrl,posterSelectionRevision,subscribePosterResources} from '../lib/posterResource';

let pdfImport:Promise<typeof import('../lib/pdfCoverFallback')>|undefined;
const pdfInFlight=new Map<string,Promise<boolean>>();
function preparePdf(key:string,nodeId:number|undefined,book:ComicBook|undefined){
 const existing=pdfInFlight.get(key);if(existing)return existing;
 const promise=(pdfImport??=import('../lib/pdfCoverFallback').catch(error=>{pdfImport=undefined;throw error;})).then(module=>module.pdfCoverFallback(nodeId,book)).finally(()=>pdfInFlight.delete(key));
 pdfInFlight.set(key,promise);return promise;
}

/** Kept as a source-compatible hook name; display uses a native binary image URL, not data IPC.
 * Every indexed ready cover gets its stable src after layout, including offscreen cards. The
 * browser schedules/decode images; native bounded raster/file reuse handles WebView cache misses.
 * Scrolling never clears src or a React byte cache.
 * enabled affects only the expensive missing-PDF preparation, never prepared image delivery.
 */
export function useCoverDataUrl(node:MediaNode|null|undefined,revision:number,enabled=true,book?:ComicBook,_priority?:RefObject<boolean>){
 const path=node?.coverSource==='MANUAL'?node.coverCachePath:(node?.tmdbBinding?.active?node.tmdbBinding.coverCachePath:null)??node?.coverCachePath??node?.binding?.coverCachePath;
 const expected=Boolean(path);
 const hasSource=Boolean(book)||expected||['COMIC','EBOOK','DOUJIN','ARTBOOK'].includes(node?.mediaKind??'')&&(node?.totalComicBookCount??0)>0;
 const {tier,measured,currentTier,ref:coverFrameRef}=usePosterThumbnailSize(hasSource);
 const epoch=useSyncExternalStore(subscribePosterResources,()=>`${posterResourceEpoch()}:${posterSelectionRevision(node?String(node.id):`book-${book?.id}`)}`);
 const localRevision=!path?`${node?.updatedAt??''}:${node?.lastSeenAt??''}:${node?.latestFileModifiedAt??''}`:'';
 const identity=node?`node:${node.id}:${path??''}:${node.binding?.providerSubjectId??''}:${node.binding?.providerImageUrl??''}:${node.binding?.updatedAt??''}:${node.tmdbBinding?.movie.id??''}:${node.tmdbBinding?.movie.posterPath??''}:${node.clientCoverRevision??0}:${localRevision}`:book?`book:${book.id}:${book.revision}`:'';
 const key=`${identity}:${revision}:${epoch}:${tier}`;
 const url=useMemo(()=>desktopAvailable&&hasSource&&measured&&currentTier.current===tier?(node?posterResourceUrl('node',node.id,tier,key):book?posterResourceUrl('book',book.id,tier,key,book.revision):null):null,[node?.id,book?.id,book?.revision,currentTier,hasSource,key,tier,measured]);
 const current=useRef(key);current.current=key;
 const [failure,setFailure]=useState('');const [pending,setPending]=useState('');const [retry,setRetry]=useState('');
 const attempted=useRef('');const mounted=useRef(true);
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
 const fallbackEligible=book?book.documentFormat==='PDF':hasSource&&['COMIC','EBOOK','DOUJIN','ARTBOOK'].includes(node?.mediaKind??'');
 const onCoverError=useCallback(()=>{if(current.current===key)setFailure(key);},[key]);
 useEffect(()=>{
  if(!enabled||failure!==key||!fallbackEligible||attempted.current===key)return;
  attempted.current=key;setPending(key);
  void preparePdf(identity,node?.id,book).then(ready=>{if(mounted.current&&current.current===key&&ready){setRetry(key);setFailure('');}}).catch(()=>{}).finally(()=>{if(mounted.current&&current.current===key)setPending('');});
 },[book?.id,book?.revision,enabled,failure,fallbackEligible,identity,key,node?.id]);
 const failed=failure===key;
 return {coverFrameRef,coverCacheKey:`${key}:${tier}`,coverUrl:failed?null:url?(retry===key?url.replace(/v=([a-f0-9]+)/,(_match,value:string)=>`v=${value}1`):url):null,coverFailed:failed&&pending!==key,coverLoading:pending===key,onCoverError};
}

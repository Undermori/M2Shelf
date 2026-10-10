import {useEffect,useRef,useState} from 'react';
import {useI18n} from '../lib/i18n';
import type {ComicPageCache} from '../lib/comicReader';

/** Uses the same two-read, twelve-entry, 128 MiB decoded cache as image comics. */
export function EpubIllustration({cache,index,visible,onLayout}:{cache:ComicPageCache;index:number;visible:Set<number>;onLayout?:()=>void}) {
 const {t}=useI18n();const ref=useRef<HTMLElement>(null);const [preview,setPreview]=useState(cache.peek(index));const [failed,setFailed]=useState(false);const [retry,setRetry]=useState(0);
 useEffect(()=>cache.subscribe(()=>setPreview(cache.peek(index))),[cache,index]);
 useEffect(()=>{
  const element=ref.current;if(!element)return;let active=true;
  const observer=new IntersectionObserver(entries=>{
   if(entries[0]?.isIntersecting){visible.add(index);cache.retain(visible);cache.demand(visible);setFailed(false);void cache.load(index).then(value=>{if(active&&value){setPreview(value);onLayout?.();}}).catch(()=>{if(active)setFailed(true);});}
   else{visible.delete(index);cache.retain(visible);cache.demand(visible);setPreview(undefined);}
  },{root:element.closest('.text-page-frame'),rootMargin:'160px'});
  observer.observe(element);return()=>{active=false;observer.disconnect();visible.delete(index);cache.retain(visible);cache.demand(visible);};
 },[cache,index,visible,retry,onLayout]);
 const size=cache.sizes.get(index);
 return <figure ref={ref} className="epub-lazy-image" style={{aspectRatio:size?`${size.width}/${size.height}`:undefined,minHeight:!size?160:undefined}}>{preview&&cache.peek(index)?.url===preview.url?<img src={preview.url} alt={t('ebook.illustration')}/>:failed?<figcaption role="status">{t('epub.imageError')}<button type="button" onClick={()=>setRetry(value=>value+1)}>{t('epub.retryImage')}</button></figcaption>:<span role="status">{t('sidebar.loading')}</span>}</figure>;
}

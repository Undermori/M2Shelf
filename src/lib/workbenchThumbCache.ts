import {useCallback, useLayoutEffect, useRef, useState} from 'react';

type Entry = {key:string; url:string|null; bytes:number; refs:number; retained:boolean; started:boolean; done:boolean; load:()=>Promise<Blob>; promise:Promise<string|null>; resolve:(url:string|null)=>void; reject:(error:unknown)=>void};
export interface ThumbLease {url:string|null; ready:Promise<string|null>; release:(image?:HTMLImageElement|null)=>void;}

/** Workbench-only transport. No work starts before a visible component requests it. */
export class WorkbenchThumbCache {
  private entries=new Map<string,Entry>();
  private queue:Entry[]=[];
  private reads=0;
  constructor(private maxEntries=160,private maxBytes=48*1024*1024,private concurrency=4) {}
  peek(key:string) {return this.entries.get(key)?.url??null;}
  acquire(key:string,load:()=>Promise<Blob>,retain=true):ThumbLease {
    let entry=this.entries.get(key);
    if(!entry){
      let resolve!:(url:string|null)=>void,reject!:(error:unknown)=>void;
      const promise=new Promise<string|null>((yes,no)=>{resolve=yes;reject=no;});
      entry={key,url:null,bytes:0,refs:0,retained:retain,started:false,done:false,load,promise,resolve,reject};
      this.entries.set(key,entry);this.queue.push(entry);
    }
    const current=entry;current.refs++;this.entries.delete(key);this.entries.set(key,current);this.drain();
    let released=false;
    return {url:current.url,ready:current.promise,release:(image)=>{
      if(released)return;released=true;
      // Detaching an <img> does not cancel a browser decode already in progress.
      const finish=()=>{current.refs--;if(!current.refs&&(!current.retained||!current.done)){this.remove(current);}this.trim();};
      if(image?.src===current.url&&!image.complete&&typeof image.decode==='function')void image.decode().catch(()=>{}).finally(finish);else finish();
    }};
  }
  private remove(entry:Entry) {
    if(this.entries.get(entry.key)===entry)this.entries.delete(entry.key);
    entry.retained=false;
    if(!entry.started){this.queue=this.queue.filter(item=>item!==entry);entry.resolve(null);}
    if(entry.url&&!entry.refs){const url=entry.url;entry.url=null;const release=()=>URL.revokeObjectURL(url);if(typeof requestAnimationFrame==='function')requestAnimationFrame(release);else setTimeout(release,0);}
  }
  private trim() {
    let bytes=0;for(const item of this.entries.values())bytes+=item.bytes;
    for(const item of this.entries.values()){
      if(this.entries.size<=this.maxEntries&&bytes<=this.maxBytes)break;
      if(item.refs)continue;bytes-=item.bytes;this.remove(item);
    }
  }
  private drain() {
    while(this.reads<this.concurrency&&this.queue.length){
      const entry=this.queue.shift()!;if(!entry.refs){this.remove(entry);continue;}
      entry.started=true;this.reads++;
      void entry.load().then(blob=>{
        entry.done=true;
        // An abandoned read may finish, but must not allocate or retain a Blob URL.
        if(!entry.refs){this.remove(entry);entry.resolve(null);return;}
        entry.bytes=blob.size;entry.url=URL.createObjectURL(blob);entry.resolve(entry.url);this.trim();
      }).catch(error=>{entry.done=true;this.remove(entry);entry.reject(error);}).finally(()=>{this.reads--;this.drain();});
    }
  }
  clear() {for(const entry of this.entries.values()){entry.retained=false;if(!entry.refs)this.remove(entry);}}
  get size() {return this.entries.size;}
}
export const workbenchThumbCache=new WorkbenchThumbCache();

/** Synchronous cache lookup gives the first returning paint the existing URL. */
export function useWorkbenchRaster(key:string,enabled:boolean,load:()=>Promise<Blob>,retain=true) {
  const loader=useRef(load);loader.current=load;
  const image=useRef<HTMLImageElement|null>(null);
  const retainImage=useCallback((element:HTMLImageElement|null)=>{if(element)image.current=element;},[]);
  const [state,setState]=useState<{key:string;url:string|null;failed:boolean}>({key,url:null,failed:false});
  useLayoutEffect(()=>{
    if(!enabled)return;
    // Capture this key's loader: a later render may point the component at another source.
    let live=true;const lease=workbenchThumbCache.acquire(key,loader.current,retain);
    setState({key,url:lease.url,failed:false});
    void lease.ready.then(url=>{if(live)setState({key,url,failed:false});}).catch(()=>{if(live)setState({key,url:null,failed:true});});
    const currentImage=image.current;
    return()=>{live=false;lease.release(image.current??currentImage);};
  },[key,enabled,retain]);
  return {url:enabled?workbenchThumbCache.peek(key):null,failed:state.key===key&&state.failed,image:retainImage,fail:()=>setState({key,url:null,failed:true})};
}

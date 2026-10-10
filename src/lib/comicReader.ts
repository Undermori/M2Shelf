import { api } from './api';
import type { ComicPage } from '../types/comic';

export type PageSize={width:number;height:number};
export function isWide(size:PageSize|undefined,threshold=1.2):boolean{return !!size&&size.width/size.height>threshold;}
/** Pair from the book's start so previous/next and restored positions use one stable order. */
export function comicSpreads(count:number,double:boolean,wideAlone:boolean,sizes:ReadonlyMap<number,PageSize>):number[][]{
  const spreads:number[][]=[];
  for(let i=0;i<count;){
    if(!double||(wideAlone&&isWide(sizes.get(i)))){spreads.push([i++]);continue;}
    if(i+1<count&&!(wideAlone&&isWide(sizes.get(i+1)))){spreads.push([i,i+1]);i+=2;}else{spreads.push([i++]);}
  }
  return spreads;
}
export function readerKeyStep(key:string,rtl:boolean):number{
  if(key==='PageDown'||key===' ')return 1;
  if(key==='PageUp')return -1;
  if(key==='ArrowRight')return rtl?-1:1;
  if(key==='ArrowLeft')return rtl?1:-1;
  return 0;
}
export interface ComicPreview extends PageSize{url:string;cost:number;}
type Task={index:number;resolve:(value:ComicPreview|undefined)=>void;reject:(reason:unknown)=>void};
const MAX_BYTES=128*1024*1024;
const MAX_ENTRIES=12;
/** Per-open-book binary/decoded LRU. No page data is stored in localStorage or on disk. */
export class ComicPageCache{
  readonly sizes=new Map<number,PageSize>();
  private entries=new Map<number,ComicPreview>();
  private pending=new Map<number,Promise<ComicPreview|undefined>>();
  private queue:Task[]=[];
  private active=0;
  private disposed=false;
  private cost=0;
  private listeners=new Set<()=>void>();
  subscribe(listener:()=>void){this.listeners.add(listener);return()=>{this.listeners.delete(listener);};}
  private notify(){this.changed();for(const listener of this.listeners)listener();}
  private protectedPages=new Set<number>();
  constructor(private bookId:number,private pages:ComicPage[],private changed:()=>void,private read=api.readComicPage){}
  peek(index:number){return this.entries.get(index);}
  retain(indexes:ReadonlySet<number>){this.protectedPages=new Set(indexes);this.trim();}
  demand(indexes:ReadonlySet<number>){
    // Drop superseded speculative reads before they start; in-flight reads may finish.
    this.queue=this.queue.filter(task=>{
      if(indexes.has(task.index))return true;
      this.pending.delete(task.index);task.resolve(undefined);return false;
    });
  }
  load(index:number):Promise<ComicPreview|undefined>{
    if(this.disposed||index<0||index>=this.pages.length)return Promise.resolve(undefined);
    const cached=this.entries.get(index);
    if(cached){this.entries.delete(index);this.entries.set(index,cached);return Promise.resolve(cached);}
    const pending=this.pending.get(index);if(pending)return pending;
    const promise=new Promise<ComicPreview|undefined>((resolve,reject)=>{this.queue.push({index,resolve,reject});});
    this.pending.set(index,promise);
    while(this.queue.length>MAX_ENTRIES){const task=this.queue.shift()!;this.pending.delete(task.index);task.resolve(undefined);}
    this.pump();return promise;
  }
  private pump(){while(!this.disposed&&this.active<2&&this.queue.length){const task=this.queue.shift()!;this.active++;void this.fetch(task).finally(()=>{this.active--;this.pending.delete(task.index);this.pump();});}}
  private async fetch(task:Task){
    let url:string|undefined;
    try{
      const bytes=await this.read(this.bookId,task.index);
      if(this.disposed){task.resolve(undefined);return;}
      if(bytes.byteLength>64*1024*1024)throw new Error('COMIC_PAGE_LIMIT');
      // Native validation accepts supported image signatures, including mislabeled suffixes.
      // Let the image decoder detect the validated bytes instead of assigning a suffix MIME.
      url=URL.createObjectURL(new Blob([bytes]));
      const img=new Image();img.src=url;await img.decode();
      if(this.disposed){URL.revokeObjectURL(url);task.resolve(undefined);return;}
      const width=img.naturalWidth,height=img.naturalHeight;
      if(!width||!height)throw new Error('COMIC_IMAGE_INVALID');
      let cost=bytes.byteLength+width*height*4;
      // Preserve source dimensions for layout, but retain a display-sized preview.
      // Two large original pages should not fight over the entire decoded budget.
      if(width*height>8_000_000){
        const ratio=Math.sqrt(8_000_000/(width*height));const target=document.createElement('canvas');
        target.width=Math.max(1,Math.round(width*ratio));target.height=Math.max(1,Math.round(height*ratio));
        const context=target.getContext('2d');if(!context)throw new Error('COMIC_IMAGE_INVALID');
        context.imageSmoothingEnabled=true;context.imageSmoothingQuality='high';context.drawImage(img,0,0,target.width,target.height);
        const blob=await new Promise<Blob>((resolve,reject)=>target.toBlob(b=>b?resolve(b):reject(new Error('COMIC_IMAGE_INVALID')),'image/png'));
        URL.revokeObjectURL(url);url=URL.createObjectURL(blob);cost=blob.size+target.width*target.height*4;target.width=0;target.height=0;img.src='';
        if(this.disposed){URL.revokeObjectURL(url);task.resolve(undefined);return;}
      }
      if(cost>MAX_BYTES)throw new Error('COMIC_PAGE_LIMIT');
      const entry={url,width,height,cost};
      this.entries.set(task.index,entry);this.sizes.set(task.index,{width:entry.width,height:entry.height});this.cost+=cost;
      this.trim(task.index);
      this.notify();task.resolve(this.entries.get(task.index));
    }catch(error){if(url)URL.revokeObjectURL(url);task.reject(error);}
  }
  private trim(newPage?:number){
    while(this.entries.size>MAX_ENTRIES||this.cost>MAX_BYTES){
      const index=[...this.entries.keys()].find(i=>!this.protectedPages.has(i)&&i!==newPage)
        ?? [...this.entries.keys()].find(i=>i!==newPage);
      if(index===undefined)break;
      const entry=this.entries.get(index)!;URL.revokeObjectURL(entry.url);this.cost-=entry.cost;this.entries.delete(index);for(const listener of this.listeners)listener();
    }
  }
  dispose(){this.disposed=true;this.listeners.clear();for(const entry of this.entries.values())URL.revokeObjectURL(entry.url);this.entries.clear();this.cost=0;for(const task of this.queue)task.resolve(undefined);this.queue=[];this.pending.clear();}
}

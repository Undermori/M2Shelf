import type {PDFDocumentProxy,PDFPageProxy,RenderTask} from 'pdfjs-dist';
import type {PageSize} from './comicReader';

export const THUMBNAIL_LIMITS={width:160,height:240,entries:32,bytes:8*1024*1024,demand:24,sourceBytes:64*1024*1024,rasterBytes:256*1024,timeoutMs:8000} as const;
export interface ThumbnailRaster {blob:Blob;width:number;height:number}
export type ThumbnailSource=(index:number,signal:AbortSignal)=>Promise<ThumbnailRaster>;
export type ThumbnailEntry={status:'ready';url:string;width:number;height:number;cost:number}|{status:'error'};
export interface ThumbnailStats {entries:number;bytes:number;active:number;queued:number;requests:number;completed:number;failed:number;cancelled:number;peakActive:number;peakBytes:number;peakEntries:number}
export const thumbnailAbort=()=>new DOMException('Page preview cancelled','AbortError');
const checkSignal=(signal:AbortSignal)=>{if(signal.aborted)throw thumbnailAbort();};

type PdfJob={priority:'foreground'|'preview'|'cleanup';controller:AbortController;run:(signal:AbortSignal)=>Promise<unknown>;resolve:(value:unknown)=>void;reject:(error:unknown)=>void;detach:()=>void};
/** One queue per already-open PDF. Foreground page/search work preempts only preview jobs. */
export class PdfPageScheduler {
 private queue:PdfJob[]=[];
 private active:PdfJob|null=null;
 private disposed=false;
 run<T>(priority:'foreground'|'preview'|'cleanup',signal:AbortSignal,run:(signal:AbortSignal)=>Promise<T>):Promise<T>{
  if(this.disposed||signal.aborted)return Promise.reject(thumbnailAbort());
  return new Promise<T>((resolve,reject)=>{
   const controller=new AbortController();
   const job:PdfJob={priority,controller,run,resolve:value=>resolve(value as T),reject,detach:()=>signal.removeEventListener('abort',abort)};
   const abort=()=>{controller.abort();const index=this.queue.indexOf(job);if(index>=0){this.queue.splice(index,1);job.detach();reject(thumbnailAbort());}};
   signal.addEventListener('abort',abort,{once:true});
   this.queue.push(job);
   if(priority==='foreground'&&this.active?.priority==='preview')this.active.controller.abort();
   this.pump();
  });
 }
 private pump(){
  if(this.disposed||this.active||!this.queue.length)return;
  const foreground=this.queue.findIndex(job=>job.priority==='foreground');
  const cleanup=this.queue.findIndex(job=>job.priority==='cleanup');
  const job=this.queue.splice(foreground>=0?foreground:cleanup>=0?cleanup:0,1)[0];this.active=job;
  void Promise.resolve().then(()=>{checkSignal(job.controller.signal);return job.run(job.controller.signal);}).then(value=>{checkSignal(job.controller.signal);job.resolve(value);}).catch(error=>job.reject(job.controller.signal.aborted?thumbnailAbort():error)).finally(()=>{job.detach();this.active=null;this.pump();});
 }
 dispose(){this.disposed=true;this.active?.controller.abort();for(const job of this.queue){job.controller.abort();job.detach();job.reject(thumbnailAbort());}this.queue=[];}
 get stats(){return{active:this.active?1:0,queued:this.queue.length};}
}

type CacheTask={index:number;controller:AbortController;resolve:(entry:ThumbnailEntry|undefined)=>void;promise:Promise<ThumbnailEntry|undefined>;retryAfterCancel?:boolean};
/** Open-panel lifetime only: no filesystem, poster cache, localStorage or full-book prefetch. */
export class PageThumbnailCache {
 private entries=new Map<number,ThumbnailEntry>();
 private pending=new Map<number,CacheTask>();
 private queue:CacheTask[]=[];
 private active:CacheTask|null=null;
 private demanded=new Set<number>();
 private listeners=new Set<()=>void>();
 private disposed=false;
 private version=0;
 private totals={bytes:0,requests:0,completed:0,failed:0,cancelled:0,peakActive:0,peakBytes:0,peakEntries:0};
 constructor(readonly identity:string,readonly count:number,private source:ThumbnailSource){}
 subscribe=(listener:()=>void)=>{this.listeners.add(listener);return()=>{this.listeners.delete(listener);};};
 snapshot=()=>this.version;
 private notify(){this.version++;for(const listener of this.listeners)listener();}
 peek(index:number){return this.entries.get(index);}
 demand(indexes:Iterable<number>){
  this.demanded=new Set([...indexes].filter(index=>this.valid(index)).slice(0,THUMBNAIL_LIMITS.demand));
  for(const task of [...this.queue])if(!this.demanded.has(task.index)){this.queue.splice(this.queue.indexOf(task),1);this.pending.delete(task.index);task.controller.abort();task.resolve(undefined);this.totals.cancelled++;}
  if(this.active&&!this.demanded.has(this.active.index))this.active.controller.abort();
  this.trim();this.notify();
 }
 load(index:number):Promise<ThumbnailEntry|undefined>{
  if(this.disposed||!this.valid(index))return Promise.resolve(undefined);
  const cached=this.entries.get(index);if(cached){this.entries.delete(index);this.entries.set(index,cached);return Promise.resolve(cached);}
  const existing=this.pending.get(index);if(existing)return existing.promise;
  let resolve:(entry:ThumbnailEntry|undefined)=>void=()=>{};
  const promise=new Promise<ThumbnailEntry|undefined>(done=>{resolve=done;});
  const task={index,controller:new AbortController(),resolve,promise};this.pending.set(index,task);this.queue.push(task);
  while(this.queue.length>THUMBNAIL_LIMITS.demand){const old=this.queue.shift()!;this.pending.delete(old.index);old.controller.abort();old.resolve(undefined);this.totals.cancelled++;}
  this.pump();return promise;
 }
 retry(index:number){this.remove(index);return this.load(index);}
 fail(index:number){if(this.disposed)return;this.remove(index);this.entries.set(index,{status:'error'});this.trim();this.notify();}
 private valid(index:number){return Number.isInteger(index)&&index>=0&&index<this.count;}
 private pump(){
  if(this.disposed||this.active||!this.queue.length)return;
  const task=this.queue.shift()!;this.active=task;this.totals.requests++;this.totals.peakActive=1;this.notify();
  void this.fetch(task).finally(()=>{this.pending.delete(task.index);if(this.active===task)this.active=null;this.notify();if(!this.disposed&&task.retryAfterCancel&&this.demanded.has(task.index)&&!this.entries.has(task.index))void this.load(task.index);this.pump();});
 }
 private async fetch(task:CacheTask){
  try{
   const raster=await this.source(task.index,task.controller.signal);
   if(this.disposed||task.controller.signal.aborted){this.totals.cancelled++;task.retryAfterCancel=true;task.resolve(undefined);return;}
   const {width,height,blob}=raster;
   if(!Number.isInteger(width)||!Number.isInteger(height)||width<1||height<1||width>THUMBNAIL_LIMITS.width||height>THUMBNAIL_LIMITS.height||blob.size>THUMBNAIL_LIMITS.rasterBytes)throw Error('PAGE_PREVIEW_LIMIT');
   const cost=blob.size+width*height*4;
   const entry:ThumbnailEntry={status:'ready',url:URL.createObjectURL(blob),width,height,cost};
   this.entries.set(task.index,entry);this.totals.bytes+=cost;this.totals.completed++;this.trim(task.index);
   this.totals.peakBytes=Math.max(this.totals.peakBytes,this.totals.bytes);this.totals.peakEntries=Math.max(this.totals.peakEntries,this.entries.size);
   this.notify();task.resolve(this.entries.get(task.index));
  }catch(error){
   if(this.disposed||task.controller.signal.aborted||error instanceof Error&&error.name==='AbortError'){this.totals.cancelled++;task.retryAfterCancel=true;task.resolve(undefined);return;}
   this.entries.set(task.index,{status:'error'});this.totals.failed++;this.trim();this.notify();task.resolve(this.entries.get(task.index));
  }
 }
 private remove(index:number){const entry=this.entries.get(index);if(entry?.status==='ready'){URL.revokeObjectURL(entry.url);this.totals.bytes-=entry.cost;}this.entries.delete(index);}
 private trim(newIndex?:number){while(this.entries.size>THUMBNAIL_LIMITS.entries||this.totals.bytes>THUMBNAIL_LIMITS.bytes){const index=[...this.entries.keys()].find(i=>i!==newIndex&&!this.demanded.has(i))??[...this.entries.keys()].find(i=>i!==newIndex);if(index===undefined)break;this.remove(index);}}
 dispose(){if(this.disposed)return;this.disposed=true;this.active?.controller.abort();for(const task of this.queue){task.controller.abort();task.resolve(undefined);}this.queue=[];this.pending.clear();for(const index of [...this.entries.keys()])this.remove(index);this.listeners.clear();}
 get stats():ThumbnailStats{return{...this.totals,entries:this.entries.size,active:this.active?1:0,queued:this.queue.length};}
}

export function thumbnailSize(width:number,height:number):PageSize{
 if(!Number.isFinite(width)||!Number.isFinite(height)||width<=0||height<=0)throw Error('PAGE_PREVIEW_INVALID');
 const scale=Math.min(1,THUMBNAIL_LIMITS.width/width,THUMBNAIL_LIMITS.height/height);
 return{width:Math.max(1,Math.floor(width*scale)),height:Math.max(1,Math.floor(height*scale))};
}
/** Metadata is an optimization only. Native already checked signature, dimensions and revision. */
export function thumbnailImageSize(bytes:Uint8Array):PageSize|undefined{
 const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);let width=0,height=0;
 if(bytes.length>=24&&bytes[0]===137&&bytes[1]===80&&bytes[2]===78&&bytes[3]===71){width=view.getUint32(16);height=view.getUint32(20);}
 else if(bytes.length>=10&&bytes[0]===71&&bytes[1]===73&&bytes[2]===70){width=view.getUint16(6,true);height=view.getUint16(8,true);}
 else if(bytes.length>=26&&bytes[0]===66&&bytes[1]===77){width=Math.abs(view.getInt32(18,true));height=Math.abs(view.getInt32(22,true));}
 else if(bytes.length>=30&&bytes[0]===82&&bytes[1]===73&&bytes[8]===87&&bytes[9]===69){
  if(bytes[12]===86&&bytes[13]===80&&bytes[14]===56&&bytes[15]===88){width=1+bytes[24]+(bytes[25]<<8)+(bytes[26]<<16);height=1+bytes[27]+(bytes[28]<<8)+(bytes[29]<<16);}
  else if(bytes[12]===86&&bytes[13]===80&&bytes[14]===56&&bytes[15]===76){width=1+((bytes[21]|bytes[22]<<8)&16383);height=1+((bytes[22]>>6|bytes[23]<<2|bytes[24]<<10)&16383);}
  else if(bytes[23]===157&&bytes[24]===1&&bytes[25]===42){width=view.getUint16(26,true)&16383;height=view.getUint16(28,true)&16383;}
 }else if(bytes.length>=24&&String.fromCharCode(...bytes.subarray(4,8))==='ftyp'){
  let boxes=0;
  const scan=(start:number,end:number,depth:number):PageSize|undefined=>{
   if(depth>8)return;
   for(let at=start;at+8<=end&&boxes++<256;){
    let length=view.getUint32(at),header=8;const type=String.fromCharCode(...bytes.subarray(at+4,at+8));
    if(length===1){if(at+16>end)return;const high=view.getUint32(at+8),low=view.getUint32(at+12);length=high*4294967296+low;header=16;}
    else if(length===0)length=end-at;
    if(!Number.isSafeInteger(length)||length<header||at+length>end)return;
    const body=at+header,boxEnd=at+length;
    if(type==='ispe'&&body+12<=boxEnd)return{width:view.getUint32(body+4),height:view.getUint32(body+8)};
    if(['meta','iprp','ipco'].includes(type)){const nested=scan(body+(type==='meta'?4:0),boxEnd,depth+1);if(nested)return nested;}
    at=boxEnd;
   }
  };
  const size=scan(0,bytes.length,0);if(size){width=size.width;height=size.height;}
 }else if(bytes.length>4&&bytes[0]===255&&bytes[1]===216){
  let at=2;const end=Math.min(bytes.length,2*1024*1024);
  while(at+3<end){if(bytes[at++]!==255)continue;while(at<end&&bytes[at]===255)at++;const marker=bytes[at++];if(marker===217||marker===218)break;if(marker===216||marker===1||marker>=208&&marker<=215)continue;if(at+2>end)break;const length=view.getUint16(at);if(length<2||at+length>end)break;if([192,193,194,195,197,198,199,201,202,203,205,206,207].includes(marker)&&length>=7){height=view.getUint16(at+3);width=view.getUint16(at+5);break;}at+=length;}
 }
 return width>0&&height>0&&width<=32768&&height<=32768&&width*height<=40_000_000?{width,height}:undefined;
}
async function canvasBlob(canvas:HTMLCanvasElement,signal:AbortSignal){checkSignal(signal);const blob=await new Promise<Blob>((resolve,reject)=>canvas.toBlob(value=>value?resolve(value):reject(Error('PAGE_PREVIEW_INVALID')),'image/webp',0.72));checkSignal(signal);return blob;}
export function createComicThumbnailSource({bookId,revision,read,sizes}:{bookId:number;revision:string;read:(bookId:number,index:number,revision:string)=>Promise<ArrayBuffer>;sizes?:ReadonlyMap<number,PageSize>}):ThumbnailSource{
 return async(index,signal)=>{
  checkSignal(signal);const bytes=await read(bookId,index,revision);checkSignal(signal);if(bytes.byteLength>THUMBNAIL_LIMITS.sourceBytes)throw Error('PAGE_PREVIEW_LIMIT');
  const input=new Blob([bytes]);const known=sizes?.get(index)??thumbnailImageSize(new Uint8Array(bytes));let bitmap:ImageBitmap|undefined,image:HTMLImageElement|undefined,url:string|undefined;
  const canvas=document.createElement('canvas');
  try{
   let width:number,height:number;
   if(typeof createImageBitmap==='function'){
    const target=known?thumbnailSize(known.width,known.height):undefined;
    bitmap=await createImageBitmap(input,target?{resizeWidth:target.width,resizeHeight:target.height,resizeQuality:'medium'}:undefined);checkSignal(signal);width=bitmap.width;height=bitmap.height;
   }else{url=URL.createObjectURL(input);image=new Image();image.src=url;await image.decode();checkSignal(signal);width=image.naturalWidth;height=image.naturalHeight;}
   if(width*height>40_000_000)throw Error('PAGE_PREVIEW_LIMIT');const size=thumbnailSize(width,height);canvas.width=size.width;canvas.height=size.height;
   const context=canvas.getContext('2d');if(!context)throw Error('PAGE_PREVIEW_INVALID');context.fillStyle='#fff';context.fillRect(0,0,size.width,size.height);context.drawImage(bitmap??image!,0,0,size.width,size.height);
   return{...size,blob:await canvasBlob(canvas,signal)};
  }finally{bitmap?.close();if(image)image.src='';if(url)URL.revokeObjectURL(url);canvas.width=0;canvas.height=0;}
 };
}
function pdfWithinBudget<T>(promise:Promise<T>,signal:AbortSignal,deadline:number,stop=()=>{}):Promise<T>{
 return new Promise<T>((resolve,reject)=>{
  let settled=false;let timer:ReturnType<typeof setTimeout>|undefined;
  const clear=()=>{clearTimeout(timer);signal.removeEventListener('abort',abort);};
  const fail=(reason:unknown)=>{if(settled)return;settled=true;clear();stop();reject(reason);};
  const abort=()=>fail(thumbnailAbort());
  // Always observe the underlying promise; a late worker rejection must not become unhandled.
  void promise.then(value=>{if(!settled){settled=true;clear();resolve(value);}},error=>{if(!settled){settled=true;clear();reject(error);}});
  signal.addEventListener('abort',abort,{once:true});
  if(signal.aborted)abort();else timer=setTimeout(()=>fail(Error('PAGE_PREVIEW_TIMEOUT')),Math.max(0,deadline-Date.now()));
 });
}
export function createPdfThumbnailSource({document:pdf,scheduler}:{document:PDFDocumentProxy;scheduler:PdfPageScheduler}):ThumbnailSource{
 let unsettledPage:Promise<PDFPageProxy>|null=null,unsettledRender:Promise<unknown>|null=null;
 const cleanup=(page:PDFPageProxy)=>{void scheduler.run('cleanup',new AbortController().signal,async()=>{page.cleanup();}).catch(()=>undefined);};
 return(index,signal)=>scheduler.run('preview',signal,async innerSignal=>{
  if(!Number.isInteger(index)||index<0||index>=pdf.numPages)throw Error('PAGE_PREVIEW_INVALID');
  const deadline=Date.now()+THUMBNAIL_LIMITS.timeoutMs,canvas=document.createElement('canvas');let page:PDFPageProxy|undefined,requested:Promise<PDFPageProxy>|undefined,render:RenderTask|undefined,rendered:Promise<unknown>|undefined;
  try{
   // Cancellation releases the foreground scheduler immediately, but does not multiply
   // getPage/render requests when an upstream worker fails to settle its old operation.
   if(unsettledRender)await pdfWithinBudget(unsettledRender.catch(()=>undefined),innerSignal,deadline);
   if(unsettledPage)await pdfWithinBudget(unsettledPage,innerSignal,deadline);
   checkSignal(innerSignal);requested=pdf.getPage(index+1);unsettledPage=requested;
   void requested.then(()=>{if(unsettledPage===requested)unsettledPage=null;},()=>{if(unsettledPage===requested)unsettledPage=null;});
   page=await pdfWithinBudget(requested,innerSignal,deadline);checkSignal(innerSignal);
   const natural=page.getViewport({scale:1}),size=thumbnailSize(natural.width,natural.height);
   const viewport=page.getViewport({scale:Math.min(size.width/natural.width,size.height/natural.height)});canvas.width=size.width;canvas.height=size.height;const context=canvas.getContext('2d');if(!context)throw Error('PAGE_PREVIEW_INVALID');
   render=page.render({canvas,canvasContext:context,viewport,background:'#fff'});rendered=render.promise;unsettledRender=rendered;
   void rendered.then(()=>{if(unsettledRender===rendered)unsettledRender=null;},()=>{if(unsettledRender===rendered)unsettledRender=null;});
   await pdfWithinBudget(rendered,innerSignal,deadline,()=>render?.cancel());checkSignal(innerSignal);
   return{...size,blob:await pdfWithinBudget(canvasBlob(canvas,innerSignal),innerSignal,deadline)};
  }finally{
   // Page proxies belong to the shared PDF document. Late cleanup is queued behind
   // foreground work, and after a cancelled render truly settles, never across it.
   if(page){const owned=page;if(rendered)void rendered.then(()=>cleanup(owned),()=>cleanup(owned));else cleanup(owned);}
   else if(requested)void requested.then(cleanup,()=>undefined);
   canvas.width=0;canvas.height=0;
  }
 });
}

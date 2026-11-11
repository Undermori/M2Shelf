import {api} from './api';
import type {ComicBook} from '../types/comic';

// Only the final null-cover branch imports PDF.js. One raster at a time, inside the existing
// cover request flow without holding its transport slots. No visible canvas, preview swap or persistent frontend cache.
let rasterChain:Promise<unknown>=Promise.resolve();
export function pdfCoverFallback(nodeId:number|undefined,book:ComicBook|undefined):Promise<boolean> {
  const next=rasterChain.catch(()=>undefined).then(()=>render(nodeId,book));
  rasterChain=next;return next;
}
async function render(nodeId:number|undefined,book:ComicBook|undefined):Promise<boolean> {
  let ticket:string|undefined;
  let destroy:(()=>Promise<void>)|undefined;
  let cancelRender:(()=>void)|undefined;
  let stopped=false;
  let timer:ReturnType<typeof setTimeout>|undefined;
  try {
    const request=await api.beginPdfCover(nodeId,book?.id,book?.revision);
    if(!request)return false;
    if(request.ready)return true;
    if(!request.ticket)return false;
    const requestTicket=request.ticket;ticket=requestTicket;
    const run=async()=>{
      const pdf=await import('pdfjs-dist');const {default:workerUrl}=await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
      if(stopped)throw new Error('cancelled');
      pdf.GlobalWorkerOptions.workerSrc=workerUrl;
      let bytesRead=0,rangeChain=Promise.resolve();let fail:(e:unknown)=>void=()=>{};
      const transportError=new Promise<never>((_,reject)=>{fail=reject;});
      const read=async(begin:number,end:number)=>{
        if(stopped||bytesRead+end-begin>24*1024*1024)throw new Error('PDF_COVER_LIMIT');
        bytesRead+=end-begin;
        return new Uint8Array(await api.readPdfRange(request.bookId,begin,end,request.revision));
      };
      const initial=await read(0,Math.min(65536,request.sourceSize));
      if(stopped)throw new Error('cancelled');
      class Transport extends pdf.PDFDataRangeTransport {
        override requestDataRange(begin:number,end:number){
          rangeChain=rangeChain.then(async()=>{for(let at=begin;at<end&&!stopped;){const next=Math.min(end,at+2*1024*1024);const bytes=await read(at,next);if(!stopped)this.onDataRange(at,bytes);at=next;}}).catch(fail);
        }
        override abort(){stopped=true;}
      }
      const range=new Transport(request.sourceSize,initial,true);
      const loading=pdf.getDocument({range,rangeChunkSize:256*1024,disableAutoFetch:true,disableStream:true,useSystemFonts:true,disableFontFace:true,enableXfa:false,stopAtErrors:true,maxImageSize:4_000_000,canvasMaxAreaInBytes:12*1024*1024,cMapUrl:new URL('/pdf-resources/cmaps/',window.location.href).href,cMapPacked:true,wasmUrl:new URL('/pdf-resources/wasm/',window.location.href).href});
      destroy=()=>loading.destroy();
      loading.onPassword=()=>fail(new Error('PDF_PASSWORD'));
      const document=await Promise.race([loading.promise,transportError]);
      const page=await Promise.race([document.getPage(1),transportError]);
      try {
        if(stopped)throw new Error('cancelled');
        const natural=page.getViewport({scale:1});
        const scale=Math.min(768/natural.width,3072/natural.height,Math.sqrt(2_400_000/(natural.width*natural.height)));
        if(!Number.isFinite(scale)||scale<=0)throw new Error('PDF_COVER_LIMIT');
        const viewport=page.getViewport({scale});const canvas=documentElement();
        canvas.width=Math.max(1,Math.floor(viewport.width));canvas.height=Math.max(1,Math.floor(viewport.height));
        const context=canvas.getContext('2d');if(!context)throw new Error('PDF_COVER_CANVAS');
        const task=page.render({canvas,canvasContext:context,viewport,background:'#ffffff'});cancelRender=()=>task.cancel();
        await Promise.race([task.promise,transportError]);
        if(stopped)throw new Error('cancelled');
        const png=canvas.toDataURL('image/png').split(',')[1];
        canvas.width=canvas.height=0;
        if(!png||png.length>2*1024*1024)throw new Error('PDF_COVER_LIMIT');
        if(!await api.commitPdfCover(requestTicket,png))return false;
        return true;
      } finally {page.cleanup();}
    };
    return await Promise.race([run(),new Promise<never>((_,reject)=>{timer=setTimeout(()=>{stopped=true;cancelRender?.();reject(new Error('PDF_COVER_TIMEOUT'));},20_000);})]);
  } catch {return false;} finally {
    stopped=true;if(timer)clearTimeout(timer);cancelRender?.();
    if(ticket)void api.cancelPdfCover(ticket).catch(()=>{});
    await destroy?.().catch(()=>{});
  }
}
function documentElement(){return document.createElement('canvas');}

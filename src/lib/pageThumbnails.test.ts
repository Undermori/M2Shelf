// @vitest-environment jsdom
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {waitFor} from '@testing-library/react';
import type {PDFDocumentProxy,PDFPageProxy} from 'pdfjs-dist';
import {createComicThumbnailSource,createPdfThumbnailSource,PageThumbnailCache,PdfPageScheduler,THUMBNAIL_LIMITS,thumbnailImageSize,thumbnailSize,type ThumbnailRaster} from './pageThumbnails';

const raster=(value=0):ThumbnailRaster=>({blob:new Blob([new Uint8Array(128).fill(value)]),width:160,height:240});
beforeEach(()=>{Object.defineProperty(URL,'createObjectURL',{configurable:true,value:vi.fn(()=>`blob:thumbnail-${Math.random()}`)});Object.defineProperty(URL,'revokeObjectURL',{configurable:true,value:vi.fn()});});
afterEach(()=>vi.restoreAllMocks());

describe('transient page thumbnail budgets and lifecycle',()=>{
 it('uses one decode/read, discards obsolete demand through a 1000-page fast scroll and never preloads the book',async()=>{
  const release:Array<(value:ThumbnailRaster)=>void>=[];const indexes:number[]=[];
  const source=vi.fn((index:number)=>new Promise<ThumbnailRaster>(resolve=>{indexes.push(index);release.push(resolve);}));
  const cache=new PageThumbnailCache('book:revision',1000,source),pending:Promise<unknown>[]=[];
  for(let start=0;start<990;start+=20){const window=Array.from({length:12},(_,i)=>start+i);cache.demand(window);for(const i of window)pending.push(cache.load(i));}
  expect(source).toHaveBeenCalledTimes(1);expect(cache.stats.queued).toBeLessThanOrEqual(12);expect(cache.stats.peakActive).toBe(1);
  release[0](raster());await waitFor(()=>expect(indexes).toHaveLength(2));expect(indexes[1]).toBe(980);
  cache.dispose();release[1](raster());await Promise.all(pending);await waitFor(()=>expect(cache.stats.active).toBe(0));
  expect(cache.stats.entries).toBe(0);expect(cache.stats.bytes).toBe(0);expect(URL.createObjectURL).not.toHaveBeenCalled();expect(indexes).toEqual([0,980]);
 });
 it('bounds 500 sequential PDF previews to the independent LRU budget and releases every retained URL on close',async()=>{
  const cache=new PageThumbnailCache('pdf:revision',500,async index=>raster(index));
  for(let index=0;index<500;index++){cache.demand([index]);await cache.load(index);expect(cache.stats.entries).toBeLessThanOrEqual(32);expect(cache.stats.bytes).toBeLessThanOrEqual(8*1024*1024);}
  expect(cache.stats.requests).toBe(500);expect(cache.stats.peakActive).toBe(1);expect(cache.peek(0)).toBeUndefined();
  const count=vi.mocked(URL.createObjectURL).mock.calls.length;cache.dispose();expect(URL.revokeObjectURL).toHaveBeenCalledTimes(count);expect(cache.stats.bytes).toBe(0);
 });
 it('shows stable failure state, retries explicitly and never retains an oversized raster',async()=>{
  let attempts=0;const source=vi.fn(async()=>{if(attempts++===0)throw Error('bad page');return raster();});const cache=new PageThumbnailCache('image',30,source);
  cache.demand([4]);expect(await cache.load(4)).toEqual({status:'error'});expect(await cache.load(4)).toEqual({status:'error'});expect(source).toHaveBeenCalledTimes(1);
  await waitFor(()=>expect(cache.stats.active).toBe(0));expect((await cache.retry(4))?.status).toBe('ready');cache.fail(4);expect(URL.revokeObjectURL).toHaveBeenCalledOnce();cache.dispose();
  const invalid=new PageThumbnailCache('bad',2,async()=>({...raster(),width:THUMBNAIL_LIMITS.width+1}));expect(await invalid.load(0)).toEqual({status:'error'});invalid.dispose();
 });
 it('does not commit an old revision after the panel closes and resumes a re-entered cancelled visible page',async()=>{
  const releases:Array<(value:ThumbnailRaster)=>void>=[];const cache=new PageThumbnailCache('old-revision',300,()=>new Promise(resolve=>releases.push(resolve)));
  cache.demand([0]);const first=cache.load(0);cache.demand([200]);cache.demand([0]);await cache.load(-1);releases[0](raster());await first;
  await waitFor(()=>expect(releases).toHaveLength(2));releases[1](raster());await waitFor(()=>expect(cache.peek(0)?.status).toBe('ready'));cache.dispose();
  const old=new PageThumbnailCache('old',2,()=>new Promise(resolve=>releases.push(resolve)));const late=old.load(0);old.dispose();const before=vi.mocked(URL.createObjectURL).mock.calls.length;releases[2](raster());await late;expect(URL.createObjectURL).toHaveBeenCalledTimes(before);
 });
});

describe('safe low-resolution page sources',()=>{
 const context={fillStyle:'',fillRect:vi.fn(),drawImage:vi.fn()};
 beforeEach(()=>{vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockImplementation(()=>context as unknown as CanvasRenderingContext2D);vi.spyOn(HTMLCanvasElement.prototype,'toBlob').mockImplementation(function(callback){callback(new Blob([new Uint8Array(64)],{type:'image/webp'}));});});
 afterEach(()=>{vi.unstubAllGlobals();vi.useRealTimers();});
 it('reads using the same book revision and decodes a PNG-sized thumbnail rather than retaining original dimensions',async()=>{
  const bytes=new Uint8Array(24);bytes.set([137,80,78,71]);const view=new DataView(bytes.buffer);view.setUint32(16,2400);view.setUint32(20,3600);
  const close=vi.fn(),bitmap=vi.fn(async(_blob:Blob,options?:ImageBitmapOptions)=>({width:options?.resizeWidth??2400,height:options?.resizeHeight??3600,close}));vi.stubGlobal('createImageBitmap',bitmap);
  const read=vi.fn(async()=>bytes.buffer);const source=createComicThumbnailSource({bookId:12,revision:'exact-r2',read});const result=await source(7,new AbortController().signal);
  expect(read).toHaveBeenCalledWith(12,7,'exact-r2');expect(bitmap.mock.calls[0][1]).toEqual({resizeWidth:160,resizeHeight:240,resizeQuality:'medium'});expect(result.width).toBe(160);expect(result.height).toBe(240);expect(close).toHaveBeenCalledOnce();
 });
 it('checks cancellation after the bounded native read and releases bitmap/canvas resources',async()=>{
  let finish:(value:ArrayBuffer)=>void=()=>{};const source=createComicThumbnailSource({bookId:1,revision:'r',read:()=>new Promise(resolve=>finish=resolve)});const controller=new AbortController();const pending=source(0,controller.signal);controller.abort();finish(new ArrayBuffer(0));await expect(pending).rejects.toMatchObject({name:'AbortError'});
 });
 it('uses the already-open PDF and cancels previews before foreground rendering or search cleanup can start',async()=>{
  const scheduler=new PdfPageScheduler(),order:string[]=[];let rejectRender:(reason:unknown)=>void=()=>{};
  const cancel=vi.fn(()=>{order.push('cancel');rejectRender(Object.assign(Error('cancelled'),{name:'RenderingCancelledException'}));});
  const cleanup=vi.fn(()=>order.push('cleanup'));const page={getViewport:({scale}:{scale:number})=>({width:600*scale,height:900*scale}),render:()=>({promise:new Promise((_resolve,reject)=>rejectRender=reject),cancel}),cleanup} as unknown as PDFPageProxy;
  const getPage=vi.fn(async()=>page);const source=createPdfThumbnailSource({document:{numPages:500,getPage} as unknown as PDFDocumentProxy,scheduler});
  const pending=source(199,new AbortController().signal);await waitFor(()=>expect(getPage).toHaveBeenCalledWith(200));const cancelled=expect(pending).rejects.toMatchObject({name:'AbortError'});
  const foreground=scheduler.run('foreground',new AbortController().signal,async()=>{order.push('foreground');return 1;});await cancelled;await foreground;
  expect(cancel).toHaveBeenCalledOnce();await waitFor(()=>expect(cleanup).toHaveBeenCalledOnce());expect(order).toEqual(['cancel','foreground','cleanup']);scheduler.dispose();
 });
 it('bounds the rendered PDF canvas and clears it after encoding',async()=>{
  const scheduler=new PdfPageScheduler();let canvas:HTMLCanvasElement|undefined;const cleanup=vi.fn();const page={getViewport:({scale}:{scale:number})=>({width:1400*scale,height:700*scale}),render:(args:{canvas:HTMLCanvasElement})=>{canvas=args.canvas;return{promise:Promise.resolve(),cancel:vi.fn()};},cleanup} as unknown as PDFPageProxy;
  const source=createPdfThumbnailSource({document:{numPages:50,getPage:vi.fn(async()=>page)} as unknown as PDFDocumentProxy,scheduler});const result=await source(49,new AbortController().signal);expect(result).toMatchObject({width:160,height:80});expect(canvas?.width).toBe(0);expect(canvas?.height).toBe(0);await waitFor(()=>expect(cleanup).toHaveBeenCalledOnce());scheduler.dispose();
 });
 it('releases the foreground gate when getPage stalls, and defers the late page cleanup until foreground work finishes',async()=>{
  const scheduler=new PdfPageScheduler();let releasePage:(page:PDFPageProxy)=>void=()=>{},releaseForeground:()=>void=()=>{};const cleanup=vi.fn();
  const doc={numPages:500,getPage:vi.fn(()=>new Promise<PDFPageProxy>(resolve=>releasePage=resolve))} as unknown as PDFDocumentProxy;
  const source=createPdfThumbnailSource({document:doc,scheduler}),controller=new AbortController();const preview=source(199,controller.signal);const rejection=expect(preview).rejects.toMatchObject({name:'AbortError'});
  await waitFor(()=>expect(doc.getPage).toHaveBeenCalledOnce());controller.abort();await rejection;
  const foreground=scheduler.run('foreground',new AbortController().signal,()=>new Promise<void>(resolve=>releaseForeground=resolve));await waitFor(()=>expect(scheduler.stats.active).toBe(1));
  releasePage({cleanup} as unknown as PDFPageProxy);await Promise.resolve();await Promise.resolve();expect(cleanup).not.toHaveBeenCalled();releaseForeground();await foreground;await waitFor(()=>expect(cleanup).toHaveBeenCalledOnce());scheduler.dispose();
 });
 it('times out a hanging getPage without issuing further preview requests or blocking a later foreground task',async()=>{
  vi.useFakeTimers();const scheduler=new PdfPageScheduler();const getPage=vi.fn(()=>new Promise<PDFPageProxy>(()=>{}));const source=createPdfThumbnailSource({document:{numPages:500,getPage} as unknown as PDFDocumentProxy,scheduler});
  const first=source(1,new AbortController().signal),timeout=expect(first).rejects.toThrow('PAGE_PREVIEW_TIMEOUT');await vi.advanceTimersByTimeAsync(THUMBNAIL_LIMITS.timeoutMs+1);await timeout;
  expect(await scheduler.run('foreground',new AbortController().signal,async()=>42)).toBe(42);
  const next=source(200,new AbortController().signal),nextTimeout=expect(next).rejects.toThrow('PAGE_PREVIEW_TIMEOUT');await vi.advanceTimersByTimeAsync(THUMBNAIL_LIMITS.timeoutMs+1);await nextTimeout;expect(getPage).toHaveBeenCalledOnce();scheduler.dispose();
 });
 it('releases the foreground gate even if cancel does not settle the preview render promise',async()=>{
  const scheduler=new PdfPageScheduler();let releaseRender:()=>void=()=>{},releaseForeground:()=>void=()=>{};const cleanup=vi.fn(),cancel=vi.fn();
  const page={getViewport:({scale}:{scale:number})=>({width:600*scale,height:900*scale}),render:vi.fn(()=>({promise:new Promise<void>(resolve=>releaseRender=resolve),cancel})),cleanup} as unknown as PDFPageProxy;
  const source=createPdfThumbnailSource({document:{numPages:50,getPage:vi.fn(async()=>page)} as unknown as PDFDocumentProxy,scheduler}),controller=new AbortController();const preview=source(0,controller.signal),rejection=expect(preview).rejects.toMatchObject({name:'AbortError'});
  await waitFor(()=>expect(page.render).toHaveBeenCalledOnce());controller.abort();await rejection;expect(cancel).toHaveBeenCalledOnce();
  const foreground=scheduler.run('foreground',new AbortController().signal,()=>new Promise<void>(resolve=>releaseForeground=resolve));await Promise.resolve();await Promise.resolve();releaseRender();await Promise.resolve();expect(cleanup).not.toHaveBeenCalled();releaseForeground();await foreground;await waitFor(()=>expect(cleanup).toHaveBeenCalledOnce());scheduler.dispose();
 });
 it('preserves image aspect ratio and recognizes signatures rather than trusting a page suffix',()=>{
  expect(thumbnailSize(900,1800)).toEqual({width:120,height:240});expect(thumbnailSize(100,100)).toEqual({width:100,height:100});
  const gif=new Uint8Array([71,73,70,56,57,97,88,2,132,3]);expect(thumbnailImageSize(gif)).toEqual({width:600,height:900});expect(thumbnailImageSize(new Uint8Array(20))).toBeUndefined();
  const box=(name:string,body:Uint8Array)=>{const result=new Uint8Array(8+body.length);new DataView(result.buffer).setUint32(0,result.length);result.set([...name].map(c=>c.charCodeAt(0)),4);result.set(body,8);return result;};
  const size=new Uint8Array(12);new DataView(size.buffer).setUint32(4,1200);new DataView(size.buffer).setUint32(8,1800);const tree=box('meta',new Uint8Array([0,0,0,0,...box('iprp',box('ipco',box('ispe',size)))]));const avif=new Uint8Array([...box('ftyp',new TextEncoder().encode('avif0000')),...tree]);expect(thumbnailImageSize(avif)).toEqual({width:1200,height:1800});
 });
});

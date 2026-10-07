// @vitest-environment jsdom
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {ComicPageCache,comicSpreads,isWide,readerKeyStep} from './comicReader';
import {comicMessages} from './comicMessages';
describe('comic layout and bounded binary previews',()=>{
 beforeEach(()=>{vi.stubGlobal('Image',class{src='';naturalWidth=600;naturalHeight=900;decode(){return Promise.resolve();}});Object.defineProperty(URL,'createObjectURL',{configurable:true,value:vi.fn(()=>`blob:page-${Math.random()}`)});Object.defineProperty(URL,'revokeObjectURL',{configurable:true,value:vi.fn()});});
 afterEach(()=>vi.unstubAllGlobals());
 it('keeps wide pages alone and resumes pairing after them',()=>{const sizes=new Map([[1,{width:1800,height:900}]]);expect(comicSpreads(6,true,true,sizes)).toEqual([[0],[1],[2,3],[4,5]]);expect(comicSpreads(4,true,false,sizes)).toEqual([[0,1],[2,3]]);expect(comicSpreads(3,false,true,sizes)).toEqual([[0],[1],[2]]);expect(isWide({width:1200,height:1000})).toBe(false);});
 it('maps direction keys without changing PageUp/Down order',()=>{expect(readerKeyStep('ArrowLeft',true)).toBe(1);expect(readerKeyStep('ArrowRight',true)).toBe(-1);expect(readerKeyStep('ArrowRight',false)).toBe(1);expect(readerKeyStep('PageDown',true)).toBe(1);expect(readerKeyStep('PageUp',false)).toBe(-1);expect(readerKeyStep(' ',true)).toBe(1);expect(readerKeyStep('b',true)).toBe(0);});
 it('reuses a page, evicts old URLs, and revokes all on unmount',async()=>{const read=vi.fn(async()=>new ArrayBuffer(1));const cache=new ComicPageCache(1,Array.from({length:30},(_,i)=>({pageIndex:i,pageName:`${i}.png`})),vi.fn(),read);const first=await cache.load(0);await cache.load(0);expect(read).toHaveBeenCalledTimes(1);for(let i=1;i<20;i++)await cache.load(i);expect(cache.peek(0)).toBeUndefined();expect(URL.revokeObjectURL).toHaveBeenCalledWith(first?.url);cache.dispose();expect(URL.revokeObjectURL).toHaveBeenCalledTimes(20);});
 it('allows two native reads at once and drops disposed pending results',async()=>{const resolvers:((value:ArrayBuffer)=>void)[]=[];const read=vi.fn(()=>new Promise<ArrayBuffer>(resolve=>resolvers.push(resolve)));const cache=new ComicPageCache(1,[0,1,2].map(i=>({pageIndex:i,pageName:`${i}.png`})),vi.fn(),read);const pending=[cache.load(0),cache.load(1),cache.load(2)];expect(read).toHaveBeenCalledTimes(2);cache.dispose();resolvers.forEach(resolve=>resolve(new ArrayBuffer(1)));await Promise.all(pending);expect(URL.createObjectURL).not.toHaveBeenCalled();});
 it('has complete independent translations in every interface language',()=>{for(const locale of ['en-US','ja-JP','ko-KR'] as const){expect(Object.keys(comicMessages[locale]).sort()).toEqual(Object.keys(comicMessages['zh-CN']).sort());expect(Object.values(comicMessages[locale]).every(value=>!!value.trim())).toBe(true);}});
 it('drops superseded queued pages without cancelling the two active reads',async()=>{
   const resolvers:((value:ArrayBuffer)=>void)[]=[];const read=vi.fn((_book:number,_index:number)=>new Promise<ArrayBuffer>(resolve=>resolvers.push(resolve)));
   const cache=new ComicPageCache(1,Array.from({length:100},(_,i)=>({pageIndex:i,pageName:`${i}.png`})),vi.fn(),read);
   const active=[cache.load(0),cache.load(1)],obsolete=[cache.load(2),cache.load(3),cache.load(4)];
   cache.demand(new Set([80,81]));await Promise.all(obsolete);expect(read).toHaveBeenCalledTimes(2);
   const latest=[cache.load(80),cache.load(81)];resolvers.slice(0,2).forEach(resolve=>resolve(new ArrayBuffer(1)));await Promise.all(active);
   await new Promise(resolve=>setTimeout(resolve,0));expect(read.mock.calls.map(call=>call[1])).toEqual([0,1,80,81]);
   cache.dispose();resolvers.slice(2).forEach(resolve=>resolve(new ArrayBuffer(1)));await Promise.all(latest);
 });
});

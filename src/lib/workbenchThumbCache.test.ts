// @vitest-environment jsdom
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {WorkbenchThumbCache} from './workbenchThumbCache';

beforeEach(()=>{vi.useFakeTimers();let serial=0;URL.createObjectURL=vi.fn(()=>`blob:cache-${++serial}`);URL.revokeObjectURL=vi.fn();});
afterEach(()=>{vi.useRealTimers();});
describe('bounded workbench thumbnail leases',()=>{
 it('deduplicates reads and synchronously reuses an existing URL after return',async()=>{
  const cache=new WorkbenchThumbCache(),load=vi.fn(async()=>new Blob(['image']));
  const first=cache.acquire('screenshot:1:r1:thumb',load),second=cache.acquire('screenshot:1:r1:thumb',load);
  const url=await first.ready;expect(await second.ready).toBe(url);expect(load).toHaveBeenCalledOnce();first.release();second.release();
  const returned=cache.acquire('screenshot:1:r1:thumb',load);expect(returned.url).toBe(url);expect(cache.peek('screenshot:1:r1:thumb')).toBe(url);expect(load).toHaveBeenCalledOnce();returned.release();cache.clear();await vi.runAllTimersAsync();expect(URL.revokeObjectURL).toHaveBeenCalledWith(url);
 });
 it('binds the key to revision, enforces inactive byte/count bounds and never revokes an active lease',async()=>{
  const cache=new WorkbenchThumbCache(2,5),load=async()=>new Blob(['123']);
  const active=cache.acquire('a:r1',load),activeUrl=await active.ready;
  const other=cache.acquire('b:r1',load);await other.ready;other.release();expect(cache.size).toBe(1);expect(cache.peek('a:r1')).toBe(activeUrl);
  await vi.runAllTimersAsync();expect(URL.revokeObjectURL).not.toHaveBeenCalledWith(activeUrl);
  active.release();const revision=cache.acquire('a:r2',load);await revision.ready;revision.release();expect(cache.peek('a:r1')).toBeNull();expect(cache.size).toBe(1);cache.clear();await vi.runAllTimersAsync();
 });
 it('waits for detached pending image decode before revoking an evicted URL',async()=>{
  const cache=new WorkbenchThumbCache(1,10),lease=cache.acquire('a',async()=>new Blob(['a'])),url=await lease.ready;
  let done!:()=>void;const decode=new Promise<void>(resolve=>{done=resolve;});
  const image=document.createElement('img');image.src=url!;Object.defineProperty(image,'complete',{value:false});image.decode=()=>decode;
  lease.release(image);cache.clear();await vi.runAllTimersAsync();expect(URL.revokeObjectURL).not.toHaveBeenCalled();done();await decode;await vi.runAllTimersAsync();expect(URL.revokeObjectURL).toHaveBeenCalledWith(url);
 });
 it('runs at most four reads and cancels abandoned queued work without allocating Blob URLs',async()=>{
  const cache=new WorkbenchThumbCache(),complete:Array<(blob:Blob)=>void>=[],load=vi.fn(()=>new Promise<Blob>(resolve=>complete.push(resolve)));
  const leases=Array.from({length:8},(_,index)=>cache.acquire(String(index),load));expect(load).toHaveBeenCalledTimes(4);
  leases.forEach(lease=>lease.release());complete.forEach(resolve=>resolve(new Blob(['unused'])));await Promise.all(leases.map(lease=>lease.ready));expect(load).toHaveBeenCalledTimes(4);expect(URL.createObjectURL).not.toHaveBeenCalled();expect(cache.size).toBe(0);
 });
});

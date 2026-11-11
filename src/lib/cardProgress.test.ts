// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {CardProgressStore,bookCardProgress,cardProgressLabel} from './cardProgress';
import type {CardProgress} from '../types/cardProgress';
import type {ComicBook} from '../types/comic';
afterEach(()=>vi.useRealTimers());
it('keeps the same snapshot reference and skips notifications for unchanged confirmed values',()=>{
 const store=new CardProgressStore(vi.fn()),value={nodeId:19,workView:false,watched:2,total:12,started:true};
 store.seed(value,store.beginSnapshot());const changed=vi.fn(),stop=store.subscribe('19:false',value,true,changed);
 const original=store.get('19:false');store.seed({...value},store.beginSnapshot());
 expect(store.get('19:false')).toBe(original);expect(changed).not.toHaveBeenCalled();
 store.seed({...value,watched:3},store.beginSnapshot());expect(store.get('19:false')?.watched).toBe(3);expect(changed).toHaveBeenCalledOnce();stop();
});
it('seeds native collection summaries synchronously without a first-paint progress request',async()=>{
 vi.useFakeTimers();const read=vi.fn();const store=new CardProgressStore(read),stamp=store.beginSnapshot();
 const value={nodeId:8001,workView:false,watched:3,total:12,started:true};store.seed(value,stamp);
 expect(store.get('8001:false')).toEqual(value);const stop=store.subscribe('8001:false',value,true,()=>{});
 await vi.advanceTimersByTimeAsync(1000);expect(read).not.toHaveBeenCalled();stop();
});
it('rejects an older collection response and an older fallback read after a newer snapshot or edit',async()=>{
 vi.useFakeTimers();let finish!:(rows:CardProgress[])=>void;const read=vi.fn(()=>new Promise<CardProgress[]>(resolve=>finish=resolve));const store=new CardProgressStore(read);
 const row={nodeId:8002,workView:false,watched:0,total:12,started:false},stop=store.subscribe('8002:false',row,true,()=>{});await vi.advanceTimersByTimeAsync(1);
 const old=store.beginSnapshot(),fresh=store.beginSnapshot();store.seed({...row,watched:4,started:true},fresh);store.seed(row,old);finish([row]);await vi.advanceTimersByTimeAsync(1);expect(store.get('8002:false')?.watched).toBe(4);
 store.update({...row,watched:5,started:true});store.seed(row,fresh);expect(store.get('8002:false')?.watched).toBe(5);stop();
});
it('keeps active first-paint values beyond the inactive cache budget and revalidates old navigation snapshots',async()=>{
 vi.useFakeTimers();const read=vi.fn(async rows=>rows.map((r:CardProgress)=>({...r,watched:0,total:12,started:false}))),store=new CardProgressStore(read),stamp=store.beginSnapshot(),stops=[];
 for(let id=1;id<=4200;id++){const row={nodeId:id,workView:false,watched:2,total:12,started:true};store.seed(row,stamp);stops.push(store.subscribe(`${id}:false`,row,true,()=>{},row,stamp.generation));}
 expect(store.get('1:false')?.watched).toBe(2);await vi.advanceTimersByTimeAsync(1000);expect(read).not.toHaveBeenCalled();stops.forEach(stop=>stop());
 store.invalidate();const row={nodeId:1,workView:false,watched:2,total:12,started:true},stop=store.subscribe('1:false',row,true,()=>{},row,stamp.generation);expect(store.get('1:false')?.watched).toBe(2);await vi.advanceTimersByTimeAsync(1);expect(store.get('1:false')?.watched).toBe(0);stop();
});
it('batches visible cards once and rejects a stale in-flight result after progress clears',async()=>{
 vi.useFakeTimers();let resolve!:(rows:CardProgress[])=>void;const read=vi.fn().mockImplementationOnce(()=>new Promise(r=>resolve=r)).mockImplementation(async requests=>requests.map((r:{nodeId:number;workView:boolean})=>({...r,watched:0,total:12,started:false})));
 const store=new CardProgressStore(read),stops=[];for(let id=1;id<=18;id++)stops.push(store.subscribe(String(id),{nodeId:id,workView:false},true,()=>{}));
 await vi.advanceTimersByTimeAsync(24);expect(read).toHaveBeenCalledOnce();expect(read.mock.calls[0][0]).toHaveLength(16);
 store.invalidate();resolve([{nodeId:1,workView:false,watched:12,total:12,started:true}]);await vi.advanceTimersByTimeAsync(60);
 expect(store.get('1')?.watched).toBe(0);expect(read.mock.calls.every(([requests])=>requests.length<=16)).toBe(true);expect(read.mock.calls).toHaveLength(3);
 const calls=read.mock.calls.length;stops.forEach(stop=>stop());const stop=store.subscribe('1',{nodeId:1,workView:false},true,()=>{});await vi.advanceTimersByTimeAsync(30);expect(read).toHaveBeenCalledTimes(calls);stop();
});
it('does not read cards outside the viewport or infer completed books from an opened timestamp',async()=>{
 vi.useFakeTimers();const read=vi.fn();const store=new CardProgressStore(read);const stop=store.subscribe('hidden',{nodeId:1,workView:false},false,()=>{});await vi.advanceTimersByTimeAsync(60);expect(read).not.toHaveBeenCalled();stop();
 const book={id:1,nodeId:2,pageCount:100,progress:null} as ComicBook;expect(bookCardProgress([book])).toMatchObject({watched:0,total:100,started:false});expect(bookCardProgress([{...book,progress:{comicBookId:1,lastPageIndex:12,lastReadAt:'2026-10-26'}}])).toMatchObject({watched:13,total:100,started:true});
 const t=(key:string,values?:Record<string,string|number>)=>key+JSON.stringify(values??{});expect(cardProgressLabel(t,false,{nodeId:1,workView:false,watched:2,total:12,started:true})).toContain('card.videoProgress');expect(cardProgressLabel(t,false,undefined)).toBe('');
});

it('does not churn at the cache limit even if more than 256 cards are subscribed',async()=>{
 vi.useFakeTimers();const read=vi.fn(async requests=>requests.map((r:{nodeId:number;workView:boolean})=>({...r,watched:0,total:12,started:false})));const store=new CardProgressStore(read),stops=[];
 for(let id=1;id<=270;id++)stops.push(store.subscribe(String(id),{nodeId:id,workView:false},true,()=>{}));
 await vi.advanceTimersByTimeAsync(24*20);expect(read).toHaveBeenCalledTimes(17);await vi.advanceTimersByTimeAsync(1000);expect(read).toHaveBeenCalledTimes(17);expect(store.get('270')?.total).toBe(12);stops.forEach(stop=>stop());
});

it('retries transient failures and preserves confirmed summaries during revalidation and navigation',async()=>{
 vi.useFakeTimers();let finish!:(rows:CardProgress[])=>void;const read=vi.fn().mockRejectedValueOnce(new Error('transient')).mockResolvedValueOnce([{nodeId:1,workView:false,watched:1,total:12,started:true}]).mockImplementationOnce(()=>new Promise(resolve=>finish=resolve));
 const store=new CardProgressStore(read);let stop=store.subscribe('1:false',{nodeId:1,workView:false},true,()=>{});await vi.advanceTimersByTimeAsync(1);expect(store.get('1:false')).toBeUndefined();await vi.advanceTimersByTimeAsync(500);expect(store.get('1:false')?.watched).toBe(1);
 store.invalidate();expect(store.get('1:false')?.watched).toBe(1);await vi.advanceTimersByTimeAsync(1);stop();stop=store.subscribe('1:false',{nodeId:1,workView:false},true,()=>{});expect(store.get('1:false')?.watched).toBe(1);
 store.update({nodeId:1,workView:false,watched:0,total:12,started:false});finish([{nodeId:1,workView:false,watched:12,total:12,started:true}]);await vi.advanceTimersByTimeAsync(10);expect(store.get('1:false')?.watched).toBe(0);stop();
});
it('bounds failure retries, stops hidden work and allows a fresh attempt on remount',async()=>{
 vi.useFakeTimers();const read=vi.fn().mockRejectedValue(new Error('unavailable'));const store=new CardProgressStore(read);let stop=store.subscribe('1',{nodeId:1,workView:false},true,()=>{});await vi.advanceTimersByTimeAsync(60000);expect(read).toHaveBeenCalledTimes(3);stop();await vi.advanceTimersByTimeAsync(60000);expect(read).toHaveBeenCalledTimes(3);stop=store.subscribe('1',{nodeId:1,workView:false},true,()=>{});await vi.advanceTimersByTimeAsync(1);expect(read).toHaveBeenCalledTimes(4);stop();await vi.advanceTimersByTimeAsync(60000);expect(read).toHaveBeenCalledTimes(4);
});
it('keeps summaries for below-the-fold cards and never clears them just because a page unmounts',async()=>{
 vi.useFakeTimers();const read=vi.fn(async requests=>requests.map((r:{nodeId:number;workView:boolean})=>({...r,watched:1,total:12,started:true})));const store=new CardProgressStore(read),stops=[];for(let id=1;id<=600;id++)stops.push(store.subscribe(String(id),{nodeId:id,workView:false},true,()=>{}));await vi.advanceTimersByTimeAsync(200);expect(read).toHaveBeenCalledTimes(38);stops.forEach(s=>s());expect(store.get('1')?.watched).toBe(1);expect(store.get('600')?.watched).toBe(1);const stop=store.subscribe('1',{nodeId:1,workView:false},true,()=>{});expect(store.get('1')?.watched).toBe(1);await vi.advanceTimersByTimeAsync(10);expect(read).toHaveBeenCalledTimes(38);stop();
});

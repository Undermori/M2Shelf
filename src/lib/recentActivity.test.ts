// @vitest-environment jsdom
import {createElement,useSyncExternalStore} from 'react';
import {act,render,cleanup} from '@testing-library/react';
import {describe,it,expect,vi} from 'vitest';
import {RecentActivityStore} from './recentActivity';
import type {RecentlyWatchedEntry} from '../types/media';

const entries=(name:string)=>[{node:{id:1,displayName:name},watchedAt:'2026-01-01'}] as RecentlyWatchedEntry[];
function pending<T>(){let resolve!:(value:T)=>void;let reject!:(error:Error)=>void;const promise=new Promise<T>((yes,no)=>{resolve=yes;reject=no;});return{promise,resolve,reject};}

describe('Recently opened presentation lifecycle',()=>{
 it('does not rerender an off-page subscriber when idle preparation starts or finishes',async()=>{
  const request=pending<RecentlyWatchedEntry[]>();const store=new RecentActivityStore(()=>request.promise);let renders=0;
  function CurrentPage(){const revision=useSyncExternalStore(store.subscribe,store.getRevision);renders++;return createElement('span',null,revision);}
  const view=render(createElement(CurrentPage));expect(renders).toBe(1);
  let read!:Promise<RecentlyWatchedEntry[]|null>;
  await act(async()=>{read=store.ensureFresh();});expect(renders).toBe(1);
  await act(async()=>{request.resolve(entries('ready'));await read;});expect(renders).toBe(1);
  act(()=>store.invalidate());expect(renders).toBe(2);view.unmount();cleanup();
 });
 it('joins a current read and needs no request for a fresh navigation',async()=>{
  const request=pending<RecentlyWatchedEntry[]>();const read=vi.fn(()=>request.promise);const store=new RecentActivityStore(read);
  const a=store.ensureFresh(),b=store.ensureFresh();await Promise.resolve();expect(read).toHaveBeenCalledTimes(1);
  request.resolve(entries('ready'));expect(await a).toEqual(await b);await store.ensureFresh();expect(read).toHaveBeenCalledTimes(1);
 });
 it.each(['success','failure'])('fences an older %s and makes waiting navigation join the post-mutation read',async(kind)=>{
  const old=pending<RecentlyWatchedEntry[]>(),latest=pending<RecentlyWatchedEntry[]>();
  const read=vi.fn().mockReturnValueOnce(old.promise).mockReturnValueOnce(latest.promise);const store=new RecentActivityStore(read);
  const navigation=store.ensureFresh();await Promise.resolve();const mutation=store.refresh();await Promise.resolve();
  if(kind==='success')old.resolve(entries('stale'));else old.reject(new Error('old error'));
  await Promise.resolve();expect(store.getSnapshot().entries).toBeNull();latest.resolve(entries('current'));
  expect(await navigation).toEqual(entries('current'));await mutation;expect(read).toHaveBeenCalledTimes(2);
 });
 it('keeps a confirmed snapshot on current failure and permits a foreground retry',async()=>{
  const read=vi.fn().mockResolvedValueOnce(entries('confirmed')).mockRejectedValueOnce(new Error('offline')).mockResolvedValueOnce(entries('latest'));
  const store=new RecentActivityStore(read);await store.ensureFresh();await expect(store.refresh()).rejects.toThrow('offline');
  expect(store.getSnapshot()).toMatchObject({entries:entries('confirmed'),fresh:false,loading:false});
  expect(await store.ensureFresh()).toEqual(entries('latest'));
 });
 it('does not resurrect a cleared snapshot from a pending completion',async()=>{
  const request=pending<RecentlyWatchedEntry[]>();const store=new RecentActivityStore(()=>request.promise);
  const navigation=store.ensureFresh();await Promise.resolve();store.update(()=>[],true);request.resolve(entries('removed'));
  expect(await navigation).toEqual([]);expect(store.getSnapshot()).toMatchObject({entries:[],fresh:true,loading:false});
 });
 it('invalidates even when a mutation arrives before the first snapshot',async()=>{
  const old=pending<RecentlyWatchedEntry[]>();const read=vi.fn().mockReturnValueOnce(old.promise).mockResolvedValueOnce(entries('renamed'));
  const store=new RecentActivityStore(read);const navigation=store.ensureFresh();await Promise.resolve();store.update(value=>value);
  old.resolve(entries('old name'));expect(await navigation).toEqual(entries('renamed'));expect(read).toHaveBeenCalledTimes(2);
 });
});

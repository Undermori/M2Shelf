import { expect, it, vi } from "vitest";
import type { AppSettings } from "../types/media";
import { SettingsStore } from "./settingsStore";

const initial: AppSettings = {mpvPath:null,defaultViewMode:'GRID',videoExtensions:['.mkv'],bangumiSearchEnabled:true,autoCheckUpdates:false,autoScanOnStartup:false,allResourcesFlattened:false,language:'zh-CN',theme:'dark',coverCacheDirectory:'X:/Fixture/Covers'};
function deferred<T>() { let resolve!: (value:T)=>void; let reject!: (reason:Error)=>void; const promise=new Promise<T>((yes,no)=>{resolve=yes;reject=no;});return {promise,resolve,reject}; }

it('deduplicates reads and serializes caption/page changes without losing unrelated settings', async () => {
  const first=deferred<AppSettings>(), second=deferred<AppSettings>();
  const save=vi.fn().mockImplementationOnce(()=>first.promise).mockImplementationOnce(()=>second.promise);
  const get=vi.fn(async()=>initial), store=new SettingsStore({get,save});
  await Promise.all([store.load(),store.load()]);expect(get).toHaveBeenCalledTimes(1);
  store.change(s=>({...s,theme:'light'}));
  store.change(s=>({...s,language:'ja-JP',mpvPath:'X:/Fixture/player.exe'}));
  expect(save).toHaveBeenCalledTimes(1);
  first.resolve({...initial,theme:'light'});
  await vi.waitFor(()=>expect(save).toHaveBeenCalledTimes(2));
  expect(save.mock.calls[1][0]).toMatchObject({theme:'light',language:'ja-JP',mpvPath:'X:/Fixture/player.exe',videoExtensions:['.mkv']});
  expect(store.getSnapshot().settings?.language).toBe('ja-JP');
  second.resolve(save.mock.calls[1][0]);
  await vi.waitFor(()=>expect(store.getSnapshot().saving).toBe(false));
  expect(store.getSnapshot().savedSequence).toBe(1);
});

it('keeps the queue alive after subscribers unmount and rolls back to the last successful save', async () => {
  const pending=deferred<AppSettings>();const save=vi.fn().mockImplementationOnce(async(s:AppSettings)=>s).mockImplementationOnce(()=>pending.promise);
  const store=new SettingsStore({get:async()=>initial,save});await store.load();
  store.change(s=>({...s,theme:'light'}));await vi.waitFor(()=>expect(store.getSnapshot().saving).toBe(false));
  const unsubscribe=store.subscribe(vi.fn());
  store.change(s=>({...s,language:'ko-KR'}));unsubscribe();
  pending.reject(new Error('fixture failure'));
  await vi.waitFor(()=>expect(store.getSnapshot().saving).toBe(false));
  expect(store.getSnapshot().settings).toMatchObject({theme:'light',language:'zh-CN'});
  expect(store.getSnapshot().failure?.sequence).toBe(1);
  expect(store.getSnapshot().savedSequence).toBe(1);
});

it('ignores a superseded failure and persists the newer complete candidate', async () => {
  const first=deferred<AppSettings>();const save=vi.fn().mockImplementationOnce(()=>first.promise).mockImplementationOnce(async(s:AppSettings)=>s);
  const store=new SettingsStore({get:async()=>initial,save});await store.load();
  store.change(s=>({...s,theme:'light'}));store.change(s=>({...s,language:'en-US'}));
  first.reject(new Error('old write failed'));
  await vi.waitFor(()=>expect(store.getSnapshot().saving).toBe(false));
  expect(store.getSnapshot().settings).toMatchObject({theme:'light',language:'en-US'});
  expect(store.getSnapshot().failure).toBeNull();expect(save).toHaveBeenCalledTimes(2);
});

it('awaits onboarding persistence in the same queue and reports failure instead of continuing', async () => {
  const first=deferred<AppSettings>();const save=vi.fn().mockImplementationOnce(()=>first.promise).mockImplementation(async(s:AppSettings)=>s);
  const store=new SettingsStore({get:async()=>initial,save});await store.load();
  store.change(s=>({...s,theme:'light'}));
  const playerSaved=store.save(s=>({...s,mpvPath:'X:/Fixture/player.exe'}));
  await Promise.resolve();first.resolve({...initial,theme:'light'});
  expect(await playerSaved).toMatchObject({theme:'light',mpvPath:'X:/Fixture/player.exe'});
  save.mockRejectedValueOnce(new Error('fixture write failure'));
  await expect(store.save(s=>({...s,mpvPath:'X:/Fixture/other.exe'}))).rejects.toThrow();
  expect(store.getSnapshot().settings?.mpvPath).toBe('X:/Fixture/player.exe');
});

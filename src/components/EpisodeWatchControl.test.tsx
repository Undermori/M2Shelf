// @vitest-environment jsdom
import {act, cleanup, fireEvent, render, renderHook, screen, waitFor} from '@testing-library/react';
import {afterEach, beforeEach, expect, it, vi} from 'vitest';
import {I18nProvider} from '../lib/i18n';
import type {MediaFile, MediaNode, NodeDetail} from '../types/media';
import type {EpisodeWatchSnapshot} from '../types/episodes';
import {EpisodeWatchSummary, useEpisodeWatch} from './EpisodeWatchControl';
import {MediaFileList} from './MediaFileList';

const calls=vi.hoisted(()=>({read:vi.fn(),mark:vi.fn(),clear:vi.fn()}));
vi.mock('../lib/api',()=>({api:{episodeWatchSnapshot:calls.read,markEpisodeWatched:calls.mark,clearEpisodeWatch:calls.clear}}));
const node:MediaNode={id:2,libraryRootId:1,mediaKind:'ANIMATION',parentNodeId:1,absolutePath:'X:/Synthetic/Show',folderName:'Show',displayName:'Show',nodeType:'WORK',manualTypeOverride:false,coverSource:'PLACEHOLDER',coverCachePath:null,directVideoCount:2,totalVideoCount:2,childMediaBranchCount:0,createdAt:'2026-01-01',updatedAt:'2026-01-01',lastSeenAt:'2026-01-01',userTags:[]};
const detail:NodeDetail={node,children:[],mediaFiles:[],resourceFiles:[],breadcrumbs:[],binding:null};
const file:MediaFile={id:1,nodeId:2,fileName:'Show S01E01.mkv',absolutePath:'X:/Synthetic/Show/S01E01.mkv',extension:'.mkv',fileSize:123,modifiedAt:'2026-01-01',lastSeenAt:'2026-01-01',durationMs:null,width:null,height:null,codec:null};
const snapshot=(watched=false):EpisodeWatchSnapshot=>({nodeId:2,workView:false,snapshot:watched?'b'.repeat(64):'a'.repeat(64),rows:[{fileId:1,kind:'EPISODE',season:1,episodes:[1],watched,partiallyWatched:false}],watchedEpisodes:watched?1:0,totalEpisodes:2,watchedFiles:0,totalFiles:0,unresolvedRecords:0,latestWatchedFileId:watched?1:null,latestWatchedFileName:watched?file.fileName:null});
function deferred<T>(){let resolve!:(value:T)=>void;const promise=new Promise<T>(yes=>{resolve=yes;});return {promise,resolve};}
beforeEach(()=>{vi.resetAllMocks();calls.read.mockResolvedValue(snapshot());calls.mark.mockResolvedValue(snapshot(true));});
afterEach(cleanup);
it('reconciles natural completion locally without replacing the file row or losing confirmed state',async()=>{
 const inline={...detail,mediaFiles:[file],episodeWatch:snapshot()};
 function Probe(){const watch=useEpisodeWatch(inline);return <MediaFileList watch={watch} files={[file]} onPlay={vi.fn()} onReveal={vi.fn()}/>;}
 render(<I18nProvider><Probe/></I18nProvider>);const row=document.querySelector('.media-file-row'),box=screen.getByRole('checkbox');
 const pending=deferred<EpisodeWatchSnapshot>();calls.read.mockReturnValue(pending.promise);
 act(()=>window.dispatchEvent(new CustomEvent('m2shelf-playback-changed',{detail:{state:'FINISHED',sessionId:1,fileId:1,ownerNodeId:2}})));
 expect(document.querySelector('.media-file-row')).toBe(row);expect(box.getAttribute('aria-checked')).toBe('false');
 await act(async()=>pending.resolve(snapshot(true)));expect(document.querySelector('.media-file-row')).toBe(row);expect(box.getAttribute('aria-checked')).toBe('true');
 expect(calls.mark).not.toHaveBeenCalled();expect(calls.clear).not.toHaveBeenCalled();
});

it('renders the native inline state in the first commit and never starts a second initial read',()=>{
 const inline={...detail,episodeWatch:snapshot(true)};
 function Probe(){const watch=useEpisodeWatch(inline);return <><EpisodeWatchSummary watch={watch}/><MediaFileList watch={watch} files={[file]} onPlay={vi.fn()} onReveal={vi.fn()}/></>;}
 render(<I18nProvider><Probe/></I18nProvider>);
 expect(screen.getByRole('checkbox').getAttribute('aria-checked')).toBe('true');
 expect((screen.getByRole('checkbox') as HTMLButtonElement).disabled).toBe(false);
 expect(screen.getByRole('status').textContent).toContain(file.fileName);
 expect(calls.read).not.toHaveBeenCalled();
});
it('keeps a confirmed local write when an older inline response arrives',async()=>{
 const inline={...detail,episodeWatch:snapshot()};
 const view=renderHook(({value})=>useEpisodeWatch(value),{initialProps:{value:inline}});
 await act(async()=>view.result.current.mark(1,true));expect(view.result.current.data?.rows[0].watched).toBe(true);
 const stale={...inline,episodeWatch:snapshot(),episodeWatchGeneration:-1};
 const pending=deferred<EpisodeWatchSnapshot>();calls.read.mockReturnValue(pending.promise);
 view.rerender({value:stale});expect(view.result.current.data?.rows[0].watched).toBe(true);
 await act(async()=>pending.resolve(snapshot(true)));expect(view.result.current.data?.rows[0].watched).toBe(true);
});

it('double-click plays once and marks only after a successful launch',async()=>{
 const pending=deferred<boolean>();const play=vi.fn(()=>pending.promise);
 function Probe(){const watch=useEpisodeWatch(detail);return <MediaFileList watch={watch} files={[file]} onPlay={play} onReveal={vi.fn()}/>;}
 render(<I18nProvider><Probe/></I18nProvider>);await screen.findByRole('checkbox');
 const row=document.querySelector('.media-file-row')!;fireEvent.doubleClick(row);fireEvent.doubleClick(row);
 expect(play).toHaveBeenCalledOnce();expect(calls.mark).not.toHaveBeenCalled();
 await act(async()=>pending.resolve(true));await waitFor(()=>expect(calls.mark).toHaveBeenCalledWith(2,false,1,true,'a'.repeat(64)));
 expect(screen.getByRole('checkbox').getAttribute('aria-checked')).toBe('true');
});
it('does not mark a failed launch or a single play-button click',async()=>{
 const play=vi.fn().mockResolvedValue(false);
 function Probe(){const watch=useEpisodeWatch(detail);return <MediaFileList watch={watch} files={[file]} onPlay={play} onReveal={vi.fn()}/>;}
 render(<I18nProvider><Probe/></I18nProvider>);await screen.findByRole('checkbox');fireEvent.doubleClick(document.querySelector('.media-file-row')!);
 await waitFor(()=>expect((screen.getByRole('checkbox') as HTMLButtonElement).disabled).toBe(false));expect(calls.mark).not.toHaveBeenCalled();
 play.mockResolvedValue(true);fireEvent.click(screen.getByRole('button',{name:'点击播放 Show S01E01.mkv'}));await waitFor(()=>expect(play).toHaveBeenCalledTimes(2));expect(calls.mark).not.toHaveBeenCalled();
});
it('supports an immediate double-click before the first status load and rejects its late stale response',async()=>{
 const initial=deferred<EpisodeWatchSnapshot>();calls.read.mockReturnValueOnce(initial.promise).mockResolvedValue(snapshot());const play=vi.fn().mockResolvedValue(true);
 function Probe(){const watch=useEpisodeWatch(detail);return <MediaFileList watch={watch} files={[file]} onPlay={play} onReveal={vi.fn()}/>;}
 render(<I18nProvider><Probe/></I18nProvider>);fireEvent.doubleClick(document.querySelector('.media-file-row')!);
 await waitFor(()=>expect(calls.mark).toHaveBeenCalledOnce());await act(async()=>initial.resolve(snapshot()));expect(screen.getByRole('checkbox').getAttribute('aria-checked')).toBe('true');expect(play).toHaveBeenCalledOnce();
});

it('commits only successful writes, prevents rapid repeats and keeps play separate',async()=>{
 const pending=deferred<EpisodeWatchSnapshot>();calls.mark.mockReturnValue(pending.promise);
 const play=vi.fn();
 function Probe(){const watch=useEpisodeWatch(detail);return <><EpisodeWatchSummary watch={watch}/><MediaFileList watch={watch} files={[file]} onPlay={play} onReveal={vi.fn()}/></>;}
 render(<I18nProvider><Probe/></I18nProvider>);
 const checkbox=await screen.findByRole('checkbox',{name:'标记已看：Show S01E01.mkv'});
 fireEvent.click(checkbox);fireEvent.click(checkbox);fireEvent.doubleClick(checkbox);
 expect(play).not.toHaveBeenCalled();expect(calls.mark).toHaveBeenCalledTimes(1);
 expect(calls.mark).toHaveBeenCalledWith(2,false,1,true,'a'.repeat(64));
 expect(checkbox.getAttribute('aria-checked')).toBe('false');expect((checkbox as HTMLButtonElement).disabled).toBe(true);
 await act(async()=>pending.resolve(snapshot(true)));
 expect(screen.getByRole('checkbox').getAttribute('aria-checked')).toBe('true');expect(screen.getByRole('status').textContent).toContain('已经看到：Show S01E01.mkv');
 fireEvent.click(screen.getByRole('button',{name:'点击播放 Show S01E01.mkv'}));
 expect(play).toHaveBeenCalledWith(file);expect(calls.mark).toHaveBeenCalledTimes(1);
});

it('retains confirmed state on a stale or failed write and offers a fresh snapshot retry',async()=>{
 calls.mark.mockRejectedValue(new Error('EPISODE_WATCH_STALE'));
 function Probe(){const watch=useEpisodeWatch(detail);return <><EpisodeWatchSummary watch={watch}/><MediaFileList watch={watch} files={[file]} onPlay={vi.fn()} onReveal={vi.fn()}/></>;}
 render(<I18nProvider><Probe/></I18nProvider>);
 fireEvent.click(await screen.findByRole('checkbox'));
 expect(await screen.findByRole('alert')).toBeTruthy();expect(screen.getByRole('checkbox').getAttribute('aria-checked')).toBe('false');
 fireEvent.click(screen.getByRole('button',{name:'重试'}));await waitFor(()=>expect(calls.read).toHaveBeenCalledTimes(2));
 await waitFor(()=>expect(screen.queryByRole('alert')).toBeNull());
});

it('discards an older detail load and an unmounted completion',async()=>{
 const old=deferred<EpisodeWatchSnapshot>(),next=deferred<EpisodeWatchSnapshot>();calls.read.mockReturnValueOnce(old.promise).mockReturnValueOnce(next.promise);
 const view=renderHook(({value})=>useEpisodeWatch(value),{initialProps:{value:detail}});
 const other={...detail,node:{...node,id:5}};view.rerender({value:other});
 await act(async()=>next.resolve({...snapshot(true),nodeId:5}));expect(view.result.current.data?.nodeId).toBe(5);
 await act(async()=>old.resolve(snapshot()));expect(view.result.current.data?.nodeId).toBe(5);
 view.unmount();expect(calls.mark).not.toHaveBeenCalled();
});

it('shows merged files as partly watched and file fallback as an explicit file count',async()=>{
 const partial={...snapshot(),totalEpisodes:2,totalFiles:1,watchedFiles:1,unresolvedRecords:1,rows:[{fileId:1,kind:'EPISODE' as const,season:1,episodes:[1,2],watched:false,partiallyWatched:true}]};calls.read.mockResolvedValue(partial);
 function Probe(){const watch=useEpisodeWatch(detail);return <><EpisodeWatchSummary watch={watch}/><MediaFileList watch={watch} files={[file]} onPlay={vi.fn()} onReveal={vi.fn()}/></>;}
 render(<I18nProvider><Probe/></I18nProvider>);
 const control=await screen.findByRole('checkbox');expect(control.getAttribute('aria-checked')).toBe('mixed');
 expect(screen.getByRole('status').textContent).toContain('还未开始观看');expect(screen.getByText('1 条旧记录待确认')).toBeTruthy();
 fireEvent.click(control);await waitFor(()=>expect(calls.mark).toHaveBeenCalledWith(2,false,1,true,'a'.repeat(64)));
});

it('clears in one snapshot mutation and preserves mounted rows while saving and rerendering detail activity',async()=>{
 calls.read.mockResolvedValue(snapshot(true));const pending=deferred<EpisodeWatchSnapshot>();calls.clear.mockReturnValue(pending.promise);
 function Probe({value}:{value:NodeDetail}){const watch=useEpisodeWatch(value);return <><EpisodeWatchSummary watch={watch}/><MediaFileList watch={watch} files={[file]} onPlay={vi.fn()} onReveal={vi.fn()}/></>;}
 const view=render(<I18nProvider><Probe value={detail}/></I18nProvider>);const row=await screen.findByRole('checkbox');
 fireEvent.click(screen.getByRole('button',{name:'清空播放进度'}));fireEvent.click(screen.getByRole('button',{name:'清空播放进度'}));
 expect(calls.clear).toHaveBeenCalledExactlyOnceWith(2,false,'b'.repeat(64));expect(row.getAttribute('aria-checked')).toBe('true');
 view.rerender(<I18nProvider><Probe value={{...detail,node:{...node,lastWatchedAt:'2026-10-25T10:00:00Z'}}}/></I18nProvider>);
 expect(screen.getByRole('checkbox')).toBe(row);expect(calls.read).toHaveBeenCalledOnce();
 await act(async()=>pending.resolve(snapshot()));expect(screen.getByRole('checkbox')).toBe(row);expect(row.getAttribute('aria-checked')).toBe('false');expect(screen.getByRole('button',{name:'清空播放进度'})).toHaveProperty('disabled',true);
});

it('resumes the latest checked file, then falls back to first after clearing without marking on play',async()=>{
 const other={...file,id:2,fileName:'Show S01E02.mkv'};calls.read.mockResolvedValue({...snapshot(true),latestWatchedFileId:2,latestWatchedFileName:other.fileName});calls.clear.mockResolvedValue(snapshot());
 const view=renderHook(()=>useEpisodeWatch(detail));await waitFor(()=>expect(view.result.current.data).not.toBeNull());const play=vi.fn().mockResolvedValue(true);
 await act(async()=>view.result.current.playPreferred([file,other],file,play));expect(play).toHaveBeenLastCalledWith(other);expect(calls.mark).not.toHaveBeenCalled();
 await act(async()=>view.result.current.clear());await act(async()=>view.result.current.playPreferred([file,other],file,play));expect(play).toHaveBeenLastCalledWith(file);expect(calls.read).toHaveBeenCalledOnce();
});

it('updates a player file event locally and ignores unrelated player files',async()=>{
 function Probe(){const watch=useEpisodeWatch({...detail,mediaFiles:[file]});return <><EpisodeWatchSummary watch={watch}/><MediaFileList watch={watch} files={[file]} onPlay={vi.fn()} onReveal={vi.fn()}/></>;}
 render(<I18nProvider><Probe/></I18nProvider>);const row=await screen.findByRole('checkbox');
 act(()=>window.dispatchEvent(new CustomEvent('m2shelf-playback-changed',{detail:{state:'FILE',fileId:999,ownerNodeId:9}})));expect(calls.read).toHaveBeenCalledOnce();
 calls.read.mockResolvedValue(snapshot(true));act(()=>window.dispatchEvent(new CustomEvent('m2shelf-playback-changed',{detail:{state:'FILE',fileId:1,ownerNodeId:2}})));
 await waitFor(()=>expect(row.getAttribute('aria-checked')).toBe('true'));expect(screen.getByRole('checkbox')).toBe(row);expect(screen.getByRole('status').textContent).toContain(file.fileName);
});

it('accepts an mpv-confirmed mark that won the race with a double-click write',async()=>{
 const play=vi.fn().mockResolvedValue(true);calls.mark.mockRejectedValue(new Error('EPISODE_WATCH_STALE'));
 function Probe(){const watch=useEpisodeWatch(detail);return <><EpisodeWatchSummary watch={watch}/><MediaFileList watch={watch} files={[file]} onPlay={play} onReveal={vi.fn()}/></>;}
 render(<I18nProvider><Probe/></I18nProvider>);await screen.findByRole('checkbox');calls.read.mockResolvedValue(snapshot(true));fireEvent.doubleClick(document.querySelector('.media-file-row')!);
 await waitFor(()=>expect(screen.getByRole('checkbox').getAttribute('aria-checked')).toBe('true'));expect(screen.queryByRole('alert')).toBeNull();expect(play).toHaveBeenCalledOnce();
});

it('keeps checked UI on a failed clear and never applies a late clear to another detail',async()=>{
 calls.read.mockResolvedValue(snapshot(true));calls.clear.mockRejectedValueOnce(new Error('STALE'));
 const view=renderHook(({value})=>useEpisodeWatch(value),{initialProps:{value:detail}});await waitFor(()=>expect(view.result.current.data).not.toBeNull());
 await act(async()=>view.result.current.clear());expect(view.result.current.error).toBe(true);expect(view.result.current.data?.rows[0].watched).toBe(true);
 const pending=deferred<EpisodeWatchSnapshot>();calls.clear.mockReturnValue(pending.promise);let clearing!:Promise<void>;
 act(()=>{clearing=view.result.current.clear();});calls.read.mockResolvedValue({...snapshot(),nodeId:5});view.rerender({value:{...detail,node:{...node,id:5}}});
 await act(async()=>pending.resolve(snapshot()));await act(async()=>clearing);await waitFor(()=>expect(view.result.current.data?.nodeId).toBe(5));
});

it('refreshes metadata changes after the active write instead of painting an older read',async()=>{
 const pending=deferred<EpisodeWatchSnapshot>();calls.mark.mockReturnValue(pending.promise);
 const view=renderHook(()=>useEpisodeWatch(detail));
 await waitFor(()=>expect(view.result.current.data).not.toBeNull());
 act(()=>{void view.result.current.mark(1,true);});
 act(()=>window.dispatchEvent(new Event('m2shelf-metadata-changed')));
 expect(calls.read).toHaveBeenCalledTimes(1);
 calls.read.mockResolvedValue(snapshot(true));
 await act(async()=>pending.resolve(snapshot(true)));
 await waitFor(()=>expect(calls.read).toHaveBeenCalledTimes(2));
 expect(view.result.current.data?.rows[0].watched).toBe(true);
});

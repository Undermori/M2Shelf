// @vitest-environment jsdom
import {act,cleanup,fireEvent,render,screen,waitFor,within} from '@testing-library/react';
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {ScreenshotRaster,ScreenshotStudio} from '../components/ScreenshotStudio';
import {workbenchThumbCache} from './workbenchThumbCache';
vi.mock('./toast',()=>({notifyToast:vi.fn()}));
import {I18nProvider} from './i18n';
import {round8GalleryCopy} from './round8GalleryCopy';
import type {ScreenshotImage} from '../types/workbench';

const mocks=vi.hoisted(()=>({images:vi.fn(),batch:vi.fn(),read:vi.fn(),update:vi.fn(),locate:vi.fn()}));
vi.mock('./api',()=>({chooseScreenshotDirectories:vi.fn(),api:{screenshots:mocks.images,batchUpdateScreenshots:mocks.batch,readScreenshot:mocks.read,updateScreenshot:mocks.update,locateScreenshot:mocks.locate,screenshotFolders:async()=>[{id:1,alias:'Frames',path:'X:/Synthetic/Frames',note:'',target:null,imageCount:51}],listWorkNotes:async()=>({items:[],total:0})}}));
const image=(id:number):ScreenshotImage=>({id,folderId:1,fileName:`Frame ${String(id).padStart(3,'0')}.png`,relativePath:`${id}.png`,revision:`r${id}`,mime:'image/png',width:1920,height:1080,note:'',tags:['作画'],starred:false,target:null,ownTarget:null});
const onError=vi.fn();
const mount=(props={})=>render(<I18nProvider><ScreenshotStudio busy={false} onError={onError} onOpenTarget={vi.fn()} {...props}/></I18nProvider>);
beforeEach(()=>{workbenchThumbCache.clear();vi.resetAllMocks();mocks.images.mockImplementation(async q=>({items:Array.from({length:Math.min(48,51-q.offset)},(_,i)=>image(i+1+q.offset)),total:51}));mocks.batch.mockImplementation(async images=>images.map((i:ScreenshotImage)=>({...i,starred:true})));mocks.read.mockResolvedValue(new ArrayBuffer(0));mocks.update.mockImplementation(async i=>i);vi.stubGlobal('IntersectionObserver',undefined);URL.createObjectURL=vi.fn(()=>'blob:synthetic');URL.revokeObjectURL=vi.fn();});
afterEach(()=>{cleanup();workbenchThumbCache.clear();vi.unstubAllGlobals();});
describe('Round 8 gallery productivity',()=>{
 it('reuses thumbnail URLs after visibility and remount, and invalidates by revision',async()=>{
  let visibility!:(entries:Array<{isIntersecting:boolean}>)=>void;
  vi.stubGlobal('IntersectionObserver',class {constructor(callback:typeof visibility){visibility=callback;}observe(){}disconnect(){}});
  let serial=0;URL.createObjectURL=vi.fn(()=>`blob:revision-${++serial}`);
  const view=render(<I18nProvider><ScreenshotRaster image={image(1)}/></I18nProvider>);
  await act(async()=>visibility([{isIntersecting:true}]));const first=await screen.findByRole('img'),url=first.getAttribute('src');
  await act(async()=>visibility([{isIntersecting:false}]));expect(screen.queryByRole('img')).toBeNull();expect(URL.revokeObjectURL).not.toHaveBeenCalled();
  await act(async()=>visibility([{isIntersecting:true}]));expect(screen.getByRole('img').getAttribute('src')).toBe(url);expect(mocks.read).toHaveBeenCalledTimes(1);
  view.rerender(<I18nProvider><ScreenshotRaster image={{...image(1),revision:'new-revision'}}/></I18nProvider>);await waitFor(()=>expect(mocks.read).toHaveBeenCalledTimes(2));await screen.findByRole('img');view.unmount();workbenchThumbCache.clear();await waitFor(()=>expect(URL.revokeObjectURL).toHaveBeenCalledTimes(2));
 });

 it('has complete independent localized copy',()=>{for(const copy of Object.values(round8GalleryCopy)){expect(Object.keys(copy).sort()).toEqual(Object.keys(round8GalleryCopy['zh-CN']).sort());expect(Object.values(copy).every(value=>value.trim())).toBe(true);}});
 it('never reattaches an evicted Blob URL when the same offscreen component becomes visible',async()=>{
  let visibility!:(entries:Array<{isIntersecting:boolean}>)=>void;vi.stubGlobal('IntersectionObserver',class {constructor(callback:typeof visibility){visibility=callback;}observe(){}disconnect(){}});
  URL.createObjectURL=vi.fn().mockReturnValueOnce('blob:evicted').mockReturnValue('blob:fresh');
  render(<I18nProvider><ScreenshotRaster image={image(1)}/></I18nProvider>);await act(async()=>visibility([{isIntersecting:true}]));expect((await screen.findByRole('img')).getAttribute('src')).toBe('blob:evicted');
  await act(async()=>visibility([{isIntersecting:false}]));workbenchThumbCache.clear();await waitFor(()=>expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:evicted'));
  let finish!:(bytes:ArrayBuffer)=>void;mocks.read.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));await act(async()=>visibility([{isIntersecting:true}]));expect(screen.queryByRole('img')).toBeNull();await act(async()=>finish(new ArrayBuffer(0)));expect(screen.getByRole('img').getAttribute('src')).toBe('blob:fresh');
 });
 it('selects a range, all on page and submits snapshots as one batch',async()=>{mount();await screen.findByRole('button',{name:'Frame 001.png'});fireEvent.click(screen.getByRole('button',{name:'多选整理'}));fireEvent.click(screen.getByRole('button',{name:'Frame 002.png'}));fireEvent.click(screen.getByRole('button',{name:'Frame 005.png'}),{shiftKey:true});expect(document.querySelectorAll('.studio-image-open[aria-pressed=true]')).toHaveLength(4);fireEvent.click(screen.getByRole('button',{name:'★ 星标'}));await waitFor(()=>expect(mocks.batch).toHaveBeenCalledWith([image(2),image(3),image(4),image(5)],{kind:'star',starred:true}));await waitFor(()=>{expect(mocks.images.mock.calls.length).toBeGreaterThan(1);expect(screen.getByRole('button',{name:'全选本页'})).toHaveProperty('disabled',false);});fireEvent.click(screen.getByRole('button',{name:'全选本页'}));expect(document.querySelectorAll('.studio-image-open[aria-pressed=true]')).toHaveLength(48);fireEvent.click(screen.getByRole('button',{name:'取消选择'}));expect(document.querySelectorAll('.studio-image-open[aria-pressed=true]')).toHaveLength(0);});
 it('uses an exact tag query and clears the filter on a second click',async()=>{mount();await screen.findByRole('button',{name:'Frame 001.png'});fireEvent.click(screen.getAllByRole('button',{name:'作画'})[0]);await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({tag:'作画',offset:0})));fireEvent.click(screen.getByRole('button',{name:'作画 ×'}));await waitFor(()=>expect(mocks.images.mock.lastCall?.[0].tag).toBeUndefined());});
 it('keeps the lightbox mounted at page boundaries and restores the preceding page',async()=>{mount();fireEvent.click(await screen.findByRole('button',{name:'Frame 048.png'}));const dialog=screen.getByRole('dialog');fireEvent.keyDown(window,{key:'ArrowRight'});await within(dialog).findByRole('heading',{name:'Frame 049.png'});expect(screen.getByRole('dialog')).toBe(dialog);fireEvent.keyDown(window,{key:'ArrowLeft'});await within(dialog).findByRole('heading',{name:'Frame 048.png'});expect(screen.getByRole('dialog')).toBe(dialog);fireEvent.click(within(dialog).getByRole('button',{name:'100%'}));expect(dialog.querySelector('.studio-lightbox-stage')?.classList.contains('is-actual')).toBe(true);fireEvent.click(within(dialog).getByRole('button',{name:'适应窗口'}));expect(dialog.querySelector('.studio-lightbox-stage')?.classList.contains('is-actual')).toBe(false);});
 it('flushes the current draft before cross-page navigation without a confirmation',async()=>{mount();fireEvent.click(await screen.findByRole('button',{name:'Frame 048.png'}));const dialog=screen.getByRole('dialog');fireEvent.change(within(dialog).getByRole('textbox',{name:'截图笔记'}),{target:{value:'Keep this draft'}});fireEvent.keyDown(window,{key:'ArrowRight'});expect(within(dialog).queryByRole('alert')).toBeNull();await within(dialog).findByRole('heading',{name:'Frame 049.png'});expect(mocks.update).toHaveBeenCalledWith(expect.objectContaining({id:48,note:'Keep this draft'}));});

 it('opens a screenshot search hit on its exact page',async()=>{mocks.locate.mockResolvedValue({image:image(50),offset:48});mount({request:{target:null,screenshotId:50,sequence:1}});await screen.findByRole('heading',{name:'Frame 050.png'});expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({folderId:1,offset:48}));});
 it('refreshes filters after closing a saved lightbox without discarding its current image early',async()=>{mount();fireEvent.click(await screen.findByRole('button',{name:'Frame 001.png'}));const dialog=screen.getByRole('dialog');fireEvent.change(within(dialog).getByRole('textbox',{name:'标签（用逗号分隔）'}),{target:{value:'changed'}});fireEvent.blur(within(dialog).getByRole('textbox',{name:'标签（用逗号分隔）'}));await waitFor(()=>expect(mocks.update).toHaveBeenCalled());expect(screen.getByRole('dialog')).toBe(dialog);await waitFor(()=>expect(within(dialog).getByRole('button',{name:'关闭'})).toHaveProperty('disabled',false));const reads=mocks.images.mock.calls.length;fireEvent.click(within(dialog).getByRole('button',{name:'关闭'}));await waitFor(()=>expect(mocks.images.mock.calls.length).toBeGreaterThan(reads));});
 it('leaves a current image and its draft intact when adjacent-page loading fails',async()=>{mount();fireEvent.click(await screen.findByRole('button',{name:'Frame 048.png'}));mocks.images.mockRejectedValueOnce(new Error('synthetic offline'));await act(async()=>fireEvent.keyDown(window,{key:'ArrowRight'}));expect(screen.getByRole('heading',{name:'Frame 048.png'})).toBeTruthy();expect(onError).toHaveBeenCalled();expect(screen.getByRole('dialog').getAttribute('aria-busy')).toBe('false');});
});


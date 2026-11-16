// @vitest-environment jsdom
import {act,cleanup,fireEvent,render,screen,waitFor,within} from '@testing-library/react';
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {ScreenshotStudio} from '../components/ScreenshotStudio';
import {I18nProvider} from './i18n';
import {workbenchThumbCache} from './workbenchThumbCache';
import {notifyToast} from './toast';
import type {ScreenshotImage,ScreenshotSession} from '../types/workbench';

const mocks=vi.hoisted(()=>({images:vi.fn(),update:vi.fn(),read:vi.fn(),refresh:vi.fn(),error:vi.fn()}));
vi.mock('./toast',()=>({notifyToast:vi.fn()}));
vi.mock('./api',()=>({chooseScreenshotDirectories:async()=>[],api:{
 screenshots:mocks.images,updateScreenshot:mocks.update,readScreenshot:mocks.read,refreshScreenshotFolder:mocks.refresh,
 screenshotFolders:async()=>[{id:1,alias:'Synthetic frames',path:'X:/Synthetic/Frames',note:'',target:null,imageCount:2}],
 listWorkNotes:async()=>({items:[{target:{kind:'NODE',id:42},nodeId:42,title:'Synthetic work',libraryName:'Synthetic library'}],total:1}),
 nodeDetail:async()=>({node:{id:42,displayName:'Synthetic work'}}),
}}));
const image=(id:number):ScreenshotImage=>({id,folderId:1,fileName:`Round11 ${id}.png`,relativePath:`${id}.png`,revision:`r${id}`,mime:'image/png',width:1920,height:1080,note:'',tags:[],starred:false,target:null,ownTarget:null});
const mount=(active=true)=>render(<I18nProvider><ScreenshotStudio busy={false} active={active} onError={mocks.error} onOpenTarget={vi.fn()}/></I18nProvider>);
const open=async()=>{fireEvent.click(await screen.findByRole('button',{name:'Round11 1.png'}));return screen.getByRole('dialog');};
beforeEach(()=>{workbenchThumbCache.clear();vi.resetAllMocks();Object.defineProperty(document,'visibilityState',{configurable:true,value:'visible'});mocks.images.mockResolvedValue({items:[image(1),image(2)],total:2});mocks.update.mockImplementation(async value=>value);mocks.read.mockResolvedValue(new ArrayBuffer(0));mocks.refresh.mockResolvedValue({id:1,autoMatch:{matchedCount:0}});vi.stubGlobal('IntersectionObserver',undefined);URL.createObjectURL=vi.fn(()=>'blob:round11');URL.revokeObjectURL=vi.fn();});
afterEach(()=>{cleanup();workbenchThumbCache.clear();vi.useRealTimers();vi.unstubAllGlobals();});

describe('Round 11 gallery autosave and ordering',()=>{
 it('restores complete navigation snapshots including page offset without publishing stale local state',async()=>{
  mocks.images.mockResolvedValue({items:[image(1),image(2)],total:144});
  const session:ScreenshotSession={folderId:'1',query:'older',target:null,stars:false,density:'compact',sort:'NAME',descending:false,tag:null,offset:48},changed=vi.fn();
  const ui=(value:ScreenshotSession)=><I18nProvider><ScreenshotStudio busy={false} session={value} onSessionChange={changed} onError={mocks.error} onOpenTarget={vi.fn()}/></I18nProvider>;
  const view=render(ui(session));await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({sort:'NAME',descending:false,offset:48,query:'older'})));
  const next={...session,query:'restored',sort:'SIZE' as const,descending:true,offset:96};view.rerender(ui(next));await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({sort:'SIZE',descending:true,offset:96,query:'restored'})));expect(changed).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button',{name:'降序'}));await waitFor(()=>expect(changed).toHaveBeenLastCalledWith(expect.objectContaining({sort:'SIZE',descending:false,offset:0,query:'restored',density:'compact',folderId:'1'})));
 });
 it('defaults to latest modification first, sends selected sort/direction and preserves filter state on tab return',async()=>{
  const view=mount();await screen.findByRole('button',{name:'Round11 1.png'});expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({sort:'MODIFIED',descending:true}));
  fireEvent.click(screen.getByRole('combobox',{name:'排序方式'}));fireEvent.click(screen.getByRole('option',{name:'大小'}));await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({sort:'SIZE',descending:true,offset:0})));
  fireEvent.click(screen.getByRole('button',{name:'降序'}));await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({sort:'SIZE',descending:false})));
  fireEvent.change(screen.getByRole('searchbox',{name:'搜索文件名、笔记或标签'}),{target:{value:'query'}});await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({query:'query'})));
  view.rerender(<I18nProvider><ScreenshotStudio busy={false} active={false} onError={mocks.error} onOpenTarget={vi.fn()}/></I18nProvider>);view.rerender(<I18nProvider><ScreenshotStudio busy={false} active onError={mocks.error} onOpenTarget={vi.fn()}/></I18nProvider>);
  expect(screen.getByRole('combobox',{name:'排序方式'}).textContent).toContain('大小');expect(screen.getByRole('searchbox',{name:'搜索文件名、笔记或标签'})).toHaveProperty('value','query');expect(screen.getByRole('button',{name:'升序'})).toBeTruthy();
 });
 it('serializes rapid star and text edits without overwriting newer draft values or using an older revision',async()=>{
  const finishes:Array<(value:ScreenshotImage)=>void>=[];mocks.update.mockImplementation(()=>new Promise(resolve=>finishes.push(resolve)));
  mount();const dialog=await open(),note=within(dialog).getByRole('textbox',{name:'截图笔记'}),tags=within(dialog).getByRole('textbox',{name:'标签（用逗号分隔）'});
  fireEvent.click(within(dialog).getByRole('button',{name:'星标'}));expect(mocks.update).toHaveBeenCalledTimes(1);
  fireEvent.change(note,{target:{value:'Newest draft'}});fireEvent.change(tags,{target:{value:'one, two'}});fireEvent.click(within(dialog).getByRole('button',{name:'取消星标'}));expect(mocks.update).toHaveBeenCalledTimes(1);
  await act(async()=>finishes[0]({...image(1),starred:true,revision:'r1-confirmed'}));expect(mocks.update).toHaveBeenCalledTimes(2);expect(mocks.update).toHaveBeenLastCalledWith(expect.objectContaining({revision:'r1-confirmed',note:'Newest draft',tags:['one','two'],starred:false}));expect(note).toHaveProperty('value','Newest draft');
  await act(async()=>finishes[1]({...mocks.update.mock.lastCall![0],revision:'r1-latest'}));expect(note).toHaveProperty('value','Newest draft');expect(tags).toHaveProperty('value','one, two');expect(within(dialog).queryByRole('button',{name:'保存'})).toBeNull();
 });
 it('saves association immediately together with the newest note draft',async()=>{
  mount();const dialog=await open();fireEvent.change(within(dialog).getByRole('textbox',{name:'截图笔记'}),{target:{value:'Merged association note'}});fireEvent.click(within(dialog).getByRole('button',{name:'关联作品'}));fireEvent.click(await screen.findByRole('button',{name:'Synthetic work Synthetic library'}));
  await waitFor(()=>expect(mocks.update).toHaveBeenCalledWith(expect.objectContaining({note:'Merged association note',ownTarget:{kind:'NODE',id:42}})));
 });
 it('updates the retained gallery row when an in-flight save completes after its lightbox is hidden',async()=>{
  let finish!:(value:ScreenshotImage)=>void;mocks.update.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
  const ui=(active:boolean)=><I18nProvider><ScreenshotStudio busy={false} active={active} onError={mocks.error} onOpenTarget={vi.fn()}/></I18nProvider>;
  const view=render(ui(true));const dialog=await open(),note=within(dialog).getByRole('textbox',{name:'截图笔记'});fireEvent.change(note,{target:{value:'Saved while away'}});fireEvent.blur(note);expect(mocks.update).toHaveBeenCalledOnce();view.rerender(ui(false));
  await act(async()=>finish({...image(1),note:'Saved while away',revision:'r1-confirmed'}));view.rerender(ui(true));expect(screen.getByRole('textbox',{name:'截图笔记'})).toHaveProperty('value','Saved while away');
  fireEvent.change(screen.getByRole('textbox',{name:'截图笔记'}),{target:{value:'Next edit'}});fireEvent.blur(screen.getByRole('textbox',{name:'截图笔记'}));await waitFor(()=>expect(mocks.update).toHaveBeenLastCalledWith(expect.objectContaining({note:'Next edit',revision:'r1-confirmed'})));
 });
 it('debounces text for 450ms, ignores Ctrl/Cmd+S and flushes blur/next/close',async()=>{
  vi.useFakeTimers();mount();await act(async()=>{await vi.advanceTimersByTimeAsync(10);});fireEvent.click(screen.getByRole('button',{name:'Round11 1.png'}));
  let note=screen.getByRole('textbox',{name:'截图笔记'});fireEvent.change(note,{target:{value:'Debounced'}});fireEvent.keyDown(note,{key:'s',ctrlKey:true});fireEvent.keyDown(note,{key:'s',metaKey:true});expect(mocks.update).not.toHaveBeenCalled();
  await act(async()=>{await vi.advanceTimersByTimeAsync(449);});expect(mocks.update).not.toHaveBeenCalled();await act(async()=>{await vi.advanceTimersByTimeAsync(1);});expect(mocks.update).toHaveBeenCalledTimes(1);expect(screen.queryByText(/Ctrl\+S/)).toBeNull();
  fireEvent.change(note,{target:{value:'Blur flush'}});await act(async()=>fireEvent.blur(note));expect(mocks.update).toHaveBeenLastCalledWith(expect.objectContaining({note:'Blur flush'}));
  fireEvent.change(note,{target:{value:'Next flush'}});await act(async()=>fireEvent.click(screen.getByRole('button',{name:'下一张'})));expect(screen.getByRole('heading',{name:'Round11 2.png'})).toBeTruthy();expect(mocks.update).toHaveBeenLastCalledWith(expect.objectContaining({id:1,note:'Next flush'}));
  note=screen.getByRole('textbox',{name:'截图笔记'});fireEvent.change(note,{target:{value:'Close flush'}});await act(async()=>fireEvent.click(screen.getByRole('button',{name:'关闭'})));expect(screen.queryByRole('dialog')).toBeNull();expect(mocks.update).toHaveBeenLastCalledWith(expect.objectContaining({id:2,note:'Close flush'}));
 });
 it('keeps a failed draft visible, blocks navigation and succeeds only after explicit retry',async()=>{
  mocks.update.mockRejectedValue(new Error('SCREENSHOT_CHANGED'));mount();const dialog=await open();const note=within(dialog).getByRole('textbox',{name:'截图笔记'});fireEvent.change(note,{target:{value:'Retain failed draft'}});fireEvent.click(within(dialog).getByRole('button',{name:'下一张'}));await screen.findByRole('alert');expect(within(dialog).getByRole('heading',{name:'Round11 1.png'})).toBeTruthy();expect(note).toHaveProperty('value','Retain failed draft');expect(mocks.error).toHaveBeenCalled();
  mocks.update.mockImplementation(async value=>value);fireEvent.click(within(dialog).getByRole('button',{name:'重试'}));await waitFor(()=>expect(within(dialog).queryByRole('alert')).toBeNull());fireEvent.click(within(dialog).getByRole('button',{name:'下一张'}));expect(within(dialog).getByRole('heading',{name:'Round11 2.png'})).toBeTruthy();
 });
 it('rejects invalid tags without partial writes and restores safe close after correction',async()=>{
  mount();const dialog=await open(),tags=within(dialog).getByRole('textbox',{name:'标签（用逗号分隔）'});fireEvent.change(tags,{target:{value:Array.from({length:21},(_,index)=>`tag${index}`).join(',')}});fireEvent.click(within(dialog).getByRole('button',{name:'星标'}));fireEvent.click(within(dialog).getByRole('button',{name:'关闭'}));await act(async()=>{});expect(mocks.update).not.toHaveBeenCalled();expect(screen.getByRole('dialog')).toBe(dialog);expect(tags.getAttribute('aria-invalid')).toBe('true');
  fireEvent.change(tags,{target:{value:'valid'}});fireEvent.click(within(dialog).getByRole('button',{name:'关闭'}));await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());expect(mocks.update).toHaveBeenCalledWith(expect.objectContaining({tags:['valid'],starred:true}));
 });
 it('keeps quiet zero-result polling silent while explicit refresh uses Toast',async()=>{
  vi.useFakeTimers();mount();await act(async()=>{await vi.advanceTimersByTimeAsync(10);});await act(async()=>{await vi.advanceTimersByTimeAsync(1000);});expect(mocks.refresh).toHaveBeenCalledOnce();expect(notifyToast).not.toHaveBeenCalled();expect(document.querySelector('.studio-auto-match-receipt')).toBeNull();
  await act(async()=>fireEvent.click(screen.getByRole('button',{name:'刷新'})));expect(notifyToast).toHaveBeenCalledWith('info',expect.stringContaining('没有新的明确匹配'));
 });
 it('syncs persisted sort preference, invokes onSortChange and avoids disabling UI controls when starring or closing lightbox',async()=>{
  const onSortChange=vi.fn();
  const view=render(<I18nProvider><ScreenshotStudio busy={false} active sortPreference={{sort:'NAME',descending:false}} onSortChange={onSortChange} onError={mocks.error} onOpenTarget={vi.fn()}/></I18nProvider>);
  await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({sort:'NAME',descending:false})));
  view.rerender(<I18nProvider><ScreenshotStudio busy={false} active sortPreference={{sort:'SIZE',descending:true}} onSortChange={onSortChange} onError={mocks.error} onOpenTarget={vi.fn()}/></I18nProvider>);
  await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({sort:'SIZE',descending:true})));
  fireEvent.click(screen.getByRole('combobox',{name:'排序方式'}));fireEvent.click(screen.getByRole('option',{name:'名称'}));
  expect(onSortChange).toHaveBeenLastCalledWith('NAME',true);
  await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({sort:'NAME',descending:true})));
  let finishStar!:(value:ScreenshotImage)=>void;mocks.update.mockImplementationOnce(()=>new Promise(resolve=>{finishStar=resolve;}));
  const starButton=await screen.findByRole('button',{name:'星标 · Round11 1.png'});
  expect(starButton.querySelector('svg')).not.toBeNull();
  expect(screen.getByRole('button',{name:'仅看星标'}).querySelector('svg')).not.toBeNull();
  fireEvent.click(starButton);
  expect(screen.getByRole('button',{name:'导入截图文件夹'})).toHaveProperty('disabled',false);
  expect(screen.getByRole('combobox',{name:'排序方式'})).toHaveProperty('disabled',false);
  await act(async()=>finishStar({...image(1),starred:true,revision:'r1-starred'}));
  fireEvent.click(screen.getByRole('button',{name:'Round11 2.png'}));
  const dialog=screen.getByRole('dialog');
  let holdRefresh!: (value:{items:ScreenshotImage[];total:number})=>void;
  mocks.images.mockImplementationOnce(()=>new Promise(resolve=>{holdRefresh=resolve;}));
  fireEvent.click(within(dialog).getByRole('button',{name:'关闭'}));
  await waitFor(()=>expect(holdRefresh).toBeTypeOf('function'));
  expect(screen.getByRole('button',{name:'Round11 1.png'})).toHaveProperty('disabled',false);
  expect(screen.getByRole('button',{name:'多选整理'})).toHaveProperty('disabled',false);
  await act(async()=>holdRefresh({items:[image(1),image(2)],total:2}));
 });
});

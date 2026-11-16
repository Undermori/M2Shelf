// @vitest-environment jsdom
import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {ScreenshotStudio} from '../components/ScreenshotStudio';
import {notifyToast} from './toast';
import {workbenchThumbCache} from './workbenchThumbCache';
vi.mock('./toast',()=>({notifyToast:vi.fn()}));
import {I18nProvider} from './i18n';
const mocks=vi.hoisted(()=>({images:vi.fn(),refresh:vi.fn(),import:vi.fn(),choose:vi.fn(),folders:vi.fn()}));
vi.mock('./api',()=>({chooseScreenshotDirectories:mocks.choose,api:{screenshots:mocks.images,refreshScreenshotFolder:mocks.refresh,importScreenshotFolder:mocks.import,screenshotFolders:mocks.folders,readScreenshot:async()=>new ArrayBuffer(0),listWorkNotes:async()=>({items:[],total:0})}}));
const image={id:1,folderId:1,fileName:'mpv-shot0001 (2).jpg',relativePath:'mpv-shot0001 (2).jpg',revision:'r1',mime:'image/jpeg',width:1920,height:1080,note:'',tags:[],starred:false,target:null};
const result={matchedCount:1,matchedByPlayback:1,matchedByTitle:0,matchedIds:[1]};
const folder={id:1,alias:'Frames',path:'X:/Synthetic/Frames',note:'',target:null,imageCount:1,autoMatch:result};
const ui=(active=true,busy=false)=><I18nProvider><ScreenshotStudio busy={busy} onError={vi.fn()} onOpenTarget={vi.fn()} active={active}/></I18nProvider>;
beforeEach(()=>{workbenchThumbCache.clear();vi.resetAllMocks();Object.defineProperty(document,'visibilityState',{configurable:true,value:'visible'});mocks.images.mockResolvedValue({items:[image],total:1});mocks.folders.mockResolvedValue([folder]);mocks.refresh.mockResolvedValue(folder);mocks.import.mockResolvedValue(folder);mocks.choose.mockResolvedValue(['X:/Synthetic/Frames']);vi.stubGlobal('IntersectionObserver',undefined);URL.createObjectURL=vi.fn(()=>'blob:synthetic');URL.revokeObjectURL=vi.fn();});
afterEach(()=>{cleanup();workbenchThumbCache.clear();vi.useRealTimers();vi.unstubAllGlobals();});
describe('Round 10 quiet screenshot discovery',()=>{
 it('refreshes on activation and shows the newly linked title without an auto button',async()=>{
  mocks.refresh.mockImplementation(async()=>{mocks.images.mockResolvedValue({items:[{...image,target:{kind:'NODE',id:10},nodeId:10,targetTitle:'Linked Animation'}],total:1});return folder;});
  render(ui());await screen.findByRole('button',{name:image.fileName});expect(screen.queryByRole('button',{name:'自动匹配作品'})).toBeNull();
  await screen.findByText('Linked Animation');expect(mocks.refresh).toHaveBeenCalledExactlyOnceWith(1);expect(notifyToast).toHaveBeenCalledWith('info',expect.stringContaining('已自动关联 1 张截图'));
 });
 it('coalesces focus bursts, pauses inactive/busy galleries and resumes a pending refresh',async()=>{
  vi.useFakeTimers();const view=render(ui(false));await act(async()=>{await vi.advanceTimersByTimeAsync(10);});
  fireEvent.focus(window);await act(async()=>{await vi.advanceTimersByTimeAsync(3000);});expect(mocks.refresh).not.toHaveBeenCalled();
  view.rerender(ui(true,true));await act(async()=>{await vi.advanceTimersByTimeAsync(1000);});expect(mocks.refresh).not.toHaveBeenCalled();
  view.rerender(ui());await act(async()=>{await vi.advanceTimersByTimeAsync(1000);});expect(mocks.refresh).toHaveBeenCalledTimes(1);
  for(let i=0;i<5;i++)fireEvent.focus(window);await act(async()=>{await vi.advanceTimersByTimeAsync(500);});expect(mocks.refresh).toHaveBeenCalledTimes(1);
  await act(async()=>{await vi.advanceTimersByTimeAsync(2500);});expect(mocks.refresh).toHaveBeenCalledTimes(2);
 });
 it('does not replace a lightbox or selection while refresh is pending',async()=>{
  vi.useFakeTimers();render(ui());await act(async()=>{await vi.advanceTimersByTimeAsync(10);});
  fireEvent.click(screen.getByRole('button',{name:image.fileName}));await act(async()=>{await vi.advanceTimersByTimeAsync(1500);});expect(mocks.refresh).not.toHaveBeenCalled();
  fireEvent.keyDown(window,{key:'Escape'});await act(async()=>{await vi.advanceTimersByTimeAsync(20);});
  fireEvent.click(screen.getByRole('button',{name:'多选整理'}));fireEvent.click(screen.getByRole('button',{name:image.fileName}));await act(async()=>{await vi.advanceTimersByTimeAsync(1500);});expect(mocks.refresh).not.toHaveBeenCalled();expect(screen.getByRole('button',{name:image.fileName}).getAttribute('aria-pressed')).toBe('true');
 });
 it.each(['refresh','import'] as const)('retains explicit %s count feedback through the existing native path',async(action)=>{render(ui());await screen.findByRole('button',{name:image.fileName});fireEvent.click(screen.getByRole('button',{name:action==='refresh'?'刷新':'导入截图文件夹'}));await waitFor(()=>expect(notifyToast).toHaveBeenCalledWith('info',expect.stringContaining('已自动关联 1 张截图')));});
 it('reports zero and partial counts honestly for explicit refreshes',async()=>{
  mocks.refresh.mockResolvedValue({...folder,autoMatch:{...result,matchedCount:0,matchedIds:[]}});render(ui());await screen.findByRole('button',{name:image.fileName});fireEvent.click(screen.getByRole('button',{name:'刷新'}));await waitFor(()=>expect(notifyToast).toHaveBeenCalledWith('info',expect.stringContaining('没有新的明确匹配')));expect(screen.queryByRole('button',{name:'打开作品详情'})).toBeNull();
  mocks.refresh.mockResolvedValue({...folder,autoMatch:{...result,limited:true}});fireEvent.click(screen.getByRole('button',{name:'刷新'}));await waitFor(()=>expect(notifyToast).toHaveBeenCalledWith('info',expect.stringContaining('已达到本次处理上限')));
 });
});

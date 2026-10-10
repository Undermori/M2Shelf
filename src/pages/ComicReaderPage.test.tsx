// @vitest-environment jsdom
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {ComicReaderPage} from './ComicReaderPage';
import {LibraryRecognitionModeDialog} from '../components/LibraryRecognitionModeDialog';
import {ComicBookList} from '../components/ComicBookList';
import {I18nProvider} from '../lib/i18n';
import {mediaBadge} from '../lib/format';
import type {MediaNode} from '../types/media';
const mocks=vi.hoisted(()=>({reveal:vi.fn(),open:vi.fn(),read:vi.fn(),settings:vi.fn(),progress:vi.fn(),add:vi.fn(),remove:vi.fn()}));
vi.mock('../lib/api',()=>({api:{revealComicBook:mocks.reveal,openComicBook:mocks.open,readComicPage:mocks.read,getSettings:mocks.settings,updateComicProgress:mocks.progress,addComicBookmark:mocks.add,removeComicBookmark:mocks.remove}}));
const book={id:1,nodeId:2,revision:'revision',sourceKind:'ZIP_ARCHIVE' as const,displayName:'Comic fixture',pageCount:8,modifiedAt:'2026-01-01',indexError:null,progress:null};
beforeEach(()=>{vi.resetAllMocks();mocks.open.mockResolvedValue({book,pages:Array.from({length:8},(_,i)=>({pageIndex:i,pageName:`${i}.png`})),bookmarks:[]});mocks.read.mockResolvedValue(new ArrayBuffer(1));mocks.settings.mockResolvedValue({comicReader:{direction:'RTL',layout:'DOUBLE',mode:'PAGED',widePageAlone:true}});mocks.progress.mockResolvedValue(undefined);mocks.add.mockResolvedValue([0]);mocks.remove.mockResolvedValue([]);vi.stubGlobal('Image',class{src='';naturalWidth=600;naturalHeight=900;decode(){return Promise.resolve();}});vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}});Object.defineProperty(URL,'createObjectURL',{configurable:true,value:vi.fn(()=>`blob:fixture-${Math.random()}`)});Object.defineProperty(URL,'revokeObjectURL',{configurable:true,value:vi.fn()});Element.prototype.scrollBy=vi.fn();});
afterEach(()=>{cleanup();vi.unstubAllGlobals();});
const reader=(extra={})=>render(<I18nProvider><ComicReaderPage bookId={1} onBack={vi.fn()} onProgress={vi.fn()} {...extra}/></I18nProvider>);
describe('comic library setup and reader',()=>{
 it('reveals an indexed source without opening the book or triggering row double click',async()=>{
  const read=vi.fn();mocks.reveal.mockResolvedValue(undefined);render(<I18nProvider><ComicBookList books={[book]} onRead={read}/></I18nProvider>);
  const reveal=screen.getByRole('button',{name:'在资源管理器中显示'});fireEvent.click(reveal);fireEvent.doubleClick(reveal);
  await waitFor(()=>expect(mocks.reveal).toHaveBeenCalledWith(1));expect(read).not.toHaveBeenCalled();
 });
 it('offers separate animation, live action, ebook and doujin libraries without decorative headings',()=>{
  const choose=vi.fn();render(<I18nProvider><LibraryRecognitionModeDialog path="X:/Fixtures" busy={false} onChoose={choose} onClose={vi.fn()}/></I18nProvider>);
  expect(screen.queryByText('资源库识别')).toBeNull();
  fireEvent.click(screen.getByRole('button',{name:'真人电影/剧'}));fireEvent.click(screen.getByRole('button',{name:/按视频文件识别/}));fireEvent.click(screen.getByRole('button',{name:'创建资源库'}));expect(choose).toHaveBeenCalledWith('VIDEO_FILE','LIVE_ACTION',true);
  for(const [label,kind] of [['电子书','EBOOK'],['同人本','DOUJIN'],['设定集','ARTBOOK']]) {
   fireEvent.click(screen.getByRole('button',{name:label}));
   const policy=screen.getByRole('checkbox',{name:'不自动关联 Bangumi（推荐）'});
   expect((policy as HTMLInputElement).checked).toBe(true);
   fireEvent.click(screen.getByRole('button',{name:/按单个文件识别/}));fireEvent.click(screen.getByRole('button',{name:'创建资源库'}));expect(choose).toHaveBeenCalledWith('VIDEO_FILE',kind,false,'LEGACY');
   fireEvent.click(policy);fireEvent.click(screen.getByRole('button',{name:/按文件夹识别/}));fireEvent.click(screen.getByRole('button',{name:'创建资源库'}));expect(choose).toHaveBeenCalledWith('FOLDER',kind,true,'LEGACY');
   fireEvent.click(policy);
  }
 });
 it('opens readable rows by double click or keyboard, not unreadable rows',()=>{const read=vi.fn();render(<I18nProvider><ComicBookList books={[book,{...book,id:2,displayName:'Broken',pageCount:0,indexError:'COMIC_NO_PAGES'}]} onRead={read}/></I18nProvider>);const row=screen.getByText(book.displayName).closest('.comic-book-row')!;fireEvent.doubleClick(row);fireEvent.keyDown(row,{key:'Enter'});fireEvent.doubleClick(screen.getByText('Broken').closest('.comic-book-row')!);fireEvent.doubleClick(row.querySelector('button')!);expect(read).toHaveBeenCalledTimes(2);});
 it('defaults to LTR without changing saved RTL preferences',async()=>{mocks.settings.mockResolvedValueOnce({});reader();await screen.findByRole('img',{name:'第 1 / 8 页'});expect(screen.getAllByRole('img').map(img=>img.getAttribute('alt'))).toEqual(['第 1 / 8 页','第 2 / 8 页']);fireEvent.keyDown(window,{key:'ArrowRight'});await screen.findByRole('img',{name:'第 3 / 8 页'});});
 it('offers immutable folder or individual-file recognition for comics too',()=>{const choose=vi.fn();render(<I18nProvider><LibraryRecognitionModeDialog path="X:/Fixtures" busy={false} onChoose={choose} onClose={vi.fn()}/></I18nProvider>);expect(screen.getByRole('button',{name:/按视频文件识别/})).toBeTruthy();fireEvent.click(screen.getByRole('button',{name:'漫画'}));expect(screen.queryByRole('button',{name:/按视频文件识别/})).toBeNull();fireEvent.click(screen.getByRole('button',{name:/按文件夹识别/}));fireEvent.click(screen.getByRole('button',{name:'创建资源库'}));expect(choose).toHaveBeenCalledWith('FOLDER','COMIC',false,'LEGACY');fireEvent.click(screen.getByRole('button',{name:/按单个文件识别/}));fireEvent.click(screen.getByRole('button',{name:'创建资源库'}));expect(choose).toHaveBeenCalledWith('VIDEO_FILE','COMIC',false,'LEGACY');});
 it('composes media type and structural labels',()=>{const node={nodeType:'WORK',mediaKind:'VIDEO'} as MediaNode;expect(mediaBadge(node)).toBe('视频 · 作品');expect(mediaBadge({...node,binding:{providerSubjectType:2} as MediaNode['binding']})).toBe('动画 · 作品');expect(mediaBadge({...node,binding:{providerSubjectType:6} as MediaNode['binding']})).toBe('影视 · 作品');expect(mediaBadge({...node,mediaKind:'COMIC',nodeType:'CONTAINER'})).toBe('漫画 · 系列');});
 it('shows continue reading and reports page progress',()=>{const read=vi.fn();render(<I18nProvider><ComicBookList books={[{...book,progress:{comicBookId:1,lastPageIndex:3,lastReadAt:'2026-01-01'}}]} onRead={read}/></I18nProvider>);fireEvent.click(screen.getByRole('button',{name:'继续阅读'}));expect(read).toHaveBeenCalledOnce();expect(screen.getByText(/第 4 \/ 8 页/)).toBeTruthy();});
 it('renders RTL double-page order, advances directionally, and releases Blob URLs',async()=>{const view=reader();await screen.findByRole('img',{name:'第 1 / 8 页'});expect(screen.getAllByRole('img').map(img=>img.getAttribute('alt'))).toEqual(['第 2 / 8 页','第 1 / 8 页']);fireEvent.keyDown(window,{key:'ArrowLeft'});await screen.findByRole('img',{name:'第 3 / 8 页'});view.unmount();expect(URL.revokeObjectURL).toHaveBeenCalled();});
 it('restores progress and never records preload as the last read page',async()=>{mocks.open.mockResolvedValueOnce({book:{...book,progress:{comicBookId:1,lastPageIndex:4,lastReadAt:'2026-01-01'}},pages:Array.from({length:8},(_,i)=>({pageIndex:i,pageName:`${i}.png`})),bookmarks:[4]});reader();await screen.findByRole('img',{name:'第 5 / 8 页'});await waitFor(()=>expect(mocks.progress).toHaveBeenCalledWith(1,4,'revision'),{timeout:2000});expect(mocks.progress.mock.calls.every(call=>call[1]===4)).toBe(true);expect(mocks.read.mock.calls.length).toBeLessThanOrEqual(6);});
 it('keeps one bookmark marker and jumps from the adjacent dropdown',async()=>{reader();await screen.findByRole('img',{name:'第 1 / 8 页'});fireEvent.click(screen.getByRole('button',{name:'添加书签'}));await waitFor(()=>expect(mocks.add).toHaveBeenCalledWith(1,0,'revision'));expect(document.querySelectorAll('.reader-topbar button[aria-label="书签"]')).toHaveLength(0);fireEvent.keyDown(window,{key:'ArrowLeft'});await screen.findByRole('img',{name:'第 3 / 8 页'});chooseOption(screen.getByRole('combobox',{name:'跳转到书签'}),'0');await screen.findByRole('img',{name:'第 1 / 8 页'});});
 it('does not intercept page keys while editing reader settings',async()=>{reader();await screen.findByRole('img',{name:'第 1 / 8 页'});const input=screen.getByRole('spinbutton',{name:'跳转页码'});fireEvent.keyDown(input,{key:'ArrowLeft'});expect(screen.getByRole('img',{name:'第 1 / 8 页'})).toBeTruthy();});
 it('flushes a decoded visible page before leaving',async()=>{const back=vi.fn();reader({onBack:back});await screen.findByRole('img',{name:'第 1 / 8 页'});await new Promise(resolve=>setTimeout(resolve,40));fireEvent.click(screen.getByRole('button',{name:'返回作品'}));await waitFor(()=>expect(back).toHaveBeenCalledOnce());expect(mocks.progress).toHaveBeenCalledWith(1,0,'revision');});
});


function chooseOption(control: HTMLElement, value: string) {
 const option=Array.from(control.parentElement!.querySelector('select')!.options).find(option=>option.value===value)!;
 fireEvent.click(control);fireEvent.click(screen.getByRole('option',{name:option.textContent!}));
}

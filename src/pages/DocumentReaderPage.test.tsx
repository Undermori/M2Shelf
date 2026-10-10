// @vitest-environment jsdom
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {DocumentReaderPage} from './DocumentReaderPage';
import {I18nProvider} from '../lib/i18n';
import {defaultTextReaderSettings} from '../types/textReader';
import type {ComicOpenResult} from '../types/comic';

const mocks=vi.hoisted(()=>({read:vi.fn(),progress:vi.fn(),add:vi.fn(),remove:vi.fn(),textGet:vi.fn(),textSave:vi.fn()}));
vi.mock('../lib/api',()=>({api:{readBookDocument:mocks.read,updateComicProgress:mocks.progress,addComicBookmark:mocks.add,removeComicBookmark:mocks.remove,getTextReaderSettings:mocks.textGet,updateTextReaderSettings:mocks.textSave}}));
const opened:ComicOpenResult={book:{id:3,nodeId:7,revision:'epub-revision',sourceKind:'ZIP_ARCHIVE',documentFormat:'EPUB',displayName:'EPUB fixture',pageCount:3,modifiedAt:'2026-01-01',indexError:null,progress:{comicBookId:3,lastPageIndex:1,lastReadAt:'2026-01-01'}},pages:Array.from({length:3},(_,pageIndex)=>({pageIndex,pageName:`Chapter ${pageIndex+1}`})),bookmarks:[]};
beforeEach(()=>{
 vi.resetAllMocks();mocks.textGet.mockResolvedValue({...defaultTextReaderSettings,mode:"SCROLL"});mocks.textSave.mockImplementation(async settings=>settings);
 mocks.read.mockImplementation(async(_id,index)=>new TextEncoder().encode(JSON.stringify([{kind:'text',text:`Chapter ${index+1} <script>literal only</script>`}])).buffer);
 mocks.progress.mockResolvedValue(undefined);mocks.add.mockResolvedValue([1]);mocks.remove.mockResolvedValue([]);
 vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}});
});
afterEach(()=>{cleanup();vi.unstubAllGlobals();});
const reader=(onBack=vi.fn())=>render(<I18nProvider><DocumentReaderPage opened={opened} onBack={onBack} onProgress={vi.fn()}/></I18nProvider>);
describe('local EPUB reader',()=>{
 it.each(['MOBI','AZW3'] as const)('reuses text preferences, directory jumping and bookmarks for %s',async format=>{
  render(<I18nProvider><DocumentReaderPage opened={{...opened,book:{...opened.book,documentFormat:format}}} onBack={vi.fn()} onProgress={vi.fn()}/></I18nProvider>);
  await screen.findByText('Chapter 2 <script>literal only</script>');
  fireEvent.click(screen.getByRole('button',{name:'目录'}));chooseOption(screen.getByRole('combobox',{name:'目录'}),'page-2');await screen.findByText('Chapter 3 <script>literal only</script>');
  fireEvent.click(screen.getByRole('button',{name:'阅读设置'}));fireEvent.change(screen.getByRole('slider',{name:'字号'}),{target:{value:'26'}});
  expect((document.querySelector('.epub-chapter') as HTMLElement).style.fontSize).toBe('26px');
  expect(mocks.read).toHaveBeenCalledTimes(2);
  fireEvent.click(screen.getByRole('button',{name:'添加书签'}));await waitFor(()=>expect(mocks.add).toHaveBeenCalledWith(3,2,'epub-revision',{blockIndex:0,characterOffset:0}));
 });
 it('changes text typography without re-reading a chapter or resetting scroll',async()=>{
  reader();await screen.findByText('Chapter 2 <script>literal only</script>');
  const viewport=document.querySelector('.document-viewport')!;viewport.scrollTop=250;
  fireEvent.click(screen.getByRole('button',{name:'阅读设置'}));
  fireEvent.change(screen.getByRole('slider',{name:'字号'}),{target:{value:'27'}});
  expect(mocks.read).toHaveBeenCalledTimes(1);expect(viewport.scrollTop).toBe(250);
  expect((document.querySelector('.epub-chapter') as HTMLElement).style.fontSize).toBe('27px');
 });
 it('preserves safe headings and inline emphasis and ignores external images',async()=>{
  mocks.read.mockResolvedValueOnce(new TextEncoder().encode(JSON.stringify([{kind:'text',tag:'h2',text:'Heading'},{kind:'text',tag:'p',text:'Bold prose',runs:[{text:'Bold',bold:true},{text:' prose',italic:true}]},{kind:'image',data_url:'https://example.org/remote.jpg'},{kind:'text',tag:'script',text:'literal script'}])).buffer);
  reader();await screen.findByRole('heading',{name:'Heading'});
  expect(document.querySelector('.epub-chapter strong')?.textContent).toBe('Bold');
  expect(document.querySelector('.epub-chapter em')?.textContent).toBe(' prose');
  expect(document.querySelector('.epub-chapter script,.epub-chapter img')).toBeNull();
 });
 it('jumps to a saved chapter from the sole lower bookmark control',async()=>{
  reader();await screen.findByText('Chapter 2 <script>literal only</script>');
  fireEvent.click(screen.getByRole('button',{name:'添加书签'}));
  fireEvent.click(screen.getByRole('button',{name:'目录'}));
  await waitFor(()=>expect(screen.getByRole('combobox',{name:'跳转到书签'}).getAttribute('disabled')).toBeNull());
  fireEvent.click(screen.getByRole('button',{name:'下一页'}));await screen.findByText('Chapter 3 <script>literal only</script>');
  chooseOption(screen.getByRole('combobox',{name:'跳转到书签'}),'1');await screen.findByText('Chapter 2 <script>literal only</script>');
  expect(document.querySelectorAll('.reader-topbar [aria-label="书签"]')).toHaveLength(0);
 });
 it('keeps zoom inside the shared settings panel and exposes a selected bookmark',async()=>{
  reader();await screen.findByText('Chapter 2 <script>literal only</script>');
  expect(screen.queryByRole('slider')).toBeNull();
  fireEvent.click(screen.getByRole('button',{name:'阅读设置'}));expect(screen.getByRole('slider',{name:'字号'})).toBeTruthy();
  fireEvent.click(screen.getByRole('button',{name:'添加书签'}));
  await waitFor(()=>expect(screen.getByRole('button',{name:'移除书签'}).getAttribute('aria-pressed')).toBe('true'));
 });
 it('restores a chapter, renders safe text, bookmarks and flushes on leave',async()=>{
  const back=vi.fn();reader(back);await screen.findByText('Chapter 2 <script>literal only</script>');
  expect(document.querySelector('.epub-chapter script')).toBeNull();
  expect(mocks.read).toHaveBeenCalledWith(3,1,'epub-revision');
  fireEvent.click(screen.getByRole('button',{name:'添加书签'}));await waitFor(()=>expect(mocks.add).toHaveBeenCalledWith(3,1,'epub-revision',{blockIndex:0,characterOffset:0}));
  await new Promise(resolve=>setTimeout(resolve,30));
  fireEvent.click(screen.getByRole('button',{name:'返回作品'}));await waitFor(()=>expect(back).toHaveBeenCalledOnce());
  expect(mocks.progress).toHaveBeenCalledWith(3,1,'epub-revision',{blockIndex:0,characterOffset:0});
 });
 it('moves LTR and does not hijack an input key',async()=>{
  reader();await screen.findByText('Chapter 2 <script>literal only</script>');
  fireEvent.click(screen.getByRole('button',{name:'目录'}));
  fireEvent.keyDown(screen.getByRole('spinbutton'),{key:'ArrowRight'});expect(mocks.read).toHaveBeenCalledTimes(1);
  fireEvent.keyDown(window,{key:'ArrowRight'});await screen.findByText('Chapter 3 <script>literal only</script>');
  fireEvent.keyDown(window,{key:'ArrowRight'});expect(mocks.read).toHaveBeenCalledTimes(2);
 });
 it('does not record a failed chapter as read',async()=>{
  mocks.read.mockRejectedValueOnce(new Error('COMIC_PAGE_CHANGED'));reader();await screen.findByRole('alert');
  await new Promise(resolve=>setTimeout(resolve,750));expect(mocks.progress).not.toHaveBeenCalled();
 });
});


function chooseOption(control: HTMLElement, value: string) {
 const option=Array.from(control.parentElement!.querySelector('select')!.options).find(option=>option.value===value)!;
 fireEvent.click(control);fireEvent.click(screen.getByRole('option',{name:option.textContent!}));
}

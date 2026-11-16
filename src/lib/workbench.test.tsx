// @vitest-environment jsdom
import {act,cleanup,fireEvent,render,screen,waitFor,within} from '@testing-library/react';
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {OrganizePage} from '../pages/OrganizePage';
import {ScreenshotRaster} from '../components/ScreenshotStudio';
import {workbenchThumbCache} from './workbenchThumbCache';
import {I18nProvider,useI18n} from './i18n';
import type {WorkNoteEntry,ScreenshotFolder,ScreenshotImage} from '../types/workbench';
import type {LibraryRoot} from '../types/media';

const mocks=vi.hoisted(()=>({notes:vi.fn(),get:vi.fn(),save:vi.fn(),folders:vi.fn(),images:vi.fn(),read:vi.fn(),choose:vi.fn(),import:vi.fn(),refresh:vi.fn(),remove:vi.fn(),update:vi.fn(),folderUpdate:vi.fn(),reveal:vi.fn(),nodeDetail:vi.fn(),comicDetail:vi.fn()}));
vi.mock('./api',()=>({desktopAvailable:false,chooseScreenshotDirectories:mocks.choose,api:{workspaceFolders:async()=>[],listWorkNotes:mocks.notes,getNote:mocks.get,saveNote:mocks.save,screenshotFolders:mocks.folders,screenshots:mocks.images,readScreenshot:mocks.read,importScreenshotFolder:mocks.import,refreshScreenshotFolder:mocks.refresh,removeScreenshotFolder:mocks.remove,updateScreenshot:mocks.update,updateScreenshotFolder:mocks.folderUpdate,revealScreenshot:mocks.reveal,nodeDetail:mocks.nodeDetail,comicDetail:mocks.comicDetail}}));
const root:LibraryRoot={id:1,path:'X:/Synthetic/Animation',displayName:'Animation',mediaKind:'ANIMATION',recognitionMode:'FOLDER',createdAt:'2026-11-16',lastScanAt:null};
const note=(id:number,body='A saved thought'):WorkNoteEntry=>({target:{kind:'NODE',id},nodeId:id,title:`Animation ${id}`,libraryId:1,libraryName:'Animation',mediaKind:'ANIMATION',body,revision:3,updatedAt:'2026-11-16T12:00:00Z'});
const folder:ScreenshotFolder={id:1,path:'X:/Synthetic/Frames',alias:'Frames',note:'Favorite frames',target:null,createdAt:'2026-11-16',imageCount:2};
const image=(id:number):ScreenshotImage=>({id,folderId:1,fileName:`Frame ${id}.png`,relativePath:`Frame ${id}.png`,revision:`r${id}`,mime:'image/png',width:1280,height:720,note:'',tags:[],starred:false,target:null});
const callbacks=()=>({onOpenTarget:vi.fn().mockResolvedValue(undefined),onError:vi.fn()});
const mount=(props={})=>render(<I18nProvider><OrganizePage roots={[root]} revision={0} busy={false} {...callbacks()} {...props}/></I18nProvider>);
async function gallery(){fireEvent.click(screen.getByRole('tab',{name:'截图画廊'}));await screen.findByText('Frame 1.png');}
async function choose(label:string,option:string){fireEvent.click(screen.getByRole('combobox',{name:label}));fireEvent.click(screen.getByRole('option',{name:option}));}
beforeEach(()=>{workbenchThumbCache.clear();vi.resetAllMocks();mocks.notes.mockResolvedValue({items:[note(501)],total:1});mocks.get.mockResolvedValue({body:'A saved thought',revision:3});mocks.save.mockImplementation(async(_target,body,revision)=>({body,revision:revision+1}));mocks.folders.mockResolvedValue([folder]);mocks.images.mockResolvedValue({items:[image(1),image(2)],total:2});mocks.read.mockResolvedValue(new Uint8Array([137,80,78,71]).buffer);mocks.choose.mockResolvedValue(['X:/Synthetic/New frames','X:/Synthetic/More frames']);mocks.import.mockResolvedValue(folder);mocks.refresh.mockResolvedValue(folder);mocks.remove.mockResolvedValue(undefined);mocks.update.mockImplementation(async value=>value);mocks.folderUpdate.mockResolvedValue(folder);mocks.reveal.mockResolvedValue(undefined);vi.stubGlobal('IntersectionObserver',undefined);URL.createObjectURL=vi.fn(()=>`blob:synthetic-${Math.random()}`);URL.revokeObjectURL=vi.fn();});
afterEach(()=>{cleanup();workbenchThumbCache.clear();vi.unstubAllGlobals();vi.restoreAllMocks();});

describe('daily workbench',()=>{
 it('switches adjacent modes with arrows, Home, End and Ctrl shortcuts without escaping a modal',async()=>{
  mount();await screen.findByRole('heading',{name:'Animation 501'});
  const notes=screen.getByRole('tab',{name:'笔记'}),galleryTab=screen.getByRole('tab',{name:'截图画廊'});
  fireEvent.keyDown(notes,{key:'ArrowRight'});await screen.findByText('Frame 1.png');expect(galleryTab.getAttribute('aria-selected')).toBe('true');expect(document.activeElement).toBe(galleryTab);
  fireEvent.keyDown(galleryTab,{key:'Home'});expect(notes.getAttribute('aria-selected')).toBe('true');fireEvent.keyDown(notes,{key:'End'});expect(screen.getByRole('tab',{name:'项目文件'}).getAttribute('aria-selected')).toBe('true');
  fireEvent.keyDown(window,{key:'1',ctrlKey:true});expect(notes.getAttribute('aria-selected')).toBe('true');expect(document.activeElement).toBe(notes);
  fireEvent.keyDown(window,{key:'2',ctrlKey:true});expect(galleryTab.getAttribute('aria-selected')).toBe('true');fireEvent.click(screen.getByRole('button',{name:'Frame 1.png'}));fireEvent.keyDown(window,{key:'1',ctrlKey:true});expect(galleryTab.getAttribute('aria-selected')).toBe('true');expect(screen.getByRole('dialog')).toBeTruthy();
 });
 it('preserves both mode sessions including selected work, filters, density and unsaved note draft',async()=>{
  mocks.notes.mockResolvedValue({items:[note(681),note(682)],total:2});mount();await screen.findByRole('heading',{name:'Animation 681'});
  fireEvent.click(screen.getByRole('button',{name:'全部作品'}));fireEvent.change(screen.getByRole('searchbox',{name:'搜索作品或笔记'}),{target:{value:'saved search'}});await waitFor(()=>expect(mocks.notes).toHaveBeenLastCalledWith(expect.objectContaining({query:'saved search',notesOnly:false})));
  fireEvent.click(screen.getByRole('option',{name:/Animation 682/}));const editor=screen.getByRole('textbox',{name:'笔记'});await waitFor(()=>expect(editor).toHaveProperty('disabled',false));fireEvent.change(editor,{target:{value:'Draft retained between modes'}});
  await gallery();await choose('全部截图文件夹','Frames (2)');fireEvent.change(screen.getByRole('searchbox',{name:'搜索文件名、笔记或标签'}),{target:{value:'blue'}});fireEvent.click(screen.getByRole('button',{name:'仅看星标'}));await choose('网格密度','紧凑网格');await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({folderId:1,query:'blue',starredOnly:true})));
  const reads=mocks.images.mock.calls.length;fireEvent.click(screen.getByRole('tab',{name:'笔记'}));expect(screen.getByRole('heading',{name:'Animation 682'})).toBeTruthy();expect(screen.getByRole('searchbox',{name:'搜索作品或笔记'})).toHaveProperty('value','saved search');expect(screen.getByRole('button',{name:'全部作品'}).getAttribute('aria-pressed')).toBe('true');expect(screen.getByRole('textbox',{name:'笔记'})).toHaveProperty('value','Draft retained between modes');
  fireEvent.click(screen.getByRole('tab',{name:'截图画廊'}));expect(screen.getByRole('searchbox',{name:'搜索文件名、笔记或标签'})).toHaveProperty('value','blue');expect(screen.getByRole('button',{name:'仅看星标'}).getAttribute('aria-pressed')).toBe('true');expect(screen.getByRole('combobox',{name:'全部截图文件夹'}).textContent).toContain('Frames');expect(screen.getByRole('combobox',{name:'网格密度'}).textContent).toContain('紧凑网格');expect(mocks.images).toHaveBeenCalledTimes(reads);
 });
 it('opens a note work in the gallery, clears conflicting filters and keeps an explicit clearable work filter',async()=>{
  mocks.notes.mockResolvedValue({items:[note(683)],total:1});mount();await screen.findByRole('heading',{name:'Animation 683'});await gallery();await choose('全部截图文件夹','Frames (2)');fireEvent.change(screen.getByRole('searchbox',{name:'搜索文件名、笔记或标签'}),{target:{value:'old query'}});fireEvent.click(screen.getByRole('button',{name:'仅看星标'}));
  fireEvent.click(screen.getByRole('tab',{name:'笔记'}));fireEvent.click(screen.getByRole('button',{name:'查看截图'}));await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith({sort:'MODIFIED',descending:true,folderId:null,query:'',target:{kind:'NODE',id:683},starredOnly:false,offset:0,limit:48}));expect(screen.getByRole('searchbox',{name:'搜索文件名、笔记或标签'})).toHaveProperty('value','');expect(screen.getByRole('button',{name:'仅看星标'}).getAttribute('aria-pressed')).toBe('false');await waitFor(()=>expect(screen.getByRole('button',{name:'按关联作品筛选'}).textContent).toContain('Animation 683'));fireEvent.click(screen.getByRole('button',{name:'清除作品筛选'}));await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({target:null})));
 });
 it('opens and focuses an exact linked note beyond the current filtered notes page',async()=>{
  mocks.images.mockResolvedValue({items:[{...image(1),target:{kind:'NODE',id:987},nodeId:987}],total:1});mocks.nodeDetail.mockResolvedValue({node:{id:987,libraryRootId:1,displayName:'Unlisted linked work',folderName:'Source',nodeType:'WORK'},binding:null});mount();await screen.findByRole('heading',{name:'Animation 501'});fireEvent.change(screen.getByRole('searchbox',{name:'搜索作品或笔记'}),{target:{value:'keep note filter'}});await gallery();fireEvent.click(screen.getByRole('button',{name:'查看笔记'}));await screen.findByRole('heading',{name:'Unlisted linked work'});await waitFor(()=>expect(document.activeElement).toBe(screen.getByRole('textbox',{name:'笔记'})));expect(mocks.get).toHaveBeenCalledWith({kind:'NODE',id:987});expect(screen.getByRole('searchbox',{name:'搜索作品或笔记'})).toHaveProperty('value','keep note filter');
 });
 it('protects a dirty lightbox before opening the associated book note and resolves the exact book identity',async()=>{
  const linked={...image(1),target:{kind:'BOOK' as const,id:991},nodeId:992};mocks.images.mockResolvedValue({items:[linked],total:1});mocks.nodeDetail.mockResolvedValue({node:{id:992,libraryRootId:1,displayName:'Book container',folderName:'Container',nodeType:'WORK'},binding:null});mocks.comicDetail.mockResolvedValue([{id:990,displayName:'Other book'},{id:991,displayName:'Exact linked book'}]);mount();await gallery();fireEvent.click(screen.getByRole('button',{name:'Frame 1.png'}));const dialog=screen.getByRole('dialog');fireEvent.change(within(dialog).getByRole('textbox',{name:'截图笔记'}),{target:{value:'Do not lose this edit'}});fireEvent.click(within(dialog).getByRole('button',{name:'查看笔记'}));expect(screen.getByRole('dialog')).toBe(dialog);expect(within(dialog).queryByRole('alert')).toBeNull();await waitFor(()=>expect(mocks.update).toHaveBeenCalledWith(expect.objectContaining({note:'Do not lose this edit'})));await screen.findByRole('heading',{name:'Exact linked book'});expect(screen.queryByRole('dialog')).toBeNull();await waitFor(()=>expect(mocks.get).toHaveBeenCalledWith({kind:'BOOK',id:991}));await waitFor(()=>expect(document.activeElement).toBe(screen.getByRole('textbox',{name:'笔记'})));
 });
 it('provides notes, gallery and project files, defaults to saved notes and opens exact details',async()=>{
  const cb=callbacks();mount(cb);await screen.findByRole('heading',{name:'Animation 501'});
  expect(screen.getAllByRole('tab').map(tab=>tab.textContent)).toEqual(['笔记','截图画廊','项目文件']);
  expect(mocks.notes).toHaveBeenCalledWith(expect.objectContaining({notesOnly:true,offset:0,limit:60}));
  fireEvent.click(screen.getByRole('button',{name:'打开作品详情'}));expect(cb.onOpenTarget).toHaveBeenCalledWith({kind:'NODE',id:501},501);
  fireEvent.click(screen.getByRole('button',{name:'全部作品'}));await waitFor(()=>expect(mocks.notes).toHaveBeenLastCalledWith(expect.objectContaining({notesOnly:false})));
 });
 it('shares note CAS writes, retains conflicted drafts and reloads explicitly',async()=>{
  mocks.notes.mockResolvedValue({items:[note(502)],total:1});mount();const editor=await screen.findByRole('textbox',{name:'笔记'});await waitFor(()=>expect(editor).toHaveProperty('disabled',false));
  mocks.save.mockRejectedValueOnce(new Error('NOTE_CONFLICT'));fireEvent.change(editor,{target:{value:'My retained draft'}});fireEvent.click(screen.getByRole('button',{name:'保存'}));
  await screen.findByRole('alert');expect(editor).toHaveProperty('value','My retained draft');expect(mocks.save).toHaveBeenCalledWith({kind:'NODE',id:502},'My retained draft',3);
  mocks.get.mockResolvedValue({body:'Latest saved revision',revision:5});fireEvent.click(screen.getByRole('button',{name:'放弃草稿并读取已保存内容'}));await waitFor(()=>expect(editor).toHaveProperty('value','Latest saved revision'));
  fireEvent.click(screen.getByRole('button',{name:'清空'}));fireEvent.click(screen.getByRole('button',{name:'保存'}));await waitFor(()=>expect(mocks.save).toHaveBeenLastCalledWith({kind:'NODE',id:502},'',5));
 });
 it('saves notes immediately with keyboard shortcuts and flushes when keyboard navigation changes work',async()=>{
  mocks.notes.mockResolvedValue({items:[note(601),note(602)],total:2});mount();let editor=await screen.findByRole('textbox',{name:'笔记'});await waitFor(()=>expect(editor).toHaveProperty('disabled',false));
  fireEvent.change(editor,{target:{value:'Keyboard save'}});fireEvent.keyDown(editor,{key:'s',ctrlKey:true});expect(mocks.save).toHaveBeenCalledWith({kind:'NODE',id:601},'Keyboard save',3);
  await waitFor(()=>expect(screen.getByRole('button',{name:'保存'})).toHaveProperty('disabled',false));fireEvent.change(editor,{target:{value:'Flush before changing work'}});
  const first=screen.getByRole('option',{name:/Animation 601/});first.focus();fireEvent.keyDown(first,{key:'ArrowDown'});await screen.findByRole('heading',{name:'Animation 602'});
  await waitFor(()=>expect(mocks.save).toHaveBeenCalledWith({kind:'NODE',id:601},'Flush before changing work',4));expect(document.activeElement).toBe(screen.getByRole('option',{name:/Animation 602/}));
  editor=screen.getByRole('textbox',{name:'笔记'});await waitFor(()=>expect(editor).toHaveProperty('disabled',false));fireEvent.change(editor,{target:{value:'Command save'}});fireEvent.keyDown(editor,{key:'s',metaKey:true});expect(mocks.save).toHaveBeenCalledWith({kind:'NODE',id:602},'Command save',3);
 });
 it('offers a working first-note action when the saved-notes list is empty',async()=>{
  mocks.notes.mockImplementation(async query=>query.notesOnly?{items:[],total:0}:{items:[note(603,'')],total:1});mount();fireEvent.click(await screen.findByRole('button',{name:'选择作品，开始记录'}));await screen.findByRole('heading',{name:'Animation 603'});expect(mocks.notes).toHaveBeenLastCalledWith(expect.objectContaining({notesOnly:false,query:'',rootId:null,mediaKind:null}));
 });
 it('serializes delayed note reloads, locks input, retains failures and resumes from the returned revision',async()=>{
  mocks.notes.mockResolvedValue({items:[note(506)],total:1});mount();const editor=await screen.findByRole('textbox',{name:'笔记'});await waitFor(()=>expect(editor).toHaveProperty('disabled',false));
  mocks.save.mockRejectedValueOnce(new Error('NOTE_CONFLICT'));fireEvent.change(editor,{target:{value:'Retained conflict draft'}});fireEvent.click(screen.getByRole('button',{name:'保存'}));await screen.findByRole('alert');
  let reject!:(error:Error)=>void;mocks.get.mockImplementationOnce(()=>new Promise((_resolve,fail)=>{reject=fail;}));const reload=screen.getByRole('button',{name:'放弃草稿并读取已保存内容'});fireEvent.click(reload);expect(editor).toHaveProperty('disabled',true);expect(reload).toHaveProperty('disabled',true);fireEvent.click(reload);fireEvent.change(editor,{target:{value:'Blocked edit'}});expect(editor).toHaveProperty('value','Retained conflict draft');expect(mocks.get).toHaveBeenCalledTimes(2);
  await act(async()=>reject(new Error('Synthetic reload failure')));expect(editor).toHaveProperty('disabled',false);expect(editor).toHaveProperty('value','Retained conflict draft');
  let resolve!:(note:{body:string;revision:number})=>void;mocks.get.mockImplementationOnce(()=>new Promise(done=>{resolve=done;}));fireEvent.click(screen.getByRole('button',{name:'放弃草稿并读取已保存内容'}));await act(async()=>resolve({body:'Latest server note',revision:7}));expect(editor).toHaveProperty('value','Latest server note');fireEvent.change(editor,{target:{value:'Safe next edit'}});fireEvent.click(screen.getByRole('button',{name:'保存'}));await waitFor(()=>expect(mocks.save).toHaveBeenLastCalledWith({kind:'NODE',id:506},'Safe next edit',7));
 });
 it('debounces automatic saving and fences an older filtered list',async()=>{
  let finish!:(page:{items:WorkNoteEntry[];total:number})=>void;mocks.notes.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;})).mockResolvedValue({items:[note(503)],total:1});mount();await waitFor(()=>expect(mocks.notes).toHaveBeenCalledTimes(1));fireEvent.click(screen.getByRole('button',{name:'全部作品'}));await screen.findByRole('heading',{name:'Animation 503'});await act(async()=>finish({items:[note(599)],total:1}));expect(screen.queryByText('Animation 599')).toBeNull();const editor=screen.getByRole('textbox',{name:'笔记'});await waitFor(()=>expect(editor).toHaveProperty('disabled',false));fireEvent.change(editor,{target:{value:'Automatic save'}});await waitFor(()=>expect(mocks.save).toHaveBeenCalledWith({kind:'NODE',id:503},'Automatic save',3),{timeout:2000});
 });
 it('keeps a confirmed note preview when an earlier list snapshot completes later',async()=>{
  mocks.notes.mockResolvedValueOnce({items:[note(504)],total:1});let finish!:(page:{items:WorkNoteEntry[];total:number})=>void;mocks.notes.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));mount();await screen.findByRole('heading',{name:'Animation 504'});
  fireEvent.click(screen.getByRole('button',{name:'刷新'}));await waitFor(()=>expect(mocks.notes).toHaveBeenCalledTimes(2));
  await act(async()=>{window.dispatchEvent(new CustomEvent('m2shelf-note-saved',{detail:{target:{kind:'NODE',id:504},body:'Confirmed preview',revision:9}}));finish({items:[note(504,'Old snapshot')],total:1});});
  expect(screen.getByText('Confirmed preview')).toBeTruthy();expect(screen.queryByText('Old snapshot')).toBeNull();
 });
 it('imports multiple folders and removal requires confirmation without filesystem deletion',async()=>{
  mount();await gallery();fireEvent.click(screen.getByRole('button',{name:'导入截图文件夹'}));await waitFor(()=>expect(mocks.import).toHaveBeenCalledTimes(2));
  await choose('全部截图文件夹','Frames (2)');fireEvent.click(screen.getByRole('button',{name:'移除截图夹'}));const dialog=screen.getByRole('dialog');expect(dialog.textContent).toContain('绝不删除磁盘图片');expect(mocks.remove).not.toHaveBeenCalled();fireEvent.click(within(dialog).getByRole('button',{name:'移除截图夹'}));await waitFor(()=>expect(mocks.remove).toHaveBeenCalledExactlyOnceWith(1));
 });
 it('allows editing, refreshing and removing any folder from the all-folders view',async()=>{
  mocks.folders.mockResolvedValue([folder,{...folder,id:2,alias:'Second folder'}]);mount();await gallery();fireEvent.click(screen.getByRole('button',{name:'管理截图夹'}));fireEvent.click(within(screen.getByRole('dialog')).getByRole('button',{name:/Second folder/}));
  let dialog=screen.getByRole('dialog');expect(within(dialog).getByRole('textbox',{name:'截图夹名称'})).toHaveProperty('value','Second folder');fireEvent.change(within(dialog).getByRole('textbox',{name:'截图夹名称'}),{target:{value:'Renamed in database'}});fireEvent.click(within(dialog).getByRole('button',{name:'保存'}));await waitFor(()=>expect(mocks.folderUpdate).toHaveBeenCalledWith(2,'Renamed in database',folder.note,null));await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());
  fireEvent.click(screen.getByRole('button',{name:'管理截图夹'}));fireEvent.click(within(screen.getByRole('dialog')).getByRole('button',{name:/Second folder/}));dialog=screen.getByRole('dialog');fireEvent.click(within(dialog).getByRole('button',{name:'刷新'}));await waitFor(()=>expect(mocks.refresh).toHaveBeenCalledWith(2));await waitFor(()=>expect(within(dialog).getByRole('button',{name:'移除截图夹'})).toHaveProperty('disabled',false));fireEvent.click(within(dialog).getByRole('button',{name:'移除截图夹'}));dialog=screen.getByRole('dialog');expect(dialog.textContent).toContain('绝不删除磁盘图片');fireEvent.click(within(dialog).getByRole('button',{name:'移除截图夹'}));await waitFor(()=>expect(mocks.remove).toHaveBeenCalledExactlyOnceWith(2));
 });
 it('stars a card directly without opening the lightbox or changing its note, tags and association',async()=>{
  const linked={...image(1),note:'Keep me',tags:['blue'],ownTarget:{kind:'NODE' as const,id:77},target:{kind:'NODE' as const,id:77}};mocks.images.mockResolvedValue({items:[linked],total:1});let finish!:(value:ScreenshotImage)=>void;mocks.update.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));mount();await gallery();const star=screen.getByRole('button',{name:'星标 · Frame 1.png'});fireEvent.click(star);fireEvent.click(star);expect(mocks.update).toHaveBeenCalledExactlyOnceWith({...linked,starred:true});expect(screen.queryByRole('dialog')).toBeNull();await act(async()=>finish({...linked,starred:true}));expect(screen.getByRole('button',{name:'取消星标 · Frame 1.png'}).getAttribute('aria-pressed')).toBe('true');expect(screen.queryByRole('dialog')).toBeNull();
 });
 it('filters by a searched work, displays its name, closes after choosing and clears with one action',async()=>{
  mocks.notes.mockImplementation(async query=>({items:[note(query.query?777:501)],total:1}));mount();await gallery();const trigger=screen.getByRole('button',{name:'按关联作品筛选'});fireEvent.click(trigger);const search=screen.getByRole('searchbox',{name:'搜索要关联的作品'});fireEvent.change(search,{target:{value:'777'}});fireEvent.click(await screen.findByRole('button',{name:'Animation 777 Animation'}));
  expect(screen.queryByRole('group',{name:'搜索要关联的作品'})).toBeNull();expect(trigger.textContent).toContain('Animation 777');expect(trigger.getAttribute('aria-expanded')).toBe('false');expect(document.activeElement).toBe(trigger);await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({target:{kind:'NODE',id:777},offset:0})));
  fireEvent.click(screen.getByRole('button',{name:'清除作品筛选'}));await waitFor(()=>expect(mocks.images).toHaveBeenLastCalledWith(expect.objectContaining({target:null})));expect(trigger.textContent).toContain('全部关联作品');
  fireEvent.click(trigger);fireEvent.pointerDown(document.body);expect(trigger.getAttribute('aria-expanded')).toBe('false');fireEvent.click(trigger);fireEvent.keyDown(screen.getByRole('searchbox',{name:'搜索要关联的作品'}),{key:'Escape'});expect(trigger.getAttribute('aria-expanded')).toBe('false');expect(document.activeElement).toBe(trigger);
 });
 it('keeps successful imports visible when another folder fails and continues independent imports',async()=>{
  const cb=callbacks();mocks.import.mockRejectedValueOnce(new Error('Synthetic failure')).mockResolvedValueOnce(folder);mount(cb);await gallery();const reads=mocks.folders.mock.calls.length;
  fireEvent.click(screen.getByRole('button',{name:'导入截图文件夹'}));await waitFor(()=>expect(mocks.import).toHaveBeenCalledTimes(2));await waitFor(()=>expect(mocks.folders.mock.calls.length).toBeGreaterThan(reads));expect(cb.onError).toHaveBeenCalledWith(expect.stringContaining('1 个截图夹未完成操作'));
 });
 it('refreshes successfully processed folders even when a later folder fails',async()=>{
  const cb=callbacks();mocks.folders.mockResolvedValue([folder,{...folder,id:2,alias:'Second folder'}]);mocks.refresh.mockResolvedValueOnce(folder).mockRejectedValueOnce(new Error('Synthetic failure'));mount(cb);await gallery();const reads=mocks.images.mock.calls.length;
  fireEvent.click(screen.getByRole('button',{name:'刷新'}));await waitFor(()=>expect(mocks.refresh).toHaveBeenCalledTimes(2));await waitFor(()=>expect(mocks.images.mock.calls.length).toBeGreaterThan(reads));expect(cb.onError).toHaveBeenCalledWith(expect.stringContaining('1 个截图夹未完成操作'));
 });
 it('shows an uncropped lightbox, saves note tags and star, reveals original and handles arrow/Escape focus',async()=>{
  mount();await gallery();const card=screen.getByRole('button',{name:'Frame 1.png'});card.focus();fireEvent.click(card);const dialog=screen.getByRole('dialog');await waitFor(()=>expect(mocks.read).toHaveBeenCalledWith(1,'r1',false));
  fireEvent.change(within(dialog).getByRole('textbox',{name:'截图笔记'}),{target:{value:'Lovely scene'}});fireEvent.change(within(dialog).getByRole('textbox',{name:'标签（用逗号分隔）'}),{target:{value:'night, blue'}});fireEvent.click(within(dialog).getByRole('button',{name:'星标'}));await waitFor(()=>expect(mocks.update).toHaveBeenCalledWith(expect.objectContaining({id:1,note:'Lovely scene',tags:['night','blue'],starred:true})));
  await within(dialog).findByText('已保存');fireEvent.click(within(dialog).getByRole('button',{name:'用资源管理器打开'}));expect(mocks.reveal).toHaveBeenCalledWith(1,'r1');fireEvent.keyDown(dialog,{key:'ArrowRight'});expect(screen.getByRole('heading',{name:'Frame 2.png'})).toBeTruthy();fireEvent.keyDown(window,{key:'Escape'});expect(screen.queryByRole('dialog')).toBeNull();expect(document.activeElement).toBe(card);
 });
 it('keeps each image draft clean through rapid bidirectional navigation and guards a real edit',async()=>{
  const first={...image(1),note:'First saved note',tags:['first'],starred:true},second={...image(2),note:'Second saved note',tags:['second'],starred:false};mocks.images.mockResolvedValue({items:[first,second],total:2});mount();await gallery();const card=screen.getByRole('button',{name:first.fileName});card.focus();fireEvent.click(card);const dialog=screen.getByRole('dialog');
  for(let turn=0;turn<8;turn++){
   fireEvent.keyDown(dialog,{key:'ArrowRight'});expect(within(dialog).getByRole('heading',{name:second.fileName})).toBeTruthy();expect(within(dialog).getByRole('textbox',{name:'截图笔记'})).toHaveProperty('value',second.note);expect(within(dialog).getByRole('textbox',{name:'标签（用逗号分隔）'})).toHaveProperty('value','second');expect(within(dialog).queryByRole('button',{name:'保存'})).toBeNull();
   fireEvent.keyDown(dialog,{key:'ArrowLeft'});expect(within(dialog).getByRole('heading',{name:first.fileName})).toBeTruthy();expect(within(dialog).getByRole('textbox',{name:'截图笔记'})).toHaveProperty('value',first.note);expect(within(dialog).getByRole('textbox',{name:'标签（用逗号分隔）'})).toHaveProperty('value','first');expect(within(dialog).getByRole('button',{name:'取消星标'}).getAttribute('aria-pressed')).toBe('true');expect(within(dialog).queryByRole('alert')).toBeNull();
  }
  fireEvent.change(within(dialog).getByRole('textbox',{name:'截图笔记'}),{target:{value:'Real unsaved edit'}});fireEvent.keyDown(dialog,{key:'ArrowRight'});await within(dialog).findByRole('heading',{name:second.fileName});expect(mocks.update).toHaveBeenCalledWith(expect.objectContaining({id:1,note:'Real unsaved edit'}));expect(within(dialog).queryByRole('alert')).toBeNull();fireEvent.keyDown(window,{key:'Escape'});expect(screen.queryByRole('dialog')).toBeNull();expect(document.activeElement).toBe(card);
 });
 it('flushes a dirty screenshot on Escape and restores focus without a confirmation',async()=>{
  mount();await gallery();const card=screen.getByRole('button',{name:'Frame 1.png'});card.focus();fireEvent.click(card);fireEvent.change(screen.getByRole('textbox',{name:'截图笔记'}),{target:{value:'Unsaved'}});fireEvent.keyDown(window,{key:'Escape'});await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());expect(mocks.update).toHaveBeenCalledWith(expect.objectContaining({note:'Unsaved'}));expect(document.activeElement).toBe(card);
 });

 it('dismisses the inner work picker before the lightbox or folder dialog and autosaves',async()=>{
  mount();await gallery();fireEvent.click(screen.getByRole('button',{name:'Frame 1.png'}));let dialog=screen.getByRole('dialog');fireEvent.change(within(dialog).getByRole('textbox',{name:'截图笔记'}),{target:{value:'Keep this draft'}});let trigger=within(dialog).getByRole('button',{name:'关联作品'});fireEvent.click(trigger);fireEvent.keyDown(window,{key:'Escape'});expect(screen.getByRole('dialog')).toBe(dialog);expect(trigger.getAttribute('aria-expanded')).toBe('false');expect(document.activeElement).toBe(trigger);expect(screen.queryByRole('alert')).toBeNull();
  fireEvent.keyDown(within(dialog).getByRole('textbox',{name:'截图笔记'}),{key:'s',ctrlKey:true});expect(mocks.update).not.toHaveBeenCalled();await waitFor(()=>expect(mocks.update).toHaveBeenCalledWith(expect.objectContaining({note:'Keep this draft'})));await within(dialog).findByText('已保存');fireEvent.keyDown(window,{key:'Escape'});expect(screen.queryByRole('dialog')).toBeNull();
  fireEvent.click(screen.getByRole('button',{name:'管理截图夹'}));fireEvent.click(within(screen.getByRole('dialog')).getByRole('button',{name:/Frames/}));dialog=screen.getByRole('dialog');trigger=within(dialog).getByRole('button',{name:'关联作品'});fireEvent.click(trigger);fireEvent.keyDown(window,{key:'Escape'});expect(screen.getByRole('dialog')).toBe(dialog);expect(trigger.getAttribute('aria-expanded')).toBe('false');expect(document.activeElement).toBe(trigger);
 });
 it('resolves saved associations beyond the first page and refreshes localized node titles',async()=>{
  const LanguageControl=()=>{const {setLanguage}=useI18n();return <button onClick={()=>setLanguage('en-US')}>Switch language</button>;};
  mocks.folders.mockResolvedValue([{...folder,target:{kind:'NODE',id:888},nodeId:888}]);mocks.nodeDetail.mockResolvedValue({node:{id:888,displayName:'Source',folderName:'Source',nodeType:'WORK'},binding:{providerTitle:'原题',providerTitleCn:'中文关联标题',providerTitleEn:'Linked English title'}});
  render(<I18nProvider><LanguageControl/><OrganizePage roots={[root]} revision={0} busy={false} {...callbacks()}/></I18nProvider>);await gallery();fireEvent.click(screen.getByRole('button',{name:'管理截图夹'}));fireEvent.click(within(screen.getByRole('dialog')).getByRole('button',{name:/Frames/}));await waitFor(()=>expect(screen.getByRole('button',{name:'关联作品'}).textContent).toContain('中文关联标题'));expect(mocks.nodeDetail).toHaveBeenCalledWith(888);
  fireEvent.click(screen.getByText('Switch language'));await waitFor(()=>expect(screen.getByRole('button',{name:'Linked work'}).textContent).toContain('Linked English title'));expect(mocks.nodeDetail).toHaveBeenCalledTimes(2);
 });
 it('resolves a saved book association through its indexed owner without opening the reader',async()=>{
  mocks.folders.mockResolvedValue([{...folder,target:{kind:'BOOK',id:99},nodeId:42}]);mocks.comicDetail.mockResolvedValue([{id:99,displayName:'Exact associated book'}]);mount();await gallery();fireEvent.click(screen.getByRole('button',{name:'管理截图夹'}));fireEvent.click(within(screen.getByRole('dialog')).getByRole('button',{name:/Frames/}));await waitFor(()=>expect(screen.getByRole('button',{name:'关联作品'}).textContent).toContain('Exact associated book'));expect(mocks.comicDetail).toHaveBeenCalledExactlyOnceWith(42);
 });
 it('validates tag limits visibly without truncating or sending a partial metadata write',async()=>{
  mount();await gallery();fireEvent.click(screen.getByRole('button',{name:'Frame 1.png'}));const input=screen.getByRole('textbox',{name:'标签（用逗号分隔）'});const tags=Array.from({length:21},(_,index)=>`tag ${index}`).join(', ');fireEvent.change(input,{target:{value:tags}});
  expect(screen.getByRole('alert').textContent).toContain('20');expect(input).toHaveProperty('value',tags);expect(screen.queryByRole('button',{name:'保存'})).toBeNull();expect(mocks.update).not.toHaveBeenCalled();fireEvent.change(input,{target:{value:''}});
 });
 it('disables retained gallery cards while a replacement filter is loading',async()=>{
  mount();await gallery();mocks.images.mockImplementationOnce(()=>new Promise(()=>undefined));fireEvent.change(screen.getByRole('searchbox',{name:'搜索文件名、笔记或标签'}),{target:{value:'new filter'}});
  expect(screen.getByRole('button',{name:'Frame 1.png'})).toHaveProperty('disabled',true);fireEvent.click(screen.getByRole('button',{name:'Frame 1.png'}));expect(screen.queryByRole('dialog')).toBeNull();
 });
 it('limits active image reads to four and cancels queued reads when unmounted',async()=>{
  const finishes:Array<(bytes:ArrayBuffer)=>void>=[];mocks.read.mockImplementation(()=>new Promise(resolve=>finishes.push(resolve)));mocks.images.mockResolvedValue({items:Array.from({length:8},(_,index)=>image(index+1)),total:8});const view=mount();await gallery();await waitFor(()=>expect(mocks.read).toHaveBeenCalledTimes(4));view.unmount();await act(async()=>{finishes.forEach(resolve=>resolve(new ArrayBuffer(0)));});expect(mocks.read).toHaveBeenCalledTimes(4);expect(URL.createObjectURL).not.toHaveBeenCalled();
 });
 it('normalizes AVIF thumbnails inside the bounded reader and closes decoded bitmaps',async()=>{
  mocks.read.mockResolvedValue(new Uint8Array([0,0,0,32,102,116,121,112,97,118,105,102]).buffer);const close=vi.fn(),drawImage=vi.fn(),convertToBlob=vi.fn().mockResolvedValue(new Blob(['bounded'],{type:'image/png'}));
  const bitmap=vi.fn().mockResolvedValue({close,width:3840,height:2160});vi.stubGlobal('createImageBitmap',bitmap);
  vi.stubGlobal('OffscreenCanvas',class {getContext(){return {drawImage};}convertToBlob=convertToBlob;});
  render(<I18nProvider><ScreenshotRaster image={{...image(12),mime:'image/avif',width:3840,height:2160}}/></I18nProvider>);
  await waitFor(()=>expect(URL.createObjectURL).toHaveBeenCalled());expect(bitmap).toHaveBeenCalledWith(expect.any(Blob));expect(close).toHaveBeenCalledOnce();expect(drawImage).toHaveBeenCalledWith(expect.objectContaining({width:3840,height:2160}),0,0,768,432);expect((URL.createObjectURL as ReturnType<typeof vi.fn>).mock.calls[0][0]).toHaveProperty('type','image/png');
 });
 it('shows a safe thumbnail fallback when an AVIF decoder is unavailable',async()=>{
  mocks.read.mockResolvedValue(new ArrayBuffer(0));vi.stubGlobal('createImageBitmap',undefined);render(<I18nProvider><ScreenshotRaster image={{...image(13),mime:'image/avif'}}/></I18nProvider>);
  await screen.findByRole('status');expect(URL.createObjectURL).not.toHaveBeenCalled();
 });
 it('adopts persisted notes sort preference and forwards updates to parent',async()=>{
  const onSortPreferenceChange=vi.fn();
  mount({sortPreferences:{filesSort:'SIZE',filesDescending:true,gallerySort:'NAME',galleryDescending:false,notesSort:'title'},onSortPreferenceChange});
  await waitFor(()=>expect(mocks.notes).toHaveBeenLastCalledWith(expect.objectContaining({sort:'title'})));
  fireEvent.click(screen.getByRole('combobox',{name:'笔记排序'}));fireEvent.click(screen.getByRole('option',{name:'最近编辑优先'}));
  expect(onSortPreferenceChange).toHaveBeenCalledWith({scope:'notes',sort:'recent'});
  await waitFor(()=>expect(mocks.notes).toHaveBeenLastCalledWith(expect.objectContaining({sort:'recent'})));
 });
});

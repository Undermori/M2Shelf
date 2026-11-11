// @vitest-environment jsdom
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {I18nProvider} from './i18n';
import {CollectionStateProvider,CollectionStatusControl,CollectionStatusFilter,useCollection} from './collectionState';
import {BookPosterCard} from '../components/BookPosterCard';
import {RecentlyWatchedPage} from '../pages/RecentlyWatchedPage';
import {CollectionBackupSettings} from '../components/CollectionBackupSettings';
import type {ComicBook} from '../types/comic';
import type {MediaNode} from '../types/media';
import type {CollectionSnapshot} from '../types/collection';
import {collectionMessages} from './collectionMessages';
import {backupMessages} from './backupMessages';

const mocks=vi.hoisted(()=>({snapshot:vi.fn(),set:vi.fn(),notice:vi.fn(),flush:vi.fn(),save:vi.fn(),open:vi.fn(),export:vi.fn(),prepare:vi.fn(),apply:vi.fn(),discard:vi.fn()}));
vi.mock('./api',()=>({desktopAvailable:true,api:{collectionStatuses:mocks.snapshot,setCollectionStatus:mocks.set,collectionRestoreNotice:mocks.notice,exportCollectionBackup:mocks.export,prepareCollectionRestore:mocks.prepare,applyCollectionRestore:mocks.apply,discardCollectionRestore:mocks.discard}}));
vi.mock('./settingsStore',()=>({useAppSettings:()=>({saving:false,flush:mocks.flush})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({save:mocks.save,open:mocks.open}));
vi.mock('../hooks/useCoverDataUrl',()=>({useCoverDataUrl:()=>({coverUrl:null})}));
vi.mock('../hooks/usePosterViewportLifecycle',()=>({usePosterViewportLifecycle:()=>({coverRequested:false,coverVisible:false})}));
const book:ComicBook={id:5,nodeId:12,displayName:'Book',sourcePath:'X:/Synthetic/Book.pdf',sourceKind:'ZIP_ARCHIVE',documentFormat:'PDF',revision:'r',pageCount:3,modifiedAt:'2026-10-22',indexError:null,progress:null};
const node:MediaNode={id:5,libraryRootId:1,parentNodeId:1,absolutePath:'X:/Synthetic/Movie',folderName:'Movie',displayName:'Movie',nodeType:'WORK',mediaKind:'ANIMATION',manualTypeOverride:false,coverSource:'PLACEHOLDER',coverCachePath:null,totalVideoCount:2,createdAt:'2026-10-22',updatedAt:'2026-10-22',lastSeenAt:'2026-10-22',userTags:[]};
const mount=(children:React.ReactNode)=>render(<I18nProvider>{children}</I18nProvider>);
beforeEach(()=>{vi.resetAllMocks();mocks.notice.mockResolvedValue(null);mocks.flush.mockResolvedValue(undefined);mocks.snapshot.mockResolvedValue({entries:[],bookMemberships:[],nodeMemberships:[]});Element.prototype.scrollIntoView=vi.fn();});
afterEach(cleanup);

it.each(['PDF','CBZ','EPUB','MOBI','AZW3','TXT','IMAGE_FOLDER'])('keeps %s title/time/quantity order in grid and list, without a time in ordinary cards',format=>{
 const sample:ComicBook={...book,displayName:`Book.${format}`,sourceKind:format==='IMAGE_FOLDER'?'IMAGE_FOLDER':'ZIP_ARCHIVE',documentFormat:['CBZ','IMAGE_FOLDER'].includes(format)?null:format as ComicBook['documentFormat']};
 for(const viewMode of ['grid','list'] as const){const props={book:sample,title:'Book',count:1,series:false,root:{mediaKind:'EBOOK' as const},viewMode,revision:0,onOpen:vi.fn(),onMenu:vi.fn()};const view=mount(<BookPosterCard {...props} watchedAt="2026-10-22T00:00:00Z"/>);expect([...view.container.querySelector('.media-card-copy')!.children].map(e=>e.tagName)).toEqual(['STRONG','TIME','SPAN']);view.rerender(<I18nProvider><BookPosterCard {...props}/></I18nProvider>);expect(view.container.querySelector('time')).toBeNull();expect(view.container.querySelector('.media-card-meta small')?.textContent).toContain('1');view.unmount();}
});

it.each(['grid','list'] as const)('selects real mixed Node/Book IDs in %s and requires confirmation only for clear',async viewMode=>{
 const remove=vi.fn().mockResolvedValue(undefined),clear=vi.fn().mockResolvedValue(undefined),open=vi.fn();
 mount(<RecentlyWatchedPage entries={[{node,watchedAt:'2026-10-22'},{node:{...node,id:12,mediaKind:'EBOOK'},comicBook:book,watchedAt:'2026-10-22'}]} loading={false} viewMode={viewMode} onViewMode={vi.fn()} onOpenNode={open} onReadBook={open} onRevealBook={vi.fn()} onMenu={vi.fn()} onBangumi={vi.fn()} onRetryCover={vi.fn()} coverRevision={0} onRemoveRecent={remove} onClearRecent={clear}/>);
 fireEvent.click(document.querySelector('.media-card-open')!);expect(open).toHaveBeenCalledOnce();open.mockClear();
 fireEvent.click(screen.getByRole('button',{name:'编辑模式'}));const cards=document.querySelectorAll('.media-card-open');fireEvent.click(cards[0]);fireEvent.click(cards[1]);expect(open).not.toHaveBeenCalled();fireEvent.click(screen.getByRole('button',{name:'移除所选'}));await waitFor(()=>expect(remove).toHaveBeenCalledWith([{kind:'NODE',id:5},{kind:'BOOK',id:5}]));
 await waitFor(()=>expect((screen.getByRole('button',{name:'清空最近打开'}) as HTMLButtonElement).disabled).toBe(false));fireEvent.click(screen.getByRole('button',{name:'清空最近打开'}));expect(clear).not.toHaveBeenCalled();expect(screen.getByRole('dialog').textContent).toContain('不删除文件、阅读进度或书签');fireEvent.click(screen.getByRole('button',{name:'确认'}));await waitFor(()=>expect(clear).toHaveBeenCalledOnce());
});

it('keeps Book/Node states independent and does not bubble status clicks into opening',async()=>{
 let snapshot:CollectionSnapshot={entries:[{target:{kind:'NODE',id:5},state:'WANT'}],bookMemberships:[{nodeId:12,bookIds:[5,6]}],nodeMemberships:[{nodeId:5,nodeIds:[5]}]};mocks.snapshot.mockImplementation(async()=>snapshot);mocks.set.mockImplementation(async(target,state)=>{snapshot={...snapshot,entries:[...snapshot.entries.filter(e=>e.target.kind!==target.kind||e.target.id!==target.id),...(state?[{target,state}]:[])]};});const open=vi.fn();
 function Probe(){const c=useCollection()!;return <output>{JSON.stringify([c.get({kind:'NODE',id:5}),c.get({kind:'BOOK',id:5}),c.matches([{kind:'BOOK',id:5}],'COMPLETED')])}</output>;}
 const view=mount(<CollectionStateProvider><div onClick={open}><CollectionStatusControl target={{kind:'NODE',id:5}}/><CollectionStatusControl target={{kind:'BOOK',id:5}}/></div><CollectionStatusFilter scope="all"/><Probe/></CollectionStateProvider>);
 await waitFor(()=>expect(view.container.querySelector('select')?.disabled).toBe(false));fireEvent.click(screen.getAllByRole('combobox')[1]);expect(open).not.toHaveBeenCalled();fireEvent.click(screen.getByRole('option',{name:'已完成'}));await waitFor(()=>expect(mocks.set).toHaveBeenCalledWith({kind:'BOOK',id:5},'COMPLETED'));await waitFor(()=>expect(view.container.querySelector('output')?.textContent).toBe('["WANT","COMPLETED",true]'));
 fireEvent.change(view.container.querySelectorAll('select')[1],{target:{value:''}});await waitFor(()=>expect(mocks.set).toHaveBeenLastCalledWith({kind:'BOOK',id:5},null));
});

it('offers retry after a collection snapshot error instead of leaving disabled controls',async()=>{
 mocks.snapshot.mockRejectedValueOnce(new Error('unavailable'));mount(<CollectionStateProvider><CollectionStatusControl target={{kind:'BOOK',id:5}}/></CollectionStateProvider>);fireEvent.click(await screen.findByRole('button',{name:'观看 / 阅读状态 · 重试'}));await waitFor(()=>expect(screen.getByRole('combobox').hasAttribute('disabled')).toBe(false));
});

it('flushes pending settings before export and restores only after explicit confirmation',async()=>{
 mocks.save.mockResolvedValue('X:/Backup.m2backup');mocks.open.mockResolvedValue('X:/Backup.m2backup');mocks.export.mockResolvedValue(undefined);mocks.prepare.mockResolvedValue({token:'opaque',createdAt:'2026-10-22',roots:2,offlineRoots:1,manualCovers:1});mocks.apply.mockResolvedValue(undefined);
 mount(<CollectionBackupSettings/>);fireEvent.click(screen.getByRole('button',{name:'备份收藏数据'}));await screen.findByText('备份已保存');expect(mocks.flush.mock.invocationCallOrder[0]).toBeLessThan(mocks.export.mock.invocationCallOrder[0]);
 fireEvent.click(screen.getByRole('button',{name:'从备份恢复'}));const dialog=await screen.findByRole('dialog');expect(dialog.textContent).toContain('替换本机当前收藏数据');expect(dialog.textContent).toContain('1 个资源库路径不可用');expect(mocks.apply).not.toHaveBeenCalled();fireEvent.click(screen.getByRole('button',{name:'恢复并重启'}));await waitFor(()=>expect(mocks.apply).toHaveBeenCalledWith('opaque'));
});
it('discards a cancelled restore and reports bounded localized failure copy',async()=>{
 mocks.open.mockResolvedValue('X:/Backup.m2backup');mocks.prepare.mockResolvedValue({token:'opaque',roots:1,offlineRoots:0,manualCovers:0});mocks.discard.mockResolvedValue(undefined);mount(<CollectionBackupSettings/>);fireEvent.click(screen.getByRole('button',{name:'从备份恢复'}));await screen.findByRole('dialog');fireEvent.click(screen.getByRole('button',{name:'取消'}));await waitFor(()=>expect(mocks.discard).toHaveBeenCalledWith('opaque'));expect(mocks.apply).not.toHaveBeenCalled();
 mocks.prepare.mockRejectedValue('BACKUP_SCHEMA private details');fireEvent.click(screen.getByRole('button',{name:'从备份恢复'}));await screen.findByText('此备份的数据库结构不受当前版本支持。');expect(screen.queryByText(/private details/)).toBeNull();
});
it('provides all state/recent/backup messages in four locales',()=>{
 for(const locale of ['en-US','ja-JP','ko-KR'] as const){expect(Object.keys(collectionMessages[locale]).sort()).toEqual(Object.keys(collectionMessages['zh-CN']).sort());expect(Object.keys(backupMessages[locale]).sort()).toEqual(Object.keys(backupMessages['zh-CN']).sort());expect(Object.values(backupMessages[locale]).every(s=>s.trim())).toBe(true);}
});

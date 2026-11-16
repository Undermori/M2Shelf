// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor,within} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {WorkDetailPage, type WorkDetailPageProps} from './WorkDetailPage';
import type {MediaNode} from '../types/media';
import type {ResourceFile, MediaFile} from '../types/media';
const calls=vi.hoisted(()=>({open:vi.fn(),clear:vi.fn(),retry:vi.fn(),snapshot:undefined as ReturnType<typeof vi.fn>|undefined}));
vi.mock('../lib/api',()=>({api:{tmdbOpenPage:calls.open,tmdbClear:calls.clear,tmdbRetryCover:calls.retry,get episodeWatchSnapshot(){return calls.snapshot;}}}));
vi.mock('../hooks/useCoverDataUrl',()=>({useCoverDataUrl:()=>({coverFrameRef:()=>{},coverCacheKey:'',coverUrl:null,coverFailed:false,coverLoading:false})}));
const node:MediaNode={id:84,libraryRootId:3,mediaKind:'LIVE_ACTION',parentNodeId:1,absolutePath:'X:/Fixture/Movie (2020).mkv',folderName:'Movie (2020).mkv',displayName:'Movie',nodeType:'WORK',manualTypeOverride:false,coverSource:'PLACEHOLDER',coverCachePath:null,directVideoCount:1,totalVideoCount:1,childMediaBranchCount:0,createdAt:'2026-01-01',updatedAt:'2026-01-01',lastSeenAt:'2026-01-01',userTags:[],tmdbBinding:{movie:{id:22,title:'Long Movie Title',originalTitle:'Long Movie Title',overview:'',posterPath:null,releaseDate:'2020-01-01'},active:true,coverCachePath:null,coverError:null}};
function props():WorkDetailPageProps{return {detail:{node,binding:null,mediaFiles:[],resourceFiles:[],children:[],breadcrumbs:[]},loading:false,rootLabel:'Fixture',coverRevision:0,onRoot:vi.fn(),onBreadcrumb:vi.fn(),onBack:vi.fn(),onBangumi:vi.fn(),onOpenBangumi:vi.fn(),onRetryCover:vi.fn(),onRetryCoverNode:vi.fn(),onClearBangumi:vi.fn(),onReveal:vi.fn(),onPlay:vi.fn(),onRevealMedia:vi.fn(),onOpenResource:vi.fn(),onRevealResource:vi.fn(),onOpenChild:vi.fn(),onBangumiNode:vi.fn(),onMenu:vi.fn()};}
beforeEach(()=>{vi.resetAllMocks();calls.snapshot=undefined;calls.open.mockResolvedValue(undefined);calls.clear.mockResolvedValue(undefined);calls.retry.mockResolvedValue(undefined);});
afterEach(cleanup);
it('lifts mixed and resource-only SP content without leaking wrappers or counting assets as videos',()=>{
 const p=props();
 const shell={...node,id:91,folderName:'SPs',displayName:'SPs',absolutePath:'X:/Fixture/SPs',nodeType:'CONTAINER' as const,tmdbBinding:null,directVideoCount:0,totalVideoCount:0};
 const child={...shell,id:92,folderName:'Scans',displayName:'Scans',absolutePath:'X:/Fixture/SPs/Scans'};
 const asset={id:21,nodeId:91,fileName:'Commentary.mka',absolutePath:'X:/Fixture/SPs/Commentary.mka',extension:'.mka',resourceType:'AUDIO',fileSize:10} as ResourceFile;
 p.detail={...p.detail!,children:[shell],resourceFiles:[asset],spFiles:[asset],spFolders:[child],expandedFolderIds:[91]};
 p.viewState={videoSection:'SPS'};
 const view=render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 expect(screen.getByRole('tab',{name:'SPs 2'}).getAttribute('aria-selected')).toBe('true');
 expect(screen.queryByText('其他资源',{selector:'h2'})).toBeNull();
 expect(screen.queryByText('SPs',{selector:'strong'})).toBeNull();
 expect(view.container.querySelector('.detail-stats strong')?.textContent).toBe('0');
 expect(view.container.querySelector('.video-tabs-heading > small')).toBeNull();
 fireEvent.click(screen.getByText('Scans',{selector:'strong'}).closest('button')!);expect(p.onOpenChild).toHaveBeenCalledWith(child);
 fireEvent.doubleClick(screen.getByText('Commentary.mka').closest('button')!);expect(p.onOpenResource).toHaveBeenCalledWith(asset);
});
it('retains the confirmed detail while navigation is pending without reloading watch state',async()=>{
 const p=props();calls.snapshot=vi.fn().mockResolvedValue({nodeId:node.id,workView:false,snapshot:'a'.repeat(64),rows:[],watchedEpisodes:0,totalEpisodes:0,watchedFiles:0,totalFiles:0});
 const view=render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 await waitFor(()=>expect(calls.snapshot).toHaveBeenCalledOnce());
 const hero=view.container.querySelector('.detail-hero');
 view.rerender(<I18nProvider><WorkDetailPage {...p} loading/></I18nProvider>);
 expect(view.container.querySelector('.detail-hero')).toBe(hero);
 expect(view.container.querySelector('.detail-content')?.hasAttribute('inert')).toBe(true);
 expect(view.container.querySelector('[aria-busy="true"]')).toBeTruthy();
 expect(calls.snapshot).toHaveBeenCalledOnce();
 fireEvent.click(screen.getByRole('button',{name:'返回'}));expect(p.onBack).toHaveBeenCalledOnce();
});
it('uses the shared binding panel and calls actual TMDb node actions',async()=>{
 const p=props(),changed=vi.fn();window.addEventListener('m2shelf-metadata-changed',changed);
 render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 expect(document.querySelector('.metadata-binding .binding-logo')).toBeTruthy();
 expect(screen.getByText('已绑定 · #22')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'在 TMDb 打开 ↗'}));await waitFor(()=>expect(calls.open).toHaveBeenCalledWith(84));
 fireEvent.click(screen.getByRole('button',{name:'修改绑定'}));expect(p.onBangumi).toHaveBeenCalledOnce();
 fireEvent.click(screen.getByRole('button',{name:'清除 TMDB 绑定'}));await waitFor(()=>expect(calls.clear).toHaveBeenCalledWith(84));await waitFor(()=>expect(changed).toHaveBeenCalledOnce());
 expect(p.onClearBangumi).not.toHaveBeenCalled();window.removeEventListener('m2shelf-metadata-changed',changed);
});
it('keeps cover retry and shows a failed source-open action',async()=>{
 const p=props();p.detail={...p.detail!,node:{...node,tmdbBinding:{...node.tmdbBinding!,coverError:'SYNTHETIC_FAILURE'}}};
 calls.open.mockRejectedValue(new Error('synthetic'));
 render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'重新获取封面'}));await waitFor(()=>expect(calls.retry).toHaveBeenCalledWith(84));
 fireEvent.click(screen.getByRole('button',{name:'在 TMDb 打开 ↗'}));expect(await screen.findByRole('alert')).toBeTruthy();
});
it('keeps Bangumi callbacks in the same panel',()=>{
 const p=props();p.detail={...p.detail!,node:{...node,tmdbBinding:null},binding:{nodeId:84,provider:'BANGUMI',providerSubjectId:999,providerSubjectType:6,providerTitle:'Manual Bangumi',providerTitleCn:null,providerTitleEn:null,providerTitleJa:null,providerTitleKo:null,providerDate:null,providerImageUrl:null,boundAt:'2026-01-01',updatedAt:'2026-01-01',coverCachePath:null,coverDownloadError:null}};
 render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'在 Bangumi 打开 ↗'}));expect(p.onOpenBangumi).toHaveBeenCalledOnce();
 fireEvent.click(screen.getByRole('button',{name:'清除 Bangumi 绑定'}));expect(p.onClearBangumi).toHaveBeenCalledOnce();
 expect(document.querySelector('.metadata-binding .binding-actions')).toBeTruthy();
});
it('shows Series progress even when every video belongs to independent children',async()=>{
 const p=props();p.detail={...p.detail!,node:{...node,nodeType:'CONTAINER',totalVideoCount:2,tmdbBinding:null},mediaFiles:[],nestedMediaFiles:[]};calls.snapshot=vi.fn().mockResolvedValue({nodeId:node.id,workView:false,snapshot:'a'.repeat(64),rows:[{fileId:1,watched:true},{fileId:2,watched:false}],watchedEpisodes:1,totalEpisodes:2,watchedFiles:0,totalFiles:0,latestWatchedFileId:1,latestWatchedFileName:'First Story S01E01.mkv'});
 render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);expect(await screen.findByText(/已经看到：.*First Story/)).toBeTruthy();expect(document.querySelector('.detail-stats strong')?.textContent).toBe('2');expect(screen.getByRole('button',{name:'清空播放进度'})).toBeTruthy();
});

it('starts Main before SPs with sparse direct/nested section deviations',()=>{
 const p=props(),file=(id:number,name:string)=>({id,nodeId:84,fileName:name,absolutePath:`X:/Fixture/${name}`,extension:'mkv',fileSize:1}) as MediaFile;
 const special=file(1,'00 NCOP.mkv'),main=file(2,'Episode 01.mkv'),nested=file(3,'00 Disc main.mkv');
 p.detail={...p.detail!,mediaFiles:[special,main],nestedMediaFiles:[{file:nested,sourceNodeId:84,sourceName:'Work',relativeDirectory:'Disc 1'}],videoSections:[{fileId:1,section:'SPS'},{fileId:3,section:'MAIN'}]};
 const view=render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 fireEvent.click(view.container.querySelector('.detail-actions .button.primary')!);
 expect(p.onPlay).toHaveBeenCalledWith(main);
});

it.each(['VIDEO','ANIMATION','LIVE_ACTION'] as const)('puts all %s videos before readable attachments and counts each resource once', kind => {
 const p=props();
 const file=(id:number):MediaFile=>({id,nodeId:84,absolutePath:`X:/Fixture/Episode ${id}.mkv`,fileName:`Episode ${id}.mkv`,extension:'.mkv',fileSize:123,modifiedAt:'2026-01-01',lastSeenAt:'2026-01-01',durationMs:null,width:null,height:null,codec:null});
 const readme:ResourceFile={id:20,nodeId:84,absolutePath:'X:/Fixture/readme.txt',fileName:'readme.txt',extension:'.txt',resourceType:'DOCUMENT',fileSize:10,modifiedAt:'2026-01-01',lastSeenAt:'2026-01-01'};
 p.detail={...p.detail!,node:{...node,mediaKind:kind},mediaFiles:[file(1)],nestedMediaFiles:[{file:file(2),sourceNodeId:84,sourceName:'Movie',relativeDirectory:'Extras'}],resourceFiles:[readme,{...readme,id:21,fileName:'extras.zip',extension:'.zip',resourceType:'ARCHIVE'}]};
 render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 const sections=[...document.querySelectorAll('.content-section')];
 expect(sections[0].textContent).toContain('Episode 1.mkv');
 expect(screen.queryByText('Episode 2.mkv')).toBeNull();
 expect(sections[1].textContent).toContain('readme.txt');
 expect(sections[2].textContent).toContain('extras.zip');
 fireEvent.click(screen.getByRole('tab',{name:'其他视频 1'}));
 expect(screen.getByText('Episode 2.mkv')).toBeTruthy();
 expect(document.querySelector('.detail-stats')?.textContent).toContain('1 项可阅读内容');
 expect([...document.querySelectorAll('.detail-stats strong')].map(e=>e.textContent)).toEqual(['2','1','1','0']);
 fireEvent.doubleClick(screen.getByText('Episode 2.mkv').closest('.media-file-row')!);
 expect(p.onPlay).toHaveBeenCalledWith(file(2));
});

it('shows nested-only videos without a misleading empty video table',()=>{
 const p=props();const file={id:1,nodeId:84,fileName:'Only episode.mkv',absolutePath:'X:/Fixture/Only episode.mkv',extension:'.mkv',fileSize:1,modifiedAt:'2026-01-01',lastSeenAt:'2026-01-01'} as MediaFile;
 p.detail={...p.detail!,nestedMediaFiles:[{file,sourceNodeId:84,sourceName:'Movie',relativeDirectory:'Disc 1'}]};
 render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 expect(screen.getByText('Only episode.mkv')).toBeTruthy();
 expect(document.querySelector('.generic-empty')).toBeNull();
});

it('puts a video series child destinations before its readable attachments',()=>{
 const p=props();
 const attachment={id:20,nodeId:84,absolutePath:'X:/Fixture/readme.txt',fileName:'readme.txt',extension:'.txt',resourceType:'DOCUMENT',fileSize:10,modifiedAt:'2026-01-01',lastSeenAt:'2026-01-01'} as ResourceFile;
 p.detail={...p.detail!,node:{...node,nodeType:'CONTAINER'},children:[{...node,id:85,folderName:'Season 1',displayName:'Season 1',tmdbBinding:null}],resourceFiles:[attachment]};
 render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 const sections=[...document.querySelectorAll('.content-section')];
 expect(sections[0].querySelector('.media-card')).toBeTruthy();
 expect(sections[1].textContent).toContain('readme.txt');
});

it.each(['WORK','CONTAINER'] as const)('puts unwrapped CDs inner folders and files in the top tab for %s without duplicating SPs or CDs below',kind=>{
 const p=props(),work={...node,nodeType:kind,absolutePath:'X:/Fixture/Work',tmdbBinding:null};
 const cds={...node,id:90,parentNodeId:84,absolutePath:'X:/Fixture/Work/CDs',folderName:'CDs',displayName:'CDs',nodeType:'CONTAINER' as const,tmdbBinding:null};
 const ostAlbum={...cds,id:92,parentNodeId:90,absolutePath:'X:/Fixture/Work/CDs/OST Album',folderName:'OST Album',displayName:'OST Album'};
 const sps={...cds,id:91,absolutePath:'X:/Fixture/Work/SPs',folderName:'SPs',displayName:'SPs'};
 const track={id:31,nodeId:84,absolutePath:'X:/Fixture/Work/Theme.flac',fileName:'Theme.flac',extension:'.flac',resourceType:'AUDIO',fileSize:10,modifiedAt:'2026-01-01',lastSeenAt:'2026-01-01'} as ResourceFile;
 const booklet={...track,id:32,nodeId:90,absolutePath:'X:/Fixture/Work/CDs/booklet.pdf',fileName:'booklet.pdf',extension:'.pdf',resourceType:'DOCUMENT' as const};
 const subtitle={...track,id:33,absolutePath:'X:/Fixture/Work/episode.ass',fileName:'episode.ass',extension:'.ass',resourceType:'SUBTITLE' as const};
 p.detail={...p.detail!,node:work,children:[cds,sps],cdsFolders:[ostAlbum],cdsFiles:[booklet],resourceFiles:[track,subtitle],expandedFolderIds:[91]};
 render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 const music=screen.getByRole('tabpanel',{name:'CDs 3'}),others=screen.getByRole('heading',{name:'其他资源'}).closest('section')!;
 expect(screen.getByRole('tab',{name:'CDs 3'}).getAttribute('aria-selected')).toBe('true');
 expect(screen.queryByRole('region',{name:'CDs'})).toBeNull();
 expect(document.querySelector('.detail-cds-section')).toBeNull();
 expect(document.querySelector('.generic-empty')).toBeNull();
 expect(screen.queryByText('视频已在上方展示')).toBeNull();
 expect(document.querySelector('.detail-stats strong')?.textContent).toBe(kind==='WORK'?'0':'1');
 expect(within(music).getByText('Theme.flac')).toBeTruthy();
 expect(within(music).getByText('booklet.pdf')).toBeTruthy();
 expect(within(music).getByText('OST Album')).toBeTruthy();
 expect(within(music).queryByText('CDs')).toBeNull();
 expect(within(others).queryByText('Theme.flac')).toBeNull();
 expect(within(others).queryByText('booklet.pdf')).toBeNull();
 expect(within(others).queryByText('CDs')).toBeNull();
 expect(within(others).queryByText('SPs')).toBeNull();
 expect(within(others).getByText('episode.ass')).toBeTruthy();
 expect(screen.getAllByText('booklet.pdf')).toHaveLength(1);
 fireEvent.doubleClick(within(music).getByText('Theme.flac').closest('button')!);expect(p.onOpenResource).toHaveBeenCalledWith(track);
 fireEvent.click(within(music).getByRole('button',{name:'在资源管理器中显示 Theme.flac'}));expect(p.onRevealResource).toHaveBeenCalledWith(track);
 fireEvent.click(within(music).getByText('OST Album',{selector:'strong'}).closest('button')!);expect(p.onOpenChild).toHaveBeenCalledWith(ostAlbum);
 fireEvent.click(within(music).getByRole('button',{name:'OST Album 的更多操作'}));expect(p.onMenu).toHaveBeenCalled();
 fireEvent.contextMenu(within(music).getByText('OST Album',{selector:'strong'}).closest('article')!);expect(p.onMenu).toHaveBeenCalledTimes(2);
});

it('does not create an empty CDs section',()=>{
 render(<I18nProvider><WorkDetailPage {...props()}/></I18nProvider>);
 expect(screen.queryByRole('region',{name:'CDs'})).toBeNull();
 expect(screen.queryByRole('tab',{name:/CDs/})).toBeNull();
 expect(document.querySelector('.generic-empty')).toBeTruthy();
});

it('keeps an already opened CDs booklet in exactly one section under its resource identity',()=>{
 const p=props();const resource={id:35,nodeId:84,absolutePath:'X:/Fixture/Work/CDs/booklet.pdf',fileName:'booklet.pdf',extension:'.pdf',resourceType:'DOCUMENT',fileSize:10,modifiedAt:'2026-01-01',lastSeenAt:'2026-01-01'} as ResourceFile;
 p.detail={...p.detail!,node:{...node,absolutePath:'X:/Fixture/Work'},resourceFiles:[resource],comicBooks:[{id:100,sourcePath:'x:\\fixture\\work\\cds\\booklet.pdf'} as import('../types/comic').ComicBook]};
 render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 const booklet=screen.getByText('booklet.pdf');
 expect(booklet.closest('[role="tabpanel"]')).toBe(screen.getByRole('tabpanel',{name:'CDs 1'}));
 expect(document.querySelector('.detail-cds-section')).toBeNull();
 expect(document.querySelector('.comic-reading-section')).toBeNull();
 fireEvent.doubleClick(booklet.closest('button')!);expect(p.onOpenResource).toHaveBeenCalledWith(resource);
});

it('keeps unwrapped CDs album folder navigation available without showing an outer CDs wrapper or video empty state',()=>{
 const p=props(),outerCds={...node,id:90,absolutePath:'X:/Fixture/Work/CDs',folderName:'CDs',displayName:'CDs',nodeType:'CONTAINER' as const,totalVideoCount:0,tmdbBinding:null};
 const album={...outerCds,id:91,parentNodeId:90,absolutePath:'X:/Fixture/Work/CDs/Soundtrack Vol.1',folderName:'Soundtrack Vol.1',displayName:'Soundtrack Vol.1'};
 p.detail={...p.detail!,node:{...node,absolutePath:'X:/Fixture/Work',totalVideoCount:0,directVideoCount:0},children:[outerCds],cdsFolders:[album]};
 render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 const panel=screen.getByRole('tabpanel',{name:'CDs 1'});
 expect(screen.getAllByRole('tab')).toHaveLength(1);
 expect(document.querySelector('.generic-empty')).toBeNull();
 expect(document.querySelector('.detail-stats strong')?.textContent).toBe('0');
 expect(document.querySelector('.child-section')).toBeNull();
 expect(within(panel).queryByText('CDs')).toBeNull();
 fireEvent.click(within(panel).getByText('Soundtrack Vol.1',{selector:'strong'}).closest('button')!);expect(p.onOpenChild).toHaveBeenCalledWith(album);
});

it('integrates CDs music video and attachments in one top panel while the hero counts only videos',()=>{
 const p=props(),work={...node,absolutePath:'X:/Fixture/Work',tmdbBinding:null};
 const outerCds={...work,id:90,parentNodeId:84,absolutePath:'X:/Fixture/Work/CDs',folderName:'CDs',displayName:'CDs',nodeType:'CONTAINER' as const};
 const ostAlbum={...outerCds,id:91,parentNodeId:90,absolutePath:'X:/Fixture/Work/CDs/OST',folderName:'OST',displayName:'OST'};
 const music={id:41,nodeId:90,fileName:'Concert.mkv',absolutePath:'X:/Fixture/Work/CDs/Concert.mkv',extension:'.mkv',fileSize:10} as MediaFile;
 const main={...music,id:40,nodeId:84,fileName:'Episode.mkv',absolutePath:'X:/Fixture/Work/Episode.mkv'};
 const track={id:31,nodeId:84,absolutePath:'X:/Fixture/Work/Theme.flac',fileName:'Theme.flac',extension:'.flac',resourceType:'AUDIO',fileSize:10} as ResourceFile;
 const booklet={...track,id:32,nodeId:90,absolutePath:'X:/Fixture/Work/CDs/Booklet.pdf',fileName:'Booklet.pdf',extension:'.pdf',resourceType:'DOCUMENT' as const};
 p.detail={...p.detail!,node:work,mediaFiles:[main],nestedMediaFiles:[{file:music,sourceNodeId:84,sourceName:'Work',relativeDirectory:'CDs'}],videoSections:[{fileId:41,section:'CDS'}],children:[outerCds],cdsFolders:[ostAlbum],cdsFiles:[booklet],resourceFiles:[track],expandedFolderIds:[90]};
 render(<I18nProvider><WorkDetailPage {...p}/></I18nProvider>);
 expect(screen.getByRole('tab',{name:'正片 1'}).getAttribute('aria-selected')).toBe('true');
 expect(screen.queryByText('Theme.flac')).toBeNull();
 fireEvent.click(screen.getByRole('tab',{name:'CDs 4'}));
 const panel=screen.getByRole('tabpanel',{name:'CDs 4'});
 for(const name of ['Concert.mkv','Theme.flac','Booklet.pdf','OST'])expect(within(panel).getByText(name)).toBeTruthy();
 expect(within(panel).queryByText('CDs',{selector:'strong'})).toBeNull();
 expect(document.querySelector('.detail-stats strong')?.textContent).toBe('2');
 expect(document.querySelectorAll('.detail-video-section')).toHaveLength(1);
 expect(document.querySelector('.detail-cds-section')).toBeNull();
 expect(document.querySelector('.child-section')).toBeNull();
 expect(document.querySelector('.comic-reading-section')).toBeNull();
});

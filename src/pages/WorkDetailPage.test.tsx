// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
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
 expect(screen.getByText('TMDb 已绑定 · #22')).toBeTruthy();
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

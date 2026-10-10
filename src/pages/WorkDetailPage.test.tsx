// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {WorkDetailPage, type WorkDetailPageProps} from './WorkDetailPage';
import type {MediaNode} from '../types/media';
const calls=vi.hoisted(()=>({open:vi.fn(),clear:vi.fn(),retry:vi.fn()}));
vi.mock('../lib/api',()=>({api:{tmdbOpenPage:calls.open,tmdbClear:calls.clear,tmdbRetryCover:calls.retry}}));
vi.mock('../hooks/useCoverDataUrl',()=>({useCoverDataUrl:()=>({coverFrameRef:()=>{},coverCacheKey:'',coverUrl:null,coverFailed:false,coverLoading:false})}));
const node:MediaNode={id:84,libraryRootId:3,mediaKind:'LIVE_ACTION',parentNodeId:1,absolutePath:'X:/Fixture/Movie (2020).mkv',folderName:'Movie (2020).mkv',displayName:'Movie',nodeType:'WORK',manualTypeOverride:false,coverSource:'PLACEHOLDER',coverCachePath:null,directVideoCount:1,totalVideoCount:1,childMediaBranchCount:0,createdAt:'2026-01-01',updatedAt:'2026-01-01',lastSeenAt:'2026-01-01',userTags:[],tmdbBinding:{movie:{id:22,title:'Long Movie Title',originalTitle:'Long Movie Title',overview:'',posterPath:null,releaseDate:'2020-01-01'},active:true,coverCachePath:null,coverError:null}};
function props():WorkDetailPageProps{return {detail:{node,binding:null,mediaFiles:[],resourceFiles:[],children:[],breadcrumbs:[]},loading:false,rootLabel:'Fixture',coverRevision:0,onRoot:vi.fn(),onBreadcrumb:vi.fn(),onBack:vi.fn(),onBangumi:vi.fn(),onOpenBangumi:vi.fn(),onRetryCover:vi.fn(),onRetryCoverNode:vi.fn(),onClearBangumi:vi.fn(),onReveal:vi.fn(),onPlay:vi.fn(),onRevealMedia:vi.fn(),onOpenResource:vi.fn(),onRevealResource:vi.fn(),onOpenChild:vi.fn(),onBangumiNode:vi.fn(),onMenu:vi.fn()};}
beforeEach(()=>{vi.resetAllMocks();calls.open.mockResolvedValue(undefined);calls.clear.mockResolvedValue(undefined);calls.retry.mockResolvedValue(undefined);});
afterEach(cleanup);
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

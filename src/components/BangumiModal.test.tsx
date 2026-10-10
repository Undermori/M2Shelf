// @vitest-environment jsdom
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {BangumiModal} from './BangumiModal';
import type {MediaNode} from '../types/media';
const mock=vi.hoisted(()=>({prefill:vi.fn(),bangumi:vi.fn(),bind:vi.fn(),status:vi.fn(),search:vi.fn(),tmdbBind:vi.fn(),cancel:vi.fn(),configure:vi.fn()}));
vi.mock('../lib/api',()=>({api:{bangumiPrefill:mock.prefill,searchBangumi:mock.bangumi,bindBangumi:mock.bind,tmdbStatus:mock.status,tmdbSearch:mock.search,tmdbBind:mock.tmdbBind,tmdbCancel:mock.cancel,tmdbConfigure:mock.configure},isStaleWorkError:()=>false,M2ShelfError:class extends Error{causeValue:unknown}}));
const node:MediaNode={id:84,libraryRootId:3,mediaKind:'LIVE_ACTION',parentNodeId:1,absolutePath:'X:/Fixture/Movie (2020).mkv',folderName:'Movie (2020).mkv',displayName:'Movie',nodeType:'WORK',manualTypeOverride:false,coverSource:'PLACEHOLDER',coverCachePath:null,directVideoCount:1,totalVideoCount:1,childMediaBranchCount:0,createdAt:'2026-01-01',updatedAt:'2026-01-01',lastSeenAt:'2026-01-01',userTags:[]};
beforeEach(()=>{vi.resetAllMocks();mock.prefill.mockResolvedValue({originalName:node.folderName,extractedName:'Movie',candidates:[]});mock.status.mockResolvedValue(true);mock.cancel.mockResolvedValue(undefined);mock.search.mockResolvedValue({snapshot:'exact-node-84',movies:[{id:22,title:'The Movie',originalTitle:'Original Movie',releaseDate:'2020-01-01',overview:'',posterPath:'/poster.jpg'}]});mock.tmdbBind.mockResolvedValue({});mock.bangumi.mockResolvedValue([{subjectId:22,title:'Bangumi Movie',titleCn:'Bangumi Movie',date:'2020',imageUrl:null}]);mock.bind.mockResolvedValue({});});
afterEach(cleanup);
function provider(value:string){fireEvent.click(screen.getByRole('combobox',{name:'数据来源'}));fireEvent.click(screen.getByRole('option',{name:value}));}
describe('one typed provider matching modal',()=>{
 it('sends the edited manual TMDb keyword without restoring the release filename',async()=>{
  render(<I18nProvider><BangumiModal node={node} onClose={vi.fn()} onBound={vi.fn()} onStale={vi.fn()}/></I18nProvider>);
  await screen.findByText('Movie (2020).mkv');provider('TMDb');await waitFor(()=>expect((screen.getByRole('button',{name:'搜索'}) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.change(screen.getByRole('textbox'),{target:{value:'My exact edited title 2046'}});fireEvent.click(screen.getByRole('button',{name:'搜索'}));
  await waitFor(()=>expect(mock.search).toHaveBeenCalledWith(84,'My exact edited title 2046',null,'zh-CN'));expect(mock.bangumi).not.toHaveBeenCalled();
 });
 it('preserves the original Bangumi subject object on direct selection',async()=>{
  const bound=vi.fn(),close=vi.fn();render(<I18nProvider><BangumiModal node={node} onClose={close} onBound={bound} onStale={vi.fn()}/></I18nProvider>);
  await screen.findByText('Movie (2020).mkv');fireEvent.click(screen.getByRole('button',{name:'搜索'}));await screen.findByText('Bangumi Movie');fireEvent.click(screen.getByRole('button',{name:'选择'}));await waitFor(()=>expect(close).toHaveBeenCalled());expect(mock.bind).toHaveBeenCalledWith(84,expect.objectContaining({subjectId:22,title:'Bangumi Movie'}));expect(mock.tmdbBind).not.toHaveBeenCalled();expect(bound).toHaveBeenCalledOnce();
 });
 it('chooses a TMDb poster directly with the real file Node and search snapshot',async()=>{
  const changed=vi.fn().mockResolvedValue(undefined),close=vi.fn();render(<I18nProvider><BangumiModal node={node} onClose={close} onBound={vi.fn()} onStale={vi.fn()} onMetadataChanged={changed}/></I18nProvider>);
  await screen.findByText('Movie (2020).mkv');provider('TMDb');await screen.findByRole('heading',{name:'从 TMDb 选择作品'});
  await waitFor(()=>expect((screen.getByRole('button',{name:'搜索'}) as HTMLButtonElement).disabled).toBe(false));
  expect(screen.queryByText('已配置')).toBeNull();expect(screen.queryByRole('spinbutton')).toBeNull();expect(document.querySelector('.modal-footer')).toBeNull();
  fireEvent.click(screen.getByRole('button',{name:'搜索'}));await screen.findByText('The Movie');expect(document.querySelector('.bangumi-cover img')?.getAttribute('src')).toBe('https://image.tmdb.org/t/p/w185/poster.jpg');
  fireEvent.click(screen.getByRole('button',{name:'选择'}));await waitFor(()=>expect(close).toHaveBeenCalled());expect(mock.tmdbBind).toHaveBeenCalledWith(84,22,'zh-CN','exact-node-84');expect(mock.bind).not.toHaveBeenCalled();expect(changed).toHaveBeenCalledOnce();
 });
 it('clears old results and ignores an outstanding response after switching provider',async()=>{
  let finish!:(value:unknown)=>void;mock.bangumi.mockReturnValue(new Promise(resolve=>{finish=resolve;}));render(<I18nProvider><BangumiModal node={node} onClose={vi.fn()} onBound={vi.fn()} onStale={vi.fn()}/></I18nProvider>);
  await screen.findByText('Movie (2020).mkv');fireEvent.click(screen.getByRole('button',{name:'搜索'}));provider('TMDb');finish([{subjectId:99,title:'Obsolete provider result'}]);await waitFor(()=>expect(mock.status).toHaveBeenCalled());expect(screen.queryByText('Obsolete provider result')).toBeNull();expect(screen.getByRole('heading',{name:'从 TMDb 选择作品'})).toBeTruthy();
 });
 it('shows configuration only when credentials are missing and hides TMDb for animation',async()=>{
  mock.status.mockResolvedValue(false);const {rerender}=render(<I18nProvider><BangumiModal node={node} onClose={vi.fn()} onBound={vi.fn()} onStale={vi.fn()}/></I18nProvider>);await screen.findByText('Movie (2020).mkv');provider('TMDb');await screen.findByRole('button',{name:'配置 TMDB'});expect((screen.getByRole('button',{name:'搜索'}) as HTMLButtonElement).disabled).toBe(true);expect(mock.configure).not.toHaveBeenCalled();
  rerender(<I18nProvider><BangumiModal node={{...node,id:85,mediaKind:'ANIMATION'}} onClose={vi.fn()} onBound={vi.fn()} onStale={vi.fn()}/></I18nProvider>);await screen.findByRole('heading',{name:'从 Bangumi 选择作品'});expect(screen.queryByRole('combobox',{name:'数据来源'})).toBeNull();
 });
});

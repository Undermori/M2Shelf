// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {I18nProvider} from '../lib/i18n';
import {SearchPage} from './SearchPage';
import {OrganizePage} from './OrganizePage';
import {ScanChanges} from '../components/ScanChanges';
import type {SearchFilters,TidyIssue} from '../types/incremental';
import type {MediaNode} from '../types/media';
const calls=vi.hoisted(()=>({search:vi.fn(),extensions:vi.fn(),issues:vi.fn(),remind:vi.fn(),poster:vi.fn()}));
vi.mock('../lib/api',()=>({desktopAvailable:true,api:{search:calls.search,searchExtensions:calls.extensions,organizeIssues:calls.issues,setIssueReminder:calls.remind,posterCacheStatus:calls.poster}}));
vi.mock('../hooks/useCoverDataUrl',()=>({useCoverDataUrl:()=>({coverFrameRef:()=>{},coverCacheKey:'fixture',coverUrl:null,coverFailed:false})}));
vi.mock('../hooks/usePosterViewportLifecycle',()=>({usePosterViewportLifecycle:()=>({coverRequested:false,coverVisible:false})}));
beforeEach(()=>{vi.resetAllMocks();calls.search.mockResolvedValue([]);calls.extensions.mockResolvedValue(['PDF','MKV']);calls.issues.mockResolvedValue([]);calls.remind.mockResolvedValue(undefined);calls.poster.mockResolvedValue({phase:"COMPLETED"});});afterEach(cleanup);
const node:MediaNode={userTags:[],id:1,libraryRootId:2,parentNodeId:3,absolutePath:'X:/Synthetic/Work',folderName:'Work',displayName:'Work',nodeType:'WORK',manualTypeOverride:false,coverSource:'PLACEHOLDER',coverCachePath:null,directVideoCount:1,totalVideoCount:1,childMediaBranchCount:0,createdAt:'2026-01-01',updatedAt:'2026-01-01',lastSeenAt:'2026-01-01'};

it('omits blank format values while keeping the all-formats option and valid choices',async()=>{
 calls.extensions.mockResolvedValue(['',' ','\t\r\n','MKV','PDF']);
 render(<I18nProvider><SearchPage onOpen={vi.fn()} onError={vi.fn()} coverRevision={0}/></I18nProvider>);
 await waitFor(()=>expect(calls.extensions).toHaveBeenCalled());
 const formats=screen.getByRole('combobox',{name:'包含文件格式'});
 await waitFor(()=>expect(formats.parentElement?.querySelectorAll('select option')).toHaveLength(3));
 fireEvent.click(formats);
 expect(screen.getAllByRole('option').map(option=>option.textContent?.trim())).toEqual(['包含文件格式✓','MKV','PDF']);
});

it('keeps empty-keyword search idle, combines SQL filters and exposes a clear action',async()=>{
 const change=vi.fn(),rootChange=vi.fn(),error=vi.fn();const renderPage=(filters:SearchFilters)=> <I18nProvider><SearchPage initialQuery="" filters={filters} onFiltersChange={change} onRootChange={rootChange} onOpen={vi.fn()} onError={error} coverRevision={0}/></I18nProvider>;
 const view=render(renderPage({extension:'PDF'}));await waitFor(()=>expect(calls.extensions).toHaveBeenCalled());expect(calls.search).not.toHaveBeenCalled();
 view.rerender(<I18nProvider><SearchPage initialQuery="Book" rootId={2} filters={{extension:'PDF',binding:'unbound',mediaKind:'EBOOK'}} onFiltersChange={change} onRootChange={rootChange} onOpen={vi.fn()} onError={error} coverRevision={0}/></I18nProvider>);
 await waitFor(()=>expect(calls.search).toHaveBeenLastCalledWith('Book',2,{extension:'PDF',binding:'unbound',mediaKind:'EBOOK'}));
 fireEvent.click(screen.getByRole('button',{name:'清空筛选'}));expect(change).toHaveBeenCalledWith({});expect(rootChange).toHaveBeenCalledWith(null);expect(screen.getByRole('textbox',{name:'搜索'})).toHaveProperty('value','Book');expect(error).not.toHaveBeenCalled();
});
it('replaces the cover dropdown with collection status and passes it to native search',async()=>{
 const change=vi.fn();const p={onOpen:vi.fn(),onError:vi.fn(),coverRevision:0,onFiltersChange:change,initialQuery:'Show'};
 const view=render(<I18nProvider><SearchPage {...p}/></I18nProvider>);
 expect(screen.queryByRole('combobox',{name:'全部封面状态'})).toBeNull();const status=screen.getByRole('combobox',{name:'按观看 / 阅读状态筛选'});
 fireEvent.click(status);fireEvent.click(screen.getByRole('option',{name:'想看 / 想读'}));expect(change).toHaveBeenCalledWith({collectionStatus:'WANT'});
 view.rerender(<I18nProvider><SearchPage {...p} filters={{collectionStatus:'WANT',binding:'bound'}}/></I18nProvider>);
 await waitFor(()=>expect(calls.search).toHaveBeenLastCalledWith('Show',undefined,{collectionStatus:'WANT',binding:'bound'}));
});
it('clears the keyword and pending results without clearing search filters', async () => {
  let resolveSearch!: (rows: unknown[]) => void;
  calls.search.mockImplementationOnce(() => new Promise(resolve => { resolveSearch = resolve; }));
  const onQueryChange = vi.fn();
  const onFiltersChange = vi.fn();
  const onRootChange = vi.fn();
  render(<I18nProvider><SearchPage initialQuery="Book" rootId={2} filters={{extension: 'PDF'}} onQueryChange={onQueryChange} onFiltersChange={onFiltersChange} onRootChange={onRootChange} onOpen={vi.fn()} onError={vi.fn()} coverRevision={0} /></I18nProvider>);
  await waitFor(() => expect(calls.search).toHaveBeenCalledWith('Book', 2, {extension: 'PDF'}));
  const input = screen.getByRole('textbox', {name: '搜索'});
  fireEvent.click(screen.getByRole('button', {name: '清空'}));
  expect(input).toHaveProperty('value', '');
  expect(document.activeElement).toBe(input);
  expect(onQueryChange).toHaveBeenCalledWith('');
  expect(onFiltersChange).not.toHaveBeenCalled();
  expect(onRootChange).not.toHaveBeenCalled();
  await act(async () => resolveSearch([{kind: 'NODE', node}]));
  expect(screen.queryByText('Work')).toBeNull();
  expect(screen.getByText('输入关键词开始搜索')).toBeTruthy();
});
it('rejects older asynchronous search completions and supports the scan-added view without a keyword',async()=>{
 let resolve!:(value:unknown)=>void;calls.search.mockImplementationOnce(()=>new Promise(r=>{resolve=r;})).mockResolvedValue([]);
 const props={onOpen:vi.fn(),onError:vi.fn(),coverRevision:0};const view=render(<I18nProvider><SearchPage {...props} initialQuery="old"/></I18nProvider>);
 await waitFor(()=>expect(calls.search).toHaveBeenCalled());view.rerender(<I18nProvider><SearchPage {...props} initialQuery="" filters={{scanRootId:2}}/></I18nProvider>);
 await waitFor(()=>expect(calls.search).toHaveBeenLastCalledWith('',undefined,{scanRootId:2}));
 await act(async()=>resolve([{kind:'NODE',node}]));expect(screen.queryByText('Work')).toBeNull();expect(screen.getByText('本次新增节点')).toBeTruthy();
});
it('shows node units and unconfirmed removal on an incomplete scan',()=>{
 const added=vi.fn();render(<I18nProvider><ScanChanges changes={{scanId:'test',scope:[],added:3,updated:1,unchanged:4,removed:null,errors:1,complete:false,addedNodeIds:[1,2,3]}} onAdded={added}/></I18nProvider>);
 expect(screen.getByText('变化单位：节点 · 扫描未完整完成')).toBeTruthy();expect(screen.getByText('未确认')).toBeTruthy();fireEvent.click(screen.getByRole('button',{name:'查看新增'}));expect(added).toHaveBeenCalledOnce();
});
it('persists only the selected issue reminder and refreshes resolved rows',async()=>{
 const issue:TidyIssue={node,kind:'match',reason:'multiple-candidates',ignored:false};calls.issues.mockResolvedValueOnce([issue]).mockResolvedValue([]);
 render(<I18nProvider><OrganizePage roots={[]} revision={0} busy={false} onOpen={vi.fn()} onMatch={vi.fn()} onRetryMatch={vi.fn()} onRetryCover={vi.fn()} onReveal={vi.fn()} onMenu={vi.fn()} onError={vi.fn()}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'忽略提醒'})).toBeTruthy());fireEvent.click(screen.getByRole('button',{name:'忽略提醒'}));
 await waitFor(()=>expect(calls.remind).toHaveBeenCalledWith(1,'match',true));await waitFor(()=>expect(screen.getByText('没有待处理的问题')).toBeTruthy());expect(calls.issues).toHaveBeenCalledTimes(2);
});

it('refreshes after the existing background cover retry finishes',async()=>{
 const cachedNode:MediaNode={...node,coverSource:'MANUAL',coverCachePath:'X:/SyntheticCache/retained.png'};const issue:TidyIssue={node:cachedNode,kind:'cover',reason:'cover',ignored:false};calls.issues.mockResolvedValueOnce([issue]).mockResolvedValueOnce([issue]).mockResolvedValue([]);
 const retry=vi.fn().mockResolvedValue(undefined);
 render(<I18nProvider><OrganizePage roots={[]} revision={0} busy={false} onOpen={vi.fn()} onMatch={vi.fn()} onRetryMatch={vi.fn()} onRetryCover={retry} onReveal={vi.fn()} onMenu={vi.fn()} onError={vi.fn()}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'重试封面'})).toBeTruthy());fireEvent.click(screen.getByRole('button',{name:'重试封面'}));
 await waitFor(()=>expect(retry).toHaveBeenCalledWith(cachedNode));await waitFor(()=>expect(calls.poster).toHaveBeenCalled());await waitFor(()=>expect(screen.getByText('没有待处理的问题')).toBeTruthy());
});

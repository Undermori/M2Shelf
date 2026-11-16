// @vitest-environment jsdom
import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {SmartMixedBrowser} from './SmartMixedBrowser';
import {I18nProvider} from '../lib/i18n';
import type {LibraryRoot,MediaNode} from '../types/media';
import type {ComicBook} from '../types/comic';
import type {BookCatalogue} from '../types/catalogue';

const mocks=vi.hoisted(()=>({catalogue:vi.fn()}));
vi.mock('../lib/api',()=>({api:{getBookCatalogue:mocks.catalogue}}));
vi.mock('../lib/collectionState',()=>({useCollection:()=>null}));
vi.mock('./MediaCard',()=>({MediaCard:({node,onOpen}:{node:MediaNode;onOpen:()=>void})=><button type="button" onClick={onOpen}>{node.displayName}</button>}));
vi.mock('./BookPosterCard',()=>({BookPosterCard:({title,onOpen}:{title:string;onOpen:()=>void})=><button type="button" onClick={onOpen}>{title}</button>}));
vi.mock('../pages/ComicDetailPage',()=>({ComicDetailPage:()=>null,LogicalBookDetail:({onBack,onRoot}:{onBack:()=>void;onRoot:()=>void})=><section><h2>Series detail</h2><button onClick={onBack}>Local back</button><button onClick={onRoot}>Library home</button></section>}));

beforeEach(()=>{vi.clearAllMocks();window.history.replaceState({m2ShelfNavigation:true,sequence:0},'');});
afterEach(cleanup);
const root={id:701,path:'X:/Synthetic/Books',displayName:'Books',mediaKind:'COMIC',recognitionMode:'FOLDER'} as LibraryRoot;
const catalogue:BookCatalogue={status:'READY',revision:1,directories:[{path:'Category',role:'CATEGORY'}],directoryNodes:[{id:702,libraryRootId:701,absolutePath:'X:/Synthetic/Books/Category',folderName:'Category',displayName:'Category',userTags:[]} as unknown as MediaNode],fallbackBooks:[],groups:[{id:'series-703',kind:'SERIES',title:'Series',relativePath:'Category/Series',coverNode:null,books:[{id:704,displayName:'Book one'},{id:705,displayName:'Book two'}] as ComicBook[]}]};
const props={root,sort:'title-asc' as const,filter:'',tagFilterId:null,viewMode:'grid' as const,coverRevision:0,onRead:vi.fn(),onPresentation:vi.fn(),onOpenNode:vi.fn(),onBangumi:vi.fn(),onRetryCover:vi.fn(),editMode:false,selectedNodeIds:new Set<number>(),onSelect:vi.fn(),fallback:null};

it('animates category and group drill-in, retains card instances during filtering and restores history scroll/direction',async()=>{
 mocks.catalogue.mockResolvedValue(catalogue);
 const view=render(<I18nProvider><div className="content-scroll"><SmartMixedBrowser {...props}/></div></I18nProvider>);
 const stage=()=>view.container.querySelector('.nav-stage-surface')!;
 await screen.findByRole('button',{name:'Category'});expect(stage().getAttribute('data-nav-active')).toBe('false');
 fireEvent.click(screen.getByRole('button',{name:'Category'}));expect(stage().getAttribute('data-nav-direction')).toBe('forward');expect(stage().getAttribute('data-nav-cycle')).toBe('a');
 const series=screen.getByRole('button',{name:'Series'}),categoryState=window.history.state;
 view.rerender(<I18nProvider><div className="content-scroll"><SmartMixedBrowser {...props} filter="Ser" sort="title-desc"/></div></I18nProvider>);
 expect(screen.getByRole('button',{name:'Series'})).toBe(series);expect(stage().getAttribute('data-nav-cycle')).toBe('a');
 fireEvent.click(series);expect(screen.getByRole('heading',{name:'Series detail'})).toBeTruthy();expect(stage().getAttribute('data-nav-direction')).toBe('forward');expect(stage().getAttribute('data-nav-cycle')).toBe('b');
 const groupState=window.history.state;
 await act(async()=>{window.history.replaceState({...categoryState,smartMixed:{...categoryState.smartMixed,scrollTop:86}},'');window.dispatchEvent(new PopStateEvent('popstate',{state:window.history.state}));});
 expect(screen.getByRole('button',{name:'Series'})).toBeTruthy();expect(stage().getAttribute('data-nav-direction')).toBe('backward');expect(view.container.querySelector('.content-scroll')?.scrollTop).toBe(86);
 await act(async()=>{window.history.replaceState(groupState,'');window.dispatchEvent(new PopStateEvent('popstate',{state:groupState}));});
 expect(screen.getByRole('heading',{name:'Series detail'})).toBeTruthy();expect(stage().getAttribute('data-nav-direction')).toBe('forward');
});

it('uses backward direction for logical home and leaves filtering outside the navigation identity',async()=>{
 const alternateRoot={...root,id:711};mocks.catalogue.mockResolvedValue(catalogue);
 window.history.replaceState({m2ShelfNavigation:true,sequence:0,smartMixed:{root:711,destination:{category:'Category',group:'series-703'},depth:2,scrollTop:0}},'');
 const view=render(<I18nProvider><SmartMixedBrowser {...props} root={alternateRoot}/></I18nProvider>);
 await screen.findByRole('heading',{name:'Series detail'});fireEvent.click(screen.getByRole('button',{name:'Library home'}));
 await waitFor(()=>expect(view.container.querySelector('.nav-stage-surface')?.getAttribute('data-nav-direction')).toBe('backward'));
 expect(screen.getByRole('button',{name:'Category'})).toBeTruthy();expect(window.history.state.smartMixed.destination).toEqual({category:'',group:null});
});

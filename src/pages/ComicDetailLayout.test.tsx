// @vitest-environment jsdom
import {afterEach,describe,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen} from '@testing-library/react';
import {ComicDetailPage,LogicalBookDetail} from './ComicDetailPage';
import {I18nProvider} from '../lib/i18n';
import type {CollectionTarget} from '../types/collection';
import type {ComicBook} from '../types/comic';
import type {LibraryRoot,MediaNode,NodeDetail} from '../types/media';
import type {WorkDetailPageProps} from './WorkDetailPage';

vi.mock('../lib/api',()=>({api:{revealComicBook:vi.fn()}}));
vi.mock('../hooks/useCoverDataUrl',()=>({useCoverDataUrl:()=>({coverUrl:null,coverLoading:false,coverFailed:false,coverCacheKey:''})}));
vi.mock('../components/BookPosterCard',()=>({BookPosterCover:()=>null}));
vi.mock('../lib/collectionState',()=>({CollectionStatusControl:({target}:{target:CollectionTarget})=><button data-status={`${target.kind}:${target.id}`}>State</button>}));
vi.mock('../components/CoverSelector',()=>({CoverButton:({target}:{target:CollectionTarget})=><button data-cover={`${target.kind}:${target.id}`}>Cover</button>}));
vi.mock('../components/PrivateNote',()=>({PrivateNote:({target}:{target:CollectionTarget})=><details className="private-note" data-note={`${target.kind}:${target.id}`}><summary>Note</summary><textarea aria-label={`Note ${target.id}`}/></details>}));

const book=(id:number):ComicBook=>({id,nodeId:21,revision:'fixture',sourceKind:'ZIP_ARCHIVE',documentFormat:'EPUB',displayName:`Book ${id}`,sourcePath:`X:/Fixture/${id}.epub`,pageCount:5,modifiedAt:'2026-01-01',indexError:null,progress:null});
const node:MediaNode={id:21,folderName:'Books',displayName:'Books',absolutePath:'X:/Fixture/Books',libraryRootId:1,parentNodeId:1,mediaKind:'EBOOK',nodeType:'WORK',manualTypeOverride:false,totalComicBookCount:1,directVideoCount:0,childMediaBranchCount:0,totalVideoCount:0,coverSource:'PLACEHOLDER',coverCachePath:null,createdAt:'2026-01-01',updatedAt:'2026-01-01',lastSeenAt:'2026-01-01',userTags:[]};
const props=(books:ComicBook[]):WorkDetailPageProps=>({detail:{node,comicBooks:books,children:[],resourceFiles:[],mediaFiles:[],binding:null,breadcrumbs:[]} satisfies NodeDetail,loading:false,rootLabel:'Books',coverRevision:0,onRoot:vi.fn(),onBack:vi.fn(),onBreadcrumb:vi.fn(),onBangumi:vi.fn(),onOpenBangumi:vi.fn(),onRetryCover:vi.fn(),onRetryCoverNode:vi.fn(),onClearBangumi:vi.fn(),onReveal:vi.fn(),onMenu:vi.fn(),onReadComic:vi.fn(),onPlay:vi.fn(),onRevealMedia:vi.fn(),onOpenChild:vi.fn(),onBangumiNode:vi.fn(),onOpenResource:vi.fn(),onRevealResource:vi.fn()});
afterEach(cleanup);

describe('book details follow the video detail layout without changing Book identities',()=>{
 it('places single-book state and cover in the hero, notes below it, and keeps file actions compact',()=>{
  const actions=props([book(7)]);
  const view=render(<I18nProvider><ComicDetailPage {...actions}/></I18nProvider>);
  expect(document.querySelector('.detail-actions [data-status]')?.getAttribute('data-status')).toBe('BOOK:7');
  expect(document.querySelector('.detail-actions [data-cover]')?.getAttribute('data-cover')).toBe('BOOK:7');
  expect(document.querySelector('.detail-hero + .detail-utility .private-note')?.getAttribute('data-note')).toBe('BOOK:7');
  expect(document.querySelector('.comic-book-row [data-status],.comic-book-row [data-cover],.comic-book-row .private-note')).toBeNull();
  expect(document.querySelectorAll('.comic-book-row .file-actions button')).toHaveLength(2);
  expect(document.querySelector('.binding-panel.metadata-binding')).not.toBeNull();
  fireEvent.doubleClick(document.querySelector('.comic-book-row')!);
  expect(actions.onReadComic).toHaveBeenCalledWith(book(7),[book(7)]);
  view.rerender(<I18nProvider><ComicDetailPage {...props([book(9)])}/></I18nProvider>);
  expect(document.querySelectorAll('[data-note]')).toHaveLength(1);
  expect(document.querySelector('[data-note]')?.getAttribute('data-note')).toBe('BOOK:9');
 });
 it('edits the real multi-book work once and keeps volume rows compact',()=>{
  const actions=props([book(7),book(9)]);
  render(<I18nProvider><ComicDetailPage {...actions}/></I18nProvider>);
  expect(document.querySelector('.detail-actions [data-status]')?.getAttribute('data-status')).toBe('NODE:21');
  expect(document.querySelector('.detail-actions [data-cover]')?.getAttribute('data-cover')).toBe('NODE:21');
  expect(document.querySelector('.detail-hero + .detail-utility .private-note')?.getAttribute('data-note')).toBe('NODE:21');
  const rows=document.querySelectorAll('.comic-book-row');
  for(const row of rows)expect(row.querySelector('.book-row-controls,[data-status],[data-cover],[data-note]')).toBeNull();
  fireEvent.doubleClick(screen.getByLabelText('Note 21'));
  expect(actions.onReadComic).not.toHaveBeenCalled();
  fireEvent.keyDown(rows[1],{key:'Enter'});
  expect(actions.onReadComic).toHaveBeenCalledWith(book(9),[book(7),book(9)]);
 });
 it('uses the same single-book layout for an unanchored logical group without inventing a Node target',()=>{
  const root={id:1,displayName:'Books'} as LibraryRoot;
  render(<I18nProvider><LogicalBookDetail group={{id:'fixture',title:'Book series',kind:'WORK',relativePath:'',books:[book(7)],coverNode:null}} root={root} revision={0} onBack={vi.fn()} onRoot={vi.fn()} onRead={vi.fn()} onMenu={vi.fn()}/></I18nProvider>);
  expect(document.querySelector('.detail-actions [data-status]')?.getAttribute('data-status')).toBe('BOOK:7');
  expect(document.querySelector('.detail-actions [data-cover]')?.getAttribute('data-cover')).toBe('BOOK:7');
  expect(document.querySelector('.detail-hero + .detail-utility .private-note')?.getAttribute('data-note')).toBe('BOOK:7');
  expect(document.querySelectorAll('[data-status],[data-cover],[data-note]')).toHaveLength(3);
 });
 it('gives a real series its own metadata even if only one book is currently visible',()=>{
  const value=props([book(7)]);value.detail={...value.detail!,node:{...node,nodeType:'CONTAINER'}};
  render(<I18nProvider><ComicDetailPage {...value}/></I18nProvider>);
  expect(document.querySelector('[data-status]')?.getAttribute('data-status')).toBe('NODE:21');expect(document.querySelector('[data-note]')?.getAttribute('data-note')).toBe('NODE:21');
 });
 it('requires a real book source for an unanchored multi-book series instead of editing the first implicitly',()=>{
  render(<I18nProvider><LogicalBookDetail group={{id:'unanchored',title:'Series',kind:'SERIES',relativePath:'',books:[book(7),book(9)],coverNode:null}} root={{id:1,displayName:'Books'} as LibraryRoot} revision={0} onBack={vi.fn()} onRoot={vi.fn()} onRead={vi.fn()} onMenu={vi.fn()}/></I18nProvider>);
  expect(document.querySelector('[data-note],[data-status],[data-cover]')).toBeNull();const select=screen.getByRole('combobox',{name:'书籍'});fireEvent.click(select);fireEvent.click(screen.getByRole('option',{name:'Book 9'}));
  expect(document.querySelector('[data-note]')?.getAttribute('data-note')).toBe('BOOK:9');expect(document.querySelector('.comic-book-row [data-note]')).toBeNull();
 });
});

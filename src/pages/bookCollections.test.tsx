// @vitest-environment jsdom
import {render,screen,fireEvent,cleanup} from '@testing-library/react';
import {afterEach,describe,it,expect,vi} from 'vitest';
import {I18nProvider} from '../lib/i18n';
import {SearchPage} from './SearchPage';
import {RecentlyWatchedPage} from './RecentlyWatchedPage';
import type {MediaNode,SearchHit} from '../types/media';
import type {ComicBook} from '../types/comic';
const mocks=vi.hoisted(()=>({search:vi.fn()}));
vi.mock('../lib/api',()=>({api:{search:mocks.search}}));
vi.mock('../hooks/useCoverDataUrl',()=>({useCoverDataUrl:()=>({coverUrl:null,coverCacheKey:'',coverFrameRef:{current:null}})}));
vi.mock('../hooks/usePosterViewportLifecycle',()=>({usePosterViewportLifecycle:()=>({coverRequested:false,coverVisible:false})}));
const node:MediaNode={id:100,libraryRootId:1,parentNodeId:null,mediaKind:'EBOOK',absolutePath:'R:/Books',folderName:'books',displayName:'books',nodeType:'WORK',manualTypeOverride:false,coverSource:'PLACEHOLDER',coverCachePath:null,directVideoCount:0,totalVideoCount:0,directComicBookCount:4,totalComicBookCount:4,childMediaBranchCount:0,createdAt:'2026-10-01',updatedAt:'2026-10-01',lastSeenAt:'2026-10-01',userTags:[]};
const books:ComicBook[]=Array.from({length:4},(_,i)=>({id:i+1,nodeId:100,displayName:`Book ${i+1}.epub`,sourcePath:`R:/Books/Book ${i+1}.epub`,revision:'rev',sourceKind:'ZIP_ARCHIVE',documentFormat:'EPUB',pageCount:1,modifiedAt:'2026-10-01',indexError:null,progress:null}));
afterEach(()=>{cleanup();vi.clearAllMocks();});
describe('book identity in search and recent',()=>{
 it('search renders and opens each matching book sharing a hidden Root',async()=>{
  const hits:SearchHit[]=books.map(comicBook=>({kind:'COMIC_BOOK',comicBook,node}));mocks.search.mockResolvedValue(hits);const open=vi.fn();
  render(<I18nProvider><SearchPage initialQuery="Book" onOpen={open} onError={vi.fn()} coverRevision={0}/></I18nProvider>);
  fireEvent.click((await screen.findByText('Book 3.epub')).closest('button')!);
  expect(document.querySelectorAll('.search-hit')).toHaveLength(4);expect(open).toHaveBeenCalledWith(hits[2]);expect(screen.queryByText('books')).toBeNull();
 });
 it('recent retains four books, their source actions and a separate video record',()=>{
  const read=vi.fn(),reveal=vi.fn();const video={...node,id:200,parentNodeId:20,mediaKind:'ANIMATION' as const,displayName:'Film',totalComicBookCount:0,totalVideoCount:1};
  render(<I18nProvider><RecentlyWatchedPage entries={[...books.map(comicBook=>({node,comicBook,comicBookId:comicBook.id,watchedAt:'2026-10-01'})),{node:video,watchedAt:'2026-10-01'}]} loading={false} viewMode="grid" onViewMode={vi.fn()} onOpenNode={vi.fn()} onMenu={vi.fn()} onBangumi={vi.fn()} onRetryCover={vi.fn()} coverRevision={0} onReadBook={read} onRevealBook={reveal}/></I18nProvider>);
  expect(document.querySelectorAll('.media-card')).toHaveLength(5);expect(screen.queryByText('books')).toBeNull();
  fireEvent.click(screen.getByText('Book 2.epub').closest('button')!);expect(read).toHaveBeenCalledWith(books[1]);
  fireEvent.contextMenu(screen.getByText('Book 4.epub'));fireEvent.click(screen.getByRole('menuitem'));expect(reveal).toHaveBeenCalledWith(books[3]);
  expect(screen.queryByRole('menu')).toBeNull();
 });
});

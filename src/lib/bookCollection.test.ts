import {describe,it,expect} from 'vitest';
import {allCollectionEntries,bookCollectionEntries} from './bookCollection';
import type {LibraryRoot,MediaNode,AllResourcesResult} from '../types/media';
import type {BookCatalogue} from '../types/catalogue';
import type {ComicBook} from '../types/comic';
const book=(id:number,name:string):ComicBook=>({id,nodeId:100,sourcePath:`R:\\Books\\${name}`,displayName:name,revision:'rev',sourceKind:'ZIP_ARCHIVE',pageCount:1,modifiedAt:'2026-10-01',indexError:null,progress:null});
const node=(id:number,parent:number|null=100):MediaNode=>({id,parentNodeId:parent,libraryRootId:1,absolutePath:'R:/Books/Series',folderName:'Series',displayName:'Series',nodeType:'WORK',manualTypeOverride:false,coverSource:'PLACEHOLDER',coverCachePath:null,directVideoCount:0,totalVideoCount:0,childMediaBranchCount:0,createdAt:'2026-10-01',updatedAt:'2026-10-01',lastSeenAt:'2026-10-01',userTags:[]});
const root:LibraryRoot={id:1,path:'R:/Books',displayName:'books',createdAt:'2026-10-01',lastScanAt:null,recognitionMode:'FOLDER',mediaKind:'EBOOK',bookOrganizationStrategy:'SMART_MIXED'};
const catalogue:BookCatalogue={status:'READY',revision:1,directories:[],fallbackBooks:[],groups:[1,2,3,4].map(id=>({id:`work-${id}`,title:`Book ${id}`,kind:'WORK',relativePath:`Book ${id}.epub`,books:[book(id,`Book ${id}.epub`)],coverNode:null}))};
describe('cross-library book collection',()=>{
 for(const mediaKind of ['COMIC','EBOOK','DOUJIN','ARTBOOK'] as const)for(const grouping of ['folders','works'] as const){
  it(`${mediaKind} ${grouping} replaces the physical shared parent without merging books`,()=>{
   const data:AllResourcesResult={nodes:[node(100,null),node(2)],comicNodes:[node(100,null),node(2)],works:[],totalCount:1,bookLibraries:[{root:{...root,mediaKind},catalogue}]};
   for(const edit of [false,true]){const entries=allCollectionEntries(data,grouping,edit);expect(entries).toHaveLength(4);expect(entries.every(e=>!e.node)).toBe(true);expect(entries.map(e=>e.bookEntry?.group?.books[0].id)).toEqual([1,2,3,4]);}
  });
 }
 it('preserves categories and series, including fallback books and Windows path spelling',()=>{
  const cat={...catalogue,groups:[{...catalogue.groups[0],kind:'SERIES' as const,title:'A series',books:[book(1,'v1'),book(2,'v2')]},{...catalogue.groups[2],relativePath:'Author/Book 3.epub'}],directories:[{path:'Author',role:'CATEGORY'}],directoryNodes:[{...node(7),absolutePath:'r:\\books\\Author'}],fallbackBooks:[book(9,'Author/Fallback.pdf')]};
  expect(bookCollectionEntries(root,cat).map(e=>e.name)).toEqual(['Author','A series']);
  expect(bookCollectionEntries(root,cat,'Author').map(e=>e.group?.books[0].id)).toEqual([3,9]);
 });
 it('keeps loose LEGACY root books separate, physical folder/file nodes and different libraries unchanged',()=>{
  const loose={...catalogue,groups:[],fallbackBooks:[book(9,'loose.txt')]};
  const foreign={...node(3),libraryRootId:2};
  const data:AllResourcesResult={nodes:[node(100,null),node(2),foreign],comicNodes:[node(2),foreign],works:[],totalCount:3,bookLibraries:[{root:{...root,bookOrganizationStrategy:'LEGACY'},catalogue:loose},{root:{...root,id:3},catalogue}]};
  for(const grouping of ['folders','works'] as const){const entries=allCollectionEntries(data,grouping);expect(entries).toHaveLength(7);expect(entries.flatMap(e=>e.node?[e.node.id]:[])).toEqual([2,3]);expect(new Set(entries.map(e=>e.key)).size).toBe(7);}
 });
 it('does not merge matching titles across roots',()=>{const data={nodes:[],works:[],totalCount:8,bookLibraries:[{root,catalogue},{root:{...root,id:2},catalogue}]};expect(new Set(allCollectionEntries(data,'folders').map(e=>e.key)).size).toBe(8);});
});

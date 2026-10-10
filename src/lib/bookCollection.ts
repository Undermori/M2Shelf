import type {BookCatalogue, LogicalGroup} from '../types/catalogue';
import type {LibraryRoot, MediaNode, AllResourcesResult} from '../types/media';

export type BookDestination = {category:string;group:string|null};
export type BookCollectionEntry = {key:string;root:LibraryRoot;path:string;name:string;node?:MediaNode|null;group?:LogicalGroup};

/** Project existing organization relations, never fabricate a mutable Node for a loose book. */
export function bookCollectionEntries(root:LibraryRoot, catalogue:BookCatalogue, category=''):BookCollectionEntry[] {
 const base=root.path.replaceAll('\\','/').replace(/\/$/,'')+'/';
 const relative=(path:string|undefined,fallback:string)=>{const normalized=path?.replaceAll('\\','/');return normalized?.toLowerCase().startsWith(base.toLowerCase())?normalized.slice(base.length):fallback;};
 const groups=[...catalogue.groups,...catalogue.fallbackBooks.map(book=>({id:`fallback-book-${book.id}`,title:book.displayName,kind:'WORK' as const,relativePath:relative(book.sourcePath,book.displayName),books:[book],coverNode:null}))].filter(g=>g.books.length>0);
 const prefix=category?category+'/':'';
 const directories=catalogue.directories.filter(d=>['CATEGORY','AMBIGUOUS'].includes(d.role)&&d.path!==category&&d.path.startsWith(prefix)&&!d.path.slice(prefix.length).includes('/')&&groups.some(g=>g.relativePath.startsWith(d.path+'/')));
 const nodeAt=(path:string)=>catalogue.directoryNodes?.find(node=>node.absolutePath.replaceAll('\\','/').toLowerCase()===(base+path).toLowerCase());
 return [
  ...directories.map(d=>({key:`root:${root.id}:directory:${d.path}`,root,path:d.path,name:d.path.split('/').pop()!,node:nodeAt(d.path)})),
  ...groups.filter(g=>g.relativePath.startsWith(prefix)&&!directories.some(d=>g.relativePath.startsWith(d.path+'/'))).map(group=>({key:`root:${root.id}:group:${group.id}`,root,path:group.relativePath,name:group.title,node:group.coverNode,group})),
 ];
}

export function allCollectionEntries(data:AllResourcesResult|null, grouping:'folders'|'works', editMode=false) {
 const libraries=data?.bookLibraries??[];
 const smartRoots=new Set(libraries.filter(l=>l.root.bookOrganizationStrategy==='SMART_MIXED').map(l=>l.root.id));
 const physical=(grouping==='works'
  ? [...(data?.works??[]).flatMap(work=>editMode?work.sources:[{...work.node,workView:true,workTarget:work.target}]),...(data?.comicNodes??[])]
  : data?.nodes??[]).filter(node=>!smartRoots.has(node.libraryRootId)&&node.parentNodeId!==null);
 return [
  ...physical.map(node=>({key:`node:${node.id}`,node,bookEntry:undefined as BookCollectionEntry|undefined})),
  ...libraries.flatMap(l=>bookCollectionEntries(l.root,l.catalogue)).map(bookEntry=>({key:bookEntry.key,node:bookEntry.node??undefined,bookEntry})),
 ];
}

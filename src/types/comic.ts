import type { MediaNode } from "./media";
export interface ComicReaderSettings {direction: 'RTL'|'LTR'; layout: 'SINGLE'|'DOUBLE'; mode: 'PAGED'|'SCROLL'|'WEBTOON'; widePageAlone:boolean;}
export const defaultComicReaderSettings:ComicReaderSettings={direction:'LTR',layout:'DOUBLE',mode:'PAGED',widePageAlone:true};

export interface ComicProgress { comicBookId: number; lastPageIndex: number; lastReadAt: string; }
export interface ComicBook {
  sourceSize?:number;
  sourcePath?:string;
  documentFormat?: 'PDF'|'EPUB'|null;
  revision:string;
  id: number; nodeId: number; sourceKind: "IMAGE_FOLDER" | "ZIP_ARCHIVE";
  displayName: string; pageCount: number; modifiedAt: string;
  indexError: string | null; progress: ComicProgress | null;
}
export interface ComicPage { pageIndex: number; pageName: string; }
export interface ComicOpenResult { book: ComicBook; pages: ComicPage[]; bookmarks: number[]; }
export interface RecentActivityEntry {
  node: MediaNode; kind: "VIDEO_WATCH" | "COMIC_READ"; occurredAt: string;
  comicBookId: number | null; comicPageIndex: number | null;
}

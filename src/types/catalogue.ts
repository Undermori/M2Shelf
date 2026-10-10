import type {ComicBook} from './comic';
import type {MediaNode} from './media';
export interface LogicalGroup {id:string;title:string;kind:'WORK'|'SERIES';relativePath:string;books:ComicBook[];coverNode:MediaNode|null;}
export interface BookCatalogue {status:string;revision:number;groups:LogicalGroup[];directories:Array<{path:string;role:string}>;fallbackBooks:ComicBook[];directoryNodes?:MediaNode[];}
export type BookCorrection='CATEGORY'|'SERIES'|'ASSIGN'|'INDEPENDENT'|'AUTOMATIC';

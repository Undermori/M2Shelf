export interface SearchFilters {
  mediaKind?: 'ANIMATION'|'LIVE_ACTION'|'COMIC'|'EBOOK'|'DOUJIN'|'ARTBOOK';
  binding?: 'bound'|'unbound'; cover?: 'available'|'missing'; extension?: string; scanRootId?: number;
  collectionStatus?: 'UNSET'|import('./collection').CollectionStatus;
}
export interface TidyIssue { node: import('./media').MediaNode; kind: string; reason: string; ignored: boolean; }

export interface PdfCoverRequest {ticket:string|null;ready:boolean;bookId:number;revision:string;sourceSize:number;}

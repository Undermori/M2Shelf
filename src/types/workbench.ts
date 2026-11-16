import type {CollectionTarget} from './collection';
import type {LibraryMediaKind} from './media';
import type {WorkspaceSort} from './workspace';

export interface WorkNoteEntry {
  target: CollectionTarget; nodeId: number; title: string; libraryId: number;
  libraryName: string; mediaKind: LibraryMediaKind; body: string; revision: number; updatedAt: string | null;
}
export type WorkNoteSort = 'recent' | 'title';
export interface WorkbenchSortPreferences {
  filesSort: WorkspaceSort;
  filesDescending: boolean;
  gallerySort: ScreenshotSort;
  galleryDescending: boolean;
  notesSort: WorkNoteSort;
}
export type WorkbenchSortUpdate =
  | { scope: 'files'; sort: WorkspaceSort; descending: boolean }
  | { scope: 'gallery'; sort: ScreenshotSort; descending: boolean }
  | { scope: 'notes'; sort: WorkNoteSort };
export const DEFAULT_WORKBENCH_SORT_PREFERENCES: WorkbenchSortPreferences = {
  filesSort: 'NAME',
  filesDescending: false,
  gallerySort: 'MODIFIED',
  galleryDescending: true,
  notesSort: 'recent',
};
export interface WorkNoteQuery {notesOnly: boolean; rootId: number | null; mediaKind: string | null; query: string; offset: number; limit: number; sort?: 'recent' | 'title';}
export interface AutoMatchScreenshotsResult {matchedCount:number;matchedByPlayback:number;matchedByTitle:number;matchedIds:number[];limited?:boolean;}
export interface ScreenshotFolder {id: number; path: string; alias: string; note: string; target: CollectionTarget | null; nodeId?: number | null; createdAt: string; imageCount: number;autoMatch?:AutoMatchScreenshotsResult;}
export interface ScreenshotImage {
  id: number; folderId: number; fileName: string; relativePath: string; revision: string;
  mime: string; width: number; height: number; note: string; tags: string[]; starred: boolean; target: CollectionTarget | null; ownTarget?: CollectionTarget | null; targetTitle?:string|null; nodeId?: number | null;
}
export type ScreenshotSort = 'MODIFIED' | 'NAME' | 'SIZE';
export interface ScreenshotSession {folderId:string;query:string;target:CollectionTarget|null;stars:boolean;density:string;sort:ScreenshotSort;descending:boolean;tag:string|null;offset:number;}
export interface ScreenshotQuery {sort?: ScreenshotSort; descending?: boolean; folderId: number | null; query: string; target: CollectionTarget | null; starredOnly: boolean; offset: number; limit: number; tag?: string | null;}
export interface ScreenshotLocation {image: ScreenshotImage; offset: number;}
export type ScreenshotBatchAction = {kind:'star';starred:boolean} | {kind:'link';target:CollectionTarget|null} | {kind:'addTags'|'removeTags';tags:string[]};
export interface StudioPage<T> {items: T[]; total: number;}
export const targetKey = (target: CollectionTarget | null | undefined) => target ? `${target.kind}:${target.id}` : '';
export const parseTarget = (value: string): CollectionTarget | null => {
  const [kind, raw] = value.split(':'); const id = Number(raw);
  return (kind === 'NODE' || kind === 'BOOK') && Number.isSafeInteger(id) && id > 0 ? {kind, id} : null;
};

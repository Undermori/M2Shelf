import type {CollectionTarget} from './collection';

export type WorkspaceKind = 'FOLDER' | 'IMAGE' | 'PSD' | 'DOCUMENT' | 'SUBTITLE' | 'AUDIO' | 'VIDEO' | 'PROJECT' | 'ARCHIVE' | 'OTHER';
export type WorkspaceSort = 'NAME' | 'TYPE' | 'SIZE' | 'MODIFIED';
export interface WorkspaceFolder {id:number;path:string;alias:string;note:string;target:CollectionTarget|null;nodeId:number|null;createdAt:string;}
export interface WorkspaceEntry {relativePath:string;fileName:string;extension:string;fileSize:number;modifiedAt:string;kind:WorkspaceKind;canOpenDefault:boolean;revision:string;}
export interface WorkspaceBreadcrumb {name:string;relativePath:string;}
export interface WorkspaceListing {folderId:number;relativeDir:string;entries:WorkspaceEntry[];breadcrumbs:WorkspaceBreadcrumb[];total:number;truncated:boolean;}
export interface WorkspaceRequest {folderId:number;sequence:number;}
export interface WorkspaceLocation {folderId:number|null;relativeDir:string;}

export interface WorkbenchLocation { view:'notes'|'gallery'|'files'; workspace:WorkspaceLocation; gallery?:import('./workbench').ScreenshotSession; }

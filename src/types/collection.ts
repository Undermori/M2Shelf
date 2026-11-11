export type CollectionTarget={kind:'NODE';id:number}|{kind:'BOOK';id:number};
export type CollectionStatus='WANT'|'IN_PROGRESS'|'COMPLETED'|'PAUSED'|'DROPPED';
export interface CollectionSnapshot {entries:Array<{target:CollectionTarget;state:CollectionStatus}>;bookMemberships:Array<{nodeId:number;bookIds:number[]}>;nodeMemberships:Array<{nodeId:number;nodeIds:number[]}>;}
export interface BackupPreview { token:string;createdAt:string;roots:number;offlineRoots:number;manualCovers:number }

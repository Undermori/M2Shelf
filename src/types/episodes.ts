export interface EpisodeWatchRow {
  fileId:number;
  kind:'EPISODE'|'FILE';
  season:number|null;
  episodes:number[];
  watched:boolean;
  partiallyWatched:boolean;
}
export type MpvFailureReason='CONNECT_TIMEOUT'|'CONNECTION_LOST'|'RESPONSE_INVALID'|'STATE_SAVE_FAILED'|'PROCESS_MISMATCH';
export type MpvPlaybackEvent={sessionId:number}&({state:'STARTED'|'CONNECTED'|'ENDED'}|{state:'FILE'|'FINISHED';fileId:number;ownerNodeId:number}|{state:'UNAVAILABLE';reason:MpvFailureReason});
export interface EpisodeWatchSnapshot {
  nodeId:number;
  workView:boolean;
  snapshot:string;
  rows:EpisodeWatchRow[];
  watchedEpisodes:number;
  totalEpisodes:number;
  watchedFiles:number;
  totalFiles:number;
  unresolvedRecords:number;
  latestWatchedFileId?:number|null;
  latestWatchedFileName?:string|null;
}

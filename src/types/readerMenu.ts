/** References to the active reader's existing actions; no duplicated reader state or key events. */
export interface ReaderMenuActions {
 previous:()=>void; next:()=>void; fullscreen:()=>void; bookmark:()=>void;
 canPrevious:boolean; canNext:boolean; canBookmark:boolean;
}

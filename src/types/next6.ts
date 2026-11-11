export interface NoteResult{body:string;revision:number}
export interface CoverCandidate{url:string;language:string|null;width:number;height:number}
export interface RelocationPreview{token:string|null;oldPath:string;newPath:string;checked:number;missing:number;samples:string[];issues:string[];verifiedSamples:number;oldAvailable:boolean}

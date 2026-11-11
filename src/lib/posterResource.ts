/** Windows WebView raster protocol. These are indexed identities, never filesystem URLs. */
const session=crypto.randomUUID().replaceAll('-','');
let epoch=0;
let cacheDirectory:string|undefined;
const listeners=new Set<()=>void>();
const selections=new Map<string,number>();
export const posterResourceEpoch=()=>epoch;
export const subscribePosterResources=(listener:()=>void)=>{listeners.add(listener);return()=>{listeners.delete(listener);};};
export function invalidatePosterResources(){epoch++;listeners.forEach(listener=>listener());}
export function observePosterCacheDirectory(directory:string){
 if(cacheDirectory!==undefined&&cacheDirectory!==directory)invalidatePosterResources();cacheDirectory=directory;
}
export const posterSelectionRevision=(id:string)=>selections.get(id)??0;
export function markPosterNodesChanged(ids:number[]){
 for(const id of new Set(ids)){const key=String(id);selections.set(key,(selections.get(key)??0)+1);}
 if(ids.length)listeners.forEach(listener=>listener());
}
if(typeof window!=='undefined')window.addEventListener('m2shelf-cover-changed',event=>{
 const d=(event as CustomEvent<{target:{kind:string;id:number};nodeId?:number}>).detail;if(!d)return;
 const ids=[d.target.kind==='BOOK'?`book-${d.target.id}`:String(d.target.id),...(d.nodeId?[String(d.nodeId)]:[])];
 for(const id of new Set(ids))selections.set(id,(selections.get(id)??0)+1);
 listeners.forEach(listener=>listener());
});
// A cache key, not an authorization token. Session namespace prevents old persisted HTTP
// responses masking a manual book cover after restarting with unchanged reading revision.
export function posterVersion(value:string){
 let a=2166136261,b=5381;for(let i=0;i<value.length;i++){a=Math.imul(a^value.charCodeAt(i),16777619);b=Math.imul(b,33)^value.charCodeAt(i);}
 return session+(a>>>0).toString(16).padStart(8,'0')+(b>>>0).toString(16).padStart(8,'0');
}
export function posterResourceUrl(kind:'node'|'book',id:number,width:number,version:string,revision?:string){
 if(!Number.isSafeInteger(id)||id<=0||![256,384,512,768].includes(width))return null;
 if(kind==='book'&&!revision)return null;
 return `http://m2poster.localhost/${kind}/${id}/${width}?v=${posterVersion(version)}${kind==='book'?`&revision=${encodeURIComponent(revision!)}`:''}`;
}

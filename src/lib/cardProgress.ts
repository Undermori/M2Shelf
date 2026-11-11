import {useCallback,useSyncExternalStore} from 'react';
import {isTauri} from '@tauri-apps/api/core';
import {api} from './api';
import type {MediaNode} from '../types/media';
import type {ComicBook} from '../types/comic';
import type {CollectionStatus} from '../types/collection';
import type {CardProgress,CardProgressRequest} from '../types/cardProgress';

/** Native collection snapshots seed first paint. Legacy fixtures/refreshes keep one bounded fallback. */
export class CardProgressStore {
 private cache=new Map<string,CardProgress|null>();
 private active=new Map<string,{request:CardProgressRequest;count:number;value?:CardProgress|null;sequence?:number}>();
 private listeners=new Map<string,Set<()=>void>>();
 private failures=new Map<string,{attempts:number;retryAt:number}>();
 private stale=new Set<string>();
 private sequence=0;private versions=new Map<string,number>();
 private pending=new Set<string>();private timer:ReturnType<typeof setTimeout>|undefined;private running=false;private generation=0;
 constructor(private read:(requests:CardProgressRequest[])=>Promise<CardProgress[]>){}
 get=(key:string)=>{const value=this.active.get(key)?.value;return value===undefined?this.cache.get(key):value;};
 beginSnapshot=()=>({generation:this.generation,sequence:++this.sequence});
 isGenerationCurrent=(generation:number)=>generation===this.generation;
 seed(value:CardProgress,stamp:{generation:number;sequence:number}){
  this.commitSnapshot(progressKey(value.nodeId,value.workView),value,stamp);
 }
 private commitSnapshot(key:string,value:CardProgress|null,stamp:{generation:number;sequence:number}){
  if(stamp.generation!==this.generation||stamp.sequence<(this.active.get(key)?.sequence??this.versions.get(key)??0))return;
  this.versions.delete(key);this.versions.set(key,stamp.sequence);while(this.versions.size>4096)this.versions.delete(this.versions.keys().next().value!);const active=this.active.get(key);if(active)active.sequence=stamp.sequence;
  this.pending.delete(key);this.failures.delete(key);this.remember(key,value);
 }
 subscribe(key:string,request:CardProgressRequest,enabled:boolean,listener:()=>void,initial?:CardProgress,initialGeneration?:number){
  let listeners=this.listeners.get(key);if(!listeners)this.listeners.set(key,listeners=new Set());listeners.add(listener);
  if(enabled){const item=this.active.get(key);if(item)item.count++;else {this.active.set(key,{request,count:1,value:this.cache.get(key)??initial,sequence:this.versions.get(key)});if(this.failures.delete(key))this.pending.add(key);}if(initialGeneration!==undefined&&initialGeneration<this.generation)this.stale.add(key);this.enqueue(key);}
  return()=>{listeners?.delete(listener);if(!listeners?.size)this.listeners.delete(key);if(enabled){const item=this.active.get(key);if(item&&--item.count<=0){this.active.delete(key);this.pending.delete(key);}}};
 }
 private enqueue(key:string){if(this.get(key)===undefined||this.stale.has(key))this.pending.add(key);this.schedule();}
 private schedule(){
  if(this.running)return;if(this.timer)clearTimeout(this.timer);this.timer=undefined;
  let due=Infinity;for(const key of this.pending){const failure=this.failures.get(key);if(!this.active.has(key)||(failure?.attempts??0)>=3){this.pending.delete(key);continue;}due=Math.min(due,failure?.retryAt??Date.now());}
  if(Number.isFinite(due))this.timer=setTimeout(()=>{this.timer=undefined;void this.flush();},Math.max(0,due-Date.now()));
 }
 private remember(key:string,value:CardProgress|null){
  const previous=this.get(key);
  const unchanged=previous===value||!!previous&&!!value&&previous.nodeId===value.nodeId&&previous.workView===value.workView&&previous.watched===value.watched&&previous.total===value.total&&previous.started===value.started;
  const accepted=unchanged?previous!:value,active=this.active.get(key);if(active)active.value=accepted;
  this.stale.delete(key);this.cache.delete(key);this.cache.set(key,accepted);
  while(this.cache.size>4096){const oldest=this.cache.keys().next().value!;this.cache.delete(oldest);this.stale.delete(oldest);}
  if(!unchanged)this.listeners.get(key)?.forEach(f=>f());
 }
 update(value:CardProgress){const key=progressKey(value.nodeId,value.workView);this.generation++;this.pending.delete(key);this.failures.delete(key);this.remember(key,value);this.schedule();}
 private async flush(){
  if(this.running)return;const keys=[...this.pending].filter(k=>this.active.has(k)&&(this.failures.get(k)?.retryAt??0)<=Date.now()).slice(0,16);if(!keys.length){this.schedule();return;}for(const key of keys)this.pending.delete(key);
  this.running=true;const stamp=this.beginSnapshot(),generation=stamp.generation,requests=keys.map(k=>this.active.get(k)!.request);
  try{const rows=await this.read(requests);if(generation===this.generation)for(let i=0;i<keys.length;i++){const r=requests[i],value=rows.find(v=>v.nodeId===r.nodeId&&v.workView===r.workView)??null;this.commitSnapshot(keys[i],value,stamp);}}
  catch{if(generation===this.generation)for(const key of keys){const attempts=(this.failures.get(key)?.attempts??0)+1;this.failures.set(key,{attempts,retryAt:Date.now()+(attempts===1?500:2000)});if(attempts<3&&this.active.has(key))this.pending.add(key);while(this.failures.size>4096){const oldest=this.failures.keys().next().value!;this.failures.delete(oldest);this.pending.delete(oldest);}}}
  finally{this.running=false;if(generation!==this.generation)for(const key of this.active.keys())if(this.get(key)===undefined||this.stale.has(key))this.pending.add(key);this.schedule();}
 }
 invalidate=()=>{this.generation++;for(const key of [...this.cache.keys(),...this.active.keys()])this.stale.add(key);this.failures.clear();this.pending.clear();for(const [key,item] of this.active)if(item.count)this.pending.add(key);this.schedule();};
}
const progressKey=(nodeId:number,workView:boolean)=>`${nodeId}:${workView}`;
const store=new CardProgressStore(requests=>api.cardProgress(requests));
export const invalidateCardProgress=()=>store.invalidate();
export const updateCardProgress=(value:CardProgress)=>store.update(value);
export const beginCardProgressSnapshot=()=>store.beginSnapshot();
export const isCardProgressGenerationCurrent=(generation:number)=>store.isGenerationCurrent(generation);
/** Called before a native collection response reaches React; generation/sequence fence stale reads. */
export function seedCardProgressResponse(response:unknown,stamp:ReturnType<CardProgressStore['beginSnapshot']>){
 const visit=(value:unknown)=>{if(!value||typeof value!=='object')return;const object=value as {id?:number;cardProgress?:CardProgress;cardProgressGeneration?:number;episodeWatch?:unknown;episodeWatchGeneration?:number};const p=object.cardProgress;
  if(object.episodeWatch)object.episodeWatchGeneration=stamp.generation;
  if(p&&p.nodeId===object.id&&typeof p.workView==='boolean'&&typeof p.started==='boolean'&&Number.isSafeInteger(p.watched)&&Number.isSafeInteger(p.total)&&p.watched>=0&&p.total>=p.watched){object.cardProgressGeneration=stamp.generation;store.seed(p,stamp);}
  for(const nested of Object.values(value))if(nested&&typeof nested==='object')visit(nested);
 };visit(response);
}

if(typeof window!=='undefined')for(const name of ['m2shelf-progress-changed','m2shelf-playback-changed','m2shelf-metadata-changed'])window.addEventListener(name,invalidateCardProgress);
export function useCardProgress(node:MediaNode,enabled:boolean){
 const key=progressKey(node.id,node.workView===true);
 const nodeId=node.id,workView=node.workView===true;
 const subscribe=useCallback((listener:()=>void)=>store.subscribe(key,{nodeId,workView},enabled&&isTauri(),listener,node.cardProgress,node.cardProgressGeneration),[key,nodeId,workView,enabled,node.cardProgress,node.cardProgressGeneration]);
 const snapshot=useCallback(()=>{const value=store.get(key);return value===undefined&&node.cardProgress?.workView===workView?node.cardProgress:value;},[key,node.cardProgress,workView]);return useSyncExternalStore(subscribe,snapshot,()=>undefined);
}
export function bookCardProgress(books:ComicBook[]):CardProgress|undefined {
 if(!books.length)return;
 if(books.length===1){const book=books[0],watched=Math.min(book.pageCount,(book.progress?.lastPageIndex??-1)+1);return {nodeId:book.nodeId,workView:false,watched,total:book.pageCount,started:!!book.progress};}
 return {nodeId:books[0].nodeId,workView:false,watched:books.filter(b=>b.pageCount>0&&b.progress&&b.progress.lastPageIndex+1>=b.pageCount).length,total:books.length,started:books.some(b=>!!b.progress)};
}
type Copy=(key:'card.videoDone'|'card.bookDone'|'card.videoProgress'|'card.bookProgress'|'card.notStarted'|'state.IN_PROGRESS',values?:Record<string,string|number>)=>string;
export function cardProgressLabel(t:Copy,book:boolean,progress:CardProgress|null|undefined,state?:CollectionStatus){
 if(state==='COMPLETED')return t(book?'card.bookDone':'card.videoDone');
 if(progress&&progress.total>0){if(!progress.started)return t('card.notStarted');if(progress.watched>=progress.total)return t(book?'card.bookDone':'card.videoDone');return t(book?'card.bookProgress':'card.videoProgress',{seen:progress.watched,total:progress.total});}
 return state==='IN_PROGRESS'?t('state.IN_PROGRESS'):'';
}

import type {RecentlyWatchedEntry} from '../types/media';

export interface RecentActivitySnapshot {
 entries:RecentlyWatchedEntry[]|null;
 loading:boolean;
 fresh:boolean;
 revision:number;
}

/** The existing 30-entry presentation snapshot, kept outside navigation history.
 * No persistent state, progress inference or stale-while-revalidate navigation. */
export class RecentActivityStore {
 private state:RecentActivitySnapshot={entries:null,loading:false,fresh:false,revision:0};
 private epoch=0;
 private flight:{epoch:number;promise:Promise<RecentlyWatchedEntry[]>}|null=null;
 private listeners=new Set<()=>void>();
 constructor(private read:()=>Promise<RecentlyWatchedEntry[]>){}
 getSnapshot=()=>this.state;
 getRevision=()=>this.state.revision;
 subscribe=(listener:()=>void)=>{this.listeners.add(listener);return()=>{this.listeners.delete(listener);};};
 private publish(state:RecentActivitySnapshot){this.state=state;this.listeners.forEach(listener=>listener());}
 invalidate=()=>{this.epoch++;this.publish({...this.state,loading:false,fresh:false,revision:this.state.revision+1});};
 update=(change:(entries:RecentlyWatchedEntry[]|null)=>RecentlyWatchedEntry[]|null,fresh=false)=>{
  const entries=change(this.state.entries);
  this.epoch++;this.publish({...this.state,entries,loading:false,fresh:fresh&&entries!==null,revision:this.state.revision+1});
 };
 /** A successful mutation needs a new read; it cannot join a pre-mutation one. */
 refresh=()=>{this.invalidate();return this.load();};
 /** Ready snapshots need no navigation request; pending reads are shared. If a
  * mutation supersedes a read while navigation waits, join its newer read. */
 ensureFresh=async()=>{
  while(!this.state.fresh||this.state.entries===null)await this.load();
  return this.state.entries;
 };
 private load():Promise<RecentlyWatchedEntry[]> {
  if(this.state.fresh&&this.state.entries!==null)return Promise.resolve(this.state.entries);
  if(this.flight?.epoch===this.epoch)return this.flight.promise;
  const epoch=++this.epoch;
  this.publish({...this.state,loading:true});
  const promise=Promise.resolve().then(this.read).then(entries=>{
   if(epoch===this.epoch)this.publish({...this.state,entries,loading:false,fresh:true});
   return epoch===this.epoch?entries:this.state.entries??[];
  }).catch(error=>{
   if(epoch!==this.epoch)return this.state.entries??[];
   this.publish({...this.state,loading:false,fresh:false});throw error;
  }).finally(()=>{if(this.flight?.epoch===epoch)this.flight=null;});
  this.flight={epoch,promise};return promise;
 }
}

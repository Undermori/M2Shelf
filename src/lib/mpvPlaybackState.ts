import type {MpvPlaybackEvent} from '../types/episodes';

export interface MpvNoticeState {sessionId:number;phase:'STARTED'|'CONNECTED'|'FAILED'|'ENDED'}
export function mpvNoticeTransition(current:MpvNoticeState,event:MpvPlaybackEvent){
 const ignore={state:current,accept:false,dismiss:false,notify:false};
 if(!Number.isSafeInteger(event.sessionId)||event.sessionId<=0||event.sessionId<current.sessionId)return ignore;
 const newer=event.sessionId>current.sessionId;
 if(!newer&&(current.phase==='ENDED'||event.state==='STARTED'&&current.phase!=='STARTED'))return ignore;
 if(event.state==='UNAVAILABLE'){
  if(!newer&&current.phase==='FAILED')return ignore;
  return {state:{sessionId:event.sessionId,phase:'FAILED' as const},accept:true,dismiss:newer,notify:true};
 }
 const phase=event.state==='FILE'||event.state==='FINISHED'?'CONNECTED':event.state;
 return {state:{sessionId:event.sessionId,phase},accept:true,dismiss:newer||phase!=='STARTED',notify:false};
}

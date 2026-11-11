import {expect,it} from 'vitest';
import {mpvNoticeTransition,type MpvNoticeState} from './mpvPlaybackState';
const connected:MpvNoticeState={sessionId:2,phase:'CONNECTED'};
it('accepts EOF reconciliation before normal end and rejects older sessions',()=>{
 const done=mpvNoticeTransition(connected,{state:'FINISHED',sessionId:2,fileId:1,ownerNodeId:1});expect(done.accept).toBe(true);expect(done.notify).toBe(false);expect(done.state.phase).toBe('CONNECTED');
 expect(mpvNoticeTransition(connected,{state:'FINISHED',sessionId:1,fileId:1,ownerNodeId:1}).accept).toBe(false);
 const end=mpvNoticeTransition(done.state,{state:'ENDED',sessionId:2});expect(end.notify).toBe(false);expect(mpvNoticeTransition(end.state,{state:'FINISHED',sessionId:2,fileId:1,ownerNodeId:1}).accept).toBe(false);
});
it('ignores a late failure from an older playback session',()=>{
 const result=mpvNoticeTransition(connected,{state:'UNAVAILABLE',sessionId:1,reason:'CONNECTION_LOST'});expect(result.accept).toBe(false);expect(result.notify).toBe(false);expect(result.state).toBe(connected);
});
it('does not warn for normal start, connection and end',()=>{
 const start=mpvNoticeTransition({sessionId:0,phase:'ENDED'},{state:'STARTED',sessionId:1});const ready=mpvNoticeTransition(start.state,{state:'CONNECTED',sessionId:1});const end=mpvNoticeTransition(ready.state,{state:'ENDED',sessionId:1});expect([start,ready,end].every(result=>!result.notify)).toBe(true);expect(end.dismiss).toBe(true);
});
it('shows a real failure once and dismisses it on recovery',()=>{
 const failed=mpvNoticeTransition(connected,{state:'UNAVAILABLE',sessionId:2,reason:'CONNECT_TIMEOUT'});expect(failed.notify).toBe(true);expect(mpvNoticeTransition(failed.state,{state:'UNAVAILABLE',sessionId:2,reason:'CONNECT_TIMEOUT'}).notify).toBe(false);
 const recovered=mpvNoticeTransition(failed.state,{state:'CONNECTED',sessionId:2});expect(recovered.notify).toBe(false);expect(recovered.dismiss).toBe(true);
});
it('ignores a queued failure after normal end while allowing a new session',()=>{
 const ended=mpvNoticeTransition(connected,{state:'ENDED',sessionId:2});expect(mpvNoticeTransition(ended.state,{state:'UNAVAILABLE',sessionId:2,reason:'CONNECTION_LOST'}).accept).toBe(false);
 const start=mpvNoticeTransition(ended.state,{state:'STARTED',sessionId:3});expect(start.accept).toBe(true);expect(start.dismiss).toBe(true);expect(mpvNoticeTransition(start.state,{state:'UNAVAILABLE',sessionId:3,reason:'CONNECT_TIMEOUT'}).notify).toBe(true);
});

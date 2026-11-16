import {useEffect,useRef,type PointerEvent as ReactPointerEvent,type MouseEvent as ReactMouseEvent,type DragEvent as ReactDragEvent} from 'react';

/** Workbench-only gesture. Native code validates sources and permits COPY only. */
export function useNativeFileDrag<T>({active,disabled,onDrag}:{active:boolean;disabled:boolean;onDrag:(value:T)=>Promise<unknown>}) {
 const latest=useRef({active,disabled,onDrag});latest.current={active,disabled,onDrag};
 const cleanup=useRef<()=>void>(()=>{}),running=useRef(false),suppressUntil=useRef(0),mounted=useRef(true);
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;cleanup.current();};},[]);
 useEffect(()=>{if(!active||disabled)cleanup.current();},[active,disabled]);
 const stopClick=(event:ReactMouseEvent<HTMLElement>)=>{if(running.current||performance.now()<suppressUntil.current){event.preventDefault();event.stopPropagation();}};
 const bind=(value:T)=>({
  draggable:false,
  onDragStart:(event:ReactDragEvent<HTMLElement>)=>event.preventDefault(),
  onClickCapture:stopClick,
  onDoubleClickCapture:stopClick,
  onPointerDown:(event:ReactPointerEvent<HTMLElement>)=>{
   cleanup.current();
   if(!latest.current.active||latest.current.disabled||running.current||event.button!==0||event.isPrimary===false||(event.pointerType&&event.pointerType!=='mouse'))return;
   const target=event.target instanceof Element?event.target:null;
   const control=target?.closest('button,input,select,textarea,a,[role="checkbox"],[role="switch"],[contenteditable="true"]');
   if(control&&!control.hasAttribute('data-native-drag-surface'))return;
   const {clientX,clientY,pointerId}=event;
   const clear=()=>{window.removeEventListener('pointermove',move,true);window.removeEventListener('pointerup',clear,true);window.removeEventListener('pointercancel',clear,true);window.removeEventListener('blur',clear);cleanup.current=()=>{};};
   const move=(next:PointerEvent)=>{
    if(next.pointerId!==pointerId)return;
    if(!(next.buttons&1)||!latest.current.active||latest.current.disabled){clear();return;}
    if(Math.hypot(next.clientX-clientX,next.clientY-clientY)<7)return;
    next.preventDefault();clear();running.current=true;suppressUntil.current=performance.now()+800;
    // Remove listeners before entering OLE's modal message loop.
    void Promise.resolve().then(()=>mounted.current?latest.current.onDrag(value):undefined).finally(()=>{running.current=false;suppressUntil.current=performance.now()+350;});
   };
   cleanup.current=clear;
   window.addEventListener('pointermove',move,{capture:true,passive:false});window.addEventListener('pointerup',clear,true);window.addEventListener('pointercancel',clear,true);window.addEventListener('blur',clear);
  },
 });
 return {bind};
}

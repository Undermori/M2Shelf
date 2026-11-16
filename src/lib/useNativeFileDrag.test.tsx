// @vitest-environment jsdom
import {act,cleanup,fireEvent,render,screen} from '@testing-library/react';
import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {useNativeFileDrag} from './useNativeFileDrag';
class Pointer extends MouseEvent {pointerId=1;isPrimary=true;pointerType='mouse';}
function Surface({drag,click,active=true,disabled=false}:{drag:()=>Promise<unknown>;click:()=>void;active?:boolean;disabled?:boolean}){
 const gesture=useNativeFileDrag({active,disabled,onDrag:drag});
 return <article {...gesture.bind('fixture')}><button data-native-drag-surface onClick={click}>File</button><button>Action</button></article>;
}
beforeEach(()=>{vi.stubGlobal('PointerEvent',Pointer);});
afterEach(()=>{cleanup();vi.unstubAllGlobals();});
describe('workbench native file drag gesture',()=>{
 it('starts once at seven pixels and suppresses the trailing click',async()=>{
  const drag=vi.fn(async()=>{}),click=vi.fn();render(<Surface drag={drag} click={click}/>);
  fireEvent.pointerDown(screen.getByText('File'),{button:0,buttons:1,clientX:10,clientY:10});fireEvent.pointerMove(window,{buttons:1,clientX:16,clientY:10});expect(drag).not.toHaveBeenCalled();
  await act(async()=>{fireEvent.pointerMove(window,{buttons:1,clientX:17,clientY:10});fireEvent.pointerMove(window,{buttons:1,clientX:70,clientY:10});});
  expect(drag).toHaveBeenCalledExactlyOnceWith('fixture');fireEvent.click(screen.getByText('File'));expect(click).not.toHaveBeenCalled();
 });
 it('keeps internal actions ordinary and discards blur, release and inactive gestures',async()=>{
  const drag=vi.fn(async()=>{}),click=vi.fn();const view=render(<Surface drag={drag} click={click}/>);
  fireEvent.pointerDown(screen.getByText('Action'),{button:0,buttons:1});fireEvent.pointerMove(window,{buttons:1,clientX:20});
  fireEvent.pointerDown(screen.getByText('File'),{button:0,buttons:1});fireEvent.blur(window);fireEvent.pointerMove(window,{buttons:1,clientX:20});
  fireEvent.pointerDown(screen.getByText('File'),{button:0,buttons:1});fireEvent.pointerUp(window);fireEvent.pointerMove(window,{buttons:1,clientX:20});fireEvent.click(screen.getByText('File'));expect(click).toHaveBeenCalledOnce();
  fireEvent.pointerDown(screen.getByText('File'),{button:0,buttons:1});view.rerender(<Surface drag={drag} click={click} active={false}/>);fireEvent.pointerMove(window,{buttons:1,clientX:20});await act(async()=>{});expect(drag).not.toHaveBeenCalled();
 });
});

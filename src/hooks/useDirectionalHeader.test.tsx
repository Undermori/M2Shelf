// @vitest-environment jsdom
import {cleanup,render,fireEvent} from '@testing-library/react';
import {useRef} from 'react';
import {afterEach,describe,it,expect,vi} from 'vitest';
import {useDirectionalHeader} from './useDirectionalHeader';
function Surface({route='one',offset=0}:{route?:string;offset?:number}){const ref=useRef<HTMLDivElement>(null);useDirectionalHeader(ref,route,offset);return <div ref={ref} data-testid="scroll"/>;}
afterEach(()=>{cleanup();vi.restoreAllMocks();});
describe('real scrollport directional navigation',()=>{
 it('uses the known restore offset without reading layout during destination commit',()=>{
  const get=vi.spyOn(HTMLElement.prototype,'scrollTop','get');
  const view=render(<Surface offset={300}/>);expect(get).not.toHaveBeenCalled();
  const port=view.getByTestId('scroll');Object.defineProperty(port,'clientHeight',{value:800});
  port.scrollTop=300;fireEvent.scroll(port);expect(port.classList.contains('navigation-hidden')).toBe(false);
  port.scrollTop=330;fireEvent.scroll(port);expect(port.classList.contains('navigation-hidden')).toBe(true);get.mockRestore();
 });
 it('returns while scrolling up in the middle, tolerates jitter and resets per destination',()=>{const view=render(<Surface/>);const port=view.getByTestId('scroll');Object.defineProperty(port,'clientHeight',{value:800});const scroll=(top:number)=>{port.scrollTop=top;fireEvent.scroll(port);};scroll(300);expect(port.classList.contains('navigation-hidden')).toBe(true);scroll(297);expect(port.classList.contains('navigation-hidden')).toBe(true);scroll(282);expect(port.classList.contains('navigation-hidden')).toBe(false);scroll(310);expect(port.classList.contains('navigation-hidden')).toBe(true);view.rerender(<Surface route="two"/>);expect(port.classList.contains('navigation-hidden')).toBe(false);scroll(0);expect(port.classList.contains('navigation-hidden')).toBe(false);});
});

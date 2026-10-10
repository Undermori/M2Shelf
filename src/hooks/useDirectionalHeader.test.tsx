// @vitest-environment jsdom
import {render,fireEvent} from '@testing-library/react';
import {useRef} from 'react';
import {describe,it,expect} from 'vitest';
import {useDirectionalHeader} from './useDirectionalHeader';
function Surface({route='one'}:{route?:string}){const ref=useRef<HTMLDivElement>(null);useDirectionalHeader(ref,route);return <div ref={ref} data-testid="scroll"/>;}
describe('real scrollport directional navigation',()=>{
 it('returns while scrolling up in the middle, tolerates jitter and resets per destination',()=>{const view=render(<Surface/>);const port=view.getByTestId('scroll');Object.defineProperty(port,'clientHeight',{value:800});const scroll=(top:number)=>{port.scrollTop=top;fireEvent.scroll(port);};scroll(300);expect(port.classList.contains('navigation-hidden')).toBe(true);scroll(297);expect(port.classList.contains('navigation-hidden')).toBe(true);scroll(282);expect(port.classList.contains('navigation-hidden')).toBe(false);scroll(310);expect(port.classList.contains('navigation-hidden')).toBe(true);view.rerender(<Surface route="two"/>);expect(port.classList.contains('navigation-hidden')).toBe(false);scroll(0);expect(port.classList.contains('navigation-hidden')).toBe(false);});
});

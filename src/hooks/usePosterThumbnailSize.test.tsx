// @vitest-environment jsdom
import {StrictMode} from 'react';
import {act, cleanup, render, waitFor} from '@testing-library/react';
import {afterEach, expect, it, vi} from 'vitest';
import {posterThumbnailWidth, usePosterThumbnailSize} from './usePosterThumbnailSize';

afterEach(() => {cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals();});
it('selects the smallest sufficient physical-pixel tier and bounds DPR', () => {
  expect([1,1.25,1.5,2,3].map(dpr => posterThumbnailWidth(200,dpr))).toEqual([256,256,384,512,512]);
  expect(posterThumbnailWidth(256,1)).toBe(256);
  expect(posterThumbnailWidth(257,1)).toBe(384);
  expect(posterThumbnailWidth(384,2)).toBe(768);
  expect(posterThumbnailWidth(800,2)).toBe(768);
});
it('shares resize observation, tracks live size and DPI, and cleans up under StrictMode', async () => {
  const observed = new Set<Element>();
  let dispatch!: ResizeObserverCallback;
  const disconnect = vi.fn();
  vi.stubGlobal('ResizeObserver', class {
    constructor(callback: ResizeObserverCallback) {dispatch = callback;}
    observe(element: Element) {observed.add(element);}
    unobserve(element: Element) {observed.delete(element);}
    disconnect() {observed.clear(); disconnect();}
  });
  const measure=vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockReturnValue({width:200} as DOMRect);
  vi.stubGlobal('devicePixelRatio',1);
  function Cover() { const {tier, ref}=usePosterThumbnailSize(); return <div ref={ref}>{tier}</div>; }
  const rendered=render(<StrictMode><Cover/><Cover/></StrictMode>);
  await waitFor(() => expect(observed.size).toBe(2));
  expect(measure).not.toHaveBeenCalled();
  expect(rendered.container.textContent).toBe('512512');
  const targets=[...observed];
  act(()=>dispatch(targets.map(target=>({target,contentRect:{width:198},borderBoxSize:[{inlineSize:200}]} as unknown as ResizeObserverEntry)),{} as ResizeObserver));
  expect(rendered.container.textContent).toBe('256256');
  act(() => dispatch(targets.map(target => ({target,contentRect:{width:300}} as ResizeObserverEntry)),{} as ResizeObserver));
  expect(rendered.container.textContent).toBe('384384');
  vi.stubGlobal('devicePixelRatio',2);
  act(() => window.dispatchEvent(new Event('resize')));
  expect(rendered.container.textContent).toBe('768768');
  rendered.unmount(); expect(observed.size).toBe(0); expect(disconnect).toHaveBeenCalled();
});
it('batches a large mount without forcing layout and does not measure missing poster sources',()=>{
 const observed=new Set<Element>();let dispatch!:ResizeObserverCallback;
 vi.stubGlobal('ResizeObserver',class {constructor(callback:ResizeObserverCallback){dispatch=callback;}observe(e:Element){observed.add(e);}unobserve(e:Element){observed.delete(e);}disconnect(){observed.clear();}});
 const measure=vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockImplementation(()=>{throw Error('Forced synchronous layout');});
 function Cover({enabled}:{enabled:boolean}){const {ref,measured,tier}=usePosterThumbnailSize(enabled);return <div ref={ref} data-ready={measured}>{tier}</div>;}
 const view=render(<>{Array.from({length:400},(_,id)=><Cover key={id} enabled={id<200}/>)}</>);
 expect(observed.size).toBe(200);expect(measure).not.toHaveBeenCalled();
 act(()=>dispatch([...observed].map(target=>({target,contentRect:{width:180}} as ResizeObserverEntry)),{} as ResizeObserver));
 expect(view.container.querySelectorAll('[data-ready=true]')).toHaveLength(200);
 view.unmount();expect(observed.size).toBe(0);
});

it('measures a uniform poster grid once and publishes every tier before the first ResizeObserver delivery',()=>{
 vi.stubGlobal('ResizeObserver',class {observe(){}unobserve(){}disconnect(){}});
 vi.stubGlobal('devicePixelRatio',1);
 const measure=vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockReturnValue({width:200} as DOMRect);
 function Cover(){const {ref,measured,tier}=usePosterThumbnailSize();return <div ref={ref} data-ready={measured}>{tier}</div>;}
 const view=render(<div className="poster-grid poster-grid-grid">{Array.from({length:400},(_,id)=><Cover key={id}/>)}</div>);
 expect(measure).toHaveBeenCalledTimes(1);
 expect(view.container.querySelectorAll('[data-ready=true]')).toHaveLength(400);
 expect([...view.container.querySelectorAll('[data-ready]')].every(e=>e.textContent==='256')).toBe(true);
});
it('does not reuse one grid width for another grid or a later commit',async()=>{
 vi.stubGlobal('ResizeObserver',class {observe(){}unobserve(){}disconnect(){}});vi.stubGlobal('devicePixelRatio',1);
 const measure=vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockImplementation(function(this:HTMLElement){return {width:Number(this.dataset.width)} as DOMRect;});
 function Cover({width}:{width:number}){const {ref,tier}=usePosterThumbnailSize();return <div ref={ref} data-width={width}>{tier}</div>;}
 const view=render(<><div className="poster-grid poster-grid-grid"><Cover width={200}/></div><div className="poster-grid poster-grid-grid"><Cover width={400}/></div></>);
 expect(measure).toHaveBeenCalledTimes(2);expect(view.container.textContent).toBe('256512');
 await act(async()=>{await Promise.resolve();});
 view.rerender(<><div className="poster-grid poster-grid-grid"><Cover width={200}/><Cover width={300}/></div><div className="poster-grid poster-grid-grid"><Cover width={400}/></div></>);
 expect(measure).toHaveBeenCalledTimes(3);expect(view.container.textContent).toBe('256384512');
});

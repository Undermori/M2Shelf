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
  vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockReturnValue({width:200} as DOMRect);
  vi.stubGlobal('devicePixelRatio',1);
  function Cover() { const {tier, ref}=usePosterThumbnailSize(); return <div ref={ref}>{tier}</div>; }
  const rendered=render(<StrictMode><Cover/><Cover/></StrictMode>);
  await waitFor(() => expect(observed.size).toBe(2));
  expect(rendered.container.textContent).toBe('256256');
  const targets=[...observed];
  act(() => dispatch(targets.map(target => ({target,contentRect:{width:300}} as ResizeObserverEntry)),{} as ResizeObserver));
  expect(rendered.container.textContent).toBe('384384');
  vi.stubGlobal('devicePixelRatio',2);
  act(() => window.dispatchEvent(new Event('resize')));
  expect(rendered.container.textContent).toBe('768768');
  rendered.unmount(); expect(observed.size).toBe(0); expect(disconnect).toHaveBeenCalled();
});

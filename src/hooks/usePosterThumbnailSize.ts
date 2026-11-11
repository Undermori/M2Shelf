import {useCallback, useLayoutEffect, useRef, useState} from 'react';

export const POSTER_THUMBNAIL_WIDTHS = [256, 384, 512, 768] as const;
export function posterThumbnailWidth(cssWidth: number, dpr: number): number {
  const physicalWidth = Math.ceil(Math.max(1, cssWidth) * Math.min(2, Math.max(1, dpr || 1)));
  return POSTER_THUMBNAIL_WIDTHS.find(width => width >= physicalWidth) ?? 768;
}

type Listener = (tier: number) => void;
const targets = new Map<HTMLElement, {width: number; listener: Listener}>();
let observer: ResizeObserver | null = null;
let resolution: MediaQueryList | null = null;
let mountWidths=new WeakMap<HTMLElement,{signature:string;width:number}>();
let mountResetQueued=false;
function mountedGridWidth(element:HTMLElement):number|undefined {
  const grid=element.closest<HTMLElement>('.poster-grid');
  if(!grid)return;
  const signature=grid.className;
  let measured=mountWidths.get(grid);
  if(!measured||measured.signature!==signature){
    measured={signature,width:element.getBoundingClientRect().width};mountWidths.set(grid,measured);
  }
  if(!mountResetQueued){mountResetQueued=true;queueMicrotask(()=>{mountWidths=new WeakMap();mountResetQueued=false;});}
  return measured.width;
}
function updateResolution() {
  targets.forEach(target => {if(target.width>0)target.listener(posterThumbnailWidth(target.width, window.devicePixelRatio));});
  resolution?.removeEventListener('change', updateResolution);
  resolution = window.matchMedia?.(`(resolution: ${window.devicePixelRatio || 1}dppx)`) ?? null;
  resolution?.addEventListener('change', updateResolution);
}
function subscribe(element: HTMLElement, listener: Listener) {
  // Callback refs run during React's commit. Reading layout here for every newly
  // inserted card forces repeated synchronous layouts of the whole growing page.
  targets.set(element, {width: 0, listener});
  if (targets.size === 1) {
    observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(entries => {
      entries.forEach(entry => {
        const target = targets.get(entry.target as HTMLElement);
        if (!target) return;
        const width=entry.borderBoxSize?.[0]?.inlineSize??entry.contentRect.width;
        if(width<=0)return;
        target.width = width;
        target.listener(posterThumbnailWidth(target.width, window.devicePixelRatio));
      });
    });
    window.addEventListener('resize', updateResolution);
    updateResolution();
  }
  if(observer){
    observer.observe(element,{box:'border-box'});
    // Uniform grid tracks share one ACTUAL measurement within this commit. This
    // publishes every ready src before scroll restoration, while avoiding one
    // synchronous layout read per card. Never retain this width across commits.
    const width=mountedGridWidth(element);
    if(width&&width>0){targets.get(element)!.width=width;listener(posterThumbnailWidth(width,window.devicePixelRatio));}
  }
  else {
    // Compatibility only; current Windows WebView supplies ResizeObserver.
    const width=element.getBoundingClientRect().width;
    targets.get(element)!.width=width;
    if(width>0)listener(posterThumbnailWidth(width,window.devicePixelRatio));
  }
  return () => {
    observer?.unobserve(element);
    targets.delete(element);
    if (!targets.size) {
      observer?.disconnect(); observer = null;
      resolution?.removeEventListener('change', updateResolution); resolution = null;
      window.removeEventListener('resize', updateResolution);
    }
  };
}

export function usePosterThumbnailSize(enabled=true) {
  const [tier, setTier] = useState(512);
  const [measured,setMeasured]=useState(false);
  const currentTier = useRef(tier);
  const elementRef = useRef<HTMLElement | null>(null);
  const release = useRef<(() => void) | null>(null);
  const accept = useCallback((width: number) => {
    currentTier.current = width;
    setTier(previous => previous === width ? previous : width);
    setMeasured(true);
  }, []);
  const ref = useCallback((element: HTMLElement | null) => {
    elementRef.current = element;
    release.current?.(); release.current = null;
    if (element&&enabled) release.current = subscribe(element, accept);
  }, [accept,enabled]);
  useLayoutEffect(() => {
    if (enabled&&elementRef.current && !release.current) release.current = subscribe(elementRef.current, accept);
    return () => {release.current?.(); release.current = null;};
  }, [accept,enabled]);
  return {tier, measured, currentTier, ref};
}

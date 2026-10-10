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
function updateResolution() {
  targets.forEach(target => target.listener(posterThumbnailWidth(target.width, window.devicePixelRatio)));
  resolution?.removeEventListener('change', updateResolution);
  resolution = window.matchMedia?.(`(resolution: ${window.devicePixelRatio || 1}dppx)`) ?? null;
  resolution?.addEventListener('change', updateResolution);
}
function subscribe(element: HTMLElement, listener: Listener) {
  targets.set(element, {width: element.getBoundingClientRect().width, listener});
  if (targets.size === 1) {
    observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(entries => {
      entries.forEach(entry => {
        const target = targets.get(entry.target as HTMLElement);
        if (!target) return;
        target.width = entry.contentRect.width;
        target.listener(posterThumbnailWidth(target.width, window.devicePixelRatio));
      });
    });
    window.addEventListener('resize', updateResolution);
    updateResolution();
  }
  observer?.observe(element);
  listener(posterThumbnailWidth(targets.get(element)!.width, window.devicePixelRatio));
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

export function usePosterThumbnailSize() {
  const [tier, setTier] = useState(512);
  const currentTier = useRef(tier);
  const elementRef = useRef<HTMLElement | null>(null);
  const release = useRef<(() => void) | null>(null);
  const accept = useCallback((width: number) => {
    currentTier.current = width;
    setTier(previous => previous === width ? previous : width);
  }, []);
  const ref = useCallback((element: HTMLElement | null) => {
    elementRef.current = element;
    release.current?.(); release.current = null;
    if (element) release.current = subscribe(element, accept);
  }, [accept]);
  useLayoutEffect(() => {
    if (elementRef.current && !release.current) release.current = subscribe(elementRef.current, accept);
    return () => {release.current?.(); release.current = null;};
  }, [accept]);
  return {tier, currentTier, ref};
}

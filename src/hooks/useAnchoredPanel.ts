import {useLayoutEffect, useState, type RefObject} from 'react';
import {floatingPanelPosition} from '../lib/floatingPanel';

/** Re-measure only while the disclosure is open, including scrolls in nested viewports. */
export function useAnchoredPanel(open: boolean, trigger: RefObject<HTMLElement | null>, panel: RefObject<HTMLElement | null>, width = 280) {
  const [position, setPosition] = useState({left: 12, top: 12, width, maxHeight: 400});
  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      const anchor = trigger.current?.getBoundingClientRect(), element = panel.current;
      if (!anchor || !element) return;
      const next = floatingPanelPosition(anchor, {width, height: element.scrollHeight + 2 || 400}, {width: window.innerWidth, height: window.innerHeight}, 'end');
      setPosition(previous => Object.keys(next).every(key => previous[key as keyof typeof next] === next[key as keyof typeof next]) ? previous : next);
    };
    place();
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(place);
    if (trigger.current) observer?.observe(trigger.current);
    if (panel.current) observer?.observe(panel.current);
    window.addEventListener('resize', place);
    window.addEventListener('scroll', place, true);
    return () => {observer?.disconnect(); window.removeEventListener('resize', place); window.removeEventListener('scroll', place, true);};
  }, [open, panel, trigger, width]);
  return position;
}

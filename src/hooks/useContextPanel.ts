import {useLayoutEffect, useRef, useState, type RefObject} from 'react';

/** Pointer-positioned action group. Native buttons keep Tab/Enter/Space semantics. */
export function useContextPanel(panel: RefObject<HTMLDivElement | null>, x: number, y: number, onClose: () => void) {
  const [position, setPosition] = useState({left: x, top: y});
  const close = useRef(onClose);
  close.current = onClose;
  useLayoutEffect(() => {
    const element = panel.current;
    if (!element) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const buttons = () => [...element.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')];
    buttons()[0]?.focus();
    const pointer = (event: PointerEvent) => {if (!element.contains(event.target as Node)) close.current();};
    const key = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing) return;
      if (event.altKey || event.ctrlKey || event.metaKey || document.querySelector('dialog[open]')) return;
      if (event.key === 'Escape') {
        event.preventDefault();
        close.current();
      } else if (element.contains(event.target as Node) && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
        // Embedded selects own their arrow-key interaction.
        if (event.target instanceof Element && event.target.closest('[role="combobox"],select,input,textarea')) return;
        const items = buttons(), current = items.indexOf(document.activeElement as HTMLButtonElement);
        if (!items.length) return;
        const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (current + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
        if (next === current || !items[next]) return;
        event.preventDefault();
        items[next].focus();
        items[next].scrollIntoView?.({block: 'nearest', inline: 'nearest'});
      }
    };
    const focus = (event: FocusEvent) => {if (!element.contains(event.target as Node)) close.current();};
    window.addEventListener('pointerdown', pointer);
    window.addEventListener('keydown', key);
    window.addEventListener('focusin', focus);
    return () => {
      window.removeEventListener('pointerdown', pointer);
      window.removeEventListener('keydown', key);
      window.removeEventListener('focusin', focus);
      // Do not steal focus from a newly opened dialog or an outside control.
      if (previous?.isConnected && (element.contains(document.activeElement) || document.activeElement === document.body)) previous.focus({preventScroll: true});
    };
  }, [panel]);
  useLayoutEffect(() => {
    const element = panel.current;
    if (!element) return;
    const place = () => {
      const bounds = element.getBoundingClientRect(), gutter = 8;
      const next = {left: Math.max(gutter, Math.min(x, window.innerWidth - bounds.width - gutter)), top: Math.max(gutter, Math.min(y, window.innerHeight - bounds.height - gutter))};
      setPosition(current => current.left === next.left && current.top === next.top ? current : next);
    };
    place();
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(place);
    observer?.observe(element);
    window.addEventListener('resize', place);
    window.addEventListener('scroll', place, true);
    return () => {observer?.disconnect(); window.removeEventListener('resize', place); window.removeEventListener('scroll', place, true);};
  }, [panel, x, y]);
  return position;
}

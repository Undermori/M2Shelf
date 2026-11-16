import {Children, isValidElement, useEffect, useId, useLayoutEffect, useRef, useState, type SelectHTMLAttributes} from 'react';
import {floatingPanelPosition} from '../lib/floatingPanel';

type SelectProps = SelectHTMLAttributes<HTMLSelectElement> & {popupAlign?: 'start' | 'end'};

/** Shared single-select: native change events, application-themed popup and keyboard navigation. */
export function Select({children, className = '', popupAlign = 'start', ...props}: SelectProps) {
  const native = useRef<HTMLSelectElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const container = useRef<HTMLSpanElement>(null);
  const popup = useRef<HTMLSpanElement>(null);
  const id = useId();
  const options = Children.toArray(children).flatMap(child => {
    if (!isValidElement<{value?: string | number; disabled?: boolean; children?: React.ReactNode}>(child)) return [];
    return [{value: String(child.props.value ?? ''), label: child.props.children, disabled: !!child.props.disabled}];
  });
  const [uncontrolledValue, setUncontrolledValue] = useState(() => String(props.defaultValue ?? options[0]?.value ?? ''));
  const value = String(props.value ?? uncontrolledValue);
  const selected = options.findIndex(option => option.value === value);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [position, setPosition] = useState({left: 0, top: 0, width: 0, maxHeight: 300});
  const prefix = useRef({text: '', time: 0});
  const show = () => {
    if (props.disabled) return;
    const first = options.findIndex(option => !option.disabled);
    if (first < 0) return;
    setActive(selected >= 0 && !options[selected].disabled ? selected : first);
    setOpen(true);
  };
  const choose = (index: number) => {
    const option = options[index];
    if (!option || option.disabled || !native.current) return;
    native.current.value = option.value;
    if (props.value === undefined) setUncontrolledValue(option.value);
    native.current.dispatchEvent(new Event('change', {bubbles: true}));
    setOpen(false);
    trigger.current?.focus();
  };
  useLayoutEffect(() => {
    if (!open) return;
    const update = () => {
      const rect = trigger.current?.getBoundingClientRect();
      if (!rect) return;
      const next = floatingPanelPosition(rect, {width: Math.max(rect.width, 200), height: Math.min(300, popup.current?.scrollHeight ? popup.current.scrollHeight + 2 : options.length * 40 + 14)}, {width: window.innerWidth, height: window.innerHeight}, popupAlign);
      setPosition(current => Object.keys(next).every(key => current[key as keyof typeof next] === next[key as keyof typeof next]) ? current : next);
    };
    const outside = (event: PointerEvent) => {if (!container.current?.contains(event.target as Node)) setOpen(false);};
    update();
    document.addEventListener('pointerdown', outside);
    window.addEventListener('resize', update);
    window.addEventListener('scroll', update, true);
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(update);
    if (trigger.current) observer?.observe(trigger.current);
    if (popup.current) observer?.observe(popup.current);
    return () => {observer?.disconnect(); document.removeEventListener('pointerdown', outside); window.removeEventListener('resize', update); window.removeEventListener('scroll', update, true);};
  }, [open, options.length, popupAlign]);
  useEffect(() => {if (open) document.getElementById(`${id}-${active}`)?.scrollIntoView?.({block: 'nearest', inline: 'nearest'});}, [active, id, open]);
  useEffect(() => {if (props.disabled) setOpen(false);}, [props.disabled]);
  return <span className={`styled-select ${className}`} ref={container}>
    <button ref={trigger} id={props.id} className="select-trigger" type="button" role="combobox" aria-label={props['aria-label']} aria-labelledby={props['aria-labelledby']} aria-describedby={props['aria-describedby']} title={props.title} aria-controls={open ? id : undefined} aria-expanded={open} aria-haspopup="listbox" aria-activedescendant={open ? `${id}-${active}` : undefined} disabled={props.disabled}
      onClick={event => {event.preventDefault(); open ? setOpen(false) : show();}} onBlur={event => {if (!container.current?.contains(event.relatedTarget as Node)) setOpen(false);}}
      onKeyDown={event => {
        if (event.nativeEvent.isComposing || event.altKey || event.ctrlKey || event.metaKey) return;
        if (event.key === 'Tab') {setOpen(false); return;}
        if (event.key === 'Escape') {if (open) {event.preventDefault(); event.stopPropagation(); setOpen(false);} return;}
        if (['Enter', ' '].includes(event.key)) {event.preventDefault(); event.stopPropagation(); if (open) choose(active); else show(); return;}
        if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
          if (!open) {if (options.some(option => !option.disabled)) {event.preventDefault(); event.stopPropagation(); show();} return;}
          let next = event.key === 'Home' ? 0 : event.key === 'End' ? options.length - 1 : active + (event.key === 'ArrowDown' ? 1 : -1);
          const step = event.key === 'ArrowUp' || event.key === 'End' ? -1 : 1;
          while (next >= 0 && next < options.length && options[next].disabled) next += step;
          if (next >= 0 && next < options.length && next !== active) {event.preventDefault(); event.stopPropagation(); setActive(next);}
        } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey) {
          const now = Date.now(); prefix.current = {text: (now - prefix.current.time < 700 ? prefix.current.text : '') + event.key.toLowerCase(), time: now};
          const next = options.findIndex(option => !option.disabled && String(option.label).toLowerCase().startsWith(prefix.current.text));
          if (next >= 0) {if (!open) show(); setActive(next);}
        }
      }}><span>{options[selected]?.label ?? options[0]?.label}</span><svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24"><path d="m7 10 5 5 5-5" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"/></svg></button>
    <select {...props} id={undefined} aria-label={undefined} aria-labelledby={undefined} hidden aria-hidden="true" tabIndex={-1} ref={native}>{children}</select>
    {open && <span ref={popup} id={id} role="listbox" className="select-popup" style={position} aria-label={props['aria-label']}>
      {options.map((option, index) => <span id={`${id}-${index}`} key={`${index}:${option.value}`} role="option" aria-selected={index === selected} aria-disabled={option.disabled || undefined} className={`select-option${index === active ? ' is-highlighted' : ''}`} onPointerDown={event => event.preventDefault()} onPointerMove={() => !option.disabled && setActive(index)} onClick={event => {event.preventDefault(); event.stopPropagation(); choose(index);}}><span>{option.label}</span>{index === selected && <span aria-hidden="true">✓</span>}</span>)}
    </span>}
  </span>;
}

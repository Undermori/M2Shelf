import {useEffect,useRef} from 'react';
import type {ComicBook} from '../types/comic';
import {useI18n} from '../lib/i18n';
import {Icon} from './Icon';

/** Read-only source actions for a book without a separately editable Node. */
export function BookContextMenu({book,x,y,onClose,onReveal}:{book:ComicBook;x:number;y:number;onClose:()=>void;onReveal:(book:ComicBook)=>void}) {
 const {t}=useI18n();const menu=useRef<HTMLDivElement>(null);const close=useRef(onClose);close.current=onClose;
 useEffect(()=>{const previous=document.activeElement as HTMLElement|null;menu.current?.querySelector('button')?.focus();const pointer=(event:PointerEvent)=>{if(!menu.current?.contains(event.target as Node))close.current();};const key=(event:KeyboardEvent)=>{if(event.key==='Escape'){event.preventDefault();close.current();}};window.addEventListener('pointerdown',pointer);window.addEventListener('keydown',key);return()=>{window.removeEventListener('pointerdown',pointer);window.removeEventListener('keydown',key);previous?.focus();};},[]);
 return <div ref={menu} className="context-menu" role="menu" style={{left:Math.max(8,Math.min(x,window.innerWidth-245)),top:Math.max(8,Math.min(y,window.innerHeight-100))}}><p>{book.displayName}</p><button type="button" role="menuitem" onClick={()=>{onReveal(book);onClose();}}><Icon name="external"/>{t('files.reveal')}</button></div>;
}

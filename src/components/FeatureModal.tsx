import {useEffect,useRef,type ReactNode} from 'react';
import {createPortal} from 'react-dom';
import {isolateModalSiblings} from '../lib/modalA11y';
import {useI18n} from '../lib/i18n';
import {Icon} from './Icon';
export function FeatureModal({title,onClose,children,className=''}:{title:string;onClose:()=>void;children:ReactNode;className?:string}){
 const{t}=useI18n();const backdrop=useRef<HTMLDivElement>(null),dialog=useRef<HTMLElement>(null),close=useRef(onClose);close.current=onClose;
 useEffect(()=>{const previous=document.activeElement as HTMLElement|null;const restore=isolateModalSiblings(backdrop.current);const frame=requestAnimationFrame(()=>dialog.current?.querySelector<HTMLElement>('input,textarea,button:not([disabled])')?.focus());const key=(e:KeyboardEvent)=>{if(e.isComposing)return;if(e.key==='Escape'){e.preventDefault();e.stopImmediatePropagation();close.current();}if(e.key==='Tab'){const items=Array.from(dialog.current?.querySelectorAll<HTMLElement>('button:not([disabled]),input:not([disabled]),textarea:not([disabled]),[tabindex="0"]')??[]);if(e.shiftKey&&document.activeElement===items[0]){e.preventDefault();items.at(-1)?.focus();}else if(!e.shiftKey&&document.activeElement===items.at(-1)){e.preventDefault();items[0]?.focus();}}};window.addEventListener('keydown',key,true);return()=>{cancelAnimationFrame(frame);window.removeEventListener('keydown',key,true);restore();previous?.focus();};},[]);
 return createPortal(<div ref={backdrop} className="modal-backdrop" role="presentation" onPointerDown={e=>{if(e.target===e.currentTarget)onClose();}}><section ref={dialog} className={`feature-dialog ${className}`} role="dialog" aria-modal="true" aria-label={title}><header className="modal-header"><h2>{title}</h2><button className="modal-close" type="button" aria-label={t('common.close')} onClick={onClose}><Icon name="close"/></button></header>{children}</section></div>,document.body);
}

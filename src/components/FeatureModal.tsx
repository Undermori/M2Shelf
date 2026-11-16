import {useRef,type ReactNode} from 'react';
import {createPortal} from 'react-dom';
import {useModalFocus} from '../hooks/useModalFocus';
import {useI18n} from '../lib/i18n';
import {Icon} from './Icon';
export function FeatureModal({title,onClose,children,className=''}:{title:string;onClose:()=>void;children:ReactNode;className?:string}){
 const{t}=useI18n();const backdrop=useRef<HTMLDivElement>(null),dialog=useRef<HTMLElement>(null);
 useModalFocus(backdrop,dialog,onClose);
 return createPortal(<div ref={backdrop} className="modal-backdrop" role="presentation" onPointerDown={e=>{if(e.target===e.currentTarget)onClose();}}><section ref={dialog} tabIndex={-1} className={`feature-dialog ${className}`} role="dialog" aria-modal="true" aria-label={title}><header className="modal-header"><h2>{title}</h2><button className="modal-close" type="button" aria-label={t('common.close')} onClick={onClose}><Icon name="close"/></button></header>{children}</section></div>,document.body);
}

import type {ReactNode} from 'react';
import {Icon} from './Icon';

export function MetadataBindingPanel({provider,label,title,openLabel,onOpen,error,children}:{provider:'Bangumi'|'TMDb';label:string;title:string;openLabel?:string;onOpen?:()=>void;error?:ReactNode;children:ReactNode}){
 return <div className="binding-panel metadata-binding">
  <span className="binding-logo"><Icon name={provider==='Bangumi'?'bangumi':'external'}/></span>
  <div className="binding-copy"><small>{label}</small>{onOpen?<button className="binding-subject-link" type="button" onClick={onOpen} title={openLabel} aria-label={openLabel}><span className="binding-title">{title}</span><span className="binding-open-hint">{openLabel}</span></button>:<strong>{title}</strong>}{error&&<em>{error}</em>}</div>
  <div className="binding-actions">{children}</div>
 </div>;
}

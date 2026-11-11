import {useEffect,useRef} from 'react';
import {useI18n} from '../lib/i18n';
import type {useBookFind} from '../hooks/useBookFind';
import {Icon} from './Icon';
export function ReaderFindBar({find,total}:{find:ReturnType<typeof useBookFind>;total:number}){
 const{t}=useI18n();const input=useRef<HTMLInputElement>(null);useEffect(()=>{if(find.open)input.current?.focus();},[find.open]);if(!find.open)return null;
 return <div className="reader-find" role="search" aria-label={t('find.title')} onKeyDown={e=>e.stopPropagation()}><div className="reader-find-controls"><input ref={input} aria-label={t('find.placeholder')} placeholder={t('find.placeholder')} maxLength={128} value={find.query} onChange={e=>find.setQuery(e.target.value)} onKeyDown={e=>{if(e.nativeEvent.isComposing)return;if(e.key==='Enter'){e.preventDefault();find.choose(find.index+(e.shiftKey?-1:1));}}}/><span role="status">{t('find.matches',{current:find.index+1,count:find.matches.length})}</span><button type="button" className="icon-button" disabled={!find.matches.length} aria-label={t('find.previous')} onClick={()=>find.choose(find.index<0?find.matches.length-1:find.index-1)}><Icon name="arrow-left"/></button><button type="button" className="icon-button" disabled={!find.matches.length} aria-label={t('find.next')} onClick={()=>find.choose(find.index+1)}><Icon name="chevron"/></button><button type="button" className="icon-button" aria-label={t('find.close')} onClick={()=>find.setOpen(false)}><Icon name="close"/></button></div>
 {find.query&&<small role={find.error?'alert':'status'}>{find.error?t('comic.error'):find.busy?t('find.progress',{count:find.scanned,total}):find.limited?t('find.limit'):find.stopped?t('find.stopped'):!find.hasText?t('find.noText'):!find.matches.length?t('find.empty'):''}</small>}
 {!!find.matches.length&&<div className="reader-find-results">{find.matches.slice(Math.max(0,find.index-5),Math.max(24,find.index+6)).map((m,i)=>{const at=i+Math.max(0,find.index-5);return <button className={at===find.index?'is-selected':''} key={at} type="button" onClick={()=>find.choose(at)}>{m.page+1} · {m.excerpt}</button>;})}</div>}
 </div>;
}

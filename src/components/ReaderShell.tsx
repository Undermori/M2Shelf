import {forwardRef,useEffect,useRef,useState,type ReactNode} from 'react';
import {useI18n} from '../lib/i18n';
import {Icon} from './Icon';
import {BookmarkSelect} from './BookmarkSelect';

/** Presentation only. Engines, revision checks, visible-page progress and text anchors stay in their readers. */
export const ReaderShell=forwardRef<HTMLElement,{className:string;title:string;header:ReactNode;footer:ReactNode;children:ReactNode}>(function ReaderShell({className,title,header,footer,children},ref){
 return <section ref={ref} className={className} aria-label={title}>{header}{children}{footer}</section>;
});
export function ReaderHeader({title,onBack,onSettings,onFullscreen,settingsOpen}:{title:string;onBack:()=>void;onSettings:()=>void;onFullscreen:()=>void;settingsOpen:boolean}) {
 const {t}=useI18n();return <header className="reader-topbar"><button className="button secondary" onClick={onBack} type="button"><Icon name="arrow-left"/>{t('comic.back')}</button><strong>{title}</strong><button className="icon-button" type="button" aria-label={t('comic.controls')} aria-expanded={settingsOpen} onClick={onSettings}><Icon name="settings"/></button><button className="button secondary" onClick={onFullscreen} type="button">{t('comic.fullscreen')}</button></header>;
}
export function ReaderFooter({status,page,count,chapters=false,canPrevious,canNext,onPrevious,onNext,onJump,onToc,tocOpen=false,bookmarked,bookmarkDisabled,onBookmark,bookmarks,onBookmarkJump}:{status:string;page:number;count:number;chapters?:boolean;canPrevious:boolean;canNext:boolean;onPrevious:()=>void;onNext:()=>void;onJump:(index:number)=>void;onToc?:()=>void;tocOpen?:boolean;bookmarked:boolean;bookmarkDisabled:boolean;onBookmark:()=>void;bookmarks:number[];onBookmarkJump?:(index:number)=>void}){
 const {t}=useI18n();const [jumpOpen,setJumpOpen]=useState(false);const popup=useRef<HTMLDivElement>(null),trigger=useRef<HTMLButtonElement>(null);
 useEffect(()=>{if(!jumpOpen)return;popup.current?.querySelector('input')?.focus();const close=(event:PointerEvent)=>{if(!popup.current?.contains(event.target as Node)&&!trigger.current?.contains(event.target as Node))setJumpOpen(false);};const key=(event:KeyboardEvent)=>{if(event.key==='Escape'){event.preventDefault();event.stopImmediatePropagation();setJumpOpen(false);trigger.current?.focus();}};window.addEventListener('pointerdown',close);window.addEventListener('keydown',key,true);return()=>{window.removeEventListener('pointerdown',close);window.removeEventListener('keydown',key,true);};},[jumpOpen]);
 return <footer className="reader-bottombar reader-navigation">
  <div className="reader-nav-start"><button className="button secondary" disabled={!canPrevious} onClick={onPrevious} type="button">{t('comic.previous')}</button></div>
  <button ref={trigger} className="reader-nav-status" type="button" aria-haspopup="dialog" aria-expanded={jumpOpen} disabled={!count} onClick={()=>setJumpOpen(v=>!v)}>{status}</button>
  <div className="reader-nav-end"><button className="button secondary" disabled={!canNext} onClick={onNext} type="button">{t('comic.next')}</button><button className="button secondary reader-toc-toggle" disabled={!onToc} aria-expanded={tocOpen} onClick={()=>{setJumpOpen(false);onToc?.();}} type="button">{t('book.toc')}</button><button className="icon-button" type="button" disabled={bookmarkDisabled} aria-pressed={bookmarked} aria-label={t(bookmarked?'comic.removeBookmark':'comic.addBookmark')} onClick={onBookmark}><Icon name="bookmark"/></button></div>
  {jumpOpen&&<div ref={popup} className="reader-location-popup" role="dialog" aria-label={t(chapters?'ebook.jumpChapter':'comic.jump')}><label className="reader-location-input"><span>{t(chapters?'ebook.jumpChapter':'comic.jump')}</span><input aria-label={t(chapters?'ebook.jumpChapter':'comic.jump')} type="number" min={1} max={count||1} value={page+1} onChange={event=>{const n=Number(event.target.value);if(Number.isInteger(n)&&n>=1&&n<=count)onJump(n-1);}}/></label><BookmarkSelect bookmarks={bookmarks} count={count} chapters={chapters} onJump={index=>{(onBookmarkJump??onJump)(index);setJumpOpen(false);}}/></div>}
 </footer>;
}

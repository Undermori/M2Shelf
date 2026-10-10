import { useEffect, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { desktopAvailable } from '../lib/api';
import { useI18n } from '../lib/i18n';
import { useAppSettings } from '../lib/settingsStore';
import type { AppLanguage, AppTheme, ViewMode } from '../types/media';
import { Select } from './Select';
import type {ReaderMenuActions} from '../types/readerMenu';

/** Main-window controls only. Native close still runs the existing shutdown/persistence path. */
export interface WindowMenuActions {
 reload?:()=>void; canReload?:boolean;
 back?:()=>void; canBack?:boolean; reader?:ReaderMenuActions|null;
 addRoot: () => void; revealRoot: () => void; navigate: (page:'all'|'search'|'recent'|'favorites'|'settings')=>void;
 about: () => void; checkUpdate: () => void; official: () => void; setView: (mode:ViewMode)=>void;
 canBrowse: boolean; canAdd: boolean; canReveal: boolean; canView: boolean; canCheck: boolean; viewMode: ViewMode;
}
export function WindowTitlebar({actions}:{actions?:WindowMenuActions}) {
 const {t}=useI18n();const [failed,setFailed]=useState(false);
 const {settings, change} = useAppSettings();
 const [menu,setMenu]=useState<number|null>(null);const menuRef=useRef<HTMLDivElement>(null);const triggerRefs=useRef<Array<HTMLButtonElement|null>>([]);
 const groups = [
  {name:t('menu.file'),items:[{name:t('settings.addDirectory'),run:actions?.addRoot,disabled:!actions?.canAdd},{name:t('menu.openExplorerRoot'),run:actions?.revealRoot,disabled:!actions?.canReveal}]},
  {name:t('menu.view'),items:[{name:t('common.grid'),run:()=>actions?.setView('grid'),disabled:!actions?.canView,checked:actions?.viewMode==='grid'},{name:t('common.list'),run:()=>actions?.setView('list'),disabled:!actions?.canView,checked:actions?.viewMode==='list'},...(['system','light','dark'] as const).map(theme=>({name:t(theme==='system'?'settings.themeSystem':theme==='light'?'settings.themeLight':'settings.themeDark'),run:()=>change(current=>({...current,theme})),disabled:!settings,checked:settings?.theme===theme}))]},
  {name:t('menu.go'),items:([['all','sidebar.allResources'],['search','sidebar.search'],['recent','comic.recentTitle'],['favorites','sidebar.favorites'],['settings','sidebar.settings']] as const).map(([page,label])=>({name:t(label),run:()=>actions?.navigate(page),disabled:!actions?.canBrowse}))},
  {name:t('menu.debug'),items:[
   {name:t('menu.reload'),shortcut:'F5',run:actions?.reload,disabled:!actions?.canReload||!!document.querySelector('[data-unsaved-edit="true"]')},
   {name:t('detail.back'),shortcut:'Alt + ←',run:actions?.back,disabled:!actions?.canBack,separator:true},
   {name:t('comic.previous'),shortcut:'PgUp',run:actions?.reader?.previous,disabled:!actions?.reader?.canPrevious},
   {name:t('comic.next'),shortcut:'PgDn',run:actions?.reader?.next,disabled:!actions?.reader?.canNext},
   {name:t('comic.fullscreen'),shortcut:'F',run:actions?.reader?.fullscreen,disabled:!actions?.reader},
   {name:t('menu.bookmark'),shortcut:'B',run:actions?.reader?.bookmark,disabled:!actions?.reader?.canBookmark},
  ]},
  {name:t('menu.help'),items:[{name:t('menu.about'),run:actions?.about,disabled:!actions?.canBrowse},{name:t('update.check'),run:actions?.checkUpdate,disabled:!actions?.canCheck},{name:t('settings.website'),run:actions?.official,disabled:!desktopAvailable}]},
 ];
 useEffect(()=>{
  const key=(event:KeyboardEvent)=>{
   if(!event.ctrlKey||event.key.toLowerCase()!=='r')return;
   event.preventDefault();
   if(event.shiftKey||event.altKey)return;
   if((event.target as HTMLElement)?.closest('input,textarea,select,[contenteditable="true"]'))return;
   if(actions?.canReload&&!document.querySelector('[aria-modal="true"],[data-unsaved-edit="true"]'))actions.reload?.();
  };
  window.addEventListener('keydown',key);return()=>window.removeEventListener('keydown',key);
 },[actions]);
 useEffect(()=>{if(menu===null)return;const close=(event:PointerEvent)=>{if(!menuRef.current?.contains(event.target as Node))setMenu(null);};document.addEventListener('pointerdown',close);const frame=requestAnimationFrame(()=>menuRef.current?.querySelector<HTMLButtonElement>('.window-menu-popup [role^="menuitem"]:not(:disabled)')?.focus());return()=>{document.removeEventListener('pointerdown',close);cancelAnimationFrame(frame);};},[menu]);
 const run=(action:'minimize'|'toggleMaximize'|'close')=>{if(!desktopAvailable)return;setFailed(false);void getCurrentWindow()[action]().catch(()=>setFailed(true));};
 return <header className="window-titlebar">
   <div className="window-menubar" role="menubar" ref={menuRef} onKeyDown={event=>{
    if(event.key==='Escape'&&menu!==null){event.preventDefault();setMenu(null);triggerRefs.current[menu]?.focus();}
    if(event.key==='Tab')setMenu(null);
    if(menu!==null&&['ArrowDown','ArrowUp','Home','End'].includes(event.key)){event.preventDefault();const items=Array.from(menuRef.current?.querySelectorAll<HTMLButtonElement>('.window-menu-popup [role^="menuitem"]:not(:disabled)')??[]);const index=items.indexOf(document.activeElement as HTMLButtonElement);const next=event.key==='Home'?0:event.key==='End'?items.length-1:(index+(event.key==='ArrowUp'?-1:1)+items.length)%items.length;items[next]?.focus();}
    if(menu!==null&&['ArrowLeft','ArrowRight'].includes(event.key)){event.preventDefault();setMenu((menu+(event.key==='ArrowLeft'?-1:1)+groups.length)%groups.length);}
   }}>
    {groups.map((group,index)=><div className="window-menu-group" key={index}><button ref={element=>{triggerRefs.current[index]=element;}} role="menuitem" aria-haspopup="menu" aria-expanded={menu===index} type="button" onClick={()=>setMenu(menu===index?null:index)} onPointerEnter={()=>{if(menu!==null)setMenu(index);}} onKeyDown={event=>{if(menu===null&&['ArrowDown','Enter',' '].includes(event.key)){event.preventDefault();setMenu(index);}}}>{group.name}</button>{menu===index&&<div className="window-menu-popup" role="menu" aria-label={group.name}>{group.items.map((item,itemIndex)=><button key={itemIndex} className={'separator' in item&&item.separator?'menu-separated':undefined} type="button" role={'checked' in item?'menuitemcheckbox':'menuitem'} aria-checked={'checked' in item?item.checked:undefined} disabled={item.disabled} onClick={()=>{setMenu(null);triggerRefs.current[index]?.focus();if(!document.querySelector('[aria-modal="true"],[data-unsaved-edit="true"]'))item.run?.();}}><span aria-hidden="true">{'checked' in item&&item.checked?'✓':''}</span><span className="window-menu-label">{item.name}</span>{'shortcut' in item&&<kbd>{item.shortcut}</kbd>}</button>)}</div>}</div>)}
   </div>
   <div className="window-drag-region" data-tauri-drag-region />
   {failed&&<button className="window-action-error" onClick={()=>setFailed(false)} type="button" role="alert">{t('window.error')}</button>}
   <div className="window-quick-settings">
     <Select className="window-theme-select" popupAlign="end" aria-label={t('settings.theme')} disabled={!desktopAvailable || !settings} value={settings?.theme ?? 'system'} onChange={event=>change(current=>({...current,theme:event.target.value as AppTheme}))}>
       <option value="system">{t('settings.themeSystem')}</option><option value="light">{t('settings.themeLight')}</option><option value="dark">{t('settings.themeDark')}</option>
     </Select>
     <span className="window-language-control"><Select className="window-language-select" popupAlign="end" title={t('settings.language')} aria-label={t('settings.language')} disabled={!desktopAvailable || !settings} value={settings?.language ?? 'zh-CN'} onChange={event=>change(current=>({...current,language:event.target.value as AppLanguage}))}>
       <option value="zh-CN">{t('settings.languageZh')}</option><option value="en-US">{t('settings.languageEn')}</option><option value="ja-JP">{t('settings.languageJa')}</option><option value="ko-KR">{t('settings.languageKo')}</option>
     </Select></span>
   </div>
   <div className="window-controls">
    <button onClick={()=>run('minimize')} disabled={!desktopAvailable} title={t('window.minimize')} aria-label={t('window.minimize')} type="button"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5h10"/></svg></button>
    <button onClick={()=>run('toggleMaximize')} disabled={!desktopAvailable} title={t('window.maximize')} aria-label={t('window.maximize')} type="button"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 3.5h9v9h-9z"/></svg></button>
    <button className="window-close" onClick={()=>run('close')} disabled={!desktopAvailable} title={t('window.close')} aria-label={t('window.close')} type="button"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8"/></svg></button>
   </div>
 </header>;
}

import { useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { desktopAvailable } from '../lib/api';
import { useI18n } from '../lib/i18n';
import brandMark from '../../src-tauri/icons/icon.png';

/** Main-window controls only. Native close still runs the existing shutdown/persistence path. */
export function WindowTitlebar() {
 const {t}=useI18n();const [failed,setFailed]=useState(false);
 const run=(action:'minimize'|'toggleMaximize'|'close')=>{if(!desktopAvailable)return;setFailed(false);void getCurrentWindow()[action]().catch(()=>setFailed(true));};
 return <header className="window-titlebar">
   <div className="window-drag-region" data-tauri-drag-region><img src={brandMark} alt=""/><span>M²Shelf</span></div>
   {failed&&<button className="window-action-error" onClick={()=>setFailed(false)} type="button" role="alert">{t('window.error')}</button>}
   <div className="window-controls">
    <button onClick={()=>run('minimize')} disabled={!desktopAvailable} title={t('window.minimize')} aria-label={t('window.minimize')} type="button"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5h10"/></svg></button>
    <button onClick={()=>run('toggleMaximize')} disabled={!desktopAvailable} title={t('window.maximize')} aria-label={t('window.maximize')} type="button"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 3.5h9v9h-9z"/></svg></button>
    <button className="window-close" onClick={()=>run('close')} disabled={!desktopAvailable} title={t('window.close')} aria-label={t('window.close')} type="button"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8"/></svg></button>
   </div>
 </header>;
}

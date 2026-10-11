import {useEffect,useRef,useState} from 'react';
import {api,desktopAvailable} from '../lib/api';
import {useI18n} from '../lib/i18n';
import type {TmdbCredentialStatus} from '../types/tmdb';
import tmdbLogo from '../assets/tmdb-logo.svg';
import {Icon} from './Icon';

export function TmdbCredentialSettings() {
 const {t}=useI18n();
 const [status,setStatus]=useState<TmdbCredentialStatus|null>(null),[busy,setBusy]=useState(false),[failed,setFailed]=useState(false);
 const active=useRef(false),running=useRef(false),generation=useRef(0);
 useEffect(()=>{active.current=true;const request=++generation.current;if(desktopAvailable)void api.tmdbCredentialStatus().then(s=>{if(active.current&&request===generation.current)setStatus(s);}).catch(()=>{if(active.current&&request===generation.current)setFailed(true);});return()=>{active.current=false;generation.current++;};},[]);
 const run=async(action:()=>Promise<unknown>)=>{
  if(running.current)return;
  const request=++generation.current;running.current=true;setBusy(true);setFailed(false);
  try{await action();const next=await api.tmdbCredentialStatus();if(active.current&&request===generation.current)setStatus(next);}
  catch{if(active.current&&request===generation.current)setFailed(true);}
  finally{running.current=false;if(active.current)setBusy(false);}
 };
 const selectedStatus=()=>t(status?.check==='VERIFIED'?'next.verified':status?.check==='AUTH_ERROR'?'next.authFailed':status?.check==='RATE_LIMITED'?'tmdb.rateError':status?.check==='NETWORK_ERROR'?'next.connectionFailed':'next.savedUnchecked');
 const feedback=failed?<small className="service-status is-error" role="alert">{t('tmdb.error')} <button type="button" className="button secondary" disabled={busy} onClick={()=>void run(()=>api.tmdbCredentialStatus())}>{t('common.retry')}</button></small>:null;
 return <fieldset className="service-settings tmdb-settings" disabled={!desktopAvailable||busy}><legend>TMDb</legend>
  <div className="service-option">
   <label className="service-choice"><input type="radio" name="tmdb-source" checked={status?.source==='DEFAULT'} disabled={!status} onChange={()=>void run(()=>api.tmdbSelectSource('DEFAULT'))}/><span><strong>{t('next.tmdbDefault')}</strong><small>{t(status?.defaultAvailable?'next.defaultReady':'next.defaultUnavailable')}</small></span></label>
   {status?.source==='DEFAULT'&&<>{status.check!=='UNCHECKED'&&<small className={`service-status${['AUTH_ERROR','RATE_LIMITED','NETWORK_ERROR'].includes(status.check)?' is-error':''}`} role="status">{selectedStatus()}</small>}<div className="settings-metadata-actions service-actions"><button className="button secondary" type="button" onClick={()=>void run(()=>api.tmdbVerify())}>{t(busy?'settings.testing':'next.verifyConnection')}</button></div>{feedback}</>}
  </div>
  <div className="service-option">
   <label className="service-choice"><input type="radio" name="tmdb-source" checked={status?.source==='PERSONAL'} disabled={!status} onChange={()=>void run(()=>api.tmdbSelectSource('PERSONAL'))}/><span><strong>{t('next.tmdbPersonal')}</strong><small className={status?.source==='PERSONAL'&&['AUTH_ERROR','RATE_LIMITED','NETWORK_ERROR'].includes(status.check)?'is-error':''} role={status?.source==='PERSONAL'?'status':undefined}>{!status?.personalConfigured?t('tmdb.notConfigured'):status.source==='PERSONAL'?selectedStatus():t('next.savedUnchecked')}</small></span></label>
   <div className="settings-metadata-actions service-actions"><button className="button secondary" type="button" onClick={()=>void run(()=>api.tmdbConfigure(t('tmdb.token'),t('tmdb.credentialsHelp')))}>{t(status?.personalConfigured?'next.changePersonal':'tmdb.configure')}</button>{status?.source==='PERSONAL'&&<button className="button secondary" type="button" onClick={()=>void run(()=>api.tmdbVerify())}>{t(busy?'settings.testing':'next.verifyConnection')}</button>}{status?.personalConfigured&&<button className="button secondary" type="button" onClick={()=>void run(()=>api.tmdbDeletePersonal(t('next.deletePersonal'),t('next.deletePersonalConfirm')))}>{t('next.deletePersonal')}</button>}</div>
   {status?.source==='PERSONAL'&&feedback}
  </div>
  {!status&&feedback}
  <details className="service-credits"><summary>{t('next.serviceCredits')}</summary><div className="tmdb-credits settings-metadata"><img src={tmdbLogo} alt="TMDB"/><div><p>{t('tmdb.notice')}</p><p>{t('tmdb.license')}</p><button className="button secondary" type="button" onClick={()=>void run(()=>api.openExternalUrl('https://www.themoviedb.org'))}>TMDB <Icon name="external"/></button></div></div></details>
 </fieldset>;
}

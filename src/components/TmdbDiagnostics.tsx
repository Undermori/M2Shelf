import {useRef,useState,useEffect} from 'react';
import {api,desktopAvailable} from '../lib/api';
import {useI18n} from '../lib/i18n';
import type {TmdbMatchDiagnostic} from '../types/tmdb';

export function TmdbDiagnostics(){
 const {t}=useI18n();const [rows,setRows]=useState<TmdbMatchDiagnostic[]>([]);const [status,setStatus]=useState<boolean|null>(null);const [busy,setBusy]=useState(false);const [failed,setFailed]=useState(false);const generation=useRef(0);
 useEffect(()=>()=>{generation.current++;},[]);
 const load=async()=>{const token=++generation.current;setBusy(true);setFailed(false);try{const [configured,entries]=await Promise.all([api.tmdbStatus(),api.tmdbMatchDiagnostics()]);if(token===generation.current){setStatus(configured);setRows(entries);}}catch{if(token===generation.current)setFailed(true);}finally{if(token===generation.current)setBusy(false);}};
 return <details className="tmdb-diagnostics" onToggle={e=>{if(e.currentTarget.open&&desktopAvailable)void load();}}><summary>{t('tmdb.autoDiagnostics')}</summary><div><p>{t(status===null?'common.processing':status?'tmdb.configured':'tmdb.notConfigured')}</p><button className="button secondary" disabled={busy||!desktopAvailable} onClick={()=>void load()} type="button">{t('tmdb.refreshDiagnostics')}</button>{failed?<p role="alert">{t('tmdb.error')}</p>:<ul>{rows.map(row=><li key={row.nodeId}><strong>{row.query||`#${row.nodeId}`}{row.year?` · ${row.year}`:''}</strong><span>{t(diagnosticKey(row.outcome))}</span></li>)}</ul>}</div></details>;
}
export function diagnosticKey(outcome:string){
 switch(outcome){
 case 'credentials-unavailable':return 'tmdb.notConfigured';case 'unauthorized':return 'tmdb.authError';case 'rate-limited':return 'tmdb.rateError';case 'request-failed':return 'tmdb.error';case 'no-results':return 'tmdb.autoNoResults';case 'year-uncertain':return 'tmdb.autoYearUncertain';case 'no-unique-title-year':case 'detail-conflict':return 'tmdb.autoUncertain';case 'budget-deferred':return 'tmdb.autoDeferred';case 'matched':return 'tmdb.autoMatched';case 'matched-cover-failed':return 'tmdb.autoCoverFailed';case 'searching':return 'common.searching';default:return 'tmdb.autoIneligible';
 }
}

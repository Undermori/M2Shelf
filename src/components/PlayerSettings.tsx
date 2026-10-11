import {useEffect,useRef,useState} from 'react';
import {api,choosePlayerExecutable,desktopAvailable} from '../lib/api';
import {useI18n} from '../lib/i18n';
import {useAppSettings} from '../lib/settingsStore';
import type {PlayerTestResult} from '../types/media';

export function PlayerSettings() {
 const {t}=useI18n();const {settings,saving,change,flush}=useAppSettings();
 const [testing,setTesting]=useState(false),[result,setResult]=useState<PlayerTestResult|null>(null);
 const active=useRef(false),generation=useRef(0),running=useRef(false);
 useEffect(()=>{active.current=true;return()=>{active.current=false;generation.current++;};},[]);
 useEffect(()=>{generation.current++;setResult(null);},[settings?.playerMode,settings?.mpvPath]);
 const test=async()=>{
  if(!settings||running.current)return;
  const request=++generation.current;running.current=true;setTesting(true);setResult(null);
  try{await flush();const next=await api.testMpv();if(active.current&&request===generation.current)setResult(next);}
  catch{if(active.current&&request===generation.current)setResult({ok:false,version:null,message:''});}
  finally{running.current=false;if(active.current)setTesting(false);}
 };
 const choose=async()=>{try{const path=await choosePlayerExecutable();if(active.current&&path)change(s=>({...s,mpvPath:path,playerMode:'CUSTOM'}));}catch{if(active.current)setResult({ok:false,version:null,message:''});}};
 const mode=settings?.playerMode??(settings?.mpvPath?'CUSTOM':'SYSTEM');
 const version=result?.version?.match(/^(?:mpv|mpvnet|VLC media player)\b[^\r\n]{0,160}$/i)?.[0];
 return <>
  <fieldset className="service-settings" disabled={!settings}><legend className="sr-only">{t('settings.playerTitle')}</legend>{(['CUSTOM','SYSTEM'] as const).map(value=><label className="service-choice" key={value}><input type="radio" name="player-mode" checked={mode===value} onChange={()=>change(s=>({...s,playerMode:value}))}/><strong>{t(value==='SYSTEM'?'next.systemPlayer':'next.customPlayer')}</strong></label>)}</fieldset>
  <div className="setting-form-row player-path-row"><label><span>{t('settings.playerPath')}</span><div className="path-input"><input disabled={!settings} onChange={e=>change(s=>({...s,mpvPath:e.target.value||null,playerMode:'CUSTOM'}))} placeholder={t('settings.playerPlaceholder')} value={settings?.mpvPath??''}/><button disabled={!desktopAvailable||testing} onClick={()=>void choose()} type="button">{t('common.chooseEllipsis')}</button></div></label><button className="button secondary" disabled={!desktopAvailable||!settings||saving||testing} onClick={()=>void test()} type="button">{t(testing?'settings.testing':'settings.testPlayer')}</button></div>
  {result&&<div className="player-test-result"><p className={`service-status${result.ok?'':' is-error'}`} role={result.ok?'status':'alert'}>{result.ok?result.version==='SYSTEM'?t('next.systemPlayer'):version?t('settings.playerAvailable',{version}):t('settings.playerReady'):t(result.message.startsWith('PLAYER_CONFIG_ERROR')?'next.playerConfigError':'settings.playerUnavailable')}</p>{!result.ok&&result.message&&<details className="player-test-log"><summary>{t('next.viewTestLog')}</summary><pre>{result.message}</pre></details>}</div>}
 </>;
}

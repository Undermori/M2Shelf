import {useI18n} from '../lib/i18n';
import type {MatchResult} from './TmdbMatch';
import {Icon} from './Icon';
import {LoadingState} from './LoadingState';
export function MatchingResults({results,loading,searched,error,bindingId,currentBindingId=null,provider,onChoose}:{results:MatchResult[];loading:boolean;searched:boolean;error:boolean;bindingId:number|null;currentBindingId?:number|null;provider:'BANGUMI'|'TMDB';onChoose:(result:MatchResult)=>void}) {
 const {t}=useI18n();
 return <div className="bangumi-results" aria-busy={loading||bindingId!==null}>
  {loading&&<LoadingState label={t('match.connecting',{provider:provider==='TMDB'?'TMDb':'Bangumi'})}/>}
  {!loading&&!searched&&<div className="search-prompt"><Icon name="search"/><strong>{t('bangumi.promptTitle')}</strong></div>}
  {!loading&&searched&&!error&&!results.length&&<div className="search-prompt"><Icon name="info"/><strong>{t('match.noneTitle')}</strong><p>{t('bangumi.noneDescription')}</p></div>}
  {!loading&&results.map(result=><article className={`bangumi-result${bindingId===result.id?' is-binding':bindingId!==null?' is-disabled':''}${currentBindingId===result.id?' is-bound':''}`} key={`${result.provider}-${result.id}`} aria-current={currentBindingId===result.id?true:undefined}>
   <span className="bangumi-cover">{result.poster?<img alt="" src={result.poster}/>:<Icon name="image"/>}</span>
   <div className="bangumi-result-copy"><strong title={result.title}>{result.title}</strong>{result.originalTitle!==result.title&&<p title={result.originalTitle}>{result.originalTitle}</p>}<small>{result.date||t('bangumi.unknownDate')}<span/>{result.provider==='TMDB'?`TMDb #${result.id}`:t('bangumi.subjectId',{id:result.id})}</small></div>
   <button className="button secondary match-choice" disabled={bindingId!==null} type="button" title={result.title} onClick={()=>onChoose(result)}>{bindingId===result.id?<><Icon name="refresh"/>{t('bangumi.binding')}</>:<>{currentBindingId===result.id&&<Icon name="check"/>}{t('common.choose')}</>}</button>
  </article>)}
 </div>;
}

import { useCallback, useEffect, useRef, useState } from "react";
import type { BangumiSearchPrefill, MediaNode, MetadataBinding } from "../types/media";
import { api, isStaleWorkError } from "../lib/api";
import { canBindTmdb, cleanBangumiKeyword, errorMessage, nodeDisplayTitle } from "../lib/format";
import {tmdbResult,tmdbErrorKey,type MatchResult} from "./TmdbMatch";
import {MatchingResults} from "./MatchingResults";
import {Select} from "./Select";
import { Icon } from "./Icon";
import {useModalFocus} from "../hooks/useModalFocus";
import { useI18n } from "../lib/i18n";

interface BangumiModalProps {
  node: MediaNode | null;
  onClose: () => void;
  onStale: () => Promise<void>;
  onMetadataChanged?:()=>Promise<void>;
  onBound: (binding: MetadataBinding) => void | Promise<void>;
}

export function BangumiModal({ node, onClose, onBound, onStale, onMetadataChanged }: BangumiModalProps) {
  const { language, t } = useI18n();
  const [provider,setProvider]=useState<'BANGUMI'|'TMDB'>('BANGUMI');
  const [tmdbType,setTmdbType]=useState<'AUTO'|'MOVIE'|'TV'>('AUTO');
  const [configured,setConfigured]=useState<boolean|null>(null);
  const providerRef=useRef(provider);providerRef.current=provider;
  const backdropRef=useRef<HTMLDivElement>(null);
  const dialogRef=useRef<HTMLElement>(null);
  const [keyword, setKeyword] = useState("");
  const [results, setResults] = useState<MatchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [bindingId, setBindingId] = useState<number | null>(null);
  const [searched, setSearched] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [errorKind, setErrorKind] = useState<"search" | "bind">("search");
  const [prefill, setPrefill] = useState<BangumiSearchPrefill | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const requestGenerationRef = useRef(0);
  const activeNodeIdRef = useRef<number | null>(null);

  const requestIsCurrent = useCallback((generation: number, nodeId: number) => (
    requestGenerationRef.current === generation && activeNodeIdRef.current === nodeId
  ), []);

  const close = useCallback(() => {
    requestGenerationRef.current += 1;
    activeNodeIdRef.current = null;
    if(providerRef.current==='TMDB')void api.tmdbCancel().catch(()=>undefined);
    setLoading(false);
    setBindingId(null);
    onClose();
  }, [onClose]);

  const updateKeyword = useCallback((value: string) => {
    requestGenerationRef.current += 1;
    if(providerRef.current==='TMDB')void api.tmdbCancel().catch(()=>undefined);
    setKeyword(value);
    setResults([]);
    setError(null);
    setSearched(false);
    setLoading(false);
  }, []);

  useEffect(() => {
    const generation = requestGenerationRef.current + 1;
    requestGenerationRef.current = generation;
    const nodeId = node?.id ?? null;
    activeNodeIdRef.current = nodeId;
    setProvider(node && canBindTmdb(node) && node.tmdbBinding?.active ? 'TMDB' : 'BANGUMI');
    setTmdbType(node?.nodeType === 'CONTAINER' ? 'TV' : node?.tmdbBinding?.active ? node.tmdbBinding.movie.mediaType ?? 'MOVIE' : 'AUTO');
    setResults([]);
    setError(null);
    setSearched(false);
    setPrefill(null);
    setLoading(false);
    setBindingId(null);
    setKeyword("");
    if (!node || nodeId == null) return;
    setKeyword(cleanBangumiKeyword(node.displayName || node.folderName));
    void api.bangumiPrefill(node.id).then((value) => {
      if (!requestIsCurrent(generation, nodeId)) return;
      setPrefill(value);
      setKeyword(value.extractedName || value.originalName);
    }).catch(() => undefined);
    return () => {requestGenerationRef.current++;if(providerRef.current==='TMDB')void api.tmdbCancel().catch(()=>undefined);};
  }, [node?.id, requestIsCurrent]);

  useEffect(()=>{
    if(!node||provider!=='TMDB')return;let active=true;setConfigured(null);
    void api.tmdbStatus().then(value=>{if(active)setConfigured(value);}).catch(value=>{if(active)setError(t(tmdbErrorKey(value)));});return()=>{active=false;};
  },[node?.id,provider,t]);
  const previousLanguage=useRef(language);
  useEffect(()=>{if(previousLanguage.current!==language){previousLanguage.current=language;requestGenerationRef.current++;setResults([]);setError(null);setSearched(false);setLoading(false);if(provider==='TMDB')void api.tmdbCancel().catch(()=>undefined);}},[language,provider]);
  // Searches can be cancelled; binding mutations retain their busy guard.
  useModalFocus(backdropRef, dialogRef, close, 'input:not(:disabled)', bindingId !== null, Boolean(node));

  if (!node) return null;

  const search = async (event?: React.FormEvent) => {
    event?.preventDefault();
    const query = keyword.trim();
    if (!query || loading || bindingId !== null) return;
    const nodeId = node.id;
    const generation = requestGenerationRef.current + 1;
    requestGenerationRef.current = generation;
    setLoading(true);
    setSearched(true);
    setError(null);
    setErrorKind("search");
    try {
      const nextResults:MatchResult[]=provider==='TMDB'?await api.tmdbSearch(nodeId,query,null,language,tmdbType==='AUTO'?undefined:tmdbType).then(found=>found.movies.map(movie=>tmdbResult(movie,found.snapshot))):(await api.searchBangumi(query,20,node.id)).map(subject=>({provider:'BANGUMI',id:subject.subjectId,title:(language==='en-US'?subject.titleEn:language==='ja-JP'?subject.titleJa:language==='ko-KR'?subject.titleKo:subject.titleCn)||subject.titleCn||subject.title,originalTitle:subject.title,date:subject.date,poster:subject.imageUrl,subject}));
      if (!requestIsCurrent(generation, nodeId)) return;
      setResults(nextResults);
    } catch (caught) {
      if (!requestIsCurrent(generation, nodeId)) return;
      const key=provider==='TMDB'?tmdbErrorKey(caught):null;setError(key?t(key):errorMessage(caught));if(key==='tmdb.authError')setConfigured(false);
      setResults([]);
    } finally {
      if (requestIsCurrent(generation, nodeId)) setLoading(false);
    }
  };

  const bind = async (result:MatchResult) => {
    if (bindingId !== null || loading || result.provider!==provider) return;
    const nodeId = node.id;
    const generation = requestGenerationRef.current + 1;
    requestGenerationRef.current = generation;
    setBindingId(result.id);
    setError(null);
    setErrorKind("bind");
    try {
      if(result.provider==='TMDB'){
        await api.tmdbBind(nodeId,result.id,language,result.snapshot,result.movie.mediaType??"MOVIE");if(!requestIsCurrent(generation,nodeId))return;await(onMetadataChanged?.()??onStale());
      }else{
        const binding=node.workTarget?await api.bindWorkBangumi(node.workTarget,result.subject):await api.bindBangumi(nodeId,result.subject);if(!requestIsCurrent(generation,nodeId))return;await onBound(binding);
      }
      if (!requestIsCurrent(generation, nodeId)) return;
      close();
    } catch (caught) {
      if (!requestIsCurrent(generation, nodeId)) return;
      if (isStaleWorkError(caught)) { await onStale(); close(); return; }
      const key=provider==='TMDB'?tmdbErrorKey(caught):null;setError(key?t(key):errorMessage(caught));if(key==='tmdb.authError')setConfigured(false);
    } finally {
      if (requestIsCurrent(generation, nodeId)) setBindingId(null);
    }
  };

  const configure=async()=>{const generation=++requestGenerationRef.current,nodeId=node.id;setLoading(true);setError(null);try{const ok=await api.tmdbConfigure(t('tmdb.token'),t('tmdb.credentialsHelp'));if(requestIsCurrent(generation,nodeId)&&ok)setConfigured(true);}catch(value){if(requestIsCurrent(generation,nodeId))setError(t(tmdbErrorKey(value)));}finally{if(requestIsCurrent(generation,nodeId))setLoading(false);}};
  const providerName=provider==='TMDB'?'TMDb':'Bangumi';
  return (
    <div className="modal-backdrop" ref={backdropRef} onMouseDown={(event) => event.target === event.currentTarget && bindingId === null && close()} role="presentation">
      <section ref={dialogRef} tabIndex={-1} className="bangumi-modal" role="dialog" aria-modal="true" aria-labelledby="bangumi-title">
        <header className="modal-header">
          <div><h2 id="bangumi-title">{t('match.title',{provider:providerName})}</h2><p>{nodeDisplayTitle(node)}</p>{node.workTarget && <p>{t("works.wholeGroup", { count: node.workTarget.sourceNodeIds.length })}</p>}</div>
          <button className="modal-close" disabled={bindingId !== null} aria-label={t("common.close")} onClick={close} type="button"><Icon name="close" /></button>
        </header>

        {canBindTmdb(node)&&<div className="metadata-provider">
          <div className="metadata-provider-field"><span>{t('match.provider')}</span><div className="metadata-provider-control" data-provider={providerName}><span className="metadata-provider-icon"><Icon name={provider==='TMDB'?'tmdb':'bangumi'}/></span><Select aria-label={t('match.provider')} value={provider} disabled={bindingId!==null} onChange={e=>{requestGenerationRef.current++;setLoading(false);setResults([]);setError(null);setSearched(false);setBindingId(null);if(provider==='TMDB')void api.tmdbCancel().catch(()=>undefined);setProvider(e.target.value as 'BANGUMI'|'TMDB');}}><option value="BANGUMI">Bangumi</option><option value="TMDB">TMDb</option></Select></div></div>
          {provider==='TMDB'&&node.nodeType!=='CONTAINER'&&<div className="metadata-provider-field"><span>{t('match.tmdbType')}</span><Select aria-label={t('match.tmdbType')} value={tmdbType} disabled={bindingId!==null} onChange={event=>{requestGenerationRef.current++;setLoading(false);setResults([]);setError(null);setSearched(false);void api.tmdbCancel().catch(()=>undefined);setTmdbType(event.target.value as 'AUTO'|'MOVIE'|'TV');}}><option value="AUTO">{t('match.typeAuto')}</option><option value="MOVIE">{t('match.typeMovie')}</option><option value="TV">{t('match.typeTv')}</option></Select></div>}
        </div>}
        {prefill && <div className="bangumi-keyword-context">
          <div><span>{t("bangumi.originalName")}</span><strong title={prefill.originalName}>{prefill.originalName}</strong></div>
          <div><span>{t("bangumi.extractedName")}</span><button disabled={bindingId !== null} onClick={() => updateKeyword(prefill.extractedName)} type="button">{prefill.extractedName || t("bangumi.noExtracted")}</button></div>
          {prefill.candidates.length > 1 && <div className="keyword-candidates"><span>{t("bangumi.otherCandidates")}</span><p>{prefill.candidates.filter((candidate) => candidate !== prefill.extractedName).map((candidate) => <button disabled={bindingId !== null} key={candidate} onClick={() => updateKeyword(candidate)} type="button">{candidate}</button>)}</p></div>}
        </div>}
        {provider==='TMDB'&&configured===false&&<div className="tmdb-credential-notice" role="status"><span>{t('tmdb.notConfigured')}</span><button className="button secondary" disabled={loading||bindingId!==null} onClick={()=>void configure()} type="button">{t('tmdb.configure')}</button></div>}
        <form className="bangumi-search" onSubmit={search}>
          <Icon name="search" />
          <input ref={inputRef} autoCorrect="off" autoCapitalize="none" spellCheck={false} aria-label={t('match.keywordAria',{provider:providerName})} disabled={bindingId !== null} onChange={(event) => updateKeyword(event.target.value)} placeholder={t("bangumi.keywordPlaceholder")} value={keyword} />
          {keyword && <button className="clear-input" aria-label={t("common.clear")} disabled={bindingId !== null} onClick={() => updateKeyword("")} type="button"><Icon name="close" /></button>}
          <button className="button primary" disabled={!keyword.trim() || loading || bindingId !== null || provider==='TMDB'&&configured!==true} type="submit">{loading ? t("common.searching") : t("common.search")}</button>
        </form>

        {error && <div className="inline-error" role="alert"><Icon name="warning" /><span><strong>{t(errorKind==='search'?'match.searchFailed':'match.saveFailed',{provider:providerName})}</strong><small>{error}</small></span>{errorKind === "search" && <button onClick={() => void search()} type="button">{t("common.retry")}</button>}</div>}
        <MatchingResults currentTmdbType={node.tmdbBinding?.movie.mediaType??"MOVIE"} results={results} loading={loading} searched={searched} error={!!error} bindingId={bindingId} currentBindingId={provider==='TMDB'?(node.tmdbBinding?.active?node.tmdbBinding.movie.id:null):node.binding?.providerSubjectId} provider={provider} onChoose={result=>void bind(result)}/>
      </section>
    </div>
  );
}

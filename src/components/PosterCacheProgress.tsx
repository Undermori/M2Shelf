import {useEffect, useRef, useState} from "react";
import {api, desktopAvailable} from "../lib/api";
import {errorMessage} from "../lib/format";
import {useI18n} from "../lib/i18n";
import type {PosterCacheFailure, PosterCacheStatus} from "../types/media";

const phaseKeys = {IDLE:"poster.idle",QUEUED:"poster.queued",RUNNING:"poster.running",COMPLETED:"poster.completed",CANCELLED:"poster.cancelled",FAILED:"poster.failed"} as const;
const reasonKeys = {SOURCE_READ:"poster.sourceRead",SOURCE_CHANGED:"poster.sourceChanged",PROCESSING:"poster.processing"} as const;

// Poll only this small panel. Settings fields and the rest of the page do not rerender per tick.
export function PosterCacheProgress({cacheDirectory, onCompleted}: {cacheDirectory?:string;onCompleted:()=>void}) {
  const {t} = useI18n();
  const [status,setStatus] = useState<PosterCacheStatus|null>(null);
  const [unavailable,setUnavailable] = useState(false);
  const [expanded,setExpanded] = useState(false);
  const [failures,setFailures] = useState<PosterCacheFailure[]>([]);
  const [detailsLoading,setDetailsLoading] = useState(false);
  const [actionError,setActionError] = useState("");
  const [retrying,setRetrying] = useState(false);
  const completed = useRef(onCompleted);
  completed.current = onCompleted;
  const generation = useRef(0);
  const diagnostics = useRef(0);
  useEffect(()=>{
    if (!desktopAvailable) return;
    const request=++generation.current;
    let active=true;
    let timer:number|undefined;
    let previous:PosterCacheStatus["phase"]|undefined;
    setExpanded(false);setFailures([]);setActionError("");setRetrying(false);
    const poll=async()=>{
      try {
        const next=await api.posterCacheStatus();
        if (!active) return;
        setStatus(next);setUnavailable(false);
        if (previous && previous!==next.phase && next.phase==="COMPLETED") completed.current();
        previous=next.phase;
      } catch {if (active) setUnavailable(true);}
      finally {if (active) timer=window.setTimeout(()=>void poll(),1000);}
    };
    void poll();
    return ()=>{active=false;window.clearTimeout(timer);if(generation.current===request) generation.current++;diagnostics.current++;};
  },[cacheDirectory]);

  const showReasons=async()=>{
    if(expanded){setExpanded(false);diagnostics.current++;return;}
    setExpanded(true);setDetailsLoading(true);setActionError("");
    const request=++diagnostics.current;
    const current=generation.current;
    try {const next=await api.posterCacheFailures();if(request===diagnostics.current && current===generation.current) setFailures(next);}
    catch(error){if(request===diagnostics.current && current===generation.current) setActionError(errorMessage(error));}
    finally{if(request===diagnostics.current && current===generation.current) setDetailsLoading(false);}
  };
  const retry=async()=>{
    const current=generation.current;
    setRetrying(true);setActionError("");setExpanded(false);diagnostics.current++;
    try {await api.retryPosterCache();if(current===generation.current) {const next=await api.posterCacheStatus();if(current===generation.current) setStatus(next);}}
    catch(error){if(current===generation.current) setActionError(errorMessage(error));}
    finally{if(current===generation.current) setRetrying(false);}
  };
  const busy=retrying || status?.phase==="RUNNING" || status?.phase==="QUEUED";
  const hasFailures=(status?.failed??0)>0 || status?.phase==="FAILED";
  return <div className="poster-cache-progress" aria-busy={busy}>
    <div className="poster-cache-progress-heading"><strong>{t("poster.title")}</strong><span>{unavailable?t("poster.unavailable"):status?t(phaseKeys[status.phase]):t("poster.checking")}</span></div>
    {status && status.total>0 && <><progress aria-label={t("poster.title")} value={status.processed} max={status.total}/><div className="poster-cache-progress-count"><span>{t("poster.count",{processed:status.processed,total:status.total})}</span>{status.failed>0 && <span>{t("poster.errors",{count:status.failed})}</span>}</div></>}
    {(status?.deferred??0)>0 && <p>{t("poster.deferred",{count:status!.deferred})}</p>}
    {hasFailures && <div className="poster-cache-actions"><button className="button secondary" aria-expanded={expanded} onClick={()=>void showReasons()} type="button">{t(expanded?"poster.hideReasons":"poster.showReasons")}</button><button className="button secondary" disabled={busy || unavailable} onClick={()=>void retry()} type="button">{t(retrying?"poster.retrying":"poster.retry")}</button></div>}
    {actionError && <p role="alert">{actionError}</p>}
    {expanded && <div className="poster-cache-diagnostics">
      {status?.error && <p role="alert">{status.error}</p>}
      {detailsLoading ? <p>{t("poster.checking")}</p> : <>
        <ul>{failures.map(failure=><li key={failure.nodeId}><strong>{failure.name}</strong><span>{t(reasonKeys[failure.reason]??"poster.processing")}</span><p>{failure.detail==="COMIC_IMAGE_INVALID"?t("poster.invalidImage"):failure.detail.startsWith("COMIC_")?t("comic.error"):failure.detail}</p>{failure.detail.startsWith("COMIC_") && <small>{failure.detail}</small>}</li>)}</ul>
        {failures.length===100 && <p>{t("poster.first100")}</p>}
      </>}
    </div>}
  </div>;
}

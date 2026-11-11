import {DetailVideoTabs} from '../components/DetailVideoTabs';
import {DetailUtility} from '../components/DetailUtility';
import {editableDetailNode} from '../components/DetailMetadata';
import {Select} from '../components/Select';
import {useEpisodeWatch,EpisodeWatchSummary} from '../components/EpisodeWatchControl';
import {CoverButton} from '../components/CoverSelector';
import {PrivateNote} from '../components/PrivateNote';
import {CollectionStatusControl} from '../lib/collectionState';
import {api} from "../lib/api";
import { useEffect, useMemo, useState } from "react";
import type { MediaFile, NodeDetail, ResourceFile } from "../types/media";
import { Breadcrumb } from "../components/Breadcrumb";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { LoadingState } from "../components/LoadingState";
import { PosterGrid } from "../components/PosterGrid";
import { OtherResourceList } from "../components/OtherResourceList";
import { bindingDisplayTitle, canBindBangumi, nodeDisplayTitle, nodeHasVideo, nodeTypeLabel, naturalCompare } from "../lib/format";
import { useCoverDataUrl } from "../hooks/useCoverDataUrl";
import { useI18n } from "../lib/i18n";
import { PosterImage } from "../components/PosterImage";
import {ComicBookList} from '../components/ComicBookList';
import {readingFiles} from '../lib/readingFiles';
import {MetadataBindingPanel} from '../components/MetadataBindingPanel';
import {DetailCount} from '../components/DetailCount';

export interface WorkDetailPageProps {
  onBookMenu?:(event:React.MouseEvent,book:import('../types/comic').ComicBook)=>void;
  onReadComic?: (book:import('../types/comic').ComicBook,list?:import('../types/comic').ComicBook[])=>void;
  detail: NodeDetail | null;
  loading: boolean;
  rootLabel: string;
  onRoot: () => void;
  onBreadcrumb: (nodeId: number) => void;
  onBack: () => void;
  onBangumi: () => void;
  onOpenBangumi: () => void;
  onRetryCover: (imageDecodeFailed?: boolean) => void;
  onRetryCoverNode: (node: NodeDetail["node"], imageDecodeFailed?: boolean) => void;
  onClearBangumi: () => void;
  onReveal: () => void;
  onPlay: (file: MediaFile) => void | boolean | Promise<boolean>;
  onRevealMedia: (file: MediaFile) => void;
  onOpenResource: (file: ResourceFile) => void;
  onRevealResource: (file: ResourceFile) => void;
  onOpenChild: (node: NodeDetail["node"]) => void;
  onBangumiNode: (node: NodeDetail["node"]) => void;
  onMenu: (event: React.MouseEvent, node: NodeDetail["node"]) => void;
  coverRevision: number;
}

export function WorkDetailPage({ detail, loading, rootLabel, onRoot, onBreadcrumb, onBack, onBangumi, onOpenBangumi, onRetryCover, onRetryCoverNode, onClearBangumi, onPlay, onRevealMedia, onOpenResource, onRevealResource, onOpenChild, onBangumiNode, onMenu, coverRevision, onReadComic }: WorkDetailPageProps) {
  const { t } = useI18n();
  const episodeWatch=useEpisodeWatch(detail);
  const coverNode = detail ? { ...detail.node, binding: detail.binding } : null;
  const { coverFrameRef, coverCacheKey, coverUrl: cover, coverFailed: coverReadFailed, coverLoading, onCoverError } = useCoverDataUrl(coverNode, coverRevision);
  const [metadataError,setMetadataError]=useState('');
  const [imageFailed, setImageFailed] = useState(false);
  const [metadataSource,setMetadataSource]=useState<number|null>(null);
  useEffect(() => setImageFailed(false), [cover]);
  const firstPlayable=useMemo(()=>{
    const sections=new Map(detail?.videoSections?.map(e=>[e.fileId,e.section]));
    const hasMain=detail?.mediaFiles.some(file=>(sections.get(file.id)??'MAIN')==='MAIN')||detail?.nestedMediaFiles?.some(entry=>(sections.get(entry.file.id)??'OTHER')==='MAIN');
    const eligible=(file:MediaFile,nested=false)=>!hasMain||(sections.get(file.id)??(nested?'OTHER':'MAIN'))==='MAIN';
    let first:MediaFile|undefined;
    for(const file of (detail?.mediaFiles??[]).filter(file=>eligible(file)))if(!first||(naturalCompare(file.fileName,first.fileName)||file.id-first.id)<0)first=file;
    if(first)return first;
    let directory='';
    for(const entry of (detail?.nestedMediaFiles??[]).filter(e=>eligible(e.file,true)))if(!first||(naturalCompare(entry.relativeDirectory,directory)||naturalCompare(entry.file.fileName,first.fileName)||entry.file.id-first.id)<0){first=entry.file;directory=entry.relativeDirectory;}
    return first;
  },[detail?.mediaFiles,detail?.nestedMediaFiles,detail?.videoSections]);
  if (!detail) return <LoadingState label={t("detail.loading")} />;
  const { node, binding, mediaFiles, children, workSources } = detail;
  const reading = readingFiles(detail.comicBooks ?? [], detail.resourceFiles);
  const resourceFiles = reading.other;
  const nested = detail.nestedMediaFiles ?? [];
  const displayNode = binding ? { ...node, binding } : node;
  const stateNode = !node.workView ? node : workSources?.length===1 ? workSources[0] : workSources?.find(source=>source.id===metadataSource)??null;
  const title = nodeDisplayTitle(displayNode);

  const isContainer = node.nodeType === "CONTAINER";
  const displayVideoCount = isContainer ? Math.max(node.totalVideoCount??0,mediaFiles.length+nested.length,episodeWatch.data?.rows.length??0) : mediaFiles.length+nested.length;
  const bindable = canBindBangumi(node);
  const resourceOnlyContainer = isContainer && !nodeHasVideo(node);
  const coverFailed = coverReadFailed || imageFailed;
  const coverError = binding != null && !coverLoading && (Boolean(binding.coverDownloadError) || !cover || coverFailed);
  return (
    <section className="detail-page" aria-busy={loading}>
      <header className="detail-toolbar"><button className="back-button" onClick={onBack} type="button"><Icon name="arrow-left" />{t("detail.back")}</button><Breadcrumb rootLabel={rootLabel} items={detail.breadcrumbs} currentNodeId={node.id} onRoot={onRoot} onNode={onBreadcrumb} /></header>
      <div className="detail-content" inert={loading}>
        {metadataError&&<p className="inline-error" role="alert">{metadataError}</p>}
        <section className="detail-hero">
          <div ref={coverFrameRef} className={`detail-cover ${cover && !coverFailed ? "has-cover" : ""}`}>{cover && !coverFailed ? <PosterImage alt={t("detail.coverAlt", { title })} cacheKey={coverCacheKey} onError={() => {setImageFailed(true);onCoverError?.();}} src={cover} /> : <><Icon name={isContainer ? "folder-open" : "work"} /><span>{coverError ? t("detail.coverFailed") : isContainer ? t("detail.resourceContainer") : t("detail.noCover")}</span>{bindable && (coverError ? <button onClick={() => onRetryCover(imageFailed)} type="button"><Icon name="refresh" />{t("detail.retrieveAgain")}</button> : <button onClick={onBangumi} type="button"><Icon name="plus" />{t("provider.bangumi")}</button>)}</>}</div>
          <div className="detail-copy">
            <p className="eyebrow">{nodeTypeLabel(node.nodeType)}{node.manualTypeOverride && ` · ${t("node.manual")}`}</p>
            <h1>{title}</h1>
            {!workSources && <p className="detail-folder-name" title={node.folderName}>{node.folderName}</p>}
            {!workSources && <p className="detail-path" title={node.absolutePath}>{node.absolutePath}</p>}
            <div className="detail-stats"><span><strong>{displayVideoCount}</strong> {t("detail.totalWorkVideos")}</span>{reading.count>0&&<DetailCount count={reading.count} message="comic.readableCount"/>}<span><strong>{resourceFiles.length}</strong> {t("detail.otherFileCount", { count: "" }).trim()}</span><span><strong>{children.length}</strong> {t("detail.childDirectoryCount", { count: "" }).trim()}</span></div>
          <div className="detail-actions">{firstPlayable&&<button className="button primary" type="button" onClick={()=>void episodeWatch.playPreferred([...mediaFiles,...nested.map(entry=>entry.file)],firstPlayable,onPlay)} aria-label={t("files.playNamed",{name:episodeWatch.data?.latestWatchedFileName??firstPlayable.fileName})} title={episodeWatch.data?.latestWatchedFileName??firstPlayable.fileName}><Icon name="play"/>{t("layout.play")}</button>}{node.workView&&workSources&&workSources.length>1&&<Select className="detail-book-source" aria-label={t("works.chooseSource")} value={stateNode?.id??""} onChange={event=>setMetadataSource(Number(event.target.value)||null)}><option value="">{t("works.chooseSource")}</option>{workSources.map(source=><option key={source.id} value={source.id}>{source.folderName}</option>)}</Select>}{stateNode&&editableDetailNode(stateNode)&&<><CollectionStatusControl target={{kind:'NODE',id:stateNode.id}}/><CoverButton target={{kind:'NODE',id:stateNode.id}}/></>}<button className="button ghost detail-more" onClick={(event) => onMenu(event, node)} aria-label={t("detail.organize")} title={t("detail.organize")} type="button"><Icon name="more" /></button></div>
            {bindable && !node.tmdbBinding?.active && <MetadataBindingPanel provider="Bangumi" label={binding?t("detail.bound", {id:binding.providerSubjectId}):t("provider.bangumi")} title={binding?bindingDisplayTitle(displayNode)??title:t("detail.unbound")} openLabel={t("detail.openBangumi")} onOpen={binding?onOpenBangumi:undefined} error={coverError?<><Icon name="warning"/>{t("detail.bindingCoverFailed")}</>:undefined}>
              <button className="button secondary" onClick={onBangumi} type="button">{binding ? t("detail.changeBinding") : t("detail.searchAdd")}</button>
              {binding && coverError && <button className="icon-button" aria-label={t("detail.retryCoverAria")} onClick={() => onRetryCover(imageFailed)} title={t("detail.retryCover")} type="button"><Icon name="refresh" /></button>}
              {binding && <button className="icon-button" aria-label={t("detail.clearBindingAria")} onClick={onClearBangumi} title={t("detail.clearBinding")} type="button"><Icon name="trash" /></button>}
            </MetadataBindingPanel>}
            {node.tmdbBinding?.active&&<MetadataBindingPanel provider="TMDb" label={t('tmdb.bound',{id:node.tmdbBinding.movie.id})} title={node.tmdbBinding.movie.title} openLabel={t('tmdb.openMovie')} onOpen={()=>void api.tmdbOpenPage(node.id).catch(()=>setMetadataError(t('tmdb.error')))} error={node.tmdbBinding.coverError?t('detail.bindingCoverFailed'):undefined}>
              <button className="button secondary" onClick={onBangumi} type="button">{t('detail.changeBinding')}</button>
              {node.tmdbBinding.coverError&&<button className="icon-button" aria-label={t('detail.retryCover')} title={t('detail.retryCover')} type="button" onClick={()=>void api.tmdbRetryCover(node.id).then(()=>window.dispatchEvent(new CustomEvent('m2shelf-metadata-changed'))).catch(()=>setMetadataError(t('tmdb.error')))}><Icon name="refresh"/></button>}
              <button className="icon-button" aria-label={t('tmdb.clear')} title={t('tmdb.clear')} onClick={()=>void api.tmdbClear(node.id).then(()=>window.dispatchEvent(new CustomEvent('m2shelf-metadata-changed'))).catch(()=>setMetadataError(t('tmdb.error')))} type="button"><Icon name="trash"/></button>
            </MetadataBindingPanel>}
          </div>
        </section>

        <DetailUtility note={stateNode&&editableDetailNode(stateNode)&&<PrivateNote key={`node-${stateNode.id}`} target={{kind:'NODE',id:stateNode.id}} detail/>} progress={mediaFiles.length+nested.length>0||Boolean(episodeWatch.data&&(episodeWatch.data.totalEpisodes+episodeWatch.data.totalFiles)>0)||episodeWatch.error?<EpisodeWatchSummary watch={episodeWatch}/>:null}/>
        {(mediaFiles.length+nested.length>0)&&<DetailVideoTabs detail={detail} watch={episodeWatch} onPlay={onPlay} onReveal={onRevealMedia}/>}
        {mediaFiles.length+nested.length===0&&!isContainer&&<EmptyState compact icon="file" title={t('detail.noVideoTitle')} description={t('detail.noVideoDescription')}/>}

        {isContainer && !resourceOnlyContainer && children.length > 0 && <section className="content-section child-section"><div className="section-heading"><div><p className="eyebrow">{t("detail.children")}</p><h2>{t("detail.continueSeries")}</h2></div><span>{t("browse.nodeCount", { count: children.length })}</span></div><PosterGrid nodes={children} viewMode="grid" onOpen={onOpenChild} onMenu={onMenu} onBangumi={onBangumiNode} onRetryCover={onRetryCoverNode} coverRevision={coverRevision} /></section>}

        {reading.count>0&&<section className="content-section comic-reading-section"><div className="section-heading"><div><p className="eyebrow">{t('book.readableFiles')}</p><h2>{t('comic.doubleClickRead')}</h2></div><span>{t('comic.books',{count:reading.count})}</span></div><ComicBookList books={reading.books} resources={reading.readable} onRead={(book,list)=>onReadComic?.(book,list)} onReadResource={onOpenResource} onRevealResource={onRevealResource}/></section>}

        {!isContainer && (children.length > 0 || resourceFiles.length > 0) && <section className="content-section child-section"><div className="section-heading"><div><p className="eyebrow">{t("detail.otherResources")}</p><h2>{t("detail.workResources")}</h2></div><span>{t("detail.itemCount", { count: children.length + resourceFiles.length })}</span></div><OtherResourceList expandedFolderIds={detail.expandedFolderIds} files={resourceFiles} folders={children} onOpenFile={onOpenResource} onRevealFile={onRevealResource} onOpenFolder={onOpenChild} onFolderMenu={onMenu} /></section>}
        {resourceOnlyContainer && (children.length > 0 || resourceFiles.length > 0) && <section className="content-section child-section"><div className="section-heading"><div><p className="eyebrow">{t("detail.directoryContent")}</p><h2>{t("detail.browseFilesFolders")}</h2></div><span>{t("detail.itemCount", { count: children.length + resourceFiles.length })}</span></div><OtherResourceList files={resourceFiles} folders={children} onOpenFile={onOpenResource} onRevealFile={onRevealResource} onOpenFolder={onOpenChild} onFolderMenu={onMenu} /></section>}
        {isContainer && !resourceOnlyContainer && resourceFiles.length > 0 && <section className="content-section child-section"><div className="section-heading"><div><p className="eyebrow">{t("detail.otherResources")}</p><h2>{t("detail.containerAttachments")}</h2></div><span>{t("browse.fileCount", { count: resourceFiles.length })}</span></div><OtherResourceList files={resourceFiles} folders={[]} onOpenFile={onOpenResource} onRevealFile={onRevealResource} onOpenFolder={onOpenChild} onFolderMenu={onMenu} /></section>}
        {(detail.recognitionWarnings?.length ?? 0) > 0 && <p className="preview-banner">{t("works.recognitionWarning")}</p>}
        {workSources && <details className="content-section work-sources"><summary>{t("works.sources")} · {workSources.length}</summary><p>{t("works.wholeGroup", { count: workSources.length })}</p><p>{t("works.sourcesHelp")}</p><OtherResourceList files={[]} folders={workSources} onOpenFile={onOpenResource} onRevealFile={onRevealResource} onOpenFolder={onOpenChild} onFolderMenu={onMenu} /></details>}
      </div>
    </section>
  );
}

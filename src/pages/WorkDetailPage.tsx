import { useEffect, useState } from "react";
import type { MediaFile, NodeDetail, ResourceFile } from "../types/media";
import { Breadcrumb } from "../components/Breadcrumb";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { LoadingState } from "../components/LoadingState";
import { MediaFileList } from "../components/MediaFileList";
import { PosterGrid } from "../components/PosterGrid";
import { OtherResourceList } from "../components/OtherResourceList";
import { bindingDisplayTitle, canBindBangumi, nodeDisplayTitle, nodeHasVideo, nodeTypeLabel } from "../lib/format";
import { useCoverDataUrl } from "../hooks/useCoverDataUrl";
import { useI18n } from "../lib/i18n";
import { PosterImage } from "../components/PosterImage";

interface WorkDetailPageProps {
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
  onPlay: (file: MediaFile) => void;
  onRevealMedia: (file: MediaFile) => void;
  onOpenResource: (file: ResourceFile) => void;
  onRevealResource: (file: ResourceFile) => void;
  onOpenChild: (node: NodeDetail["node"]) => void;
  onBangumiNode: (node: NodeDetail["node"]) => void;
  onMenu: (event: React.MouseEvent, node: NodeDetail["node"]) => void;
  coverRevision: number;
}

export function WorkDetailPage({ detail, loading, rootLabel, onRoot, onBreadcrumb, onBack, onBangumi, onOpenBangumi, onRetryCover, onRetryCoverNode, onClearBangumi, onReveal, onPlay, onRevealMedia, onOpenResource, onRevealResource, onOpenChild, onBangumiNode, onMenu, coverRevision }: WorkDetailPageProps) {
  const { t } = useI18n();
  const coverNode = detail ? { ...detail.node, binding: detail.binding } : null;
  const { coverCacheKey, coverUrl: cover, coverFailed: coverReadFailed, coverLoading } = useCoverDataUrl(coverNode, coverRevision);
  const [imageFailed, setImageFailed] = useState(false);
  useEffect(() => setImageFailed(false), [cover]);
  if (loading || !detail) return <LoadingState label={t("detail.loading")} />;
  const { node, binding, mediaFiles, resourceFiles, children, workSources } = detail;
  const nested = detail.nestedMediaFiles ?? [];
  const directPaths = workSources && workSources.length > 1 ? Object.fromEntries(mediaFiles.map(file => [file.id, workSources.find(source => source.id === file.nodeId)?.folderName ?? ""])) : undefined;
  const directorySortKeys = Object.fromEntries(nested.map(entry => [entry.file.id, entry.relativeDirectory]));
  const nestedPaths = Object.fromEntries(nested.map(entry => [entry.file.id, `${workSources && workSources.length > 1 ? `${entry.sourceName} / ` : ""}${entry.relativeDirectory}`]));
  const displayNode = binding ? { ...node, binding } : node;
  const title = nodeDisplayTitle(displayNode);
  const isContainer = node.nodeType === "CONTAINER";
  const bindable = canBindBangumi(node);
  const resourceOnlyContainer = isContainer && !nodeHasVideo(node);
  const coverFailed = coverReadFailed || imageFailed;
  const coverError = binding != null && !coverLoading && (Boolean(binding.coverDownloadError) || !cover || coverFailed);
  return (
    <section className="detail-page">
      <header className="detail-toolbar"><button className="back-button" onClick={onBack} type="button"><Icon name="arrow-left" />{t("detail.back")}</button><Breadcrumb rootLabel={rootLabel} items={detail.breadcrumbs} currentNodeId={node.id} onRoot={onRoot} onNode={onBreadcrumb} /></header>
      <div className="detail-content">
        <section className="detail-hero">
          <div className={`detail-cover ${cover && !coverFailed ? "has-cover" : ""}`}>{cover && !coverFailed ? <PosterImage alt={t("detail.coverAlt", { title })} cacheKey={coverCacheKey} onError={() => setImageFailed(true)} src={cover} /> : <><Icon name={isContainer ? "folder-open" : "work"} /><span>{coverError ? t("detail.coverFailed") : isContainer ? t("detail.resourceContainer") : t("detail.noCover")}</span>{bindable && (coverError ? <button onClick={() => onRetryCover(imageFailed)} type="button"><Icon name="refresh" />{t("detail.retrieveAgain")}</button> : <button onClick={onBangumi} type="button"><Icon name="plus" />{t("provider.bangumi")}</button>)}</>}</div>
          <div className="detail-copy">
            <p className="eyebrow">{nodeTypeLabel(node.nodeType)}{node.manualTypeOverride && ` · ${t("node.manual")}`}</p>
            <h1>{title}</h1>
            {!workSources && <p className="detail-folder-name">{node.folderName}</p>}
            {!workSources && <p className="detail-path" title={node.absolutePath}>{node.absolutePath}</p>}
            <div className="detail-stats"><span><strong>{mediaFiles.length + nested.length}</strong> {t("detail.totalWorkVideos")}</span><span><strong>{resourceFiles.length}</strong> {t("detail.otherFileCount", { count: "" }).trim()}</span><span><strong>{children.length}</strong> {t("detail.childDirectoryCount", { count: "" }).trim()}</span></div>
            {bindable && <div className="binding-panel">
              <span className="binding-logo"><Icon name="bangumi" /></span>
              {binding ? <div><small>{t("detail.bound", { id: binding.providerSubjectId })}</small><button className="binding-subject-link" type="button" onClick={onOpenBangumi} title={t("detail.openBangumi")} aria-label={t("detail.openBangumi")}><strong>{bindingDisplayTitle(displayNode) ?? title}</strong><span>{t("detail.openBangumi")}</span></button>{coverError && <em><Icon name="warning" />{t("detail.bindingCoverFailed")}</em>}</div> : <div><small>{t("provider.bangumi")}</small><strong>{t("detail.unbound")}</strong></div>}
              <button className="button secondary" onClick={onBangumi} type="button">{binding ? t("detail.changeBinding") : t("detail.searchAdd")}</button>
              {binding && coverError && <button className="icon-button" aria-label={t("detail.retryCoverAria")} onClick={() => onRetryCover(imageFailed)} title={t("detail.retryCover")} type="button"><Icon name="refresh" /></button>}
              {binding && <button className="icon-button" aria-label={t("detail.clearBindingAria")} onClick={onClearBangumi} title={t("detail.clearBinding")} type="button"><Icon name="trash" /></button>}
            </div>}
            <div className="detail-actions"><button className="button secondary" onClick={onReveal} type="button"><Icon name="external" />{t("detail.openExplorer")}</button><button className="button ghost" onClick={(event) => onMenu(event, node)} type="button"><Icon name="more" />{t("detail.organize")}</button></div>
          </div>
        </section>

        {(mediaFiles.length > 0 || !isContainer) && <section className="content-section">
          <div className="section-heading"><div><p className="eyebrow">{t("detail.videoFiles")}</p><h2>{mediaFiles.length ? t("detail.doubleClickReady") : t("detail.noDirectVideo")}</h2></div><span>{t("browse.videoCount", { count: mediaFiles.length })}</span></div>
          {mediaFiles.length ? <MediaFileList files={mediaFiles} relativePaths={directPaths} onPlay={onPlay} onReveal={onRevealMedia} /> : <EmptyState compact icon="file" title={t("detail.noVideoTitle")} description={t("detail.noVideoDescription")} />}
        </section>}

        {!isContainer && (children.length > 0 || resourceFiles.length > 0 || nested.length > 0) && <section className="content-section child-section"><div className="section-heading"><div><p className="eyebrow">{t("detail.otherResources")}</p><h2>{t("detail.workResources")}</h2></div><span>{t("detail.itemCount", { count: children.length + resourceFiles.length + nested.length })}</span></div>{nested.length > 0 && <><h3>{t("detail.nestedVideos")}</h3><MediaFileList files={nested.map(entry => entry.file)} relativePaths={nestedPaths} directorySortKeys={directorySortKeys} onPlay={onPlay} onReveal={onRevealMedia} /></>}<OtherResourceList expandedFolderIds={detail.expandedFolderIds} files={resourceFiles} folders={children} onOpenFile={onOpenResource} onRevealFile={onRevealResource} onOpenFolder={onOpenChild} onFolderMenu={onMenu} /></section>}
        {resourceOnlyContainer && (children.length > 0 || resourceFiles.length > 0) && <section className="content-section child-section"><div className="section-heading"><div><p className="eyebrow">{t("detail.directoryContent")}</p><h2>{t("detail.browseFilesFolders")}</h2></div><span>{t("detail.itemCount", { count: children.length + resourceFiles.length })}</span></div><OtherResourceList files={resourceFiles} folders={children} onOpenFile={onOpenResource} onRevealFile={onRevealResource} onOpenFolder={onOpenChild} onFolderMenu={onMenu} /></section>}
        {isContainer && !resourceOnlyContainer && resourceFiles.length > 0 && <section className="content-section child-section"><div className="section-heading"><div><p className="eyebrow">{t("detail.otherResources")}</p><h2>{t("detail.containerAttachments")}</h2></div><span>{t("browse.fileCount", { count: resourceFiles.length })}</span></div><OtherResourceList files={resourceFiles} folders={[]} onOpenFile={onOpenResource} onRevealFile={onRevealResource} onOpenFolder={onOpenChild} onFolderMenu={onMenu} /></section>}
        {isContainer && !resourceOnlyContainer && children.length > 0 && <section className="content-section child-section"><div className="section-heading"><div><p className="eyebrow">{t("detail.children")}</p><h2>{t("detail.continueSeries")}</h2></div><span>{t("browse.nodeCount", { count: children.length })}</span></div><PosterGrid nodes={children} viewMode="grid" onOpen={onOpenChild} onMenu={onMenu} onBangumi={onBangumiNode} onRetryCover={onRetryCoverNode} coverRevision={coverRevision} /></section>}
        {(detail.recognitionWarnings?.length ?? 0) > 0 && <p className="preview-banner">{t("works.recognitionWarning")}</p>}
        {workSources && <details className="content-section work-sources"><summary>{t("works.sources")} · {workSources.length}</summary><p>{t("works.wholeGroup", { count: workSources.length })}</p><p>{t("works.sourcesHelp")}</p><OtherResourceList files={[]} folders={workSources} onOpenFile={onOpenResource} onRevealFile={onRevealResource} onOpenFolder={onOpenChild} onFolderMenu={onMenu} /></details>}
      </div>
    </section>
  );
}

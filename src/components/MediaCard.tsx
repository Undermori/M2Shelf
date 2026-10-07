import { memo, useEffect, useRef, useState } from "react";
import type { MediaNode, ViewMode } from "../types/media";
import { canBindBangumi, formatDate, nodeDisplayTitle, mediaBadge } from "../lib/format";
import { FileModifiedTime } from "./FileModifiedTime";
import { Icon } from "./Icon";
import { useCoverDataUrl } from "../hooks/useCoverDataUrl";
import { usePosterViewportLifecycle } from "../hooks/usePosterViewportLifecycle";
import { useI18n } from "../lib/i18n";
import { PosterImage } from "./PosterImage";

interface MediaCardProps {
  node: MediaNode;
  viewMode: ViewMode;
  onOpen: (node: MediaNode) => void;
  onMenu: (event: React.MouseEvent, node: MediaNode) => void;
  onBangumi: (node: MediaNode) => void;
  onRetryCover: (node: MediaNode, imageDecodeFailed?: boolean) => void;
  coverRevision: number;
  watchedAt?: string;
  showModifiedTime?: boolean;
  editMode?: boolean;
  selected?: boolean;
  onSelect?: (node: MediaNode) => void;
}

function MediaCardComponent({ node, viewMode, onOpen, onMenu, onBangumi, onRetryCover, coverRevision, watchedAt, showModifiedTime = false, editMode = false, selected = false, onSelect }: MediaCardProps) {
  const { t } = useI18n();
  const cardRef = useRef<HTMLElement>(null);
  const hasCachedCover = Boolean(node.coverCachePath ?? node.binding?.coverCachePath);
  const { coverRequested, coverVisible } = usePosterViewportLifecycle(cardRef, node.id, {
    activationMarginPx: 1_000,
    retentionEnabled: hasCachedCover,
    retentionMarginPx: 1_800,
  });
  const { coverCacheKey, coverUrl: cover, coverFailed: coverReadFailed, coverLoading } = useCoverDataUrl(node, coverRevision, coverRequested);
  const [imageFailed, setImageFailed] = useState(false);
  useEffect(() => setImageFailed(false), [cover]);
  const videos = node.totalVideoCount ?? node.directVideoCount ?? 0;
  const container = node.nodeType === "CONTAINER" || node.nodeType === "MIXED";
  const systemTag = node.nodeType === "CONTAINER"
    ? { icon: "folder" as const, label: t("card.systemSeries") }
    : node.nodeType === "MIXED"
      ? { icon: "archive" as const, label: t("card.systemOtherResources") }
      : { icon: "work" as const, label: t("card.systemWork") };
  const bindable = canBindBangumi(node);
  const title = nodeDisplayTitle(node);
  const coverFailed = coverReadFailed || imageFailed;
  const coverError = node.binding != null && !coverLoading && (Boolean(node.binding.coverDownloadError) || !cover || coverFailed);

  return (
    <article
      ref={cardRef}
      className={`media-card media-card-${viewMode} ${watchedAt ? "has-watch-time" : ""} ${showModifiedTime ? "has-modified-time" : ""} ${editMode ? "is-editing" : ""} ${selected ? "is-selected" : ""}`}
      onContextMenu={(event) => { event.preventDefault(); onMenu(event, node); }}
    >
      <button aria-pressed={editMode ? selected : undefined} className="media-card-open" onClick={() => editMode ? onSelect?.(node) : onOpen(node)} type="button">
        <span className={`cover-frame ${cover ? "has-cover" : ""} ${container ? "is-container" : ""}`}>
          {cover && !coverFailed ? <PosterImage active={coverVisible} alt={t("card.coverAlt", { title })} cacheKey={coverCacheKey} onError={() => setImageFailed(true)} src={cover} /> : (
            <span className="cover-placeholder">
              <span className="cover-art"><Icon name={container ? "folder-open" : "work"} /></span>
              <small>{coverError ? t("card.coverFailed") : container ? t("card.resourceContainer") : t("card.noCover")}</small>
            </span>
          )}
          <span className="type-pill system-tag"><Icon name={systemTag.icon} />{mediaBadge(node)}</span>
          {editMode && <span aria-hidden="true" className="selection-indicator"><Icon name={selected ? "check" : "plus"} /></span>}
        </span>
        <span className="media-card-copy">
          <strong title={title}>{title}</strong>
          {watchedAt && <time className="media-card-watch-time" dateTime={watchedAt}>{t("comic.openedAt", { time: formatDate(watchedAt) })}</time>}
          {showModifiedTime && <FileModifiedTime value={node.latestFileModifiedAt} />}
          <small>
            {(node.mediaKind==='COMIC'||node.mediaKind==='EBOOK')?t('comic.books',{count:node.totalComicBookCount??0}):videos > 0 ? t("card.videoCount", { count: videos }) : container ? t("card.childCount", { count: node.childMediaBranchCount ?? 0 }) : t("card.awaitingScan")}
          </small>
          {viewMode === "list" && !node.binding && <span className="card-path">{node.absolutePath}</span>}
          {(node.userTags?.length ?? 0) > 0 && (
            <span aria-label={t("card.customTags")} className="media-card-tags">
              {node.userTags.map((tag) => <span className="user-tag-pill" key={tag.id}>{tag.name}</span>)}
            </span>
          )}
        </span>
      </button>
      {!editMode && bindable && !node.binding && (
        <button className="quick-bind" onClick={() => onBangumi(node)} title={t("card.searchCover")} type="button"><Icon name="plus" /><span>{t("provider.bangumi")}</span></button>
      )}
      {!editMode && bindable && node.binding && coverError && (
        <button className="quick-bind is-retry" onClick={() => onRetryCover(node, imageFailed)} title={t("card.retryCover")} type="button"><Icon name="refresh" /><span>{t("card.retryShort")}</span></button>
      )}
      {!editMode && <button className="card-menu" aria-label={t("card.moreActions", { title })} onClick={(event) => onMenu(event, node)} type="button"><Icon name="more" /></button>}
    </article>
  );
}

export const MediaCard = memo(MediaCardComponent);

import type { CollectionSort, MediaFile } from "../types/media";
import { compareFileModifiedTimes, formatBytes, formatDuration, naturalCompare } from "../lib/format";
import { FileModifiedTime } from "./FileModifiedTime";
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";

interface MediaFileListProps {
  modifiedSort?: CollectionSort;
  relativePaths?: Record<number, string>;
  directorySortKeys?: Record<number, string>;
  files: MediaFile[];
  onPlay: (file: MediaFile) => void;
  onReveal: (file: MediaFile) => void;
}

export function MediaFileList({ relativePaths, directorySortKeys, modifiedSort, files, onPlay, onReveal }: MediaFileListProps) {
  const { t } = useI18n();
  const sorted = [...files].sort((a, b) => (modifiedSort ? compareFileModifiedTimes(a.modifiedAt, b.modifiedAt, modifiedSort) : 0) || (directorySortKeys ? naturalCompare(directorySortKeys[a.id] ?? "", directorySortKeys[b.id] ?? "") : 0) || naturalCompare(a.fileName, b.fileName) || a.id - b.id);
  return (
    <div className="media-file-list">
      <div className="file-list-heading"><span>#</span><span>{t("files.fileName")}</span><span>{t("files.info")}</span><span /></div>
      {sorted.map((file, index) => {
        const duration = formatDuration(file.durationMs);
        return (
          <div
            className="media-file-row"
            key={file.id}
            onDoubleClick={(event) => { event.preventDefault(); onPlay(file); }}
            onMouseDown={(event) => { if (event.detail > 1) event.preventDefault(); }}
            title={`${file.absolutePath} · ${t("files.doubleClickPlay")}`}
          >
            <span className="file-index">{String(index + 1).padStart(2, "0")}</span>
            <span className="file-name"><Icon name="file" /><span><strong>{file.fileName}</strong>{modifiedSort && <FileModifiedTime value={file.modifiedAt} />}<small>{relativePaths?.[file.id] && <span className="file-origin">{relativePaths[file.id]} · </span>}{file.absolutePath}</small></span></span>
            <span className="file-meta">{duration && <span>{duration}</span>}<span>{formatBytes(file.fileSize)}</span><span>{file.extension.replace(/^\./, "").toUpperCase()}</span></span>
            <span className="file-actions">
              <button aria-label={t("files.playNamed", { name: relativePaths?.[file.id] ? `${relativePaths[file.id]} / ${file.fileName}` : file.fileName })} onClick={(event) => { event.stopPropagation(); onPlay(file); }} title={t("files.play")} type="button"><Icon name="play" /></button>
              <button aria-label={t("files.revealNamed", { name: relativePaths?.[file.id] ? `${relativePaths[file.id]} / ${file.fileName}` : file.fileName })} onClick={(event) => { event.stopPropagation(); onReveal(file); }} title={t("files.reveal")} type="button"><Icon name="external" /></button>
            </span>
          </div>
        );
      })}
    </div>
  );
}

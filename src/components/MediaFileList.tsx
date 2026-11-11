import {EpisodeWatchControl,type EpisodeWatchController} from './EpisodeWatchControl';
import type { CollectionSort, MediaFile } from "../types/media";
import { compareFileModifiedTimes, formatBytes, formatDuration, naturalCompare } from "../lib/format";
import { FileModifiedTime } from "./FileModifiedTime";
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";
import {useMemo} from 'react';

interface MediaFileListProps {
  watch?: EpisodeWatchController;
  modifiedSort?: CollectionSort;
  relativePaths?: Record<number, string>;
  directorySortKeys?: Record<number, string>;
  files: MediaFile[];
  onPlay: (file: MediaFile) => void | boolean | Promise<boolean>;
  onReveal: (file: MediaFile) => void;
}

export function MediaFileList({ watch, relativePaths, directorySortKeys, modifiedSort, files, onPlay, onReveal }: MediaFileListProps) {
  const { t,language } = useI18n();
  const states=useMemo(()=>new Map(watch?.data?.rows.map(row=>[row.fileId,row])??[]),[watch?.data?.rows]);
  const sorted = useMemo(()=>[...files].sort((a, b) => (modifiedSort ? compareFileModifiedTimes(a.modifiedAt, b.modifiedAt, modifiedSort) : 0) || (directorySortKeys ? naturalCompare(directorySortKeys[a.id] ?? "", directorySortKeys[b.id] ?? "",language) : 0) || naturalCompare(a.fileName, b.fileName,language) || a.id - b.id),[files,modifiedSort,directorySortKeys,language]);
  return (
    <div className={`media-file-list${watch?" has-episode-watch":""}`}>
      <div className="file-list-heading"><span>#</span><span>{t("files.fileName")}</span><span>{t("files.info")}</span><span /></div>
      {sorted.map((file, index) => {
        const duration = formatDuration(file.durationMs);
        return (
          <div
            className={`media-file-row${!modifiedSort&&!file.durationMs?' is-layout-contained':''}`}
            key={file.id}
            onDoubleClick={(event) => { if(event.target instanceof Element&&event.target.closest('button'))return;event.preventDefault(); if(watch)void watch.playAndMark(file.id,()=>onPlay(file));else void onPlay(file); }}
            onMouseDown={(event) => { if (event.detail > 1) event.preventDefault(); }}
            title={`${file.absolutePath} · ${t("files.doubleClickPlay")}`}
          >
            <span className="file-index">{String(index + 1).padStart(2, "0")}</span>
            <span className="file-name"><Icon name="file" /><span><strong>{file.fileName}</strong>{modifiedSort && <FileModifiedTime value={file.modifiedAt} />}{relativePaths?.[file.id]&&<small className="file-origin">{relativePaths[file.id]}</small>}<small className="file-path">{file.absolutePath}</small></span></span>
            <span className="file-meta">{duration && <span>{duration}</span>}<span>{formatBytes(file.fileSize)}</span><span>{file.extension.replace(/^\./, "").toUpperCase()}</span></span>
            <span className="file-actions">
              {watch&&states.has(file.id)&&<EpisodeWatchControl row={states.get(file.id)!} name={file.fileName} busy={watch.busy} onChange={watched=>void watch.mark(file.id,watched)}/>}
              <button aria-label={t("files.playNamed", { name: relativePaths?.[file.id] ? `${relativePaths[file.id]} / ${file.fileName}` : file.fileName })} onClick={(event) => { event.stopPropagation(); onPlay(file); }} title={t("files.play")} type="button"><Icon name="play" /></button>
              <button aria-label={t("files.revealNamed", { name: relativePaths?.[file.id] ? `${relativePaths[file.id]} / ${file.fileName}` : file.fileName })} onClick={(event) => { event.stopPropagation(); onReveal(file); }} title={t("files.reveal")} type="button"><Icon name="external" /></button>
            </span>
          </div>
        );
      })}
    </div>
  );
}

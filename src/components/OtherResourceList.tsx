import type { IconName } from "./Icon";
import { FileModifiedTime } from "./FileModifiedTime";
import { Icon } from "./Icon";
import type { CollectionSort, MediaNode, ResourceFile } from "../types/media";
import { compareFileModifiedTimes, compactPath, formatBytes, naturalCompare, resourceTypeLabel } from "../lib/format";
import { useI18n } from "../lib/i18n";

interface OtherResourceListProps {
  modifiedSort?: CollectionSort;
  expandedFolderIds?: number[];
  files: ResourceFile[];
  folders: MediaNode[];
  onOpenFile: (file: ResourceFile) => void;
  onRevealFile: (file: ResourceFile) => void;
  onOpenFolder: (node: MediaNode) => void;
  onFolderMenu: (event: React.MouseEvent, node: MediaNode) => void;
}

const resourceIcons: Record<ResourceFile["resourceType"], IconName> = {
  DOCUMENT: "file",
  IMAGE: "image",
  AUDIO: "audio",
  SUBTITLE: "subtitle",
  ARCHIVE: "archive",
  FONT: "font",
  PLAYLIST: "list",
  OTHER: "file",
};

export function OtherResourceList({ expandedFolderIds, modifiedSort, files, folders, onOpenFile, onRevealFile, onOpenFolder, onFolderMenu }: OtherResourceListProps) {
  const { t } = useI18n();
  const sortedFolders = [...folders].sort((a, b) => naturalCompare(a.folderName, b.folderName));
  const sortedFiles = [...files].sort((a, b) => (modifiedSort ? compareFileModifiedTimes(a.modifiedAt, b.modifiedAt, modifiedSort) : 0) || naturalCompare(a.fileName, b.fileName));

  return (
    <div className="other-resource-list">
      {sortedFolders.map((folder) => (
        <article
          className="resource-row is-directory"
          key={`folder-${folder.id}`}
          onContextMenu={(event) => { event.preventDefault(); onFolderMenu(event, folder); }}
        >
          <button className="resource-open" onClick={() => onOpenFolder(folder)} type="button">
            <span className="resource-icon"><Icon name="folder-open" /></span>
            <span className="resource-copy">
              <strong>{folder.folderName}</strong>
              <small>{t("resources.folder")} · {(folder.mediaKind === 'COMIC' || folder.mediaKind === 'EBOOK') && (folder.totalComicBookCount ?? 0) > 0 ? t('comic.readableCount', {count: folder.totalComicBookCount ?? 0}) : (folder.totalVideoCount ?? 0) > 0 ? t("resources.videoCount", { count: folder.totalVideoCount ?? 0 }) : t("resources.attachmentDirectory")}</small>
              {expandedFolderIds?.includes(folder.id) && <small>{t(folder.mediaKind === 'COMIC' || folder.mediaKind === 'EBOOK' ? 'comic.expandedBooks' : "resources.expandedVideos")}</small>}
              <em title={folder.absolutePath}>{compactPath(folder.absolutePath, 96)}</em>
            </span>
            <Icon className="resource-chevron" name="chevron" />
          </button>
          <button className="resource-more" aria-label={t("resources.moreActions", { name: folder.folderName })} onClick={(event) => onFolderMenu(event, folder)} type="button"><Icon name="more" /></button>
        </article>
      ))}
      {sortedFiles.map((file) => (
        <article className={`resource-row resource-${file.resourceType.toLocaleLowerCase()}`} key={`file-${file.id}`}>
          <button className="resource-open" onDoubleClick={() => onOpenFile(file)} title={t("resources.doubleClickDefault")} type="button">
            <span className="resource-icon"><Icon name={resourceIcons[file.resourceType]} /></span>
            <span className="resource-copy">
              <strong>{file.fileName}</strong>
              {modifiedSort && <FileModifiedTime value={file.modifiedAt} />}
              <small>{resourceTypeLabel(file.resourceType, file.extension)} · {formatBytes(file.fileSize)}</small>
              <em title={file.absolutePath}>{compactPath(file.absolutePath, 96)}</em>
            </span>
            <span className="resource-hint">{t("resources.doubleClickOpen")}</span>
          </button>
          <button className="resource-reveal" aria-label={t("files.revealNamed", { name: file.fileName })} onClick={() => onRevealFile(file)} title={t("files.reveal")} type="button"><Icon name="external" /></button>
        </article>
      ))}
    </div>
  );
}

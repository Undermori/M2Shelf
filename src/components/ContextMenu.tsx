import { useRef } from "react";
import {useContextPanel} from '../hooks/useContextPanel';
import type { MediaNode } from "../types/media";
import { canBindBangumi, canBindTmdb, nodeDisplayTitle } from "../lib/format";
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";

export type NodeAction =
  | "work" | "container" | "other" | "ignore" | "reset" | "rename" | "bangumi"
  | "clear-bangumi" | "retry-cover" | "cover" | "clear-cover" | "tags" | "favorites" | "explorer" | "copy-path" | "scan";

interface ContextMenuProps {
  node: MediaNode;
  x: number;
  y: number;
  onAction: (action: NodeAction, node: MediaNode) => void;
  onClose: () => void;
  onRemoveRecent?:()=>void;
}

export function ContextMenu({ node, x, y, onAction, onClose, onRemoveRecent }: ContextMenuProps) {
  const { t } = useI18n();
  const menuRef = useRef<HTMLDivElement>(null);
  const position = useContextPanel(menuRef, x, y, onClose);
  const isContainer = node.nodeType === "CONTAINER" || node.nodeType === "MIXED";
  const bindable = canBindBangumi(node);
  const multipleProviders = canBindTmdb(node);

  const action = (id: NodeAction) => (event: React.MouseEvent) => {
    event.stopPropagation();
    onAction(id, node);
    onClose();
  };

  return (
    <div className="context-menu" role="group" aria-label={nodeDisplayTitle(node)} ref={menuRef} style={position} onPointerDown={(event) => event.stopPropagation()}>
      <p>{nodeDisplayTitle(node)}</p>
      {onRemoveRecent&&<button type="button" onClick={()=>{onRemoveRecent();onClose();}}><Icon name="close"/>{t("recent.remove")}</button>}
      <button onClick={action("work")} type="button"><Icon name="work" />{t("menu.setWork")}</button>
      <button onClick={action("container")} type="button"><Icon name="folder" />{t("menu.setContainer")}</button>
      <button onClick={action("other")} type="button"><Icon name="archive" />{t("menu.setOtherResources")}</button>
      <button onClick={action("ignore")} type="button"><Icon name="close" />{t("menu.ignore")}</button>
      {node.manualTypeOverride && <button onClick={action("reset")} type="button"><Icon name="refresh" />{t("menu.reset")}</button>}
      <span className="menu-divider" />
      <button onClick={action("rename")} type="button"><Icon name="edit" />{t("menu.rename")}</button>
      <button onClick={action("tags")} type="button"><Icon name="tag" />{t("menu.manageTags")}</button>
      <button onClick={action("favorites")} type="button"><Icon name="bookmark" />{t("menu.addToFavorites")}</button>
      {bindable && <button onClick={action("bangumi")} type="button"><Icon name={multipleProviders ? "match" : "bangumi"} />{multipleProviders ? t(node.binding || node.tmdbBinding?.active ? "detail.changeBinding" : "detail.searchAdd") : node.binding ? t("menu.changeBangumi") : t("menu.addBangumi")}</button>}
      {bindable && node.binding && (node.binding.coverDownloadError || (!node.coverCachePath && !node.binding.coverCachePath)) && <button onClick={action("retry-cover")} type="button"><Icon name="refresh" />{t("menu.retryCover")}</button>}
      {node.binding && <button onClick={action("clear-bangumi")} type="button"><Icon name="trash" />{t("menu.clearBangumi")}</button>}
      {isContainer && <button onClick={action("cover")} type="button"><Icon name="image" />{t("menu.chooseCover")}</button>}
      {node.coverSource === "MANUAL" && <button onClick={action("clear-cover")} type="button"><Icon name="trash" />{t("menu.clearCover")}</button>}
      <span className="menu-divider" />
      <button onClick={action("explorer")} type="button"><Icon name="external" />{t("menu.openExplorer")}</button>
      <button onClick={action("copy-path")} type="button"><Icon name="file" />{t("menu.copyPath")}</button>
      <button onClick={action("scan")} type="button"><Icon name="refresh" />{t("menu.rescanDirectory")}</button>
    </div>
  );
}

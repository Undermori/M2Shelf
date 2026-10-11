import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { MediaNode } from "../types/media";
import { canBindBangumi, nodeDisplayTitle } from "../lib/format";
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";

export type NodeAction =
  | "work" | "container" | "other" | "ignore" | "reset" | "rename" | "bangumi"
  | "clear-bangumi" | "retry-cover" | "cover" | "clear-cover" | "tags" | "favorites" | "explorer" | "scan";

interface ContextMenuProps {
  node: MediaNode;
  x: number;
  y: number;
  onAction: (action: NodeAction, node: MediaNode) => void;
  onClose: () => void;
}

export function ContextMenu({ node, x, y, onAction, onClose }: ContextMenuProps) {
  const { t } = useI18n();
  const menuRef = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });
  const isContainer = node.nodeType === "CONTAINER" || node.nodeType === "MIXED";
  const bindable = canBindBangumi(node);

  useEffect(() => {
    const close = () => onClose();
    const key = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", key);
    menuRef.current?.focus();
    return () => { window.removeEventListener("pointerdown", close); window.removeEventListener("keydown", key); };
  }, [onClose]);

  useLayoutEffect(() => {
    const placeMenu = () => {
      const menu = menuRef.current;
      if (!menu) return;
      const gutter = 8;
      const bounds = menu.getBoundingClientRect();
      const maxLeft = Math.max(gutter, window.innerWidth - bounds.width - gutter);
      const maxTop = Math.max(gutter, window.innerHeight - bounds.height - gutter);
      setPosition({
        left: Math.max(gutter, Math.min(x, maxLeft)),
        top: Math.max(gutter, Math.min(y, maxTop)),
      });
    };
    placeMenu();
    window.addEventListener("resize", placeMenu);
    return () => window.removeEventListener("resize", placeMenu);
  }, [x, y]);

  const action = (id: NodeAction) => (event: React.MouseEvent) => {
    event.stopPropagation();
    onAction(id, node);
    onClose();
  };

  return (
    <div className="context-menu" ref={menuRef} style={position} tabIndex={-1} onPointerDown={(event) => event.stopPropagation()}>
      <p>{nodeDisplayTitle(node)}</p>
      <button onClick={action("work")} type="button"><Icon name="work" />{t("menu.setWork")}</button>
      <button onClick={action("container")} type="button"><Icon name="folder" />{t("menu.setContainer")}</button>
      <button onClick={action("other")} type="button"><Icon name="archive" />{t("menu.setOtherResources")}</button>
      <button onClick={action("ignore")} type="button"><Icon name="close" />{t("menu.ignore")}</button>
      {node.manualTypeOverride && <button onClick={action("reset")} type="button"><Icon name="refresh" />{t("menu.reset")}</button>}
      <span className="menu-divider" />
      <button onClick={action("rename")} type="button"><Icon name="edit" />{t("menu.rename")}</button>
      <button onClick={action("tags")} type="button"><Icon name="tag" />{t("menu.manageTags")}</button>
      <button onClick={action("favorites")} type="button"><Icon name="bookmark" />{t("menu.addToFavorites")}</button>
      {bindable && <button onClick={action("bangumi")} type="button"><Icon name="bangumi" />{node.binding ? t("menu.changeBangumi") : t("menu.addBangumi")}</button>}
      {bindable && node.binding && (node.binding.coverDownloadError || (!node.coverCachePath && !node.binding.coverCachePath)) && <button onClick={action("retry-cover")} type="button"><Icon name="refresh" />{t("menu.retryCover")}</button>}
      {node.binding && <button onClick={action("clear-bangumi")} type="button"><Icon name="trash" />{t("menu.clearBangumi")}</button>}
      {isContainer && <button onClick={action("cover")} type="button"><Icon name="image" />{t("menu.chooseCover")}</button>}
      {node.coverSource === "MANUAL" && <button onClick={action("clear-cover")} type="button"><Icon name="trash" />{t("menu.clearCover")}</button>}
      <span className="menu-divider" />
      <button onClick={action("explorer")} type="button"><Icon name="external" />{t("menu.openExplorer")}</button>
      <button onClick={action("scan")} type="button"><Icon name="refresh" />{t("menu.rescanDirectory")}</button>
    </div>
  );
}

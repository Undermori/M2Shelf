import { useEffect, useRef } from "react";
import type { LibraryRoot } from "../types/media";
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";

export type LibraryRootAction = "rename" | "explorer" | "scan" | "toggle-all-visibility";

interface LibraryRootContextMenuProps {
  root: LibraryRoot;
  x: number;
  y: number;
  onAction: (action: LibraryRootAction, root: LibraryRoot) => void;
  onClose: () => void;
}

export function LibraryRootContextMenu({ root, x, y, onAction, onClose }: LibraryRootContextMenuProps) {
  const { t } = useI18n();
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const close = () => onClose();
    const key = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", key);
    menuRef.current?.focus();
    return () => { window.removeEventListener("pointerdown", close); window.removeEventListener("keydown", key); };
  }, [onClose]);
  const action = (id: LibraryRootAction) => (event: React.MouseEvent) => {
    event.stopPropagation();
    onAction(id, root);
    onClose();
  };
  return (
    <div className="context-menu library-root-menu" ref={menuRef} style={{ left: Math.min(x, window.innerWidth - 270), top: Math.min(y, window.innerHeight - 230) }} tabIndex={-1} onPointerDown={(event) => event.stopPropagation()}>
      <p title={root.path}>{root.displayName}</p>
      <button onClick={action("rename")} type="button"><Icon name="edit" />{t("menu.rename")}</button>
      <button onClick={action("explorer")} type="button"><Icon name="external" />{t("menu.openExplorerRoot")}</button>
      <button onClick={action("toggle-all-visibility")} type="button"><Icon name={root.hiddenFromAll ? "folder" : "eye-off"} />{t(root.hiddenFromAll ? "library.showInAll" : "library.hideFromAll")}</button>
      <button onClick={action("scan")} type="button"><Icon name="refresh" />{t("menu.rescan")}</button>
    </div>
  );
}

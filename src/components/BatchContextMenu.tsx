import { useRef } from "react";
import {useContextPanel} from '../hooks/useContextPanel';
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";

export type BatchNodeAction = "work" | "container" | "other" | "ignore" | "reset" | "tags" | "favorites" | "rematch";

interface BatchContextMenuProps {
  count: number;
  x: number;
  y: number;
  onAction: (action: BatchNodeAction) => void;
  onClose: () => void;
}

export function BatchContextMenu({ count, x, y, onAction, onClose }: BatchContextMenuProps) {
  const { number, t } = useI18n();
  const menuRef = useRef<HTMLDivElement>(null);
  const position = useContextPanel(menuRef, x, y, onClose);

  const action = (id: BatchNodeAction) => (event: React.MouseEvent) => {
    event.stopPropagation();
    onAction(id);
    onClose();
  };

  return (
    <div className="context-menu batch-context-menu" role="group" aria-label={t("selection.menuTitle", { count: number(count) })} ref={menuRef} style={position} tabIndex={-1} onPointerDown={(event) => event.stopPropagation()}>
      <p>{t("selection.menuTitle", { count: number(count) })}</p>
      <button onClick={action("work")} type="button"><Icon name="work" />{t("menu.setWork")}</button>
      <button onClick={action("container")} type="button"><Icon name="folder" />{t("menu.setContainer")}</button>
      <button onClick={action("other")} type="button"><Icon name="archive" />{t("menu.setOtherResources")}</button>
      <button onClick={action("ignore")} type="button"><Icon name="close" />{t("selection.ignoreSelected")}</button>
      <button onClick={action("reset")} type="button"><Icon name="refresh" />{t("menu.reset")}</button>
      <span className="menu-divider" />
      <button onClick={action("tags")} type="button"><Icon name="tag" />{t("selection.applyTag")}</button>
      <button onClick={action("favorites")} type="button"><Icon name="bookmark" />{t("selection.addToFavorites")}</button>
      <button onClick={action("rematch")} type="button"><Icon name="match" />{t("selection.rematchSelected")}</button>
    </div>
  );
}

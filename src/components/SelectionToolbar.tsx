import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";

interface SelectionToolbarProps {
  selectedCount: number;
  visibleCount: number;
  busy?: boolean;
  onSelectAll: () => void;
  onClear: () => void;
  onTags?: () => void;
  onFavorites?: () => void;
  onRemoveFromFavorite?: () => void;
  onMore?: (event: React.MouseEvent<HTMLButtonElement>) => void;
  onRematch?: () => void;
  onExit: () => void;
  onRemoveSelected?: () => void;
}

/** Compact, keyboard-accessible controls shown only while collection editing is active. */
export function SelectionToolbar({ selectedCount, visibleCount, busy, onSelectAll, onClear, onTags, onFavorites, onRemoveFromFavorite, onMore, onRematch, onExit, onRemoveSelected }: SelectionToolbarProps) {
  const { number, t } = useI18n();
  const hasSelection = selectedCount > 0;
  return (
    <div className="selection-toolbar" role="region" aria-label={t("selection.toolbarAria")}>
      <strong>{t("selection.selectedCount", { count: number(selectedCount) })}</strong>
      <div className="selection-toolbar-actions">
        <button className="button ghost" disabled={busy || visibleCount === 0} onClick={onSelectAll} type="button"><Icon name="check" />{t("selection.selectAllVisible", { count: number(visibleCount) })}</button>
        <button className="button ghost" disabled={busy || !hasSelection} onClick={onClear} type="button">{t("selection.clear")}</button>
        <span className="selection-divider" />
        {onRemoveSelected&&<button className="button secondary" disabled={busy||!hasSelection} onClick={onRemoveSelected} type="button"><Icon name="close"/>{t("recent.removeSelected")}</button>}
        {onTags && <button className="button secondary" disabled={busy || !hasSelection} onClick={onTags} type="button"><Icon name="tag" />{t("selection.applyTag")}</button>}
        {onFavorites && <button className="button secondary" disabled={busy || !hasSelection} onClick={onFavorites} type="button"><Icon name="bookmark" />{t("selection.addToFavorites")}</button>}
        {onRemoveFromFavorite && <button className="button secondary" disabled={busy || !hasSelection} onClick={onRemoveFromFavorite} type="button"><Icon name="close" />{t("selection.removeFromFavorite")}</button>}
        {onMore && <button className="button secondary" disabled={busy || !hasSelection} onClick={onMore} type="button"><Icon name="more" />{t("selection.moreActions")}</button>}
        {onRematch && <button className="button primary" disabled={busy || !hasSelection} onClick={onRematch} type="button"><Icon name="bangumi" />{busy ? t("selection.matching") : t("selection.rematchSelected")}</button>}
        <button className="button ghost" disabled={busy} onClick={onExit} type="button"><Icon name="close" />{t("selection.exit")}</button>
      </div>
    </div>
  );
}

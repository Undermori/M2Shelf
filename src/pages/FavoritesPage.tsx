import { CollectionSortControl } from "../components/CollectionSortControl";
import { useMemo } from "react";
import type { CollectionSort, FavoriteFolder, MediaNode, ViewMode } from "../types/media";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { LoadingState } from "../components/LoadingState";
import { PosterGrid } from "../components/PosterGrid";
import { SelectionToolbar } from "../components/SelectionToolbar";
import { compareMediaNodes, nodeMatchesQuery } from "../lib/format";
import { useI18n } from "../lib/i18n";

interface FavoritesPageProps {
  folders: FavoriteFolder[] | null;
  nodes: MediaNode[] | null;
  selectedFolderId: number | null;
  loading: boolean;
  viewMode: ViewMode;
  onViewMode: (mode: ViewMode) => void;
  filter: string;
  onFilter: (value: string) => void;
  sort: CollectionSort;
  onSort: (value: CollectionSort) => void;
  onOpenFolder: (folderId: number) => void;
  onBack: () => void;
  onCreate: () => void;
  onRename: (folder: FavoriteFolder) => void;
  onDelete: (folder: FavoriteFolder) => void;
  onOpenNode: (node: MediaNode) => void;
  onMenu: (event: React.MouseEvent, node: MediaNode) => void;
  onBangumi: (node: MediaNode) => void;
  onRetryCover: (node: MediaNode, imageDecodeFailed?: boolean) => void;
  coverRevision: number;
  editMode: boolean;
  selectedNodeIds: ReadonlySet<number>;
  busy: boolean;
  onEditMode: (active: boolean) => void;
  onToggleSelection: (node: MediaNode) => void;
  onSelectAll: (nodeIds: number[]) => void;
  onClearSelection: () => void;
  onBatchTags: () => void;
  onBatchFavorites: () => void;
  onBatchMenu: (event: React.MouseEvent<HTMLButtonElement>) => void;
  onRematch: () => void;
  onRemoveSelected: () => void;
}

export function FavoritesPage(props: FavoritesPageProps) {
  const { language, number, t } = useI18n();
  const selectedFolder = props.folders?.find((folder) => folder.id === props.selectedFolderId) ?? null;
  const visibleNodes = useMemo(() => {
    const query = props.filter.trim().toLocaleLowerCase(language);
    const nodes = (props.nodes ?? [])
      .filter((node) => nodeMatchesQuery(node, query, language));
    // Rust returns favorite membership in added_at DESC / node_id DESC order. Keep or reverse
    // that stable relationship order instead of accidentally sorting by MediaNode.createdAt.
    if (props.sort === "added-desc") return nodes;
    if (props.sort === "added-asc") return nodes.reverse();
    return nodes.sort((left, right) => compareMediaNodes(left, right, props.sort, language));
  }, [language, props.filter, props.nodes, props.sort]);

  if (props.loading && props.folders == null) return <LoadingState label={t("favorites.loading")} />;

  return (
    <section className="browse-page favorites-page">
      <header className="page-toolbar">
        <div className="toolbar-topline">
          {selectedFolder
            ? <button className="all-resources-location favorites-location" onClick={props.onBack} type="button"><Icon name="arrow-left" />{t("favorites.back")}</button>
            : <span className="all-resources-location favorites-location"><Icon name="bookmark" />{t("favorites.location")}</span>}
          <div className="toolbar-actions">
            {selectedFolder && <button aria-pressed={props.editMode} className={`button secondary edit-mode-button ${props.editMode ? "is-active" : ""}`} onClick={() => props.onEditMode(!props.editMode)} type="button"><Icon name="edit" />{props.editMode ? t("selection.exit") : t("selection.editMode")}</button>}
            {selectedFolder && <button className="button secondary" onClick={() => props.onRename(selectedFolder)} type="button"><Icon name="edit" />{t("favorites.rename")}</button>}
            {selectedFolder && <button className="button secondary" onClick={() => props.onDelete(selectedFolder)} type="button"><Icon name="trash" />{t("favorites.delete")}</button>}
            {!selectedFolder && <button className="button primary" onClick={props.onCreate} type="button"><Icon name="plus" />{t("favorites.create")}</button>}
          </div>
        </div>
        <div className="page-title-row">
          <div><h1>{selectedFolder?.name ?? t("favorites.title")}</h1><p>{selectedFolder ? t("favorites.folderDescription") : t("favorites.description")}</p></div>
          {selectedFolder && <div className="browse-controls">
            <label className="search-field"><Icon name="search" /><input aria-label={t("favorites.filterAria")} onChange={(event) => props.onFilter(event.target.value)} placeholder={t("favorites.filterPlaceholder")} value={props.filter} />{props.filter && <button aria-label={t("common.clear")} onClick={() => props.onFilter("")} type="button"><Icon name="close" /></button>}</label>
            <CollectionSortControl value={props.sort} onChange={props.onSort} />
            <div className="view-toggle" aria-label={t("common.displayMode")}><button className={props.viewMode === "grid" ? "is-active" : ""} onClick={() => props.onViewMode("grid")} title={t("common.grid")} type="button"><Icon name="grid" /></button><button className={props.viewMode === "list" ? "is-active" : ""} onClick={() => props.onViewMode("list")} title={t("common.list")} type="button"><Icon name="list" /></button></div>
          </div>}
        </div>
      </header>
      <div className="page-content">
        {!selectedFolder && <>
          {(props.folders?.length ?? 0) > 0 && <div className="favorite-folder-grid">{props.folders!.map((folder) => <article className="favorite-folder-card" key={folder.id}>
            <button className="favorite-folder-open" onClick={() => props.onOpenFolder(folder.id)} type="button"><span><Icon name="bookmark" /></span><strong>{folder.name}</strong><small>{t("favorites.itemCount", { count: number(folder.itemCount) })}</small></button>
            <div className="favorite-folder-actions"><button aria-label={t("favorites.renameNamed", { name: folder.name })} onClick={() => props.onRename(folder)} title={t("favorites.rename")} type="button"><Icon name="edit" /></button><button aria-label={t("favorites.deleteNamed", { name: folder.name })} onClick={() => props.onDelete(folder)} title={t("favorites.delete")} type="button"><Icon name="trash" /></button></div>
          </article>)}</div>}
          {!props.loading && (props.folders?.length ?? 0) === 0 && <EmptyState compact icon="bookmark" title={t("favorites.emptyTitle")} description={t("favorites.emptyDescription")} action={<button className="button primary" onClick={props.onCreate} type="button"><Icon name="plus" />{t("favorites.create")}</button>} />}
        </>}
        {selectedFolder && <>
          {props.editMode && <SelectionToolbar selectedCount={props.selectedNodeIds.size} visibleCount={visibleNodes.length} busy={props.busy} onSelectAll={() => props.onSelectAll(visibleNodes.map((node) => node.id))} onClear={props.onClearSelection} onTags={props.onBatchTags} onFavorites={props.onBatchFavorites} onRemoveFromFavorite={props.onRemoveSelected} onMore={props.onBatchMenu} onRematch={props.onRematch} onExit={() => props.onEditMode(false)} />}
          <div className="all-resources-summary"><strong>{t("favorites.itemCount", { count: number(visibleNodes.length) })}</strong></div>
          {visibleNodes.length > 0 && <PosterGrid showModifiedTime={props.sort.startsWith("modified-")} nodes={visibleNodes} viewMode={props.viewMode} onOpen={props.onOpenNode} onMenu={props.onMenu} onBangumi={props.onBangumi} onRetryCover={props.onRetryCover} coverRevision={props.coverRevision} editMode={props.editMode} selectedNodeIds={props.selectedNodeIds} onSelect={props.onToggleSelection} />}
          {!props.loading && visibleNodes.length === 0 && <EmptyState compact icon={props.filter ? "search" : "bookmark"} title={props.filter ? t("favorites.noMatch") : t("favorites.folderEmptyTitle")} description={props.filter ? t("favorites.noMatchDescription") : t("favorites.folderEmptyDescription")} />}
        </>}
      </div>
    </section>
  );
}

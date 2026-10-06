import { CollectionSortControl } from "../components/CollectionSortControl";
import { useMemo } from "react";
import type { BrowseResult, CollectionSort, MediaFile, MediaNode, ResourceFile, ViewMode } from "../types/media";
import { Breadcrumb } from "../components/Breadcrumb";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { LoadingState } from "../components/LoadingState";
import { MediaFileList } from "../components/MediaFileList";
import { PosterGrid } from "../components/PosterGrid";
import { TagFilter } from "../components/TagFilter";
import { OtherResourceList } from "../components/OtherResourceList";
import { SelectionToolbar } from "../components/SelectionToolbar";
import { compareMediaNodes, nodeMatchesQuery, nodeDisplayTitle, nodeTypeLabel } from "../lib/format";
import { useI18n } from "../lib/i18n";

interface BrowsePageProps {
  data: BrowseResult | null;
  currentNode: MediaNode | null;
  loading: boolean;
  viewMode: ViewMode;
  onViewMode: (mode: ViewMode) => void;
  filter: string;
  onFilter: (value: string) => void;
  tagFilterId: number | null;
  onTagFilter: (tagId: number | null) => void;
  sort: CollectionSort;
  onSort: (value: CollectionSort) => void;
  onRoot: () => void;
  onBreadcrumb: (nodeId: number) => void;
  onOpenNode: (node: MediaNode) => void;
  onMenu: (event: React.MouseEvent, node: MediaNode) => void;
  onBangumi: (node: MediaNode) => void;
  onRetryCover: (node: MediaNode, imageDecodeFailed?: boolean) => void;
  onPlay: (file: MediaFile) => void;
  onRevealMedia: (file: MediaFile) => void;
  onOpenResource: (file: ResourceFile) => void;
  onRevealResource: (file: ResourceFile) => void;
  onScan: () => void;
  onAddRoot: () => void;
  onSearch: (query: string) => void;
  coverRevision: number;
  editMode: boolean;
  selectedNodeIds: ReadonlySet<number>;
  matchBusy: boolean;
  onEditMode: (active: boolean) => void;
  onToggleSelection: (node: MediaNode) => void;
  onSelectAll: (nodeIds: number[]) => void;
  onClearSelection: () => void;
  onBatchTags: () => void;
  onBatchFavorites: () => void;
  onBatchMenu: (event: React.MouseEvent<HTMLButtonElement>) => void;
  onMatch: (nodeIds: number[] | null, rematchExisting: boolean) => void;
}

export function BrowsePage({ data, currentNode, loading, viewMode, onViewMode, filter, onFilter, tagFilterId, onTagFilter, sort, onSort, onRoot, onBreadcrumb, onOpenNode, onMenu, onBangumi, onRetryCover, onPlay, onRevealMedia, onOpenResource, onRevealResource, onScan, onAddRoot, onSearch, coverRevision, editMode, selectedNodeIds, matchBusy, onEditMode, onToggleSelection, onSelectAll, onClearSelection, onBatchTags, onBatchFavorites, onBatchMenu, onMatch }: BrowsePageProps) {
  const { language, number, t } = useI18n();
  const effectiveTagFilterId = tagFilterId != null && data?.nodes.some((node) => node.userTags.some((tag) => tag.id === tagFilterId))
    ? tagFilterId
    : null;
  const nodes = useMemo(() => {
    const query = filter.trim().toLocaleLowerCase();
    return [...(data?.nodes ?? [])]
      .filter((node) => nodeMatchesQuery(node, query, language))
      .filter((node) => effectiveTagFilterId == null || node.userTags.some((tag) => tag.id === effectiveTagFilterId))
      .sort((a, b) => compareMediaNodes(a, b, sort, language));
  }, [data?.nodes, effectiveTagFilterId, filter, language, sort]);
  const mediaFiles = data?.mediaFiles ?? [];
  const resourceFiles = data?.resourceFiles ?? [];
  const title = currentNode ? nodeDisplayTitle(currentNode, language) : data?.root.displayName ?? t("browse.libraryFallback");
  const hasActiveFilter = Boolean(filter.trim()) || effectiveTagFilterId != null;

  if (loading && !data) return <LoadingState />;
  if (!data) return <EmptyState eyebrow={t("browse.emptyEyebrow")} title={t("browse.emptyTitle")} description={t("browse.emptyDescription")} action={<button className="button primary" onClick={onAddRoot} type="button"><Icon name="plus" />{t("app.addMediaDirectory")}</button>} />;

  return (
    <section className="browse-page">
      <header className="page-toolbar">
        <div className="toolbar-topline">
          <Breadcrumb rootLabel={data.root.displayName} items={data.breadcrumbs} currentNodeId={currentNode?.id} onRoot={onRoot} onNode={onBreadcrumb} />
          <div className="toolbar-actions">
            <button aria-pressed={editMode} className={`button secondary edit-mode-button ${editMode ? "is-active" : ""}`} onClick={() => onEditMode(!editMode)} type="button"><Icon name="edit" />{editMode ? t("selection.exit") : t("selection.editMode")}</button>
            <button className="button secondary scan-button" disabled={matchBusy || (editMode && selectedNodeIds.size === 0)} onClick={() => onMatch(editMode ? [...selectedNodeIds] : data.nodes.map((node) => node.id), editMode)} type="button"><Icon name="bangumi" />{matchBusy ? t("selection.matching") : editMode ? t("selection.rematchSelectedCount", { count: number(selectedNodeIds.size) }) : t("browse.matchExisting")}</button>
            <button className="button secondary scan-button" disabled={matchBusy} onClick={onScan} type="button"><Icon name="refresh" />{currentNode ? t("browse.scanAndMatchDirectory") : t("browse.scanAndMatchLibrary")}</button>
          </div>
        </div>
        <div className="page-title-row">
          <div><p className="eyebrow">{currentNode ? nodeTypeLabel(currentNode.nodeType) : t("browse.localLibrary")}</p><h1>{title}</h1>{currentNode && <p className="page-folder-name">{currentNode.folderName}</p>}<p>{currentNode?.absolutePath ?? data.root.path}</p></div>
          <div className="browse-controls">
            <label className="search-field"><Icon name="search" /><input aria-label={t("browse.filterAria")} onChange={(event) => onFilter(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && filter.trim()) onSearch(filter.trim()); }} placeholder={t("browse.filterPlaceholder")} value={filter} />{filter && <button aria-label={t("all.clearFilter")} onClick={() => onFilter("")} type="button"><Icon name="close" /></button>}</label>
            <TagFilter nodes={data.nodes} value={tagFilterId} onChange={onTagFilter} />
            <CollectionSortControl value={sort} onChange={onSort} />
            <div className="view-toggle" aria-label={t("common.displayMode")}><button className={viewMode === "grid" ? "is-active" : ""} onClick={() => onViewMode("grid")} title={t("common.grid")} type="button"><Icon name="grid" /></button><button className={viewMode === "list" ? "is-active" : ""} onClick={() => onViewMode("list")} title={t("common.list")} type="button"><Icon name="list" /></button></div>
          </div>
        </div>
      </header>

      <div className="page-content">
        {editMode && <SelectionToolbar selectedCount={selectedNodeIds.size} visibleCount={nodes.length} busy={matchBusy} onSelectAll={() => onSelectAll(nodes.map((node) => node.id))} onClear={onClearSelection} onTags={onBatchTags} onFavorites={onBatchFavorites} onMore={onBatchMenu} onRematch={() => onMatch([...selectedNodeIds], true)} onExit={() => onEditMode(false)} />}
        {mediaFiles.length > 0 && <section className="content-section"><div className="section-heading"><div><p className="eyebrow">{t("browse.directVideos")}</p><h2>{t("browse.doubleClickPlay")}</h2></div><span>{t("browse.videoCount", { count: mediaFiles.length })}</span></div><MediaFileList modifiedSort={sort.startsWith("modified-") ? sort : undefined} files={mediaFiles} onPlay={onPlay} onReveal={onRevealMedia} /></section>}
        {nodes.length > 0 && <section className="content-section"><div className="section-heading"><div><p className="eyebrow">{t("browse.children")}</p><h2>{t("browse.continue")}</h2></div><span>{t("browse.nodeCount", { count: nodes.length })}</span></div><PosterGrid showModifiedTime={sort.startsWith("modified-")} nodes={nodes} viewMode={viewMode} onOpen={onOpenNode} onMenu={onMenu} onBangumi={onBangumi} onRetryCover={onRetryCover} coverRevision={coverRevision} editMode={editMode} selectedNodeIds={selectedNodeIds} onSelect={onToggleSelection} /></section>}
        {resourceFiles.length > 0 && <section className="content-section"><div className="section-heading"><div><p className="eyebrow">{t("browse.otherResources")}</p><h2>{t("browse.nonVideoFiles")}</h2></div><span>{t("browse.fileCount", { count: resourceFiles.length })}</span></div><OtherResourceList modifiedSort={sort.startsWith("modified-") ? sort : undefined} files={resourceFiles} folders={[]} onOpenFile={onOpenResource} onRevealFile={onRevealResource} onOpenFolder={onOpenNode} onFolderMenu={onMenu} /></section>}
        {nodes.length === 0 && mediaFiles.length === 0 && resourceFiles.length === 0 && <EmptyState compact icon={hasActiveFilter ? "search" : "folder"} title={hasActiveFilter ? t("browse.noMatch") : t("browse.empty")} description={hasActiveFilter ? t("browse.noMatchDescription") : t("browse.emptyFolderDescription")} />}
      </div>
    </section>
  );
}

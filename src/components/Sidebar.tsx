import type { LibraryRoot } from "../types/media";
import brandMark from "../../src-tauri/icons/icon.png";
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";

export type AppPage = "all" | "library" | "organize" | "search" | "recent" | "favorites" | "settings";

interface SidebarProps {
  page: AppPage;
  roots: LibraryRoot[];
  selectedRootId: number | null;
  loading?: boolean;
  onNavigate: (page: AppPage) => void;
  onSelectRoot: (rootId: number) => void;
  onAddRoot: () => void;
  onRootMenu: (event: React.MouseEvent, root: LibraryRoot) => void;
  projectCount: number;
}

export function Sidebar({ page, roots, selectedRootId, loading, onNavigate, onSelectRoot, onAddRoot, onRootMenu, projectCount }: SidebarProps) {
  const { t, number } = useI18n();
  return (
    <aside className="sidebar">
      <button className="brand" onClick={() => onNavigate("all")} type="button" aria-label={t("sidebar.home")}>
        <img className="brand-mark" src={brandMark} alt="" aria-hidden="true" />
        <span className="brand-copy"><strong>{t("brand.name")}</strong><small>{t("brand.subtitle")}</small></span>
      </button>

      <nav className="sidebar-nav" aria-label={t("sidebar.mainNavigation")}>
        <button className={`nav-item ${page === "all" ? "is-active" : ""}`} onClick={() => onNavigate("all")} type="button">
          <Icon name="archive" /><span>{t("sidebar.allResources")}</span>
        </button>
        <button className={`nav-item ${page === "search" ? "is-active" : ""}`} onClick={() => onNavigate("search")} type="button">
          <Icon name="search" /><span>{t("sidebar.search")}</span>
        </button>
        <button className={`nav-item ${page === "recent" ? "is-active" : ""}`} onClick={() => onNavigate("recent")} type="button">
          <Icon name="clock" /><span>{t('comic.recentTitle')}</span>
        </button>
        <button className={`nav-item ${page === "favorites" ? "is-active" : ""}`} onClick={() => onNavigate("favorites")} type="button">
          <Icon name="bookmark" /><span>{t("sidebar.favorites")}</span>
        </button>

        <button className={`nav-item ${page === "organize" ? "is-active" : ""}`} onClick={() => onNavigate("organize")} type="button"><Icon name="shelf"/><span>{t('tidy.title')}</span></button>

        <div className="nav-library-heading">
          <p className="nav-section-label">{t("sidebar.library")}</p>
          <button aria-label={t("sidebar.addDirectory")} className="sidebar-add" onClick={onAddRoot} type="button"><Icon name="plus" /></button>
        </div>
        <div className="root-list">
          {loading && roots.length === 0 && <p className="sidebar-muted">{t("sidebar.loading")}</p>}
          {!loading && roots.length === 0 && (
            <button className="library-placeholder" onClick={onAddRoot} type="button">
              <span className="placeholder-folder"><Icon name="folder" /></span>
              <span>{t("sidebar.addFirst")}</span><small>{t("sidebar.multipleDrives")}</small>
            </button>
          )}
          {roots.map((root) => (
            <div className="root-entry" key={root.id}><button
              className={`root-item ${page === "library" && selectedRootId === root.id ? "is-active" : ""}`}
              key={root.id}
              onClick={() => onSelectRoot(root.id)}
              onContextMenu={(event) => { event.preventDefault(); onRootMenu(event, root); }}
              title={root.path}
              aria-label={`${root.displayName} · ${root.path}${root.hiddenFromAll ? ` · ${t("library.hiddenFromAll")}` : ""}`}
              type="button"
            >
              <Icon name={root.hiddenFromAll ? "eye-off" : "folder"} />
              <span><strong>{root.displayName}</strong></span>
            </button>{root.scanHealth && !root.scanHealth.warningsIgnored && ["PARTIAL", "FAILED"].includes(root.scanHealth.outcome) && <button className="root-scan-warning" onClick={() => onNavigate("settings")} title={`${root.displayName}: ${t("health.showDetails")}`} aria-label={`${root.displayName}: ${t("health.showDetails")}`} type="button"><Icon name="warning" /></button>}</div>
          ))}
        </div>
      </nav>

      <div className="sidebar-project-count" aria-live="polite"><strong>{number(projectCount)}</strong><span>{t("sidebar.projectUnit")}</span></div>
      <button className={`nav-item sidebar-settings ${page === "settings" ? "is-active" : ""}`} onClick={() => onNavigate("settings")} type="button">
        <Icon name="settings" /><span>{t("sidebar.settings")}</span>
      </button>
    </aside>
  );
}

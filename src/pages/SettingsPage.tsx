import { useEffect, useRef, useState } from "react";
import type { AppBootstrap, AppLanguage, AppSettings, AppTheme, CacheStats, LibraryRoot, UpdateDownloadStatus } from "../types/media";
import { LibraryScanHealth } from "../components/LibraryScanHealth";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { LoadingState } from "../components/LoadingState";
import { api, chooseCoverCacheDirectory, choosePlayerExecutable, desktopAvailable } from "../lib/api";
import { compactPath, errorMessage, formatBytes, formatDate } from "../lib/format";
import { useI18n } from "../lib/i18n";

interface SettingsPageProps {
  roots: LibraryRoot[];
  bootstrap: AppBootstrap | null;
  onAddRoot: () => void;
  onHiddenNodes: () => void;
  onRemoveRoot: (root: LibraryRoot) => void;
  onScanRoot: (root: LibraryRoot) => void;
  onAppearanceChange: (settings: AppSettings) => number;
  onPersistenceFailure: (failed: AppSettings, rollback: AppSettings | null, message: string, appearanceRevision: number) => void;
  updateDownloadStatus: UpdateDownloadStatus;
  onCheckForUpdate: () => void;
  onError: (message: string) => void;
  onSuccess: (message: string) => void;
}

export function SettingsPage({ roots, bootstrap, onAddRoot, onHiddenNodes, onRemoveRoot, onScanRoot, onAppearanceChange, onPersistenceFailure, updateDownloadStatus, onCheckForUpdate, onError, onSuccess }: SettingsPageProps) {
  const { t } = useI18n();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [cache, setCache] = useState<CacheStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved">("idle");
  const [testing, setTesting] = useState(false);
  const [extensionDraft, setExtensionDraft] = useState("");
  const persistedSettings = useRef<AppSettings | null>(null);
  const latestSettings = useRef<AppSettings | null>(null);
  const settingsRevision = useRef(0);
  const parentAppearanceRevision = useRef(0);
  const saveInFlight = useRef(false);
  const saveRequested = useRef(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  useEffect(() => {
    let active = true;
    if (!desktopAvailable) { setLoading(false); return; }
    void Promise.allSettled([api.getSettings(), api.cacheStats()]).then(([settingsResult, cacheResult]) => {
      if (!active) return;
      if (settingsResult.status === "fulfilled") {
        persistedSettings.current = settingsResult.value;
        latestSettings.current = settingsResult.value;
        setSettings(settingsResult.value);
        setSaveState("saved");
      }
      else onError(errorMessage(settingsResult.reason));
      if (cacheResult.status === "fulfilled") setCache(cacheResult.value);
      else onError(errorMessage(cacheResult.reason));
    }).finally(() => active && setLoading(false));
    return () => { active = false; };
  // Settings are loaded when this page is mounted; parent notification callbacks do not affect the request.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const applyAppearance = (next: AppSettings) => {
    parentAppearanceRevision.current = onAppearanceChange(next);
  };

  const drainAutoSave = async () => {
    if (saveInFlight.current) return;
    saveInFlight.current = true;
    if (mounted.current) {
      setSaving(true);
      setSaveState("saving");
    }
    try {
      while (saveRequested.current) {
        saveRequested.current = false;
        const candidate = latestSettings.current;
        if (!candidate) continue;
        const revision = settingsRevision.current;
        const appearanceRevision = parentAppearanceRevision.current;
        const previousPersisted = persistedSettings.current;
        try {
          const saved = await api.updateSettings(candidate);
          persistedSettings.current = saved;
          if (previousPersisted?.coverCacheDirectory !== saved.coverCacheDirectory) {
            try {
              const nextCache = await api.cacheStats();
              if (mounted.current) setCache(nextCache);
            } catch (error) {
              if (mounted.current) onError(errorMessage(error));
            }
          }
          if (revision === settingsRevision.current) {
            latestSettings.current = saved;
            if (mounted.current) setSettings(saved);
            if (mounted.current) applyAppearance(saved);
          }
        } catch (error) {
          if (revision !== settingsRevision.current) continue;
          const rollback = persistedSettings.current;
          settingsRevision.current += 1;
          saveRequested.current = false;
          latestSettings.current = rollback;
          if (rollback) {
            if (mounted.current) setSettings(rollback);
          }
          // The parent outlives this page. It must restore persisted appearance/update-check state
          // and surface the error even if navigation unmounted Settings while the IPC was pending.
          onPersistenceFailure(candidate, rollback, errorMessage(error), appearanceRevision);
        }
      }
    } finally {
      saveInFlight.current = false;
      if (mounted.current) {
        setSaving(false);
        setSaveState("saved");
      }
    }
  };

  const changeSettings = (update: (current: AppSettings) => AppSettings) => {
    const current = latestSettings.current;
    if (!current) return;
    const next = update(current);
    latestSettings.current = next;
    settingsRevision.current += 1;
    saveRequested.current = true;
    setSettings(next);
    setSaveState("saving");
    applyAppearance(next);
    void drainAutoSave();
  };

  const choosePlayer = async () => {
    try {
      const path = await choosePlayerExecutable();
      if (path) changeSettings((current) => ({ ...current, mpvPath: path }));
    }
    catch (error) { onError(errorMessage(error)); }
  };
  const testMpv = async () => {
    if (!settings?.mpvPath) return;
    setTesting(true);
    try { const result = await api.testMpv(settings.mpvPath); result.ok ? onSuccess(result.version ? t("settings.playerAvailable", { version: result.version }) : t("settings.playerReady")) : onError(t("settings.playerUnavailable")); }
    catch (error) { onError(errorMessage(error)); }
    finally { setTesting(false); }
  };
  const addExtension = () => {
    const value = extensionDraft.trim().toLowerCase().replace(/^\.?/, ".");
    const current = latestSettings.current;
    if (!current || !/^\.[a-z0-9]{1,10}$/.test(value) || current.videoExtensions.includes(value)) return;
    changeSettings((draft) => ({ ...draft, videoExtensions: [...draft.videoExtensions, value] }));
    setExtensionDraft("");
  };
  const clearCache = async () => {
    try { setCache(await api.clearCoverCache()); onSuccess(t("settings.cacheCleared")); }
    catch (error) { onError(errorMessage(error)); }
  };
  const chooseCacheDirectory = async () => {
    try {
      const path = await chooseCoverCacheDirectory();
      if (!path || path === latestSettings.current?.coverCacheDirectory) return;
      changeSettings((current) => ({ ...current, coverCacheDirectory: path }));
    } catch (error) { onError(errorMessage(error)); }
  };
  const openCache = async () => {
    try { await api.openCoverCacheDirectory(); }
    catch (error) { onError(errorMessage(error)); }
  };
  const openAuthorLink = async (url?: string) => {
    if (!url) return;
    try { await api.openExternalUrl(url); }
    catch (error) { onError(errorMessage(error)); }
  };
  const rebuild = async () => {
    try { await api.rebuildIndex(); onSuccess(t("settings.rebuildStarted")); }
    catch (error) { onError(errorMessage(error)); }
  };

  if (loading) return <LoadingState label={t("settings.loading")} />;
  return (
    <section className="settings-page">
      <header className="settings-header"><p className="eyebrow">{t("brand.name")}</p><h1>{t("settings.title")}</h1><p>{t("settings.description")}</p></header>
      {!desktopAvailable && <div className="preview-banner"><Icon name="info" /><span><strong>{t("settings.previewTitle")}</strong><small>{t("settings.previewDescription")}</small></span></div>}

      <div className="settings-layout">
        <section className="settings-section">
          <div className="settings-section-heading"><span className="settings-symbol coral"><Icon name="folder" /></span><div><h2>{t("settings.rootsTitle")}</h2><p>{t("settings.rootsDescription")}</p></div><button className="button secondary" disabled={!desktopAvailable} onClick={onAddRoot} type="button"><Icon name="plus" />{t("settings.addDirectory")}</button></div>
          {roots.length === 0 ? <EmptyState compact icon="folder" title={t("settings.noDirectory")} description={t("settings.noDirectoryDescription")} /> : <div className="settings-root-list">{roots.map((root) => (
            <article className="settings-root" key={root.id}><span><Icon name="folder-open" /></span><div><strong>{root.displayName}</strong><p title={root.path}>{compactPath(root.path, 74)}</p><small>{t("settings.lastScan", { date: formatDate(root.lastScanAt) })}</small><LibraryScanHealth health={root.scanHealth} /></div><button aria-label={t("settings.scanNamed", { name: root.displayName })} onClick={() => onScanRoot(root)} title={t("settings.scanLibrary")} type="button"><Icon name="refresh" /></button><button className="danger-icon" aria-label={t("settings.removeNamed", { name: root.displayName })} onClick={() => onRemoveRoot(root)} title={t("settings.removeIndexOnly")} type="button"><Icon name="trash" /></button></article>
          ))}</div>}
          <div className="settings-hidden-entry"><div><strong>{t("hidden.title")}</strong><p>{t("settings.hiddenDescription")}</p></div><button className="button secondary" disabled={!desktopAvailable} onClick={onHiddenNodes} type="button" aria-haspopup="dialog"><Icon name="eye-off" />{t("hidden.title")}</button></div>
          <p className="safety-copy"><Icon name="shield" />{t("settings.removeSafety")}</p>
        </section>

        <section className="settings-section">
          <div className="settings-section-heading"><span className="settings-symbol navy"><Icon name="play" /></span><div><h2>{t("settings.playerTitle")}</h2><p>{t("settings.playerDescription")}</p></div></div>
          <div className="setting-form-row"><label><span>{t("settings.playerPath")}</span><div className="path-input"><input disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, mpvPath: event.target.value || null }))} placeholder={t("settings.playerPlaceholder")} value={settings?.mpvPath ?? ""} /><button disabled={!desktopAvailable} onClick={() => void choosePlayer()} type="button">{t("common.chooseEllipsis")}</button></div></label><button className="button secondary" disabled={!settings?.mpvPath || testing} onClick={() => void testMpv()} type="button">{testing ? t("settings.testing") : t("settings.testPlayer")}</button></div>
        </section>

        <section className="settings-section">
          <div className="settings-section-heading"><span className="settings-symbol gold"><Icon name="settings" /></span><div><h2>{t("settings.browseScanTitle")}</h2><p>{t("settings.browseScanDescription")}</p></div></div>
          <div className="settings-columns"><label className="field-label"><span>{t("settings.defaultView")}</span><select disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, defaultViewMode: event.target.value as "GRID" | "LIST" }))} value={settings?.defaultViewMode ?? "GRID"}><option value="GRID">{t("settings.posterGrid")}</option><option value="LIST">{t("settings.compactList")}</option></select></label><label className="switch-field settings-switch-row"><span><strong>{t("settings.enableBangumi")}</strong><small>{t("settings.bangumiNetwork")}</small></span><input aria-label={t("settings.enableBangumi")} checked={settings?.bangumiSearchEnabled ?? false} disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, bangumiSearchEnabled: event.target.checked }))} type="checkbox" /><i /></label></div>
          <label className="switch-field settings-switch-row"><span><strong>{t("settings.autoScanOnStartup")}</strong><small>{t("settings.autoScanOnStartupDescription")}</small></span><input aria-label={t("settings.autoScanOnStartup")} checked={settings?.autoScanOnStartup ?? true} disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, autoScanOnStartup: event.target.checked }))} type="checkbox" /><i /></label>
          <div className="extension-editor"><span>{t("settings.videoExtensions")}</span><div className="extension-chips">{settings?.videoExtensions.map((extension) => <button disabled={settings.videoExtensions.length <= 1} key={extension} onClick={() => changeSettings((current) => ({ ...current, videoExtensions: current.videoExtensions.filter((item) => item !== extension) }))} title={t("settings.removeExtension")} type="button">{extension}<Icon name="close" /></button>)}</div><div className="extension-add"><input onChange={(event) => setExtensionDraft(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); addExtension(); } }} placeholder={t("settings.extensionPlaceholder")} value={extensionDraft} /><button onClick={addExtension} type="button">{t("common.add")}</button></div></div>
        </section>

        <section className="settings-section">
          <div className="settings-section-heading"><span className="settings-symbol coral"><Icon name="globe" /></span><div><h2>{t("settings.appearanceTitle")}</h2><p>{t("settings.appearanceDescription")}</p></div></div>
          <div className="settings-columns">
            <label className="field-label"><span>{t("settings.language")}</span><select disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, language: event.target.value as AppLanguage }))} value={settings?.language ?? "zh-CN"}><option value="zh-CN">{t("settings.languageZh")}</option><option value="en-US">{t("settings.languageEn")}</option><option value="ja-JP">{t("settings.languageJa")}</option><option value="ko-KR">{t("settings.languageKo")}</option></select></label>
            <label className="field-label"><span>{t("settings.theme")}</span><select disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, theme: event.target.value as AppTheme }))} value={settings?.theme ?? "system"}><option value="system">{t("settings.themeSystem")}</option><option value="light">{t("settings.themeLight")}</option><option value="dark">{t("settings.themeDark")}</option></select></label>
          </div>
        </section>

        <section className="settings-section">
          <div className="settings-section-heading"><span className="settings-symbol green"><Icon name="database" /></span><div><h2>{t("settings.cacheTitle")}</h2><p>{t("settings.cacheDescription")}</p></div></div>
          <label className="field-label cache-directory-field"><span>{t("settings.cacheDirectory")}</span><div className="path-input"><input disabled={!settings} readOnly title={settings?.coverCacheDirectory} value={settings?.coverCacheDirectory ?? ""} /><button disabled={!desktopAvailable || !settings} onClick={() => void chooseCacheDirectory()} type="button">{t("settings.change")}</button><button disabled={!desktopAvailable} onClick={() => void openCache()} type="button">{t("common.open")}</button></div><small>{t("settings.cachePathHelp")}</small></label>
          <div className="maintenance-grid"><article><Icon name="image" /><div><strong>{t("settings.coverCache")}</strong><p>{cache ? t("settings.cacheFileSummary", { count: cache.fileCount, size: formatBytes(cache.totalBytes) }) : t("common.unknown")}</p><small title={cache?.cacheDirectory}>{cache ? compactPath(cache.cacheDirectory, 55) : t("common.notAvailable")}</small></div><span className="maintenance-actions"><button disabled={!desktopAvailable} onClick={() => void openCache()} type="button">{t("common.open")}</button><button disabled={!desktopAvailable} onClick={() => void clearCache()} type="button">{t("common.clean")}</button></span></article><article><Icon name="refresh" /><div><strong>{t("settings.rebuildIndex")}</strong><p>{t("settings.rebuildDescription")}</p><small>{t("settings.noMediaChanges")}</small></div><button disabled={!desktopAvailable || roots.length === 0} onClick={() => void rebuild()} type="button">{t("common.rebuild")}</button></article></div>
        </section>

        <section className="settings-section about-section">
          <div className="settings-section-heading"><span className="settings-symbol coral"><Icon name="info" /></span><div><h2>{t("settings.aboutTitle")}</h2><p>{t("settings.aboutDescription")}</p></div></div>
          <div className="about-brand"><strong>{t("brand.name")}</strong><span>{t("brand.subtitle")}</span></div>
          <div className="update-preferences">
            <label className="switch-field settings-switch-row"><span><strong>{t("settings.autoCheckUpdates")}</strong><small>{t("settings.autoCheckUpdatesDescription")}</small></span><input aria-label={t("settings.autoCheckUpdates")} checked={settings?.autoCheckUpdates ?? false} disabled={!settings || !desktopAvailable} onChange={(event) => changeSettings((current) => ({ ...current, autoCheckUpdates: event.target.checked }))} type="checkbox" /><i /></label>
            <div className="switch-field update-manual-check"><span><strong>{t("settings.manualCheckUpdates")}</strong><small>{t("settings.manualCheckUpdatesDescription")}</small></span><button aria-label={t("update.checkAria")} className={`button secondary${updateDownloadStatus.phase === "CHECKING" ? " is-busy" : ""}`} disabled={!desktopAvailable || updateDownloadStatus.phase === "CHECKING" || updateDownloadStatus.phase === "DOWNLOADING" || updateDownloadStatus.phase === "APPLYING"} onClick={onCheckForUpdate} title={t("update.checkAria")} type="button"><Icon name="refresh" />{updateDownloadStatus.phase === "CHECKING" ? t("update.checking") : t("update.check")}</button></div>
          </div>
          <dl className="about-grid">
            <div><dt>{t("settings.currentVersion")}</dt><dd>{bootstrap?.version ?? t("common.notAvailable")}</dd></div>
            <div><dt>{t("settings.updateDate")}</dt><dd>{bootstrap?.buildDate && bootstrap.buildDate !== "unknown" ? bootstrap.buildDate : t("common.notAvailable")}</dd></div>
            <div><dt>{t("settings.architecture")}</dt><dd>{bootstrap?.architecture ?? t("common.notAvailable")}</dd></div>
            <div><dt>{t("settings.originalAuthor")}</dt><dd>{t("brand.author")}</dd></div>
            <div><dt>{t("settings.contributor")}</dt><dd>Juvenile_A</dd></div>
            <div><dt>{t("settings.website")}</dt><dd className="about-author-links"><button disabled={!bootstrap?.websiteUrl || !desktopAvailable} onClick={() => void openAuthorLink(bootstrap?.websiteUrl)} type="button">{t("settings.websiteLabel")} <Icon name="external" /></button><button disabled={!bootstrap?.xUrl || !desktopAvailable} onClick={() => void openAuthorLink(bootstrap?.xUrl)} type="button">{t("settings.xLabel")} <Icon name="external" /></button></dd></div>
          </dl>
          <p className="created-by">{t("settings.createdBy")} <button disabled={!bootstrap?.websiteUrl || !desktopAvailable} onClick={() => void openAuthorLink(bootstrap?.websiteUrl)} type="button">{t("brand.author")}</button></p>
        </section>
      </div>
      <footer className="settings-savebar"><span><Icon name="shield" />{t("common.readOnlyShort")}</span><span className="settings-save-status" aria-live="polite"><Icon name={saving || saveState === "saving" ? "refresh" : "check"} />{!settings ? t("settings.autoSaveDesktop") : saving || saveState === "saving" ? t("settings.autoSaving") : t("settings.autoSaved")}</span></footer>
    </section>
  );
}

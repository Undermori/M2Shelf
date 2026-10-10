import {TmdbDiagnostics} from "../components/TmdbDiagnostics";
import tmdbLogo from "../assets/tmdb-logo.svg";
import {Select} from '../components/Select';
import { useEffect, useRef, useState } from "react";
import type { AppBootstrap, AppLanguage, AppSettings, AppTheme, CacheStats, LibraryRoot, UpdateDownloadStatus } from "../types/media";
import {PosterCacheProgress} from "../components/PosterCacheProgress";
import { LibraryScanHealth } from "../components/LibraryScanHealth";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { LoadingState } from "../components/LoadingState";
import { api, chooseCoverCacheDirectory, choosePlayerExecutable, desktopAvailable } from "../lib/api";
import { compactPath, errorMessage, formatBytes, formatDate } from "../lib/format";
import { useI18n } from "../lib/i18n";
import {defaultComicReaderSettings,type ComicReaderSettings} from '../types/comic';
import { useAppSettings } from '../lib/settingsStore';

interface SettingsPageProps {
  aboutRequest?:number;
  onRootPolicy?:(root:LibraryRoot,enabled:boolean)=>Promise<void>;
  roots: LibraryRoot[];
  bootstrap: AppBootstrap | null;
  onAddRoot: () => void;
  onHiddenNodes: () => void;
  onRemoveRoot: (root: LibraryRoot) => void;
  onScanRoot: (root: LibraryRoot) => void;
  onIgnoreScanWarnings?: (root: LibraryRoot) => void;
  updateDownloadStatus: UpdateDownloadStatus;
  onCheckForUpdate: () => void;
  onError: (message: string) => void;
  onSuccess: (message: string) => void;
}

export function SettingsPage({ aboutRequest=0, onRootPolicy, roots, bootstrap, onAddRoot, onHiddenNodes, onRemoveRoot, onScanRoot, onIgnoreScanWarnings, updateDownloadStatus, onCheckForUpdate, onError, onSuccess }: SettingsPageProps) {
  const { t } = useI18n();
  const { settings, saving, savedSequence, load, change: changeSettings } = useAppSettings();
  const [loading, setLoading] = useState(true);
  const [policyBusy,setPolicyBusy]=useState<number|null>(null);
  useEffect(()=>{if(!loading&&aboutRequest)document.getElementById("settings-about")?.scrollIntoView({block:"start"});},[aboutRequest,loading]);
  const [cache, setCache] = useState<CacheStats | null>(null);
  const [posterEpoch, setPosterEpoch] = useState(0);
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "fading">("idle");
  const [testing, setTesting] = useState(false);
  const [extensionDraft, setExtensionDraft] = useState("");
  const reader=settings?.comicReader??defaultComicReaderSettings;
  const latestSettings = useRef<AppSettings | null>(null);
  latestSettings.current = settings;
  const observedSave = useRef(savedSequence);
  const mounted = useRef(true);
  const cacheRequest = useRef(0);
  const cacheDirectory = useRef(settings?.coverCacheDirectory);

  const refreshCacheStats = (expectedDirectory?: string) => {
    const request = ++cacheRequest.current;
    const isCurrent = () => mounted.current && request === cacheRequest.current
      && (!expectedDirectory || expectedDirectory === latestSettings.current?.coverCacheDirectory);
    void api.cacheStats().then((next) => {
      if (isCurrent()) setCache(next);
    }).catch((error) => {
      if (isCurrent()) onError(errorMessage(error));
    });
  };

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  useEffect(() => {
    if (saving) setSaveState("saving");
    else if (savedSequence !== observedSave.current) setSaveState("saved");
    else setSaveState("idle");
    observedSave.current = savedSequence;
  }, [saving, savedSequence]);

  useEffect(() => {
    if (cacheDirectory.current === settings?.coverCacheDirectory) return;
    cacheDirectory.current = settings?.coverCacheDirectory;
    cacheRequest.current += 1;
    setCache(null);
    if (settings) refreshCacheStats(settings.coverCacheDirectory);
  // Cache reads follow the shared, revision-safe settings value, including rollback.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings?.coverCacheDirectory]);

  // A receipt is shown only for an actual save, never for the initial settings read.
  useEffect(() => {
    if (saveState !== "saved" && saveState !== "fading") return;
    const timer = window.setTimeout(() => setSaveState(saveState === "saved" ? "fading" : "idle"), saveState === "saved" ? 3200 : 180);
    return () => window.clearTimeout(timer);
  }, [saveState]);

  useEffect(() => {
    let active = true;
    if (!desktopAvailable) { setLoading(false); return; }
    void load().catch((error) => {
      if (active) onError(errorMessage(error));
    }).finally(() => active && setLoading(false));
    refreshCacheStats();
    return () => { active = false; cacheRequest.current += 1; };
  // Settings are loaded when this page is mounted; parent notification callbacks do not affect the request.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
    const request = ++cacheRequest.current;
    try {
      const next = await api.clearCoverCache();
      if (mounted.current && request === cacheRequest.current) {setCache(next);setPosterEpoch((epoch)=>epoch+1);}
      onSuccess(t("settings.cacheCleared"));
    }
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
      <header className="settings-header"><div className="settings-title-row"><h1>{t("settings.title")}</h1><span className={`settings-save-status${saveState === "saved" || saveState === "saving" ? " is-visible" : ""}`} role="status" aria-live="polite" aria-atomic="true">{saveState !== "idle" && <><Icon name={saving || saveState === "saving" ? "refresh" : "check"} />{saving || saveState === "saving" ? t("settings.autoSaving") : t("settings.autoSaved")}</>}</span></div></header>
      {!desktopAvailable && <div className="preview-banner"><Icon name="info" /><span><strong>{t("settings.previewTitle")}</strong><small>{t("settings.previewDescription")}</small></span></div>}

      <div className="settings-layout">
        <section className="settings-section">
          <div className="settings-section-heading"><span className="settings-symbol coral"><Icon name="folder" /></span><div><h2>{t("settings.rootsTitle")}</h2></div><button className="button secondary" disabled={!desktopAvailable} onClick={onAddRoot} type="button"><Icon name="plus" />{t("settings.addDirectory")}</button></div>
          <div className="settings-section-body">
          {roots.length === 0 ? <EmptyState compact icon="folder" title={t("settings.noDirectory")} description={t("settings.noDirectoryDescription")} /> : <div className="settings-root-list">{roots.map((root) => (
            <article className="settings-root" key={root.id}><span><Icon name="folder-open" /></span><div><strong>{root.displayName}</strong><p title={root.path}>{compactPath(root.path, 74)}</p><small>{t(root.mediaKind==='ARTBOOK'?'artbook.name':root.mediaKind==='DOUJIN'?'doujin.name':root.mediaKind==='ANIMATION'?'comic.animation':root.mediaKind==='LIVE_ACTION'?'library.liveAction':root.mediaKind==='EBOOK'?'ebook.name':root.mediaKind==='COMIC'?'comic.name':'comic.unboundVideo')} · {t(root.bookOrganizationStrategy==='SMART_MIXED'?'smart.title':root.recognitionMode==='VIDEO_FILE'?(root.mediaKind==='COMIC'||root.mediaKind==='EBOOK'||(root.mediaKind==='DOUJIN' || root.mediaKind === 'ARTBOOK')?'bookMode.fileTitle':'rootMode.videoFileTitle'):'rootMode.folderTitle')}</small><small>{t("settings.lastScan", { date: formatDate(root.lastScanAt) })}</small><LibraryScanHealth health={root.scanHealth} />{['COMIC','EBOOK','DOUJIN','ARTBOOK'].includes(root.mediaKind??'') && <label className="root-policy-inline"><input type="checkbox" disabled={!onRootPolicy||policyBusy!==null} checked={root.autoBangumi===false || root.autoBangumi===undefined&&root.mediaKind==='DOUJIN'} onChange={event=>{setPolicyBusy(root.id);void onRootPolicy?.(root,!event.target.checked).finally(()=>setPolicyBusy(null));}} />{t('library.noAutoBangumi')}</label>}</div><div className="settings-root-actions">{root.scanHealth && root.scanHealth.outcome !== 'SUCCESS' && <button className="icon-button" aria-pressed={!!root.scanHealth.warningsIgnored} aria-label={t(root.scanHealth.warningsIgnored?'comic.restoreWarnings':'comic.ignoreWarnings')} title={t(root.scanHealth.warningsIgnored?'comic.restoreWarnings':'comic.ignoreWarnings')} disabled={!onIgnoreScanWarnings} onClick={()=>onIgnoreScanWarnings?.(root)} type="button"><Icon name={root.scanHealth.warningsIgnored?'warning':'close'}/></button>}<button aria-label={t("settings.scanNamed", { name: root.displayName })} onClick={() => onScanRoot(root)} title={t("settings.scanLibrary")} type="button"><Icon name="refresh" /></button><button className="danger-icon" aria-label={t("settings.removeNamed", { name: root.displayName })} onClick={() => onRemoveRoot(root)} title={t("settings.removeIndexOnly")} type="button"><Icon name="trash" /></button></div></article>
          ))}</div>}
          <div className="settings-hidden-entry"><div><strong>{t("hidden.title")}</strong><p>{t("settings.hiddenDescription")}</p></div><button className="button secondary" disabled={!desktopAvailable} onClick={onHiddenNodes} type="button" aria-haspopup="dialog"><Icon name="eye-off" />{t("hidden.title")}</button></div>
          </div>
        </section>

        <section className="settings-section">
          <div className="settings-section-heading"><span className="settings-symbol navy"><Icon name="play" /></span><div><h2>{t("settings.playerTitle")}</h2></div></div>
          <div className="settings-section-body">
          <div className="setting-form-row"><label><span>{t("settings.playerPath")}</span><div className="path-input"><input disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, mpvPath: event.target.value || null }))} placeholder={t("settings.playerPlaceholder")} value={settings?.mpvPath ?? ""} /><button disabled={!desktopAvailable} onClick={() => void choosePlayer()} type="button">{t("common.chooseEllipsis")}</button></div></label><button className="button secondary" disabled={!settings?.mpvPath || testing} onClick={() => void testMpv()} type="button">{testing ? t("settings.testing") : t("settings.testPlayer")}</button></div>
          </div>
        </section>

        <section className="settings-section">
          <div className="settings-section-heading"><span className="settings-symbol gold"><Icon name="settings" /></span><div><h2>{t("settings.browseScanTitle")}</h2></div></div>
          <div className="settings-section-body">
          <div className="settings-preference-list"><label className="field-label"><span>{t("settings.defaultView")}</span><Select disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, defaultViewMode: event.target.value as "GRID" | "LIST" }))} value={settings?.defaultViewMode ?? "GRID"}><option value="GRID">{t("settings.posterGrid")}</option><option value="LIST">{t("settings.compactList")}</option></Select></label><label className="switch-field settings-switch-row"><span><strong>{t("settings.enableBangumi")}</strong><small>{t("settings.bangumiNetwork")}</small></span><input aria-label={t("settings.enableBangumi")} checked={settings?.bangumiSearchEnabled ?? false} disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, bangumiSearchEnabled: event.target.checked }))} type="checkbox" /><i /></label>
          <div className="tmdb-credits settings-metadata"><img src={tmdbLogo} alt="TMDB"/><div><p>{t('tmdb.notice')}</p><p>{t('tmdb.license')}</p><div className="settings-metadata-actions"><button className="button secondary" disabled={!desktopAvailable} onClick={()=>void openAuthorLink('https://www.themoviedb.org')} type="button">TMDB <Icon name="external"/></button><button className="button secondary" disabled={!desktopAvailable} onClick={()=>void api.tmdbConfigure(t('tmdb.token'),t('tmdb.credentialsHelp')).catch(()=>onError(t('tmdb.error')))} type="button">{t('tmdb.configure')}</button></div></div></div>
          <TmdbDiagnostics/>
          <label className="switch-field settings-switch-row"><span><strong>{t("settings.autoScanOnStartup")}</strong><small>{t("settings.autoScanOnStartupDescription")}</small></span><input aria-label={t("settings.autoScanOnStartup")} checked={settings?.autoScanOnStartup ?? true} disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, autoScanOnStartup: event.target.checked }))} type="checkbox" /><i /></label>
          <label className="switch-field settings-switch-row"><span><strong>{t("settings.allResourcesFlattened")}</strong><small>{t("settings.allResourcesFlattenedDescription")}</small></span><input aria-label={t("settings.allResourcesFlattened")} checked={settings?.allResourcesFlattened ?? false} disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, allResourcesFlattened: event.target.checked }))} type="checkbox" /><i /></label>
          </div>
          <div className="extension-editor"><span>{t("settings.videoExtensions")}</span><div className="extension-chips">{settings?.videoExtensions.map((extension) => <button disabled={settings.videoExtensions.length <= 1} key={extension} onClick={() => changeSettings((current) => ({ ...current, videoExtensions: current.videoExtensions.filter((item) => item !== extension) }))} title={t("settings.removeExtension")} type="button">{extension}<Icon name="close" /></button>)}</div><div className="extension-add"><input onChange={(event) => setExtensionDraft(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); addExtension(); } }} placeholder={t("settings.extensionPlaceholder")} value={extensionDraft} /><button onClick={addExtension} type="button">{t("common.add")}</button></div></div>
          </div>
        </section>

        <section className="settings-section comic-settings">
          <div className="settings-section-heading"><span className="settings-symbol navy"><Icon name="work"/></span><div><h2>{t('comic.defaults')}</h2><p>{t('comic.emptyHelp')}</p></div></div>
          <div className="settings-section-body">
          <div className="settings-columns">
            <label className="field-label"><span>{t('comic.direction')}</span><Select disabled={!settings} value={reader.direction} onChange={e=>changeSettings(s=>({...s,comicReader:{...(s.comicReader??defaultComicReaderSettings),direction:e.target.value as ComicReaderSettings['direction']}}))}><option value="RTL">{t('comic.rtl')}</option><option value="LTR">{t('comic.ltr')}</option></Select></label>
            <label className="field-label"><span>{t('comic.layout')}</span><Select disabled={!settings} value={reader.layout} onChange={e=>changeSettings(s=>({...s,comicReader:{...(s.comicReader??defaultComicReaderSettings),layout:e.target.value as ComicReaderSettings['layout']}}))}><option value="DOUBLE">{t('comic.double')}</option><option value="SINGLE">{t('comic.single')}</option></Select></label>
            <label className="field-label"><span>{t('comic.mode')}</span><Select disabled={!settings} value={reader.mode} onChange={e=>changeSettings(s=>({...s,comicReader:{...(s.comicReader??defaultComicReaderSettings),mode:e.target.value as ComicReaderSettings['mode']}}))}><option value="PAGED">{t('comic.paged')}</option><option value="SCROLL">{t('comic.scroll')}</option><option value="WEBTOON">{t('comic.webtoon')}</option></Select></label>
          </div><label className="switch-field settings-switch-row"><span><strong>{t('comic.wideAlone')}</strong></span><input disabled={!settings} type="checkbox" checked={reader.widePageAlone} onChange={e=>changeSettings(s=>({...s,comicReader:{...(s.comicReader??defaultComicReaderSettings),widePageAlone:e.target.checked}}))}/><i/></label>
          </div>
        </section>
        <section className="settings-section">
          <div className="settings-section-heading"><span className="settings-symbol coral"><Icon name="globe" /></span><div><h2>{t("settings.appearanceTitle")}</h2></div></div>
          <div className="settings-section-body">
          <div className="settings-columns">
            <label className="field-label"><span>{t("settings.language")}</span><Select disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, language: event.target.value as AppLanguage }))} value={settings?.language ?? "zh-CN"}><option value="zh-CN">{t("settings.languageZh")}</option><option value="en-US">{t("settings.languageEn")}</option><option value="ja-JP">{t("settings.languageJa")}</option><option value="ko-KR">{t("settings.languageKo")}</option></Select></label>
            <label className="field-label"><span>{t("settings.theme")}</span><Select disabled={!settings} onChange={(event) => changeSettings((current) => ({ ...current, theme: event.target.value as AppTheme }))} value={settings?.theme ?? "system"}><option value="system">{t("settings.themeSystem")}</option><option value="light">{t("settings.themeLight")}</option><option value="dark">{t("settings.themeDark")}</option></Select></label>
          </div>
          </div>
        </section>

        <section className="settings-section">
          <div className="settings-section-heading"><span className="settings-symbol green"><Icon name="database" /></span><div><h2>{t("settings.cacheTitle")}</h2></div></div>
          <div className="settings-section-body">
          <label className="field-label cache-directory-field"><span>{t("settings.cacheDirectory")}</span><div className="path-input"><input disabled={!settings} readOnly title={settings?.coverCacheDirectory} value={settings?.coverCacheDirectory ?? ""} /><button disabled={!desktopAvailable || !settings} onClick={() => void chooseCacheDirectory()} type="button">{t("settings.change")}</button><button disabled={!desktopAvailable} onClick={() => void openCache()} type="button">{t("common.open")}</button></div></label>
          <PosterCacheProgress key={posterEpoch} cacheDirectory={settings?.coverCacheDirectory} onCompleted={()=>refreshCacheStats()} />
          <div className="maintenance-grid"><article><Icon name="image" /><div><strong>{t("settings.coverCache")}</strong><p>{cache ? t("settings.cacheFileSummary", { count: cache.fileCount, size: formatBytes(cache.totalBytes) }) : t("common.unknown")}</p><small title={cache?.cacheDirectory}>{cache ? compactPath(cache.cacheDirectory, 55) : t("common.notAvailable")}</small></div><span className="maintenance-actions"><button disabled={!desktopAvailable} onClick={() => void openCache()} type="button">{t("common.open")}</button><button disabled={!desktopAvailable} onClick={() => void clearCache()} type="button">{t("common.clean")}</button></span></article><article><Icon name="refresh" /><div><strong>{t("settings.rebuildIndex")}</strong><p>{t("settings.rebuildDescription")}</p></div><button disabled={!desktopAvailable || roots.length === 0} onClick={() => void rebuild()} type="button">{t("common.rebuild")}</button></article></div>
          </div>
        </section>

        <section id="settings-about" className="settings-section about-section">
          <div className="settings-section-heading"><span className="settings-symbol coral"><Icon name="info" /></span><div><h2>{t("settings.aboutTitle")}</h2><p>{t("settings.aboutDescription")}</p></div></div>
          <div className="settings-section-body">
          <div className="about-brand"><strong>{t("brand.name")}</strong><span>{t("brand.subtitle")}</span></div>
          <div className="update-preferences">
            <label className="switch-field settings-switch-row"><span><strong>{t("settings.autoCheckUpdates")}</strong><small>{t("settings.autoCheckUpdatesDescription")}</small></span><input aria-label={t("settings.autoCheckUpdates")} checked={settings?.autoCheckUpdates ?? false} disabled={!settings || !desktopAvailable} onChange={(event) => changeSettings((current) => ({ ...current, autoCheckUpdates: event.target.checked }))} type="checkbox" /><i /></label>
            <div className="switch-field update-manual-check"><span><strong>{t("settings.manualCheckUpdates")}</strong><small>{t("settings.manualCheckUpdatesDescription")}</small></span><button aria-label={t("update.checkAria")} className={`button secondary${updateDownloadStatus.phase === "CHECKING" ? " is-busy" : ""}`} disabled={!desktopAvailable || updateDownloadStatus.phase === "CHECKING" || updateDownloadStatus.phase === "DOWNLOADING" || updateDownloadStatus.phase === "APPLYING"} onClick={onCheckForUpdate} title={t("update.checkAria")} type="button"><Icon name="refresh" />{updateDownloadStatus.phase === "CHECKING" ? t("update.checking") : t("update.check")}</button></div>
          </div>
          <dl className="about-grid">
            <div><dt>{t("settings.currentVersion")}</dt><dd>{bootstrap?.version ?? t("common.notAvailable")}</dd></div>
            <div><dt>{t("settings.updateDate")}</dt><dd>{bootstrap?.buildDate && bootstrap.buildDate !== "unknown" ? bootstrap.buildDate : t("common.notAvailable")}</dd></div>
            <div><dt>{t("settings.architecture")}</dt><dd>{bootstrap?.architecture ?? t("common.notAvailable")}</dd></div>
            <div><dt>{t("settings.website")}</dt><dd className="about-author-links"><button disabled={!bootstrap?.websiteUrl || !desktopAvailable} onClick={() => void openAuthorLink(bootstrap?.websiteUrl)} type="button">{t("settings.websiteLabel")} <Icon name="external" /></button></dd></div>
            <div className="about-credit"><dt>{t("settings.specialThanks")}</dt><dd>Juvenile_A</dd></div>
          </dl>
          </div>
        </section>
      </div>
    </section>
  );
}

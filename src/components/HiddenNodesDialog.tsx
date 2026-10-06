import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { nodeDisplayTitle } from "../lib/format";
import { useI18n } from "../lib/i18n";
import type { AllResourcesResult, LibraryRoot, MediaNode } from "../types/media";
import { Icon } from "./Icon";

interface HiddenNodesDialogProps {
  roots: LibraryRoot[];
  refreshKey: AllResourcesResult | null;
  onClose: () => void;
  onRestored: () => Promise<void>;
}

export function HiddenNodesDialog({ roots, refreshKey, onClose, onRestored }: HiddenNodesDialogProps) {
  const { language, number, t } = useI18n();
  const [nodes, setNodes] = useState<MediaNode[] | null>(null);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const sequence = useRef(0);
  const mutationBusy = useRef(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLElement>(null);

  const load = useCallback(async () => {
    const request = ++sequence.current;
    setLoading(true);
    setError(null);
    try {
      const next = await api.listHiddenNodes();
      if (request === sequence.current) setNodes(next);
    } catch (cause) {
      if (request === sequence.current) setError(cause instanceof Error ? cause.message : t("hidden.failed"));
    } finally {
      if (request === sequence.current) setLoading(false);
    }
  }, [t]);

  // Catalogue refreshes after scans also refresh this index-backed list.
  useEffect(() => {
    void load();
    return () => { sequence.current += 1; };
  }, [load, refreshKey]);

  useEffect(() => {
    const previousFocus = document.activeElement;
    inputRef.current?.focus();
    return () => { if (previousFocus instanceof HTMLElement) previousFocus.focus(); };
  }, []);

  const restore = async (node: MediaNode) => {
    if (mutationBusy.current) return;
    mutationBusy.current = true;
    setBusyId(node.id);
    setError(null);
    setNotice(null);
    try {
      await api.resetNodeType(node.id);
      // A pending pre-mutation list must never put this row back.
      sequence.current += 1;
      setLoading(false);
      setNodes((current) => current?.filter((item) => item.id !== node.id) ?? null);
      setNotice(t("hidden.restored", { name: nodeDisplayTitle(node, language) }));
      try {
        await onRestored();
      } catch {
        setError(t("hidden.refreshFailed"));
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("hidden.failed"));
    } finally {
      mutationBusy.current = false;
      setBusyId(null);
      inputRef.current?.focus();
    }
  };

  const normalized = query.trim().toLocaleLowerCase(language);
  const visible = nodes?.filter((node) => [nodeDisplayTitle(node, language), node.displayName,
    node.folderName, node.absolutePath, roots.find((root) => root.id === node.libraryRootId)?.displayName ?? ""]
    .some((value) => value.toLocaleLowerCase(language).includes(normalized))) ?? [];

  return <div className="modal-backdrop" onPointerDown={(event) => {
    if (event.target === event.currentTarget && !mutationBusy.current) onClose();
  }}>
    <section ref={dialogRef} className="hidden-nodes-dialog" role="dialog" aria-modal="true"
      aria-labelledby="hidden-nodes-title" aria-describedby="hidden-nodes-help" tabIndex={-1}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          if (!mutationBusy.current) onClose();
        }
        if (event.key !== "Tab") return;
        const controls = dialogRef.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)');
        if (!controls?.length) return;
        const first = controls[0], last = controls[controls.length - 1];
        if (event.shiftKey && (document.activeElement === first || document.activeElement === dialogRef.current)) {
          event.preventDefault(); last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault(); first.focus();
        }
      }}>
      <header className="modal-header">
        <span className="modal-heading-icon"><Icon name="eye-off" /></span>
        <div><h2 id="hidden-nodes-title">{t("hidden.title")}</h2><p id="hidden-nodes-help">{t("hidden.description")}</p></div>
        <button className="modal-close" aria-label={t("common.close")} disabled={busyId !== null} onClick={onClose} type="button"><Icon name="close" /></button>
      </header>
      <div className="hidden-nodes-controls">
        <label className="search-field"><Icon name="search" /><input ref={inputRef} value={query}
          aria-label={t("hidden.search")} placeholder={t("hidden.search")} onChange={(event) => setQuery(event.target.value)} /></label>
        <button className="button secondary" disabled={loading || busyId !== null} onClick={() => void load()} type="button"><Icon name="refresh" />{t("hidden.refresh")}</button>
      </div>
      <p className="hidden-nodes-help">{t("hidden.parentHelp")}</p>
      {notice && <p className="hidden-nodes-notice" role="status">{notice}</p>}
      {error && <div className="inline-error hidden-nodes-error" role="alert"><span>{error}</span><button disabled={loading || busyId !== null} onClick={() => void load()} type="button">{t("common.retry")}</button></div>}
      <div className="hidden-nodes-list" aria-busy={loading || busyId !== null}>
        {loading && nodes === null && <p className="tag-empty">{t("hidden.loading")}</p>}
        {!loading && !error && nodes?.length === 0 && <p className="tag-empty">{t("hidden.empty")}</p>}
        {nodes !== null && nodes.length > 0 && visible.length === 0 && <p className="tag-empty">{t("hidden.noMatch")}</p>}
        {visible.map((node) => <article className="hidden-node-row" key={node.id}>
          <Icon name="eye-off" />
          <div className="hidden-node-copy">
            <h3>{nodeDisplayTitle(node, language)}</h3>
            <p>{roots.find((root) => root.id === node.libraryRootId)?.displayName} · {t("works.videoCount", { count: number(node.totalVideoCount ?? node.directVideoCount ?? 0) })}</p>
            <small>{node.absolutePath}</small>
          </div>
          <button className="button secondary" disabled={busyId !== null} type="button"
            aria-label={t("hidden.restoreNamed", { name: nodeDisplayTitle(node, language) })}
            onClick={() => void restore(node)}><Icon name="refresh" />{busyId === node.id ? t("common.processing") : t("hidden.restore")}</button>
        </article>)}
      </div>
    </section>
  </div>;
}

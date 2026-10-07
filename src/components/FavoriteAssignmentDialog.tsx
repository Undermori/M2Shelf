import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { errorMessage } from "../lib/format";
import { useI18n } from "../lib/i18n";
import type { BatchMutationResult, FavoriteFolder } from "../types/media";
import { Icon } from "./Icon";

interface FavoriteAssignmentDialogProps {
  nodeIds: number[];
  onClose: () => void;
  onApplied: (result: BatchMutationResult, folder: FavoriteFolder) => void | Promise<void>;
  onFoldersChanged?: () => unknown | Promise<unknown>;
}

export function FavoriteAssignmentDialog({ nodeIds, onClose, onApplied, onFoldersChanged }: FavoriteAssignmentDialogProps) {
  const { number, t } = useI18n();
  const inputRef = useRef<HTMLInputElement>(null);
  const [folders, setFolders] = useState<FavoriteFolder[] | null>(null);
  const [newName, setNewName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try { setFolders(await api.listFavoriteFolders()); }
    catch (cause) { setFolders([]); setError(errorMessage(cause)); }
  }, []);

  useEffect(() => { if (nodeIds.length > 0) void load(); }, [load, nodeIds.length]);
  useEffect(() => { if (folders) inputRef.current?.focus(); }, [folders]);
  useEffect(() => {
    if (nodeIds.length === 0) return;
    const key = (event: KeyboardEvent) => event.key === "Escape" && !busy && onClose();
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [busy, nodeIds.length, onClose]);

  if (nodeIds.length === 0) return null;

  const assign = async (folder: FavoriteFolder) => {
    setBusy(true);
    setError(null);
    try {
      const result = await api.batchAddNodesToFavorite(folder.id, nodeIds);
      await onApplied(result, { ...folder, itemCount: folder.itemCount + result.updated });
      onClose();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const create = async (event: React.FormEvent) => {
    event.preventDefault();
    const name = newName.trim();
    if (!name || busy) return;
    setBusy(true);
    setError(null);
    try {
      const folder = await api.createFavoriteFolder(name);
      const result = await api.batchAddNodesToFavorite(folder.id, nodeIds);
      await onFoldersChanged?.();
      await onApplied(result, { ...folder, itemCount: result.updated });
      onClose();
    } catch (cause) {
      setError(errorMessage(cause));
      await onFoldersChanged?.();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onPointerDown={(event) => { if (event.currentTarget === event.target && !busy) onClose(); }}>
      <section aria-labelledby="favorite-assignment-title" aria-modal="true" className="tag-manager-dialog favorite-assignment-dialog" role="dialog">
        <header className="modal-header">
          <span className="modal-heading-icon"><Icon name="bookmark" /></span>
          <div><p className="eyebrow">{t("favorites.organizeEyebrow")}</p><h2 id="favorite-assignment-title">{t("favorites.assignTitle")}</h2><p>{t("favorites.assignDescription", { count: number(nodeIds.length) })}</p></div>
          <button aria-label={t("common.close")} className="modal-close" disabled={busy} onClick={onClose} type="button"><Icon name="close" /></button>
        </header>
        <form className="tag-create-form" onSubmit={(event) => void create(event)}>
          <label htmlFor="new-favorite-folder">{t("favorites.newFolder")}</label>
          <div><input id="new-favorite-folder" maxLength={80} onChange={(event) => setNewName(event.target.value)} placeholder={t("favorites.namePlaceholder")} ref={inputRef} value={newName} /><button className="button primary" disabled={busy || !newName.trim()} type="submit"><Icon name="plus" />{busy ? t("common.processing") : t("favorites.createAndAdd")}</button></div>
        </form>
        {error && <div className="inline-error tag-inline-error" role="alert"><Icon name="warning" /><span><strong>{t("favorites.operationFailed")}</strong><small>{error}</small></span><button onClick={() => void load()} type="button">{t("common.retry")}</button></div>}
        <div className="tag-memberships favorite-options" aria-busy={folders === null || busy}>
          {folders === null && <p className="tag-empty"><Icon name="refresh" />{t("favorites.loading")}</p>}
          {folders?.length === 0 && <p className="tag-empty"><Icon name="bookmark" />{t("favorites.noFolders")}</p>}
          {folders?.map((folder) => <button className="batch-tag-option" disabled={busy} key={folder.id} onClick={() => void assign(folder)} type="button"><span><Icon name="bookmark" /></span><strong>{folder.name}</strong><small>{t("favorites.itemCount", { count: number(folder.itemCount) })}</small><Icon name="chevron" /></button>)}
        </div>
        <footer className="modal-footer"><Icon name="shield" />{t("favorites.footer")}</footer>
      </section>
    </div>
  );
}

import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { errorMessage } from "../lib/format";
import { useI18n } from "../lib/i18n";
import type { BatchMutationResult, UserTagMembership } from "../types/media";
import { Icon } from "./Icon";

interface BatchTagDialogProps {
  nodeIds: number[];
  onClose: () => void;
  onApplied: (result: BatchMutationResult) => void | Promise<void>;
}

export function BatchTagDialog({ nodeIds, onClose, onApplied }: BatchTagDialogProps) {
  const { number, t } = useI18n();
  const inputRef = useRef<HTMLInputElement>(null);
  const [tags, setTags] = useState<UserTagMembership[] | null>(null);
  const [newName, setNewName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const firstNodeId = nodeIds[0];

  const load = useCallback(async () => {
    if (firstNodeId == null) return;
    setError(null);
    try { setTags(await api.listUserTags(firstNodeId)); }
    catch (cause) { setTags([]); setError(errorMessage(cause)); }
  }, [firstNodeId]);

  useEffect(() => { void load(); }, [load]);
  useEffect(() => { if (tags) inputRef.current?.focus(); }, [tags]);
  useEffect(() => {
    if (nodeIds.length === 0) return;
    const key = (event: KeyboardEvent) => event.key === "Escape" && !busy && onClose();
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [busy, nodeIds.length, onClose]);

  if (nodeIds.length === 0) return null;

  const run = async (operation: () => Promise<BatchMutationResult>) => {
    setBusy(true);
    setError(null);
    try {
      const result = await operation();
      await onApplied(result);
      onClose();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const create = (event: React.FormEvent) => {
    event.preventDefault();
    const name = newName.trim();
    if (!name || busy) return;
    void run(() => api.batchCreateAndAssignTag(nodeIds, name));
  };

  return (
    <div className="modal-backdrop" onPointerDown={(event) => { if (event.currentTarget === event.target && !busy) onClose(); }}>
      <section aria-labelledby="batch-tags-title" aria-modal="true" className="tag-manager-dialog batch-tag-dialog" role="dialog">
        <header className="modal-header">
          <span className="modal-heading-icon"><Icon name="tag" /></span>
          <div><p className="eyebrow">{t("selection.tagEyebrow")}</p><h2 id="batch-tags-title">{t("selection.tagTitle")}</h2><p>{t("selection.tagDescription", { count: number(nodeIds.length) })}</p></div>
          <button aria-label={t("common.close")} className="modal-close" disabled={busy} onClick={onClose} type="button"><Icon name="close" /></button>
        </header>
        <form className="tag-create-form" onSubmit={create}>
          <label htmlFor="new-batch-tag">{t("selection.createTag")}</label>
          <div><input id="new-batch-tag" maxLength={40} onChange={(event) => setNewName(event.target.value)} placeholder={t("tags.addPlaceholder")} ref={inputRef} value={newName} /><button className="button primary" disabled={busy || !newName.trim()} type="submit"><Icon name="plus" />{busy ? t("common.processing") : t("selection.createAndApply")}</button></div>
        </form>
        {error && <div className="inline-error tag-inline-error" role="alert"><Icon name="warning" /><span><strong>{t("tags.operationFailed")}</strong><small>{error}</small></span><button onClick={() => void load()} type="button">{t("common.retry")}</button></div>}
        <div className="tag-memberships" aria-busy={tags === null || busy}>
          {tags === null && <p className="tag-empty"><Icon name="refresh" />{t("tags.loading")}</p>}
          {tags?.length === 0 && <p className="tag-empty"><Icon name="tag" />{t("selection.noExistingTags")}</p>}
          {tags?.map((tag) => <button className="batch-tag-option" disabled={busy} key={tag.id} onClick={() => void run(() => api.batchAssignTag(nodeIds, tag.id))} type="button"><span><Icon name="tag" /></span><strong>{tag.name}</strong><small>{t("selection.applyExistingTag")}</small><Icon name="chevron" /></button>)}
        </div>
      </section>
    </div>
  );
}

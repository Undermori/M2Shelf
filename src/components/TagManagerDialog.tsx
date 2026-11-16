import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { nodeDisplayTitle } from "../lib/format";
import { useI18n } from "../lib/i18n";
import type { MediaNode, UserTagMembership } from "../types/media";
import { Icon } from "./Icon";
import { useModalFocus } from "../hooks/useModalFocus";

interface TagManagerDialogProps {
  node: MediaNode | null;
  onClose: () => void;
  onChanged: () => void | Promise<void>;
}

export function TagManagerDialog({ node, onClose, onChanged }: TagManagerDialogProps) {
  const { t } = useI18n();
  const inputRef = useRef<HTMLInputElement>(null);
  const backdropRef = useRef<HTMLDivElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const loadSequence = useRef(0);
  const [memberships, setMemberships] = useState<UserTagMembership[] | null>(null);
  const [newName, setNewName] = useState("");
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editingName, setEditingName] = useState("");
  const [deletingId, setDeletingId] = useState<number | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!node) return;
    const requestSequence = ++loadSequence.current;
    const nodeId = node.id;
    setError(null);
    try {
      const nextMemberships = await api.listUserTags(nodeId);
      if (loadSequence.current === requestSequence) setMemberships(nextMemberships);
    } catch (cause) {
      if (loadSequence.current !== requestSequence) return;
      setError(cause instanceof Error ? cause.message : t("error.tagsFailed"));
      setMemberships([]);
    }
  }, [node?.id, t]);

  useEffect(() => {
    setMemberships(null);
    setNewName("");
    setEditingId(null);
    setDeletingId(null);
    if (node) void load();
    return () => { loadSequence.current += 1; };
  }, [load, node]);

  useModalFocus(backdropRef, dialogRef, onClose, '#new-user-tag', Boolean(busyKey), Boolean(node));

  if (!node) return null;

  const refreshAfterMutation = async () => {
    await load();
    try {
      await onChanged();
    } catch {
      // The tag mutation and dialog state are already authoritative. A parent
      // collection refresh may be retried by normal navigation without
      // misreporting the completed tag operation as failed.
    }
  };

  const run = async (key: string, operation: () => Promise<unknown>) => {
    setBusyKey(key);
    setError(null);
    try {
      await operation();
      await refreshAfterMutation();
      return true;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("error.tagsFailed"));
      return false;
    } finally {
      setBusyKey(null);
    }
  };

  const addTag = async (event: React.FormEvent) => {
    event.preventDefault();
    const name = newName.trim();
    if (!name || busyKey) return;
    if (await run("add", () => api.createOrAssignUserTag(node.id, name))) setNewName("");
  };

  const toggleTag = async (tag: UserTagMembership) => {
    if (busyKey) return;
    await run(`toggle-${tag.id}`, () => tag.assigned
      ? api.unassignUserTag(node.id, tag.id)
      : api.assignUserTag(node.id, tag.id));
  };

  const saveRename = async (tag: UserTagMembership) => {
    const name = editingName.trim();
    if (!name || busyKey) return;
    if (await run(`rename-${tag.id}`, () => api.renameUserTag(tag.id, name))) {
      setEditingId(null);
      setEditingName("");
    }
  };

  const deleteTag = async (tag: UserTagMembership) => {
    if (busyKey) return;
    if (await run(`delete-${tag.id}`, () => api.deleteUserTag(tag.id))) setDeletingId(null);
  };

  return (
    <div
      ref={backdropRef}
      className="modal-backdrop"
      onPointerDown={(event) => { if (event.currentTarget === event.target && !busyKey) onClose(); }}
    >
      <section
        ref={dialogRef}
        tabIndex={-1}
        aria-labelledby="tag-manager-title"
        aria-modal="true"
        className="tag-manager-dialog"
        role="dialog"
      >
        <header className="modal-header">
          <div>
            <h2 id="tag-manager-title">{t("tags.title")}</h2>
            <p>{nodeDisplayTitle(node)}</p>
          </div>
          <button aria-label={t("common.close")} className="modal-close" disabled={Boolean(busyKey)} onClick={onClose} type="button"><Icon name="close" /></button>
        </header>

        <form className="tag-create-form" onSubmit={(event) => void addTag(event)}>
          <label htmlFor="new-user-tag">{t("tags.addLabel")}</label>
          <div>
            <input
              id="new-user-tag"
              maxLength={40}
              onChange={(event) => setNewName(event.target.value)}
              placeholder={t("tags.addPlaceholder")}
              ref={inputRef}
              value={newName}
            />
            <button className="button primary" disabled={Boolean(busyKey) || !newName.trim()} type="submit"><Icon name="plus" />{busyKey === "add" ? t("common.processing") : t("tags.add")}</button>
          </div>
        </form>

        {error && <div className="inline-error tag-inline-error" role="alert"><Icon name="warning" /><span><strong>{t("tags.operationFailed")}</strong><small>{error}</small></span><button onClick={() => void load()} type="button">{t("common.retry")}</button></div>}

        <div className="tag-memberships" aria-busy={memberships === null || Boolean(busyKey)}>
          {memberships === null && <p className="tag-empty"><Icon name="refresh" />{t("tags.loading")}</p>}
          {memberships?.length === 0 && <p className="tag-empty"><Icon name="tag" />{t("tags.empty")}</p>}
          {memberships?.map((tag) => (
            <div className="tag-membership" key={tag.id}>
              {editingId === tag.id ? (
                <form className="tag-rename-form" onSubmit={(event) => { event.preventDefault(); void saveRename(tag); }}>
                  <label className="sr-only" htmlFor={`edit-user-tag-${tag.id}`}>{t("tags.editNamed", { name: tag.name })}</label>
                  <input autoFocus id={`edit-user-tag-${tag.id}`} maxLength={40} onChange={(event) => setEditingName(event.target.value)} value={editingName} />
                  <button className="icon-button" aria-label={t("tags.saveRename")} disabled={Boolean(busyKey) || !editingName.trim()} title={t("tags.saveRename")} type="submit"><Icon name="check" /></button>
                  <button className="icon-button" aria-label={t("common.cancel")} disabled={Boolean(busyKey)} onClick={() => setEditingId(null)} title={t("common.cancel")} type="button"><Icon name="close" /></button>
                </form>
              ) : (
                <>
                  <button
                    aria-pressed={tag.assigned}
                    className={`tag-assignment ${tag.assigned ? "is-assigned" : ""}`}
                    disabled={Boolean(busyKey)}
                    onClick={() => void toggleTag(tag)}
                    type="button"
                  >
                    <span><Icon name={tag.assigned ? "check" : "plus"} /></span>
                    <strong>{tag.name}</strong>
                  </button>
                  <button className="icon-button" aria-label={t("tags.editNamed", { name: tag.name })} disabled={Boolean(busyKey)} onClick={() => { setEditingId(tag.id); setEditingName(tag.name); setDeletingId(null); }} title={t("tags.edit")} type="button"><Icon name="edit" /></button>
                  <button className="icon-button is-danger" aria-label={t("tags.deleteNamed", { name: tag.name })} disabled={Boolean(busyKey)} onClick={() => { setDeletingId(tag.id); setEditingId(null); }} title={t("tags.delete")} type="button"><Icon name="trash" /></button>
                </>
              )}
              {deletingId === tag.id && (
                <div className="tag-delete-confirm" role="alert">
                  <p>{t("tags.deleteDescription", { name: tag.name })}</p>
                  <button className="button secondary" disabled={Boolean(busyKey)} onClick={() => setDeletingId(null)} type="button">{t("common.cancel")}</button>
                  <button className="button danger" disabled={Boolean(busyKey)} onClick={() => void deleteTag(tag)} type="button">{busyKey === `delete-${tag.id}` ? t("common.processing") : t("tags.deleteEverywhere")}</button>
                </div>
              )}
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

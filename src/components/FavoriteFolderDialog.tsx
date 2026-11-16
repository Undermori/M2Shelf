import { useEffect, useRef, useState } from "react";
import type { FavoriteFolder } from "../types/media";
import { useI18n } from "../lib/i18n";
import { Icon } from "./Icon";
import { useModalFocus } from "../hooks/useModalFocus";

interface FavoriteFolderDialogProps {
  folder: FavoriteFolder | "new" | null;
  busy: boolean;
  onClose: () => void;
  onSave: (name: string) => void;
}

export function FavoriteFolderDialog({ folder, busy, onClose, onSave }: FavoriteFolderDialogProps) {
  const { t } = useI18n();
  const [name, setName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const backdropRef = useRef<HTMLDivElement>(null);
  const dialogRef = useRef<HTMLElement>(null);

  useEffect(() => {
    setName(folder && folder !== "new" ? folder.name : "");
  }, [folder]);
  useModalFocus(backdropRef, dialogRef, onClose, '#favorite-folder-name', busy, Boolean(folder));

  if (!folder) return null;
  const creating = folder === "new";
  return (
    <div ref={backdropRef} className="modal-backdrop" onPointerDown={(event) => { if (event.currentTarget === event.target && !busy) onClose(); }}>
      <section ref={dialogRef} tabIndex={-1} aria-labelledby="favorite-folder-title" aria-modal="true" className="rename-dialog favorite-folder-dialog" role="dialog">
        <header className="modal-header">
          <span className="modal-heading-icon"><Icon name="bookmark" /></span>
          <div><h2 id="favorite-folder-title">{creating ? t("favorites.createTitle") : t("favorites.renameTitle")}</h2></div>
          <button aria-label={t("common.close")} className="modal-close" disabled={busy} onClick={onClose} type="button"><Icon name="close" /></button>
        </header>
        <form onSubmit={(event) => { event.preventDefault(); const value = name.trim(); if (value && !busy) onSave(value); }}>
          <label htmlFor="favorite-folder-name">{t("favorites.folderName")}</label>
          <input id="favorite-folder-name" maxLength={80} onChange={(event) => setName(event.target.value)} placeholder={t("favorites.namePlaceholder")} ref={inputRef} value={name} />
          <div className="dialog-actions"><button className="button secondary" disabled={busy} onClick={onClose} type="button">{t("common.cancel")}</button><button className="button primary" disabled={busy || !name.trim()} type="submit">{busy ? t("common.processing") : creating ? t("favorites.create") : t("favorites.saveName")}</button></div>
        </form>
      </section>
    </div>
  );
}

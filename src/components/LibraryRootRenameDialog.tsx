import { useEffect, useRef, useState } from "react";
import type { LibraryRoot } from "../types/media";
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";

interface LibraryRootRenameDialogProps {
  root: LibraryRoot | null;
  busy: boolean;
  onClose: () => void;
  onSave: (displayName: string) => void;
}

export function LibraryRootRenameDialog({ root, busy, onClose, onSave }: LibraryRootRenameDialogProps) {
  const { t } = useI18n();
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!root) return;
    setValue(root.displayName);
    requestAnimationFrame(() => inputRef.current?.select());
  }, [root]);
  if (!root) return null;
  return (
    <div className="modal-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()} role="presentation">
      <form className="rename-dialog" onSubmit={(event) => { event.preventDefault(); if (value.trim()) onSave(value.trim()); }}>
        <button className="modal-close" aria-label={t("common.close")} onClick={onClose} type="button"><Icon name="close" /></button>
        <h2>{t("rename.rootTitle")}</h2><p title={root.path}>{t("rename.realDirectory", { path: root.path })}</p>
        <label><span>{t("rename.displayName")}</span><input ref={inputRef} maxLength={200} onChange={(event) => setValue(event.target.value)} value={value} /></label>
        <div className="dialog-actions"><button className="button ghost" disabled={busy} onClick={onClose} type="button">{t("common.cancel")}</button><button className="button primary" disabled={busy || !value.trim()} type="submit">{busy ? t("common.saving") : t("rename.saveName")}</button></div>
      </form>
    </div>
  );
}

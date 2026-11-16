import { useEffect, useRef, useState } from "react";
import type { LibraryRoot } from "../types/media";
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";
import { useModalFocus } from "../hooks/useModalFocus";

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
  const backdropRef = useRef<HTMLDivElement>(null);
  const dialogRef = useRef<HTMLFormElement>(null);
  useModalFocus(backdropRef, dialogRef, onClose, 'input:not(:disabled)', busy, Boolean(root));
  useEffect(() => {
    if (!root) return;
    setValue(root.displayName);
    const frame = requestAnimationFrame(() => {
      if (document.activeElement === inputRef.current) inputRef.current?.select();
    });
    return () => cancelAnimationFrame(frame);
  }, [root]);
  if (!root) return null;
  return (
    <div ref={backdropRef} className="modal-backdrop" onMouseDown={(event) => event.target === event.currentTarget && !busy && onClose()} role="presentation">
      <form ref={dialogRef} tabIndex={-1} className="rename-dialog" role="dialog" aria-modal="true" aria-labelledby="root-rename-title" onSubmit={(event) => { event.preventDefault(); if (!busy && value.trim()) onSave(value.trim()); }}>
        <button className="modal-close" disabled={busy} aria-label={t("common.close")} onClick={onClose} type="button"><Icon name="close" /></button>
        <h2 id="root-rename-title">{t("rename.rootTitle")}</h2><p title={root.path}>{root.path}</p>
        <label><span>{t("rename.displayName")}</span><input ref={inputRef} autoCorrect="off" autoCapitalize="none" spellCheck={false} disabled={busy} maxLength={200} onChange={(event) => setValue(event.target.value)} value={value} /></label>
        <div className="dialog-actions"><button className="button ghost" disabled={busy} onClick={onClose} type="button">{t("common.cancel")}</button><button className="button primary" disabled={busy || !value.trim()} type="submit">{busy ? t("common.saving") : t("rename.saveName")}</button></div>
      </form>
    </div>
  );
}

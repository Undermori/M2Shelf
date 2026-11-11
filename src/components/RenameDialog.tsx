import { useEffect, useRef, useState } from "react";
import type { MediaNode } from "../types/media";
import {nodeDisplayTitle} from "../lib/format";
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";

interface RenameDialogProps {
  node: MediaNode | null;
  busy: boolean;
  onClose: () => void;
  onSave: (displayName: string) => void;
}

export function RenameDialog({ node, busy, onClose, onSave }: RenameDialogProps) {
  const { t, language } = useI18n();
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => { if (node) { setValue(nodeDisplayTitle(node, language)); requestAnimationFrame(() => inputRef.current?.select()); } }, [node, language]);
  if (!node) return null;
  return (
    <div className="modal-backdrop" onMouseDown={(event) => event.target === event.currentTarget && !busy && onClose()} role="presentation">
      <form className="rename-dialog" role="dialog" aria-modal="true" aria-labelledby="rename-title" onSubmit={(event) => { event.preventDefault(); if (!busy && value.trim()) onSave(value.trim()); }}>
        <button className="modal-close" disabled={busy} aria-label={t("common.close")} onClick={onClose} type="button"><Icon name="close" /></button>
        <h2 id="rename-title">{t("rename.title")}</h2><p>{t("rename.diskUnchanged")}</p>
        <label><span>{t("rename.displayName")}</span><input ref={inputRef} disabled={busy} maxLength={200} onChange={(event) => setValue(event.target.value)} value={value} /></label>
        <div className="dialog-actions"><button className="button ghost" disabled={busy} onClick={onClose} type="button">{t("common.cancel")}</button><button className="button primary" disabled={busy || !value.trim()} type="submit">{busy ? t("common.saving") : t("rename.saveName")}</button></div>
      </form>
    </div>
  );
}

import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";
import {useId, useRef} from 'react';
import {createPortal} from 'react-dom';
import {useModalFocus} from '../hooks/useModalFocus';

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  destructive?: boolean;
  busy?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}

export function ConfirmDialog(props: ConfirmDialogProps) {
  return props.open ? <OpenConfirmDialog {...props}/> : null;
}

function OpenConfirmDialog({title, description, confirmLabel, destructive, busy, onConfirm, onClose}: ConfirmDialogProps) {
  const { t } = useI18n();
  const titleId = useId(), descriptionId = useId();
  const backdrop = useRef<HTMLDivElement>(null), dialog = useRef<HTMLElement>(null);
  useModalFocus(backdrop, dialog, onClose, destructive ? '[data-cancel]:not(:disabled)' : '[data-confirm]:not(:disabled)', busy);
  return createPortal(
    <div ref={backdrop} className="modal-backdrop" role="presentation" onMouseDown={(event) => !busy && event.target === event.currentTarget && onClose()}>
      <section ref={dialog} tabIndex={-1} aria-labelledby={titleId} aria-describedby={descriptionId} aria-modal="true" className="confirm-dialog" role="dialog">
        <button className="modal-close" aria-label={t("common.close")} disabled={busy} onClick={onClose} type="button"><Icon name="close" /></button>
        <span className={`dialog-symbol ${destructive ? "is-danger" : ""}`}><Icon name={destructive ? "warning" : "info"} /></span>
        <h2 id={titleId}>{title}</h2>
        <p id={descriptionId}>{description}</p>
        <div className="dialog-actions">
          <button data-cancel className="button ghost" disabled={busy} onClick={onClose} type="button">{t("common.cancel")}</button>
          <button data-confirm className={`button ${destructive ? "danger" : "primary"}`} disabled={busy} onClick={onConfirm} type="button">
            {busy ? t("common.processing") : confirmLabel ?? t("common.confirm")}
          </button>
        </div>
      </section>
    </div>, document.body,
  );
}

import { useEffect, useRef } from "react";
import type { UpdateRecoveryNotice } from "../types/media";
import { useI18n } from "../lib/i18n";
import { isolateModalSiblings } from "../lib/modalA11y";
import { Icon } from "./Icon";

interface UpdateRecoveryDialogProps {
  notice: UpdateRecoveryNotice | null;
  busy: boolean;
  error: boolean;
  onAcknowledge: (notice: UpdateRecoveryNotice) => void;
}

export function UpdateRecoveryDialog({ notice, busy, error, onAcknowledge }: UpdateRecoveryDialogProps) {
  const { t } = useI18n();
  const backdropRef = useRef<HTMLDivElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const acknowledgeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!notice) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const restoreBackground = isolateModalSiblings(backdropRef.current);
    return () => {
      restoreBackground();
      previous?.focus();
    };
  }, [notice]);

  useEffect(() => {
    if (!notice) return;
    const frame = window.requestAnimationFrame(() => (busy ? dialogRef.current : acknowledgeRef.current)?.focus());
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        return;
      }
      if (event.key === "Tab") {
        event.preventDefault();
        (busy ? dialogRef.current : acknowledgeRef.current)?.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      window.cancelAnimationFrame(frame);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [busy, notice]);

  if (!notice) return null;
  const recoveryRequired = notice === "RECOVERY_REQUIRED";
  const title = t(recoveryRequired ? "update.recoveryRequiredTitle" : "update.rollbackTitle");
  const description = t(recoveryRequired ? "update.recoveryRequiredNotice" : "update.rollbackNotice");

  return (
    <div className="modal-backdrop" ref={backdropRef} role="presentation">
      <section
        aria-describedby="update-recovery-description"
        aria-labelledby="update-recovery-title"
        aria-modal="true"
        className="update-recovery-dialog"
        ref={dialogRef}
        role="alertdialog"
        tabIndex={-1}
      >
        <span className={`dialog-symbol ${recoveryRequired ? "is-danger" : ""}`}><Icon name="warning" /></span>
        <h2 id="update-recovery-title">{title}</h2>
        <p id="update-recovery-description">{description}</p>
        {error && <p className="update-recovery-error" role="alert"><Icon name="warning" />{t("error.updateRecoveryAcknowledgeFailed")}</p>}
        <div className="dialog-actions">
          <button
            className={`button ${recoveryRequired ? "danger" : "primary"}`}
            disabled={busy}
            onClick={() => onAcknowledge(notice)}
            ref={acknowledgeRef}
            type="button"
          >
            {busy ? t("common.processing") : t("update.recoveryAcknowledge")}
          </button>
        </div>
      </section>
    </div>
  );
}

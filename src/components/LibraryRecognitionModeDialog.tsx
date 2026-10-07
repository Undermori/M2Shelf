import { useEffect, useRef, useState } from "react";
import type { LibraryRecognitionMode, LibraryMediaKind } from "../types/media";
import { basename } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { isolateModalSiblings } from "../lib/modalA11y";
import { Icon } from "./Icon";

interface LibraryRecognitionModeDialogProps {
  path: string | null;
  busy: boolean;
  onChoose: (mode: LibraryRecognitionMode,kind:LibraryMediaKind) => void;
  onClose: () => void;
}

export function LibraryRecognitionModeDialog({ path, busy, onChoose, onClose }: LibraryRecognitionModeDialogProps) {
  const { t } = useI18n();
  const [kind,setKind]=useState<LibraryMediaKind>('ANIMATION');
  useEffect(()=>setKind('ANIMATION'),[path]);
  const backdropRef = useRef<HTMLDivElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const busyRef = useRef(busy);
  busyRef.current = busy;

  useEffect(() => {
    if (!path) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const dialog = dialogRef.current;
    const restoreBackground = isolateModalSiblings(backdropRef.current);
    const focusable = () => Array.from(dialog?.querySelectorAll<HTMLElement>("button:not([disabled]), [tabindex]:not([tabindex='-1'])") ?? []);
    const frame = window.requestAnimationFrame(() => focusable()[0]?.focus());
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busyRef.current) {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab") return;
      const items = focusable();
      if (items.length === 0) { event.preventDefault(); dialog?.focus(); return; }
      const first = items[0];
      const last = items[items.length - 1];
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      window.cancelAnimationFrame(frame);
      document.removeEventListener("keydown", onKeyDown);
      restoreBackground();
      previous?.focus();
    };
  }, [onClose, path]);

  if (!path) return null;
  return (
    <div className="modal-backdrop" onPointerDown={(event) => { if (event.currentTarget === event.target && !busy) onClose(); }} ref={backdropRef} role="presentation">
      <section aria-describedby="root-mode-description" aria-labelledby="root-mode-title" aria-modal="true" className="root-mode-dialog" ref={dialogRef} role="dialog" tabIndex={-1}>
        <header className="modal-header">
          <div><h2 id="root-mode-title">{t("comic.rootTitle")}</h2><p id="root-mode-description">{t("comic.rootDescription")}</p></div>
          <button aria-label={t("common.close")} className="modal-close" disabled={busy} onClick={onClose} type="button"><Icon name="close" /></button>
        </header>
        <div className="root-mode-body">
          <p className="root-mode-path" title={path}>{basename(path)}</p>
          <div className="media-kind-choice" role="group" aria-label={t('comic.kind')}>
            <button className="button secondary" aria-pressed={kind==='ANIMATION'} disabled={busy} onClick={()=>setKind('ANIMATION')} type="button">{t('comic.animation')}</button>
            <button className="button secondary" aria-pressed={kind==='LIVE_ACTION'} disabled={busy} onClick={()=>setKind('LIVE_ACTION')} type="button">{t('library.liveAction')}</button>
            <button className="button secondary" aria-pressed={kind==='COMIC'} disabled={busy} onClick={()=>setKind('COMIC')} type="button">{t('comic.name')}</button>
            <button className="button secondary" aria-pressed={kind==='EBOOK'} disabled={busy} onClick={()=>setKind('EBOOK')} type="button">{t('ebook.name')}</button>
          </div>
          <div className="root-mode-options">
            <button disabled={busy} onClick={() => onChoose("FOLDER",kind)} type="button">
              <span><Icon name="folder-open" /></span><strong>{t("rootMode.folderTitle")}</strong><small>{t(kind==='COMIC'||kind==='EBOOK'?'bookMode.folderDescription':'rootMode.folderDescription')}</small>
            </button>
            <button disabled={busy} onClick={() => onChoose("VIDEO_FILE",kind)} type="button">
              <span><Icon name="file" /></span><strong>{t(kind==='COMIC'||kind==='EBOOK'?'bookMode.fileTitle':'rootMode.videoFileTitle')}</strong><small>{t(kind==='COMIC'||kind==='EBOOK'?'bookMode.fileDescription':'rootMode.videoFileDescription')}</small>
            </button>
          </div>
          <p className="root-mode-safety"><Icon name="shield" />{t("rootMode.readOnly")}</p>
        </div>
      </section>
    </div>
  );
}

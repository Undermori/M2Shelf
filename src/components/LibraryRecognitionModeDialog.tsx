import { useEffect, useRef, useState } from "react";
import {isBookKind, type LibraryRecognitionMode, type LibraryMediaKind } from "../types/media";
import { basename } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { isolateModalSiblings } from "../lib/modalA11y";
import { Icon } from "./Icon";

interface LibraryRecognitionModeDialogProps {
  path: string | null;
  busy: boolean;
  onChoose: (mode: LibraryRecognitionMode,kind:LibraryMediaKind,autoBangumi:boolean,strategy?:"LEGACY"|"SMART_MIXED") => void;
  onClose: () => void;
}

export function LibraryRecognitionModeDialog({ path, busy, onChoose, onClose }: LibraryRecognitionModeDialogProps) {
  const { t } = useI18n();
  const [kind,setKind]=useState<LibraryMediaKind>('ANIMATION');
  const [noAuto,setNoAuto]=useState(true);
  const [selection,setSelection]=useState<'SMART_MIXED'|'FOLDER'|'VIDEO_FILE'|null>(null);
  const [selectionRequired,setSelectionRequired]=useState(false);
  const modesRef=useRef<HTMLDivElement>(null);
  const closeRef=useRef(onClose);closeRef.current=onClose;
  const changeKind=(value:LibraryMediaKind)=>{setKind(value);setSelection(null);setSelectionRequired(false);};
  const chooseMode=(mode:LibraryRecognitionMode,strategy:'LEGACY'|'SMART_MIXED'='LEGACY')=>{setSelection(strategy==='SMART_MIXED'?'SMART_MIXED':mode);setSelectionRequired(false);};
  const createLibrary=()=>{
    if(!selection){setSelectionRequired(true);modesRef.current?.querySelector<HTMLButtonElement>('button')?.focus();return;}
    const mode=selection==='VIDEO_FILE'?'VIDEO_FILE':'FOLDER';
    if(isBookKind(kind))onChoose(mode,kind,!noAuto,selection==='SMART_MIXED'?'SMART_MIXED':'LEGACY');
    else onChoose(mode,kind,true);
  };
  useEffect(()=>{setKind('ANIMATION');setNoAuto(true);setSelection(null);setSelectionRequired(false);},[path]);
  const backdropRef = useRef<HTMLDivElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const busyRef = useRef(busy);
  busyRef.current = busy;

  useEffect(() => {
    if (!path) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const dialog = dialogRef.current;
    const restoreBackground = isolateModalSiblings(backdropRef.current);
    const focusable = () => Array.from(dialog?.querySelectorAll<HTMLElement>("button:not([disabled]),input:not([disabled]), [tabindex]:not([tabindex='-1'])") ?? []);
    const frame = window.requestAnimationFrame(() => focusable()[0]?.focus());
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busyRef.current) {
        event.preventDefault();
        closeRef.current();
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
  }, [path]);

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
            <button className="button secondary" aria-pressed={kind==='ANIMATION'} disabled={busy} onClick={()=>changeKind('ANIMATION')} type="button">{t('comic.animation')}</button>
            <button className="button secondary" aria-pressed={kind==='LIVE_ACTION'} disabled={busy} onClick={()=>changeKind('LIVE_ACTION')} type="button">{t('library.liveAction')}</button>
            <button className="button secondary" aria-pressed={kind==='COMIC'} disabled={busy} onClick={()=>changeKind('COMIC')} type="button">{t('comic.name')}</button>
            <button className="button secondary" aria-pressed={kind==='EBOOK'} disabled={busy} onClick={()=>changeKind('EBOOK')} type="button">{t('ebook.name')}</button>
            <button className="button secondary" aria-pressed={kind==='DOUJIN'} disabled={busy} onClick={()=>changeKind('DOUJIN')} type="button">{t('doujin.name')}</button>
            <button className="button secondary" aria-pressed={kind==='ARTBOOK'} disabled={busy} onClick={()=>changeKind('ARTBOOK')} type="button">{t('artbook.name')}</button>
          </div>
          {isBookKind(kind) && <div className="root-matching-policy"><label><input type="checkbox" checked={noAuto} disabled={busy} onChange={event=>setNoAuto(event.target.checked)} />{t('library.noAutoBangumi')}</label><p>{t('library.noAutoHelp')}</p>{kind==='DOUJIN' && <p>{t('library.doujinAdvice')}</p>}</div>}
          <div ref={modesRef} className={`root-mode-options${isBookKind(kind)?" book-mode-options":""}`} role="group" aria-label={t('library.chooseMode')} aria-describedby={selectionRequired?'root-mode-validation':undefined}>
            {isBookKind(kind)&&<div className="smart-mode-row"><button className="smart-recommended" aria-pressed={selection==='SMART_MIXED'} disabled={busy} onClick={()=>chooseMode('FOLDER','SMART_MIXED')} type="button"><span><Icon name="work"/></span><strong>{t('smart.title')}</strong><small>{t('smart.help')}</small></button><p>{t('library.modeGuidance')}</p></div>}
            <button aria-pressed={selection==='FOLDER'} disabled={busy} onClick={() => chooseMode("FOLDER")} type="button">
              <span><Icon name="folder-open" /></span><strong>{t("rootMode.folderTitle")}</strong><small>{t(kind==='COMIC'||kind==='EBOOK'||(kind==='DOUJIN' || kind === 'ARTBOOK')?'bookMode.folderDescription':'rootMode.folderDescription')}</small>
            </button>
            <button aria-pressed={selection==='VIDEO_FILE'} disabled={busy} onClick={() => chooseMode("VIDEO_FILE")} type="button">
              <span><Icon name="file" /></span><strong>{t(kind==='COMIC'||kind==='EBOOK'||(kind==='DOUJIN' || kind === 'ARTBOOK')?'bookMode.fileTitle':'rootMode.videoFileTitle')}</strong><small>{t(kind==='COMIC'||kind==='EBOOK'||(kind==='DOUJIN' || kind === 'ARTBOOK')?'bookMode.fileDescription':'rootMode.videoFileDescription')}</small>
            </button>
          </div>
          <div className="root-mode-confirm">{selectionRequired&&<span className="root-mode-validation" role="alert" id="root-mode-validation">{t('library.chooseMode')}</span>}<button className="button primary" type="button" disabled={busy} onClick={createLibrary}>{t('library.create')}</button></div>
          <p className="root-mode-safety"><Icon name="shield" />{t("rootMode.readOnly")}</p>
        </div>
      </section>
    </div>
  );
}

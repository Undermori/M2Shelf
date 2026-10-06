import { useEffect, useRef } from "react";
import type { MediaNode } from "../types/media";
import { isolateModalSiblings } from "../lib/modalA11y";
import { useI18n } from "../lib/i18n";

export function SourceChoiceDialog({ sources, onChoose, onClose }: { sources: MediaNode[]; onChoose: (source: MediaNode) => void; onClose: () => void }) {
  const { t } = useI18n();
  const backdrop = useRef<HTMLDivElement>(null);
  const dialog = useRef<HTMLElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const restore = isolateModalSiblings(backdrop.current);
    dialog.current?.querySelector<HTMLButtonElement>("button")?.focus();
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); close.current(); }
      if (event.key === "Tab") {
        const buttons = [...(dialog.current?.querySelectorAll<HTMLButtonElement>("button") ?? [])];
        const first = buttons[0], last = buttons.at(-1);
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
      }
    };
    window.addEventListener("keydown", key);
    return () => { window.removeEventListener("keydown", key); restore(); previous?.focus(); };
  }, []);
  return <div className="modal-backdrop" ref={backdrop} role="presentation" onMouseDown={event => event.target === event.currentTarget && onClose()}>
    <section className="confirm-dialog" ref={dialog} role="dialog" aria-modal="true" aria-labelledby="source-choice-title">
      <h2 id="source-choice-title">{t("works.chooseSource")}</h2><p>{t("works.sourceActionHelp")}</p>
      <div className="source-choice-list">{sources.map(source => <button className="button secondary source-choice-row" type="button" key={source.id} onClick={() => onChoose(source)}><span>{source.folderName}<small>{source.absolutePath}</small></span></button>)}</div>
      <button className="button ghost" type="button" onClick={onClose}>{t("common.close")}</button>
    </section>
  </div>;
}

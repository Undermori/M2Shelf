import { useRef } from "react";
import type { MediaNode } from "../types/media";
import { useModalFocus } from "../hooks/useModalFocus";
import { useI18n } from "../lib/i18n";

export function SourceChoiceDialog({ sources, onChoose, onClose }: { sources: MediaNode[]; onChoose: (source: MediaNode) => void; onClose: () => void }) {
  const { t } = useI18n();
  const backdrop = useRef<HTMLDivElement>(null);
  const dialog = useRef<HTMLElement>(null);
  useModalFocus(backdrop, dialog, onClose, 'button');
  return <div className="modal-backdrop" ref={backdrop} role="presentation" onMouseDown={event => event.target === event.currentTarget && onClose()}>
    <section tabIndex={-1} className="confirm-dialog" ref={dialog} role="dialog" aria-modal="true" aria-labelledby="source-choice-title">
      <h2 id="source-choice-title">{t("works.chooseSource")}</h2>
      <div className="source-choice-list">{sources.map(source => <button className="button secondary source-choice-row" type="button" key={source.id} onClick={() => onChoose(source)}><span>{source.folderName}<small>{source.absolutePath}</small></span></button>)}</div>
      <button className="button ghost" type="button" onClick={onClose}>{t("common.close")}</button>
    </section>
  </div>;
}

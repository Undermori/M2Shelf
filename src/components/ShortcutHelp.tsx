import { useEffect, useId, useRef } from 'react';
import { useI18n } from '../lib/i18n';
import { isShortcutHelpKey, shortcutCopy } from '../lib/shortcutHelp';
import '../styles/shortcutHelp.css';

export function ShortcutHelp({ enabled }: { enabled: boolean }) {
  const { language } = useI18n();
  const copy = shortcutCopy[language];
  const ref = useRef<HTMLDialogElement>(null);
  const title = useId();
  useEffect(() => {
    const open = () => {
      if (enabled && !document.querySelector('[aria-modal="true"],dialog[open]')) ref.current?.showModal();
    };
    const key = (event: KeyboardEvent) => {
      if (!enabled || !isShortcutHelpKey(event) || document.querySelector('[aria-modal="true"],dialog[open]')) return;
      event.preventDefault(); open();
    };
    window.addEventListener('keydown', key);
    window.addEventListener('m2shelf-shortcuts', open);
    return () => { window.removeEventListener('keydown', key); window.removeEventListener('m2shelf-shortcuts', open); };
  }, [enabled]);
  const groups = [
    { title: copy.general, rows: [['Ctrl + K', copy.search], ['Ctrl + F', copy.find], ['Alt + ←', copy.back], ['?', copy.help], ['Esc', copy.dismiss]] },
    { title: copy.studio, rows: [['Ctrl + 1 / 2 / 3', copy.tabs], ['Enter',copy.openFile], ['Ctrl + S', copy.save], ['← / →', copy.images], ['↑ ↓ ← →', copy.grid]] },
    { title: copy.reader, rows: [['PgUp / PgDn', copy.pages], ['F', copy.fullscreen], ['B', copy.bookmark]] },
  ];
  return <dialog ref={ref} className="shortcut-help" aria-labelledby={title}>
    <header><h2 id={title}>{copy.title}</h2><form method="dialog"><button autoFocus type="submit" className="button ghost">{copy.close}</button></form></header>
    <p className="shortcut-help-hint">{copy.hint}</p>
    <div className="shortcut-help-groups">{groups.map(group => <section key={group.title}><h3>{group.title}</h3><dl>{group.rows.map(([key, label]) => <div key={key}><dt>{label}</dt><dd><kbd>{key}</kbd></dd></div>)}</dl></section>)}</div>
  </dialog>;
}

import {useEffect, useId, useRef, useState} from 'react';
import {nodeDisplayTitle} from '../lib/format';
import {useI18n} from '../lib/i18n';
import {searchKnowledge, type KnowledgeResults} from '../lib/knowledgeSearch';
import {openStudio} from '../lib/openStudio';
import {api} from '../lib/api';
import {workspaceCopy} from '../lib/workspaceCopy';
import type {WorkspaceFolder} from '../types/workspace';
import type {SearchHit} from '../types/media';
import {FeatureModal} from './FeatureModal';
import '../styles/knowledge.css';
export interface QuickAction {id: string; label: string; run: () => void;}
const empty: KnowledgeResults = {works: [], notes: [], screenshots: [], failed: false};
export function matchExcerpt(text: string, query: string): string {
 const at = text.toLocaleLowerCase().indexOf(query.trim().toLocaleLowerCase());
 const start = Math.max(0, at - 36); const end = Math.min(text.length, Math.max(start + 160, at + query.length));
 return `${start ? '…' : ''}${text.slice(start, end).replace(/\s+/g, ' ')}${end < text.length ? '…' : ''}`;
}
export function HighlightMatch({text, query}: {text: string; query: string}) {
 const needle = query.trim().toLocaleLowerCase(); const at = needle ? text.toLocaleLowerCase().indexOf(needle) : -1;
 return at < 0 ? <>{text}</> : <>{text.slice(0, at)}<mark>{text.slice(at, at + needle.length)}</mark>{text.slice(at + needle.length)}</>;
}
export function CommandPalette({enabled, actions, onHit}: {enabled: boolean; actions: QuickAction[]; onHit: (hit: SearchHit) => void}) {
 const {t,language} = useI18n(); const id = useId();
 const [projects,setProjects]=useState<WorkspaceFolder[]>([]);
 const [open, setOpen] = useState(false), [query, setQuery] = useState(''), [hits, setHits] = useState(empty), [index, setIndex] = useState(0), [busy, setBusy] = useState(false);
 const request = useRef(0);
 useEffect(() => {
  const key = (event: KeyboardEvent) => {
   if (!enabled || event.defaultPrevented || event.isComposing || !(event.ctrlKey && !event.altKey && !event.metaKey && event.key.toLowerCase() === 'k') || document.querySelector('[aria-modal="true"], dialog[open]') || event.target instanceof HTMLElement && event.target.closest('input,textarea,[contenteditable=true]')) return;
   event.preventDefault(); setQuery(''); setHits(empty); setIndex(0); setOpen(true);
  };
  window.addEventListener('keydown', key); return () => window.removeEventListener('keydown', key);
 }, [enabled]);
 useEffect(() => {
  const generation = ++request.current; setHits(empty);setProjects([]); setIndex(0); setBusy(false);
  if (!open || !query.trim()) return;
  const timer = setTimeout(() => {
   setBusy(true);
   void searchKnowledge(query).then(rows => {if (request.current === generation) setHits(rows);})
    .finally(() => {if (request.current === generation) setBusy(false);});
   // Registered folder metadata only: Ctrl+K never recursively scans project disks.
   void Promise.resolve().then(()=>api.workspaceFolders()).then(rows=>{if(request.current===generation){const needle=query.trim().toLocaleLowerCase();setProjects(rows.filter(folder=>[folder.alias,folder.note].some(value=>value.toLocaleLowerCase().includes(needle))).slice(0,12));}}).catch(()=>{/* Other sources retain their successful results. */});
  }, 220);
  return () => {clearTimeout(timer); request.current++;};
 }, [query, open]);
 const entries = [
  ...actions.filter(action => !query || action.label.toLocaleLowerCase().includes(query.toLocaleLowerCase())).slice(0, 10).map(action => ({key: action.id, label: action.label, excerpt: '', source: t('knowledge.action'), run: action.run})),
  ...hits.works.map(hit => ({key: `work:${hit.comicBook ? 'BOOK' : 'NODE'}:${hit.comicBook?.id ?? hit.node.id}`, label: hit.comicBook?.displayName ?? nodeDisplayTitle(hit.node), excerpt: hit.noteExcerpt ?? '', source: t('knowledge.work'), run: () => onHit(hit)})),
  ...hits.notes.map(note => ({key: `note:${note.target.kind}:${note.target.id}`, label: note.title, excerpt: note.body, source: t('knowledge.note'), run: () => openStudio({view: 'notes', target: note.target, nodeId: note.nodeId})})),
  ...hits.screenshots.map(shot => ({key: `shot:${shot.id}`, label: shot.fileName, excerpt: [shot.note, ...shot.tags].filter(Boolean).join(' · '), source: t('knowledge.screenshot'), run: () => openStudio({view: 'gallery', screenshotId: shot.id})})),
  ...projects.map(folder=>({key:`project:${folder.id}`,label:folder.alias,excerpt:folder.note,source:workspaceCopy[language].title,run:()=>openStudio({view:'files',folderId:folder.id})})),
 ];
 useEffect(() => {if (open) document.getElementById(`${id}-${index}`)?.scrollIntoView?.({block: 'nearest', inline: 'nearest'});}, [id, index, open]);
 if (!open || !enabled) return null;
 const execute = (value: number) => {const action = entries[value]; if (action) {setOpen(false); request.current++; action.run();}};
 return <FeatureModal title={t('palette.title')} onClose={() => setOpen(false)} className="command-palette">
  <input data-modal-initial-focus aria-label={t('knowledge.search')} placeholder={t('knowledge.search')} value={query} maxLength={200} onChange={event => setQuery(event.target.value)} role="combobox" aria-expanded="true" aria-autocomplete="list" aria-controls={`${id}-results`} aria-activedescendant={entries[index] ? `${id}-${index}` : undefined} onKeyDown={event => {
   if (event.nativeEvent.isComposing || !entries.length) return;
   if (event.key === 'Enter') {event.preventDefault(); execute(index);}
   else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    const next = index + (event.key === 'ArrowDown' ? 1 : -1);
    if (next < 0 || next >= entries.length) return;
    event.preventDefault(); setIndex(next);
   }
  }}/>
  <div id={`${id}-results`} role="listbox" aria-label={t('knowledge.search')} aria-busy={busy}>{entries.map((entry, i) => <button id={`${id}-${i}`} role="option" aria-selected={index === i} className={index === i ? 'is-selected' : ''} key={entry.key} type="button" tabIndex={-1} onPointerMove={() => setIndex(i)} onClick={() => execute(i)}><span className="knowledge-source">{entry.source}</span><span className="knowledge-entry"><strong><HighlightMatch text={entry.label} query={query}/></strong>{entry.excerpt && <small><HighlightMatch text={matchExcerpt(entry.excerpt, query)} query={query}/></small>}</span></button>)}</div>
  {(!entries.length || busy) && <p role="status">{t(busy ? 'comic.loading' : 'palette.empty')}</p>}
  {hits.failed && <p role="status">{t('knowledge.searchFailed')}</p>}
 </FeatureModal>;
}

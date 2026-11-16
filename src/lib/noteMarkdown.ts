import {save} from '@tauri-apps/plugin-dialog';
import {api, desktopAvailable} from './api';
import {round8KnowledgeCopy} from './round8KnowledgeCopy';
import type {AppLanguage} from '../types/media';
import type {WorkNoteEntry} from '../types/workbench';
const safeHeading = (text: string) => text.replace(/[\r\n\u0000-\u001f]/g, ' ').replace(/([\\`*_{}[\]<>#+.!|])/g, '\\$1');
export function noteMarkdown(rows: WorkNoteEntry[], language: AppLanguage): string {
 const copy = round8KnowledgeCopy[language];
 const content = rows.map(row => `# ${safeHeading(row.title)}\n\n${copy['knowledge.library']}: ${safeHeading(row.libraryName)}\n\n${copy['knowledge.updated']}: ${row.updatedAt ?? '—'}\n\n${row.body}\n`).join('\n---\n\n');
 if (new TextEncoder().encode(content).length > 16 * 1024 * 1024) throw new Error(copy['knowledge.exportLimit']);
 return content;
}
export function markdownFilename(title: string): string {
 let stem = title.replace(/[<>:"/\\|?*\u0000-\u001f]/g, '-').replace(/[. ]+$/g, '').slice(0, 100).replace(/[. ]+$/g, '');
 if (!stem || /^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)/i.test(stem)) stem = 'M2Shelf-notes';
 return `${stem}.md`;
}
export async function saveMarkdown(markdown: string, title: string, language: AppLanguage): Promise<boolean> {
 const filename = markdownFilename(title);
 if (desktopAvailable) {
  const destination = await save({title: round8KnowledgeCopy[language]['knowledge.export'], defaultPath: filename, filters: [{name: 'Markdown', extensions: ['md']}]});
  if (!destination) return false;
  await api.saveNoteMarkdown(destination, markdown);
 } else {
  const url = URL.createObjectURL(new Blob([markdown], {type: 'text/markdown;charset=utf-8'}));
  const anchor = document.createElement('a'); anchor.href = url; anchor.download = filename;
  document.body.append(anchor); anchor.click(); anchor.remove(); setTimeout(() => URL.revokeObjectURL(url), 1000);
 }
 return true;
}

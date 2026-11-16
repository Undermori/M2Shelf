import {api} from './api';
import type {SearchHit} from '../types/media';
import type {WorkNoteEntry, ScreenshotImage} from '../types/workbench';
export interface KnowledgeResults {works: SearchHit[]; notes: WorkNoteEntry[]; screenshots: ScreenshotImage[]; failed: boolean;}
/** Each source has its own budget, so a large works result cannot crowd out notes. */
export async function searchKnowledge(query: string, includeWorks = true, rootId?: number): Promise<KnowledgeResults> {
  const keyword = query.trim().slice(0, 200);
  if (!keyword) return {works: [], notes: [], screenshots: [], failed: false};
  const results = await Promise.allSettled([
    includeWorks ? Promise.resolve().then(()=>api.search(keyword, rootId)) : Promise.resolve([] as SearchHit[]),
    Promise.resolve().then(()=>api.listWorkNotes({notesOnly: true, rootId: rootId ?? null, mediaKind: null, query: keyword, offset: 0, limit: 12})),
    rootId == null ? Promise.resolve().then(()=>api.screenshots({folderId: null, query: keyword, target: null, starredOnly: false, offset: 0, limit: 12})) : Promise.resolve({items: [] as ScreenshotImage[], total: 0}),
  ]);
  const seen = new Set<string>();
  return {
    works: results[0].status === 'fulfilled' ? results[0].value.filter(hit => {
      const key = hit.comicBook ? `BOOK:${hit.comicBook.id}` : `NODE:${hit.node.id}`;
      if (seen.has(key)) return false; seen.add(key); return true;
    }).slice(0, 12) : [],
    notes: results[1].status === 'fulfilled' ? results[1].value.items : [],
    screenshots: results[2].status === 'fulfilled' ? results[2].value.items : [],
    failed: results.some(result => result.status === 'rejected'),
  };
}

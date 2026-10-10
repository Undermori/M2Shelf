import type {ComicBook} from '../types/comic';
import type {ResourceFile} from '../types/media';

const bookExtensions = new Set(['cbz', 'pdf', 'epub', 'txt', 'mobi', 'azw3']);
const sourceKey = (path: string) => path.replaceAll('\\', '/').replace(/\/$/, '').toLowerCase();

/** Presentation only: old attachment indexes stay readable without a rescan or fake book IDs. */
export function readingFiles(books: ComicBook[], resources: ResourceFile[]) {
  const indexed = new Set(books.flatMap(book => book.sourcePath ? [sourceKey(book.sourcePath)] : []));
  const readable: ResourceFile[] = [];
  const other: ResourceFile[] = [];
  for (const resource of resources) {
    if (indexed.has(sourceKey(resource.absolutePath))) continue;
    (bookExtensions.has(resource.extension.replace(/^\./, '').toLowerCase()) ? readable : other).push(resource);
  }
  return {books, readable, other, count: books.length + readable.length};
}

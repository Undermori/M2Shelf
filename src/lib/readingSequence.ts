import type {ComicBook} from '../types/comic';
import {naturalCompare} from './format';
export const bookReadingOrder=(books:readonly ComicBook[])=>[...books].sort((a,b)=>naturalCompare(a.displayName,b.displayName)||`book-${a.id}`.localeCompare(`book-${b.id}`));

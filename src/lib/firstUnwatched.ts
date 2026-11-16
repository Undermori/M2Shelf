import type { EpisodeWatchRow } from '../types/episodes';
import type { MediaFile, AppLanguage } from '../types/media';
import { naturalCompare } from './format';

/** Use only confirmed watch rows and the same natural order as the file table. */
export function firstUnwatched(rows: {file: MediaFile; directory: string}[], states: EpisodeWatchRow[], language?: AppLanguage) {
  const known = new Map(states.map(row => [row.fileId, row]));
  return [...rows].sort((a,b) => naturalCompare(a.directory,b.directory,language) || naturalCompare(a.file.fileName,b.file.fileName,language) || a.file.id-b.file.id)
    .find(({file}) => { const state = known.get(file.id); return state && (!state.watched || state.partiallyWatched); })?.file.id;
}

export const locateUnwatchedCopy = {
  'zh-CN': '定位首个未看', 'en-US': 'Find first unwatched', 'ja-JP': '最初の未視聴へ', 'ko-KR': '첫 미시청 항목으로',
} as const;

import type {MediaNode, NodeDetail, ResourceFile} from '../types/media';

const TECHNICAL_PAREN_TOKENS = new Set([
  'bd', 'bdrip', 'bdmv', 'dvd', 'dvdrip', 'web', 'webrip', 'webdl',
  '1080p', '720p', '480p', '2160p', '4k', 'uhd', 'hdr', 'hevc', 'x265',
  'x264', 'h265', 'h264', 'avc', 'av1', '10bit', '8bit', 'ma10p', 'hi10p',
  'flac', 'alac', 'aac', 'ac3', 'dts', 'mp3', 'wav', 'ape', 'dsd',
  'hires', 'lossless', 'raw', 'raws', 'rip', 'sub', 'subbed', 'dub',
  'chs', 'cht', 'gb', 'big5', 'jp', 'jpn', '无损', '無損', 'cd', 'cds', 'ost',
]);

function cdsNameVariants(name: string): string[] {
  const trimmed = name.trim();
  if (!trimmed) return [];
  const variants = [trimmed];
  const outer = trimmed.match(/^([\[【(（])(.+?)([\]】)）])$/);
  if (outer) {
    const inner = outer[2].trim();
    if (inner && !variants.includes(inner)) variants.push(inner);
  }
  let removed = false;
  const stripped = trimmed
    .replace(/\[([^\[\]]*)\]|【([^【】]*)】|\(([^()]*)\)|（([^（）]*)）/g, (full, sq, corner, paren, fullParen) => {
      if (sq !== undefined || corner !== undefined) {
        removed = true;
        return ' ';
      }
      const content = (paren ?? fullParen ?? '').trim();
      const tokens = content.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter(Boolean);
      if (tokens.length > 0 && tokens.every((tok: string) => TECHNICAL_PAREN_TOKENS.has(tok) || tok === 'hi-res')) {
        removed = true;
        return ' ';
      }
      return full;
    })
    .replace(/\s+/g, ' ')
    .trim();
  if (removed && stripped && !variants.includes(stripped)) variants.push(stripped);
  return variants;
}

const isCdsCompactExact = (compact: string) =>
  /^(?:cds?|osts?|soundtracks?|charasongs?|dramacds?|特典cd|音乐特典|音樂特典|原声集|原聲集)$/.test(compact);

/** Matches the scanner's music-extra directories, never numbered main discs. */
export function isCdsDirectoryName(name: string): boolean {
  return cdsNameVariants(name).some(variant => {
    const compact = variant.toLowerCase().replace(/[^\p{L}\p{N}]/gu, '');
    if (isCdsCompactExact(compact)) return true;
    const parts = variant.split(/[&+/,、·|]+/).map(part => part.toLowerCase().replace(/[^\p{L}\p{N}]/gu, '')).filter(Boolean);
    return parts.length >= 2 && parts.every(isCdsCompactExact);
  });
}

const pathKey = (path: string) => path.replaceAll('\\', '/').replace(/\/+$/, '').toLowerCase();

/** Only inspect the indexed path beneath its displayed source, not library/ancestor names. */
function inCdsDirectory(path: string, sources: MediaNode[], directory: boolean): boolean {
  const candidate = pathKey(path);
  return sources.some(source => {
    const prefix = `${pathKey(source.absolutePath)}/`;
    if (!candidate.startsWith(prefix)) return false;
    const parts = candidate.slice(prefix.length).split('/');
    if (!directory) parts.pop();
    return parts.some(isCdsDirectoryName);
  });
}

export function splitCdsResources(detail: NodeDetail): {
  cdsFiles: ResourceFile[]; otherFiles: ResourceFile[]; cdsFolders: MediaNode[]; otherFolders: MediaNode[];
  spFiles: ResourceFile[]; spFolders: MediaNode[];
} {
  const sources = detail.workSources?.length ? detail.workSources : [detail.node];
  const expandedSet = new Set(detail.expandedFolderIds ?? []);
  const cdsFiles: ResourceFile[] = [], otherFiles: ResourceFile[] = [];
  const cdsFolders: MediaNode[] = [], otherFolders: MediaNode[] = [];
  const seenFileIds = new Set<number>();
  const seenFolderIds = new Set<number>();
  // Native ownership decides which supplementary shells may be expanded. Never
  // infer an SP boundary from a title or move an independently bound child here.
  const spFiles = (detail.spFiles ?? []).filter(file => {
    if (seenFileIds.has(file.id)) return false;
    seenFileIds.add(file.id);
    return true;
  });
  const spFolders = (detail.spFolders ?? []).filter(folder => {
    if (seenFolderIds.has(folder.id)) return false;
    seenFolderIds.add(folder.id);
    return true;
  });

  for (const file of detail.cdsFiles ?? []) {
    if (!seenFileIds.has(file.id)) {
      seenFileIds.add(file.id);
      cdsFiles.push(file);
    }
  }
  // The backend already owns expansion/independent-edition boundaries. Partition the
  // returned rows only; never enumerate a directory or manufacture another resource.
  for (const file of detail.resourceFiles) {
    if (seenFileIds.has(file.id)) continue;
    if (file.resourceType === 'AUDIO' || inCdsDirectory(file.absolutePath, sources, false)) {
      seenFileIds.add(file.id);
      cdsFiles.push(file);
    } else {
      otherFiles.push(file);
    }
  }
  for (const folder of detail.cdsFolders ?? []) {
    if (!seenFolderIds.has(folder.id)) {
      seenFolderIds.add(folder.id);
      cdsFolders.push(folder);
    }
  }
  for (const folder of detail.children) {
    if (seenFolderIds.has(folder.id)) continue;
    if (folder.manualTypeOverride || folder.binding || folder.tmdbBinding?.active || folder.nodeType === 'WORK') {
      otherFolders.push(folder);
      continue;
    }
    const insideCdsParent = inCdsDirectory(folder.absolutePath, sources, false);
    const isOuterCdsWrapper = isCdsDirectoryName(folder.folderName) && !insideCdsParent;
    if (isOuterCdsWrapper || expandedSet.has(folder.id)) {
      // Unwrapped outer CDs shells and folders already expanded to tabs stay out of the lower list.
      continue;
    }
    if (insideCdsParent) {
      seenFolderIds.add(folder.id);
      cdsFolders.push(folder);
    } else {
      otherFolders.push(folder);
    }
  }
  return {cdsFiles, otherFiles, cdsFolders, otherFolders, spFiles, spFolders};
}

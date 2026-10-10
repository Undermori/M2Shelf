import type { AppLanguage, CollectionSort, MediaNode, ResourceType } from "../types/media";
import { getActiveLanguage, translate, translateActive } from "./i18n";

const naturalCollators = new Map<AppLanguage, Intl.Collator>();

export function naturalCompare(a: string, b: string, language: AppLanguage = getActiveLanguage()): number {
  let collator = naturalCollators.get(language);
  if (!collator) {
    collator = new Intl.Collator([language, "zh-Hans-CN", "ja-JP", "en"], {
      numeric: true,
      sensitivity: "base",
    });
    naturalCollators.set(language, collator);
  }
  return collator.compare(a, b);
}

export function compareMediaNodes(a: MediaNode, b: MediaNode, sort: CollectionSort, language: AppLanguage): number {
  const byTitle = naturalCompare(nodeDisplayTitle(a, language), nodeDisplayTitle(b, language), language);
  if (sort === "title-desc") return -byTitle;
  if (sort === "added-desc") return b.createdAt.localeCompare(a.createdAt) || byTitle;
  if (sort === "added-asc") return a.createdAt.localeCompare(b.createdAt) || byTitle;
  if (sort.startsWith("watched-")) return compareFileModifiedTimes(a.lastWatchedAt, b.lastWatchedAt, sort) || byTitle || a.id - b.id;
  if (sort === "modified-desc" || sort === "modified-asc") return compareFileModifiedTimes(a.latestFileModifiedAt, b.latestFileModifiedAt, sort) || byTitle || a.id - b.id;
  return byTitle;
}

export function compareFileModifiedTimes(a: string | null | undefined, b: string | null | undefined, sort: CollectionSort): number {
  const left = a ? Date.parse(a) : NaN;
  const right = b ? Date.parse(b) : NaN;
  if (!Number.isFinite(left)) return Number.isFinite(right) ? 1 : 0;
  if (!Number.isFinite(right)) return -1;
  return sort.endsWith("-asc") ? left - right : right - left;
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** index;
  return `${value.toFixed(index > 1 && value < 10 ? 1 : 0)} ${units[index]}`;
}

export function formatDuration(milliseconds: number | null): string | null {
  if (!milliseconds || milliseconds < 0) return null;
  const seconds = Math.floor(milliseconds / 1000);
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = seconds % 60;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`
    : `${minutes}:${String(rest).padStart(2, "0")}`;
}

export function formatDate(value: string | null | undefined): string {
  if (!value) return translateActive("format.neverScanned");
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(getActiveLanguage(), {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export function basename(path: string): string {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;
}

export function compactPath(path: string, max = 62): string {
  if (path.length <= max) return path;
  const head = Math.floor((max - 1) * 0.42);
  const tail = max - head - 1;
  return `${path.slice(0, head)}…${path.slice(-tail)}`;
}

export function nodeTypeLabel(type: string): string {
  switch (type) {
    case "AUTO_WORK": return translateActive("node.autoWork");
    case "WORK": return translateActive("node.work");
    case "CONTAINER": return translateActive("node.container");
    case "MIXED": return translateActive("node.mixed");
    case "IGNORED": return translateActive("node.ignored");
    default: return type;
  }
}

export function cleanBangumiKeyword(value: string): string {
  return value
    .replace(/\.[a-z0-9]{1,8}$/i, "")
    .replace(/^\s*(?:\[[^\]]{1,80}\]\s*)+/, "")
    .replace(/\s*(?:\[[^\]]*(?:720p|1080p|2160p|BDRip|BluRay|WEB-?DL|WEBRip|HEVC|AVC|H\.?26[45]|x26[45]|10bit|Ma10p|FLAC|AAC|TrueHD|DTS|[A-F0-9]{8})[^\]]*\]\s*)+$/gi, "")
    .trim();
}

/** Display metadata never mutates displayName, folderName, or the on-disk directory. */
export function bindingDisplayTitle(node: MediaNode, language: AppLanguage = getActiveLanguage()): string | null {
  const localizedTitle = language === "en-US"
    ? node.binding?.providerTitleEn
    : language === "ja-JP"
      ? node.binding?.providerTitleJa
      : language === "ko-KR"
        ? node.binding?.providerTitleKo
        : node.binding?.providerTitleCn;
  return localizedTitle?.trim()
    || node.binding?.providerTitleCn?.trim()
    || node.binding?.providerTitle?.trim()
    || null;
}

export function nodeDisplayTitle(node: MediaNode, language: AppLanguage = getActiveLanguage()): string {
  const bindingTitle = (node.tmdbBinding?.active ? node.tmdbBinding.movie.title : null) || bindingDisplayTitle(node, language);
  let title = bindingTitle || node.displayName?.trim() || node.folderName?.trim() || translate(language, "format.unnamedDirectory");
  const suffix = translate(language, "node.seriesSuffix");
  if (bindingTitle && node.nodeType === "CONTAINER" && !title.toLocaleLowerCase(language).endsWith(suffix.trim().toLocaleLowerCase(language))) title += suffix;
  return title;
}

export function nodeHasVideo(node: MediaNode): boolean {
  if (node.totalVideoCount != null) return node.totalVideoCount > 0;
  return (node.directVideoCount ?? 0) > 0 || (node.childMediaBranchCount ?? 0) > 0;
}

export function canBindBangumi(node: MediaNode): boolean {
  const work = node.nodeType === "WORK" || node.nodeType === "AUTO_WORK";
  return work || (node.nodeType === "CONTAINER" && (nodeHasVideo(node) || (node.totalComicBookCount??0)>0));
}

export function mediaBadge(node:MediaNode):string {
  const media=node.mediaKind==='ARTBOOK'?'artbook.name':node.mediaKind==='DOUJIN'?'doujin.name':node.mediaKind==='EBOOK'?'ebook.name':node.mediaKind==='COMIC'?'comic.name':node.mediaKind==='ANIMATION'||node.binding?.providerSubjectType===2?'comic.animation':node.mediaKind==='LIVE_ACTION'||node.binding?.providerSubjectType===6?'comic.liveAction':'comic.unboundVideo';
  const structure=node.nodeType==='CONTAINER'?'card.systemSeries':node.nodeType==='MIXED'?'card.systemOtherResources':'card.systemWork';
  return translateActive('comic.badge',{media:translateActive(media),structure:translateActive(structure)});
}

export function resourceTypeLabel(type: ResourceType, extension: string): string {
  const extensionLabel = extension.replace(/^\./, "").toLocaleUpperCase();
  if (type === "OTHER") {
    const other = translateActive("resource.other");
    return extensionLabel ? `${other} · ${extensionLabel}` : other;
  }
  if (extensionLabel) return extensionLabel;
  const labels: Record<ResourceType, string> = {
    DOCUMENT: translateActive("resource.document"),
    IMAGE: translateActive("resource.image"),
    AUDIO: translateActive("resource.audio"),
    SUBTITLE: translateActive("resource.subtitle"),
    ARCHIVE: translateActive("resource.archive"),
    FONT: translateActive("resource.font"),
    PLAYLIST: translateActive("resource.playlist"),
    OTHER: translateActive("resource.other"),
  };
  return labels[type];
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error && (error.name === "M2ShelfError" || error.name === "DesktopOnlyError")) {
    return error.message;
  }
  return translateActive("error.unknown");
}

export function formatRelativeTime(value: string, now = Date.now(), language: AppLanguage = getActiveLanguage()): string {
  const seconds = (Date.parse(value) - now) / 1000;
  if (!Number.isFinite(seconds)) return value;
  const magnitude = Math.abs(seconds);
  const units: [Intl.RelativeTimeFormatUnit, number][] = [["year", 31536000], ["month", 2592000], ["day", 86400], ["hour", 3600], ["minute", 60]];
  const [unit, size] = units.find(([, size]) => magnitude >= size) ?? ["second", 1];
  return new Intl.RelativeTimeFormat(language, { numeric: "auto" }).format(magnitude < 60 ? 0 : Math.trunc(seconds / size), unit);
}

export function nodeMatchesQuery(node: MediaNode, query: string, language: AppLanguage): boolean {
  const normalized = query.trim().normalize("NFKC").toLocaleLowerCase(language);
  if (!normalized) return true;
  const b = node.binding;
  return [nodeDisplayTitle(node, language), node.displayName, node.folderName,
    b?.providerTitle, b?.providerTitleCn, b?.providerTitleEn, b?.providerTitleJa, b?.providerTitleKo,
    ...(b?.providerAliases ?? []), ...node.userTags.map(tag => tag.name)]
    .some(value => value?.normalize("NFKC").toLocaleLowerCase(language).includes(normalized));
}

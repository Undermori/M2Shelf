import {Select} from './Select';
import { useI18n } from "../lib/i18n";

export function BookmarkSelect({ bookmarks, count, chapters = false, onJump }: {
  bookmarks: number[]; count: number; chapters?: boolean; onJump: (index: number) => void;
}) {
  const { t } = useI18n();
  return <Select className="reader-bookmark-select" aria-label={t("reader.bookmarkJump")} value="" disabled={!bookmarks.length}
    onChange={event => { if (event.target.value !== "") onJump(Number(event.target.value)); }}>
    <option value="">{t(bookmarks.length ? "reader.bookmarkJump" : "comic.noBookmarks")}</option>
    {[...bookmarks].sort((a, b) => a - b).map(index => <option key={index} value={index}>{t(chapters ? "ebook.chapterProgress" : "comic.progress", { page: index + 1, count })}</option>)}
  </Select>;
}

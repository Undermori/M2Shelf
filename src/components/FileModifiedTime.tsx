import { formatDate, formatRelativeTime } from "../lib/format";
import { useRelativeClock } from "../hooks/useRelativeClock";
import { useI18n } from "../lib/i18n";

export function FileModifiedTime({ value }: { value?: string | null }) {
  const { t, language } = useI18n();
  const now = useRelativeClock();
  if (!value || !Number.isFinite(Date.parse(value))) return <span className="file-modified-time">{t("card.fileModifiedUnknown")}</span>;
  const label = t("card.fileModifiedAt", { time: formatDate(value) });
  return <time className="file-modified-time" dateTime={value} title={label}>{t("card.fileModifiedAt", { time: formatRelativeTime(value, now, language) })}</time>;
}

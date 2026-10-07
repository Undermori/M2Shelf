import type { ScanHealth } from "../types/media";
import { formatDate } from "../lib/format";
import { useI18n } from "../lib/i18n";

const outcomeKeys = { SUCCESS: "health.success", PARTIAL: "health.partial", FAILED: "health.failed", CANCELLED: "health.cancelled" } as const;

export function LibraryScanHealth({ health }: { health?: ScanHealth | null }) {
  const { t } = useI18n();
  if (!health) return null;
  if (health.warningsIgnored) return <small>{t('comic.warningsIgnored')}</small>;
  return <div className={`library-scan-health ${health.outcome === "SUCCESS" ? "" : "has-warning"}`}>
    <small>{t("health.lastAttempt", { date: formatDate(health.lastAutoAttemptAt) })} · {t(outcomeKeys[health.outcome])}</small>
    <small>{t("health.lastSuccess", { date: formatDate(health.lastSuccessAt) })}</small>
    {health.outcome !== "SUCCESS" && health.outcome !== "CANCELLED" && <small>{t("health.failureHelp", { count: health.errorCount })}</small>}
    {health.detail && <details><summary>{t("health.viewError")}</summary><p>{health.detail === 'COMIC_NO_PAGES' ? t('comic.noPages') : health.detail}</p></details>}
  </div>;
}

import {ScanChanges} from "./ScanChanges";
import type { ScanHealth } from "../types/media";
import { formatDate } from "../lib/format";
import { useI18n } from "../lib/i18n";

const outcomeKeys = { SUCCESS: "health.success", PARTIAL: "health.partial", FAILED: "health.failed", CANCELLED: "health.cancelled" } as const;

export function LibraryScanHealth({ health,onAdded }: { health?: ScanHealth | null;onAdded?:()=>void }) {
  const { t } = useI18n();
  if (!health) return null;
  const hasNotableChanges = Boolean(
    health.changes && (
      health.changes.added > 0
      || health.changes.updated > 0
      || (health.changes.removed ?? 0) > 0
      || health.changes.errors > 0
      || !health.changes.complete
    ),
  );
  if (health.warningsIgnored) return <><small>{t('comic.warningsIgnored')}</small>{hasNotableChanges && <ScanChanges changes={health.changes} onAdded={onAdded}/>}</>;
  if (health.outcome === "SUCCESS" && !health.detail) {
    return hasNotableChanges ? <div className="library-scan-health is-success"><ScanChanges changes={health.changes} onAdded={onAdded}/></div> : null;
  }
  return <div className={`library-scan-health is-${health.outcome.toLowerCase()} ${health.outcome === "SUCCESS" ? "" : "has-warning"}`}>
    {hasNotableChanges && <ScanChanges changes={health.changes} onAdded={onAdded}/>}
    <small>{t("health.lastAttempt", { date: formatDate(health.lastAutoAttemptAt) })} · <span className="scan-outcome">{t(outcomeKeys[health.outcome])}</span></small>
    {health.outcome !== "SUCCESS" && health.outcome !== "CANCELLED" && <small>{t("health.failureHelp", { count: health.errorCount })}</small>}
    {health.detail && <details><summary>{t("health.viewError")}</summary><p>{health.detail === 'COMIC_NO_PAGES' ? t('comic.noPages') : health.detail}</p></details>}
  </div>;
}

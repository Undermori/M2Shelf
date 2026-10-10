import type { LibraryMediaKind, ScanProgress } from "../types/media";
import { compactPath } from "../lib/format";
import { Icon } from "./Icon";
import { useI18n } from "../lib/i18n";

interface ScanBannerProps {
  progress: ScanProgress;
  mediaKind?: LibraryMediaKind;
  onCancel: () => void;
}

export function ScanBanner({ progress, mediaKind='VIDEO', onCancel }: ScanBannerProps) {
  const { t, number } = useI18n();
  const cancelling = progress.status === "CANCELLING";
  const autoMatching = progress.phase === "AUTO_MATCHING";
  const diagnostic = autoMatching
    ? t("scan.autoMatching", { current: number(progress.autoMatchCurrent), total: number(progress.autoMatchTotal) })
    : compactPath(progress.currentPath || t("scan.preparing"), 86);
  return (
    <div className="scan-banner" role="status">
      <span className="scan-pulse"><Icon name="refresh" /></span>
      <div>
        <strong>{cancelling ? t("scan.stopping") : autoMatching ? t("scan.matching") : t("scan.scanning")}</strong>
        <p title={autoMatching ? diagnostic : progress.currentPath}>{diagnostic}</p>
      </div>
      <dl>{autoMatching ? <>
        <div><dt>{t("scan.matched")}</dt><dd>{number(progress.autoMatchMatched)}</dd></div>
        <div><dt>{t("scan.unmatched")}</dt><dd>{number(progress.autoMatchUnmatched + (progress.autoMatchPending ?? 0))}</dd></div>
      </> : <>
        <div><dt>{t("scan.directories")}</dt><dd>{number(progress.foldersScanned)}</dd></div>
        {!['COMIC','EBOOK','DOUJIN','ARTBOOK'].includes(mediaKind)&&<div><dt>{t("scan.videos")}</dt><dd>{number(progress.videosFound)}</dd></div>}
        {(['COMIC','EBOOK','DOUJIN','ARTBOOK'].includes(mediaKind)||!!progress.comicBooksFound)&&<div><dt>{t(mediaKind==='EBOOK'?'ebook.name':'comic.name')}</dt><dd>{number(progress.comicBooksFound??0)}</dd></div>}
        {progress.errors > 0 && <div className="scan-errors"><dt>{t("scan.errors")}</dt><dd>{number(progress.errors)}</dd></div>}
      </>}</dl>
      <button className="button scan-stop" disabled={cancelling} onClick={onCancel} type="button"><Icon name="stop" />{cancelling ? t("scan.stoppingShort") : t("scan.stop")}</button>
    </div>
  );
}

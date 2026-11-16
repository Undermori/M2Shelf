import type { AvailableUpdate } from "../types/media";
import { useI18n } from "../lib/i18n";
import { Icon } from "./Icon";

interface UpdateBannerProps {
  update: AvailableUpdate;
  onOpen: () => void;
  onDismiss: () => void;
}

export function UpdateBanner({ update, onOpen, onDismiss }: UpdateBannerProps) {
  const { t } = useI18n();
  return (
    <aside className="update-banner">
      <Icon name="download" />
      <span aria-atomic="true" aria-live="polite" role="status"><strong>{t("update.bannerTitle", { version: update.version })}</strong></span>
      <button className="button secondary" onClick={onOpen} type="button">{t("update.viewDetails")}</button>
      <button aria-label={t("update.dismissBanner")} className="update-banner-close" onClick={onDismiss} title={t("update.dismissBanner")} type="button"><Icon name="close" /></button>
    </aside>
  );
}

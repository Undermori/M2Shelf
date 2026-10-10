import { useState } from "react";
import brandMark from "../../src-tauri/icons/icon.png";
import { Icon } from "../components/Icon";
import { chooseDirectory, choosePlayerExecutable } from "../lib/api";
import { basename, errorMessage } from "../lib/format";
import { useI18n } from "../lib/i18n";

interface OnboardingPageProps {
  onComplete: (path: string, mpvPath: string | null) => Promise<void>;
  onError: (message: string) => void;
}

export function OnboardingPage({ onComplete, onError }: OnboardingPageProps) {
  const { t } = useI18n();
  const [directory, setDirectory] = useState<string | null>(null);
  const [mpvPath, setMpvPath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const chooseRoot = async () => {
    try { setDirectory(await chooseDirectory()); } catch (error) { onError(errorMessage(error)); }
  };
  const choosePlayer = async () => {
    try { setMpvPath(await choosePlayerExecutable()); } catch (error) { onError(errorMessage(error)); }
  };
  const complete = async () => {
    if (!directory) return;
    setBusy(true);
    try { await onComplete(directory, mpvPath); }
    catch (error) { onError(errorMessage(error)); }
    finally { setBusy(false); }
  };

  return (
    <main className="onboarding-page">
      <div className="onboarding-decoration" aria-hidden="true"><span /><span /><span /></div>
      <section className="onboarding-card">
        <div className="onboarding-brand"><img className="brand-mark" src={brandMark} alt="" aria-hidden="true" /><span><strong>{t("brand.name")}</strong><small>{t("brand.subtitle")}</small></span></div>
        <p className="onboarding-lead">{t("onboarding.lead")}</p>

        <div className="onboarding-fields">
          <button className={`setup-field ${directory ? "has-value" : ""}`} onClick={() => void chooseRoot()} type="button">
            <span className="setup-icon"><Icon name="folder-open" /></span>
            <span><small>{t("onboarding.mediaRequired")}</small><strong>{directory ? basename(directory) : t("onboarding.chooseMedia")}</strong>{directory && <em>{directory}</em>}</span>
            <Icon name={directory ? "check" : "chevron"} />
          </button>
          <button className={`setup-field ${mpvPath ? "has-value" : ""}`} onClick={() => void choosePlayer()} type="button">
            <span className="setup-icon"><Icon name="play" /></span>
            <span><small>{t("onboarding.playerOptional")}</small><strong>{mpvPath ? basename(mpvPath) : t("onboarding.choosePlayer")}</strong>{mpvPath && <em>{mpvPath}</em>}</span>
            <Icon name={mpvPath ? "check" : "chevron"} />
          </button>
        </div>
        <button className="button primary onboarding-submit" disabled={!directory || busy} onClick={() => void complete()} type="button">{busy ? t("onboarding.creating") : t("onboarding.start")}<Icon name="chevron" /></button>
        <div className="onboarding-safety"><Icon name="shield" /><span><strong>{t("onboarding.readOnlyTitle")}</strong><small>{t("onboarding.readOnlyDescription")}</small></span></div>
      </section>
    </main>
  );
}

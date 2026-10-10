import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { I18nProvider } from "./lib/i18n";
import { AppSettingsProvider } from "./lib/settingsStore";
import "./styles.css";
import "./styles/workspace.css";
import './styles/comics.css';

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <I18nProvider>
      <AppSettingsProvider>
      <App />
      </AppSettingsProvider>
    </I18nProvider>
  </StrictMode>,
);

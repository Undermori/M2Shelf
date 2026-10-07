import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { WindowTitlebar } from './components/WindowTitlebar';
import { I18nProvider } from "./lib/i18n";
import "./styles.css";
import "./styles/workspace.css";
import './styles/comics.css';

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <I18nProvider>
      <WindowTitlebar />
      <App />
    </I18nProvider>
  </StrictMode>,
);

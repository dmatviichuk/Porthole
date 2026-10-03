import { isTauri } from "@tauri-apps/api/core";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { stopAllSessions } from "./lib/ipc";
import { applyTheme, savedTheme } from "./lib/theme";
import "./styles.css";

// Before the first paint, so the window never flashes the wrong theme.
applyTheme(savedTheme());

const render = () =>
  createRoot(document.getElementById("root") as HTMLElement).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );

// A webview reload leaves the previous page's watches and shells running in the backend.
if (isTauri()) stopAllSessions().finally(render);
else render();

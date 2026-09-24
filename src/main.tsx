import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/bricolage-grotesque";
import App from "./App";
import "./index.css";
import { ErrorBoundary } from "./components/error-boundary";
import { initLocale } from "./lib/i18n";
import { applyTheme, getThemePreference, watchSystemTheme } from "./lib/theme";

applyTheme(getThemePreference());
initLocale();
const stopWatchingSystemTheme = watchSystemTheme();
if (import.meta.hot) import.meta.hot.dispose(stopWatchingSystemTheme);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
        {/* 错误边界必须包住 `App`：React 在未捕获的渲染错误上会卸载整棵树，
            而桌面端没有控制台可看 —— 没有它，任何一处渲染异常都表现为「一片白」。 */}
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);

/**
 * 文案域：**应用外壳**（英文）。
 *
 * ⚠️ 这里**不写类型注解**：键名合法性由组合入口 `src/locales/en.ts` 单点校验
 * （它把整体约束为 `Partial<Record<TranslationKey, string>>`）。域文件保持零依赖，
 * 既避免循环导入，也让「拼错键名」只报一处。
 *
 * 译文约定（全仓一致）：
 * - 术语与客户端一致：account / check-in / credits / token / gateway / profile。
 * - **产品名不译**（WorkBuddy / TraeWork / Buddy Switch）。
 * - 中文的顿号、书名号按英文习惯改为逗号与引号 —— 照搬标点是最常见的「机翻味」。
 * - 语态用祈使/陈述，不堆砌 Please；长度尽量控制在中文的两倍内，避免挤破 220px 侧栏。
 */
export const en = {
  // ---- App shell / sidebar ----
  "app.name": "Buddy Switch",
  "app.demoBadge": "Demo",
  "product.workbuddy": "WorkBuddy",
  "product.trae": "TraeWork",
  "product.switchAria": "Switch product",
  "product.navAria": "{product} navigation",

  "nav.accounts": "Accounts",
  "nav.tokenStats": "Token usage",
  "nav.credits": "Credits",
  "nav.apiService": "API service",
  "nav.settings": "Settings",

  "sidebar.version": "Version",
  "sidebar.running": "{product} running",
  "sidebar.notRunning": "{product} not running",
  "sidebar.update": "Update",

  // ---- General settings (app-level settings dialog) ----
  "appSettings.title": "General settings",
  "appSettings.description":
    "Shared by WorkBuddy and TraeWork; applies to Buddy Switch itself.",
  "appSettings.entry": "General settings",

  "appSettings.appearance.title": "Appearance",
  "appSettings.appearance.theme.label": "Theme",
  "appSettings.appearance.theme.description":
    "Use light, dark, or follow the system appearance",
  "appSettings.appearance.theme.aria": "Theme",
  "appSettings.theme.system": "System",
  "appSettings.theme.light": "Light",
  "appSettings.theme.dark": "Dark",

  "appSettings.language.title": "Language",
  "appSettings.language.label": "Interface language",
  "appSettings.language.description":
    "Applies immediately; the preference is stored on this device only.",
  "appSettings.language.zh": "简体中文",
  "appSettings.language.en": "English",

  "appSettings.startup.title": "Startup",
  "appSettings.startup.silent.label": "Start minimized to tray at login",
  "appSettings.startup.silent.description":
    "Reflects the system login item directly; reopen from the tray later",
  "appSettings.startup.enabled": "Launch at login enabled",
  "appSettings.startup.disabled": "Launch at login disabled",

  "appSettings.update.title": "Updates",
  "appSettings.update.current": "Current version: ",
  "appSettings.update.source": "Public update source",
  "appSettings.update.openRelease": "Open GitHub Releases",
  "appSettings.update.openReleaseTitle": "Open GitHub Releases",
  "appSettings.update.proxy.label": "Update proxy",
  "appSettings.update.proxy.description":
    "Used only for GitHub update checks and installer downloads; leave empty to disable an explicit proxy.",
  "appSettings.update.proxy.placeholder": "e.g. http://127.0.0.1:7897",
  "appSettings.update.proxy.save": "Save proxy",
  "appSettings.update.proxy.saved": "Update proxy saved",
  "appSettings.update.proxy.cleared": "Update proxy disabled",
  "appSettings.update.proxy.invalid":
    "Invalid proxy address. Enter an HTTP/HTTPS URL, e.g. http://127.0.0.1:7897",
  "appSettings.update.check": "Check for updates",
  "appSettings.update.checkFailed": "Update check failed",
  "appSettings.update.foundTitle": "New version available",
  "appSettings.update.doneTitle": "Update check complete",
  "appSettings.update.foundDetail": "Version v{latest} is available (current v{current})",
  "appSettings.update.upToDate": "You are on the latest version v{current}",
  "appSettings.update.installNow": "Update now",

  // ---- Shared fallbacks ----
  "common.unknownError": "Unknown error",

  // ---- Top-level error boundary (see components/error-boundary.tsx) ----
  "app.error.title": "Something went wrong",
  "app.error.description":
    "A rendering error was caught. The UI stopped here to avoid showing a garbled screen. Copy the details below and report them; reloading usually gets you back in.",
  "app.error.reload": "Reload",
  "app.error.copy": "Copy error details",
  "app.error.copied": "Copied",
};

import { t } from "./i18n";

/**
 * 演示模式下被拦截操作的统一提示。
 *
 * 走**取值函数**而不是模块级常量：常量在模块加载时定型，语言切换后不会更新
 * （与侧栏 `PRODUCT_NAV` 存键而非成品文案是同一条理由）。
 */
export function demoUnavailableMessage(): string {
  return t("shared.demo.unavailable");
}

/** Public demo and README screenshot builds share the same read-only frontend runtime. */
export const demoModeEnabled =
  import.meta.env.VITE_DEMO_MODE === "1" || import.meta.env.VITE_SCREENSHOT_DEMO === "1";

/** GitHub Pages needs project-relative assets and hash routes; other demo/dev builds do not. */
export const pagesDemoHostingEnabled = import.meta.env.VITE_PAGES_DEMO === "1";

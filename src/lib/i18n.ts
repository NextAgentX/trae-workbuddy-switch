import { useCallback, useSyncExternalStore } from "react";

import { en } from "@/locales/en";
import { zh, type TranslationKey } from "@/locales/zh";

/**
 * 轻量 i18n 内核：`Locale` 状态 + 持久化 + `useT()`。
 *
 * ## 为什么自研而不是引入 react-i18next
 *
 * 本仓只需要三件事：**一张词表、一个当前语言、一个插值**。引入 i18next 要多两个依赖、
 * 一层 Provider、以及一套与既有 zustand store 并存的订阅模型，而它带来的复数/命名空间/
 * 富文本能力在这里都用不上。内核与 `lib/theme.ts` 同构（都是「localStorage + 启动时读一次
 * + 应用副作用」），读代码的人不必再学一套约定。
 *
 * ## 三条约定
 *
 * 1. **默认中文，且刻意不跟随 `navigator.language`。** 理由不是懒：现有用户里存在
 *    「系统语言非中文、但一直在用中文界面」的人，若按系统语言自动切换，升级后他们的界面会
 *    **无故变成英文**。语言切换是显式动作，不做隐式迁移（与「CN 行为零变化」同一原则）。
 * 2. **缺英文键 → 回落中文**，绝不渲染成空白或键名。见 `translate`。
 * 3. **不做「模块加载时读 localStorage」的副作用**：读取只发生在 {@link initLocale}
 *    （`main.tsx` 在首次渲染前调用一次），此后以模块内 `current` 为准 —— 这样
 *    渲染期读到的值一定是稳定的，`useSyncExternalStore` 的快照语义才成立。
 */
export type Locale = "zh" | "en";

/** 界面可选语言（顺序即下拉框顺序）。 */
export const LOCALES: readonly Locale[] = ["zh", "en"];

const STORAGE_KEY = "buddy-switch.language";
const DEFAULT_LOCALE: Locale = "zh";

/** 缺键兜底词表 —— 同时是**键的权威**。 */
const FALLBACK: Record<string, string> = zh;

export function isLocale(value: unknown): value is Locale {
  return value === "zh" || value === "en";
}

/** 读取已持久化的偏好；不可用（隐私模式、非浏览器环境）或值非法时回落默认语言。 */
export function getLocale(): Locale {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (isLocale(stored)) return stored;
  } catch {
    // 存储不可用时仍要能跑，用默认语言。
  }
  return DEFAULT_LOCALE;
}

/** 把语言写到 `<html lang>`（无障碍与字重回退都读它）。 */
export function applyLocale(locale: Locale): void {
  if (typeof document === "undefined") return;
  document.documentElement.lang = locale === "zh" ? "zh-CN" : "en";
}

let current: Locale = DEFAULT_LOCALE;
const listeners = new Set<() => void>();

/** 启动时调用一次（`main.tsx`，在 `ReactDOM.createRoot(...).render` 之前）。 */
export function initLocale(): Locale {
  current = getLocale();
  applyLocale(current);
  return current;
}

/** 当前语言。**渲染期不要直接读它**，用 {@link useLocale} 才会订阅到变化。 */
export function currentLocale(): Locale {
  return current;
}

/** 切换语言：持久化 + 应用 + 通知订阅者（组件因此重渲染）。 */
export function setLocale(next: Locale): void {
  if (!isLocale(next)) return;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // 存不下也要立即生效，只是下次启动会回到旧值。
  }
  applyLocale(next);
  if (current === next) return;
  current = next;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** 订阅当前语言。语言切换时**只有用到它的组件**重渲染。 */
export function useLocale(): Locale {
  return useSyncExternalStore(subscribe, currentLocale, () => DEFAULT_LOCALE);
}

/**
 * 把 `{name}` 占位符替换为参数值。
 *
 * 参数**缺席**时保留原占位符而不是替换成空串：宁可让漏配一眼可见
 * （界面上出现 `{version}`），也不要静默产出「已是最新版本 v」这种半句话。
 */
export function interpolate(
  template: string,
  params?: Record<string, string | number>,
): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : whole,
  );
}

/** 纯函数版本（可测、可在非组件代码里用）。 */
export function translate(
  locale: Locale,
  key: TranslationKey,
  params?: Record<string, string | number>,
): string {
  const template = (locale === "en" ? en[key] : undefined) ?? FALLBACK[key] ?? key;
  return interpolate(template, params);
}

export type Translate = (
  key: TranslationKey,
  params?: Record<string, string | number>,
) => string;

/** 组件内翻译函数。它会订阅语言变化，因此切换后自动重渲染。 */
export function useT(): Translate {
  const locale = useLocale();
  return useCallback((key, params) => translate(locale, key, params), [locale]);
}

/** 非组件代码（工具函数、事件处理）用的翻译；**不订阅**，取的是当前值。 */
export function t(key: TranslationKey, params?: Record<string, string | number>): string {
  return translate(current, key, params);
}

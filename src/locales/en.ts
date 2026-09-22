import { en as shell } from "./domains/shell.en";
import { en as shared } from "./domains/shared.en";
import { en as traeComponents } from "./domains/traeComponents.en";
import { en as traePages } from "./domains/traePages.en";
import { en as wbAccounts } from "./domains/wbAccounts.en";
import { en as wbSettings } from "./domains/wbSettings.en";
import { en as wbStats } from "./domains/wbStats.en";
import type { TranslationKey } from "./zh";

/**
 * 文案词表（English）——按域拆分后在此组合。
 *
 * ## 为什么整体是 `Partial`
 *
 * 键的权威在 `zh.ts`。这里**允许缺键**：缺哪条就回落中文，而不是渲染成空白或键名。
 * 写成 `Partial<Record<TranslationKey, string>>` 还有一层作用 —— 拼错键名会**编译报错**，
 * 因此「加了英文却把键名打错」不会静默丢翻译。
 *
 * ## 类型校验为什么只在**这里**做
 *
 * 各 `domains/*.en.ts` 刻意不写类型注解、保持零依赖：这样它们既不会与 `zh.ts`
 * 形成循环导入，也让「拼错键名」全仓只报一处，便于定位。
 *
 * 译文约定见 `domains/shell.en.ts` 的头注释（全仓一致）。
 *
 * ⚠️ 域文件之间不得重键，重键会**静默**由后展开者胜出。
 */
export const en: Partial<Record<TranslationKey, string>> = {
  ...shell,
  ...shared,
  ...wbSettings,
  ...wbAccounts,
  ...wbStats,
  ...traePages,
  ...traeComponents,
};

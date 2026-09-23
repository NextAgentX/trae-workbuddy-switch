import { zh as shell } from "./domains/shell.zh";
import { zh as shared } from "./domains/shared.zh";
import { zh as traeComponents } from "./domains/traeComponents.zh";
import { zh as traeGateway } from "./domains/traeGateway.zh";
import { zh as traePages } from "./domains/traePages.zh";
import { zh as traeStats } from "./domains/traeStats.zh";
import { zh as wbAccounts } from "./domains/wbAccounts.zh";
import { zh as wbSettings } from "./domains/wbSettings.zh";
import { zh as wbStats } from "./domains/wbStats.zh";

/**
 * 文案词表（简体中文）——**键的唯一权威**，按域拆分后在此组合。
 *
 * ## 三条约定
 *
 * 1. 本文件与各域文件是**键的权威**：`en` 侧缺键即回落中文，绝不渲染成空白或键名。
 * 2. 键名用**扁平点分**命名（`域.子域.用途`），便于 `grep '"wbAccounts\.'` 一次看全。
 * 3. **不要**在这里放错误码文案 —— 后端错误的「码 → 英文」映射在
 *    `src/locales/errors.en.ts`，中文一律用后端下发的原文（见 `lib/error-code.ts`），
 *    避免两处中文各自演化出不同说法。同理，只有**真正被引用**的键才留下：
 *    本仓没有「未使用键」检查，多余的键会静默腐坏。
 *
 * ## 为什么要按域拆文件，而不是一张大表
 *
 * 一是可评审（每个域一个人改、一个人看）；二是**并行铺量时不会互相踩** —— 一张大表
 * 被两个执行者同时编辑必然丢失改动（本仓踩过：「同一文件的多处 Edit 只有最后一次生效」）。
 *
 * ⚠️ 域文件之间**不得重键**：组合走对象展开，重键会**静默**由后展开者胜出。
 */
export const zh = {
  ...shell,
  ...shared,
  ...wbSettings,
  ...wbAccounts,
  ...wbStats,
  ...traePages,
  ...traeStats,
  ...traeComponents,
  ...traeGateway,
} as const;

/** 词表键。`en.ts` 以它为 `Partial` 的上界，故英文侧拼错键名会编译报错。 */
export type TranslationKey = keyof typeof zh;

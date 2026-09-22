import * as api from "@/lib/api";
import type { ScheduleConfig } from "@/lib/types";

/**
 * 定时任务排程配置在前端缓存里的键。
 *
 * **只此一份**：Trae 的账号页（工具栏「自动签到」开关）与 Trae 设置页（同一开关 + 小时表）
 * 都要读写同一份配置。各自写一遍字符串字面量迟早漂移成两把键，症状是
 * 「一处改完另一处不刷新」，而且不会有任何报错 —— 与 `TRAE_VARIANTS_KEY` 同款约定。
 *
 * ⚠️ 排程配置是**全局单份**（`~/.buddy-switch/schedule_config.json`）：它按**任务**分家，
 * 不按产品、也不按 region。两个产品各自的开关只是这份配置里的两个不同字段。
 */
export const SCHEDULE_CONFIG_KEY = "schedule:config";

/** 读取排程配置（供 `useCachedResource` 当 loader 用）。 */
export function loadScheduleConfig(): Promise<ScheduleConfig> {
  return api.getScheduleConfig();
}

/**
 * 局部更新排程配置：读到的当前值 + 本次要改的键 → 整份提交。
 *
 * 后端 `save_schedule_config` 收的是**整份**配置（缺失键回落默认值），因此这里必须
 * 把当前值带上 —— 只提交一个字段会让其余字段被默认值覆盖（那是静默的配置丢失）。
 * 该语义由本函数单点承担，调用方不要自己拼对象。
 */
export function saveSchedulePatch(
  current: ScheduleConfig,
  patch: Partial<ScheduleConfig>,
): Promise<ScheduleConfig> {
  return api.saveScheduleConfig({ ...current, ...patch });
}

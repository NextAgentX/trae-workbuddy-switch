// Region 描述符（前端侧，对照架构设计 A-3.1 RegionSpec）。
// 仅承载 UI 展示与文案所需字段，业务差异全部由后端 region 参数决定。

import { t } from "@/lib/i18n";
import type { Region, RegionFilter } from "./types";

export interface RegionDescriptor {
  region: Region;
  /** 展示名：WorkBuddy / WorkBuddy AI */
  displayName: string;
  /**
   * 版本名：**读取时才翻译**（表内只存文案键，见 {@link REGION_DESCRIPTORS}）。
   *
   * ⚠️ 本表在模块加载时就定型，把中文写进表里会让语言切换对它失效 ——
   * 因此表里只存键，展示名由 getter 在**每次读取时**调 `t()` 现取。
   */
  versionLabel: string;
  /** 认证文件 basename */
  authFilename: string;
  /** 认证文件路径覆盖环境变量 */
  authEnv: string;
  /** 账号库文件名 */
  accountsFilename: string;
  /** 网关 Key 归属展示名（同 `versionLabel`：读取时才翻译）。 */
  gatewayLabel: string;
}

export const REGION_DESCRIPTORS: Record<Region, RegionDescriptor> = {
  cn: {
    region: "cn",
    displayName: "WorkBuddy",
    get versionLabel() {
      return t("shared.region.version.cn");
    },
    authFilename: "workbuddy-desktop.info",
    authEnv: "WORKBUDDY_AUTH_FILE",
    accountsFilename: "accounts.json",
    get gatewayLabel() {
      return t("shared.region.gateway.cn");
    },
  },
  global: {
    region: "global",
    displayName: "WorkBuddy AI",
    get versionLabel() {
      return t("shared.region.version.global");
    },
    authFilename: "workbuddy-desktop-ai.info",
    authEnv: "WORKBUDDY_AI_AUTH_FILE",
    accountsFilename: "accounts.global.json",
    get gatewayLabel() {
      return t("shared.region.gateway.global");
    },
  },
};

/** 全部 region，按 UI 展示顺序。 */
export const REGIONS: Region[] = ["cn", "global"];

/**
 * 统计查询范围（含合并态），按 UI 展示顺序。
 * 与 `REGIONS` 语义不同：`REGIONS` 是「实体归属」枚举，仍只含 cn / global，账号管理页依赖之。
 */
export const REGION_FILTERS: RegionFilter[] = ["cn", "global", "all"];

export function regionDescriptor(region: Region): RegionDescriptor {
  return REGION_DESCRIPTORS[region];
}

/** 中文版本名，如「国内版」。 */
export function regionLabel(region: Region): string {
  return REGION_DESCRIPTORS[region].versionLabel;
}

/** 查询范围文案：cn/global 复用 `regionLabel()`，all 固定「合并」。 */
export function regionFilterLabel(filter: RegionFilter): string {
  return filter === "all" ? t("shared.region.filter.all") : regionLabel(filter);
}

/** 另一个 region。 */
export function otherRegion(region: Region): Region {
  return region === "cn" ? "global" : "cn";
}

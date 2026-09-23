import * as api from "@/lib/api";
import { t } from "@/lib/i18n";
import type { TranslationKey } from "@/locales/zh";
import type { TraeProgramStatus, TraeRegionId, TraeVariantId, TraeVariantStatus } from "@/lib/trae-types";

/**
 * 造一个「状态未知」的程序位占位。
 *
 * `label` / `variant` 是**系统性标识**（前端比对与回传后端都用它，例如
 * `program === "trae_code"`），因此始终保持英文；只有 `nameAlias` 是**面向用户
 * 的展示名**，随语言切换。表在模块加载时定型 ⇒ 展示名由 `nameAliasKey` 延迟到
 * 读取时现取（`programNameAlias`），否则切语言后整张表不会更新。
 */
function programStub(
  program: TraeProgramStatus["program"],
  label: string,
  nameAliasKey: TranslationKey,
  variant: TraeVariantId | null,
): TraeProgramStatus {
  return {
    program,
    label,
    get nameAlias() {
      return t(nameAliasKey);
    },
    variant,
    installed: false,
    running: false,
    version: null,
    path: null,
    dataDir: null,
    dataDirExists: false,
  };
}

/**
 * 探测不到任何区域时仍列出的**两个区域**（状态未知的占位）。
 *
 * ⚠️ **只此一份**：账号页的区域切换条与账号卡片都要遍历「有哪些区域/程序位」，
 * 各自维护一份占位表迟早漂移（曾经由 `TraeVariantBar` 私有持有）。
 *
 * 程序位的 `variant` 是**切换时要回传给后端的标识**：
 * 国内两个程序位有各自的历史标识（`trae_work` / `trae_cn`，后端据此选客户端），
 * 国际版 TraeCode 尚未建模 ⇒ `null`（按钮必须禁用，不能拿同区域另一个客户端顶替）。
 *
 * `consoleBase` 是本表里**唯一**「只在后端不可用时才用到」的域值 —— 正常路径一律由后端
 * `region_endpoints(region).console_base` 提供（`platform.rs` 有单测钉住按区域分家）。
 * 它出现在这里是因为本表本就是「后端不可用时的展示快照」，与 `variantLabel` 同类；
 * ⚠️ **后端改域时必须回来同步这一处**，否则演示模式/后端挂掉时「关于」外链会指向旧站。
 */
export const TRAE_VARIANT_FALLBACK: TraeVariantStatus[] = [
  {
    variant: "cn",
    // 表在模块加载时就定型 ⇒ 只存文案键，展示名在**读取时**现取，语言切换才不会失效。
    get variantLabel() {
      return t("shared.region.version.cn");
    },
    consoleBase: "https://www.trae.cn",
    installed: false,
    running: false,
    version: null,
    path: null,
    dataDir: null,
    dataDirExists: false,
    programs: [
      programStub("trae_work", "TraeWork", "trae.program.traeWork", "trae_work"),
      programStub("trae_code", "TraeCode", "trae.program.traeCode", "trae_cn"),
    ],
  },
  {
    variant: "global",
    get variantLabel() {
      return t("shared.region.version.global");
    },
    consoleBase: "https://www.trae.ai",
    installed: false,
    running: false,
    version: null,
    path: null,
    dataDir: null,
    dataDirExists: false,
    programs: [
      programStub("trae_work", "TraeWork AI", "trae.program.traeWorkGlobal", "global"),
      programStub("trae_code", "Trae AI", "trae.program.traeCodePending", null),
    ],
  },
];

/**
 * 某个程序位当前登录的账号：**身份与展示名成对出现**。
 *
 * 两者同源于**同一次** `get_trae_profiles` 调用，因此结构上不可能出现
 * 「名字已更新、身份还是旧的」这种半更新状态 —— 这正是把它们放进一个对象、
 * 而不是两个平行的 Map 的理由（两个 Map 迟早漂移成两把不同的键）。
 */
export interface TraeVariantLogin {
  /** 身份（uid）。卡片上「是不是当前账号」的相等比较用它；**不要**拿它渲染文本。 */
  userId: string;
  /**
   * 给人看的名字：优先账号库里的 `name`，**查不到时回落 uid**。
   *
   * 回落只在这里写一次：状态条与设置页显示的是同一句话，两处各写一遍回落链
   * 迟早出现「一处显示名字、一处显示数字」。回落而不是显示「未知账号」的理由见
   * Rust 侧 `account::display_name_for` —— 那串数字仍能让用户去客户端里核对。
   */
  name: string;
}

/** 程序位标识 → 该程序当前登录账号（无 / 读不到时为 `null`）。 */
export type TraeVariantLogins = Partial<Record<TraeVariantId, TraeVariantLogin | null>>;

/**
 * 「全部区域的环境状态」在快照缓存里的键。
 *
 * **只此一份**：侧栏那颗运行状态圆点与账号页都要用这份探测结果，
 * 各自写一遍字符串字面量迟早漂移成两把不同的键（症状是「缓存命中不了、
 * 每次切换都重新探测」，而且不会有任何报错）。
 */
export const TRAE_VARIANTS_KEY = "trae:variants";

/**
 * 读取**全部区域**的环境状态（并排视角）。
 *
 * 返回**永不为空**：探测命令不可用（演示模式、后端未起来）或返回空列表时，
 * 回落到 {@link TRAE_VARIANT_FALLBACK} 的两个区域占位 —— 调用方据此渲染
 * 「有哪些区域」的控件，选项必须在后端不可用时依然存在，
 * 否则用户连切回另一条线的入口都没有。
 */
export async function loadTraeVariantStatuses(): Promise<TraeVariantStatus[]> {
  try {
    const result = await api.getTraeVariants();
    return result.variants?.length ? result.variants : TRAE_VARIANT_FALLBACK;
  } catch {
    return TRAE_VARIANT_FALLBACK;
  }
}

/**
 * 读取**每个程序位各自的**当前登录账号（`profiles.currentAccount` + 展示名）。
 *
 * ## 为什么遍历的是**程序位**而不是区域
 *
 * 登录态快照是**客户端级**的（它只能恢复到采集它的那个客户端里去），因此
 * 「当前账号」天然是**每个程序位一条**。区域级那个值只是界面汇总用的展示，
 * 不能拿来判断某个账号是否"当前账号"。
 *
 * `statuses` 由调用方传入而不是本函数自己探测：调用方（账号页）本来就要用
 * 同一份 `statuses` 去渲染控件，再探一次会得到第二个可能不一致的快照。
 *
 * ## 为什么同时返回身份与展示名
 *
 * 界面需要两个不同的事实：身份用于**相等比较**（卡片上谁是当前账号），
 * 展示名用于**给人看**（「已登录: `<名字>`」）。改造前只返回 uid，
 * 于是展示端只能把 uid 摆上去 —— 用户看到一串 16 位数字，认不出是谁。
 * 两个都由后端同一次响应给出，前端只负责回落（见 {@link TraeVariantLogin.name}）。
 *
 * **单个程序位失败不影响其余**：读不到只记 `null`（视为「没有当前账号」），
 * 不抛错 —— 卡片上少一枚「当前账号」角标，远好过整页报错。
 */
export async function loadTraeVariantLogins(
  statuses: TraeVariantStatus[],
): Promise<TraeVariantLogins> {
  const targets = statuses.flatMap((item) =>
    item.programs
      .map((program) => program.variant)
      .filter((variant): variant is TraeVariantId => variant !== null),
  );
  const pairs = await Promise.all(
    targets.map(async (variant): Promise<[TraeVariantId, TraeVariantLogin | null]> => {
      try {
        const profiles = await api.getTraeProfiles(variant);
        const userId = profiles.currentAccount ?? null;
        // 没有当前账号 ⇒ `null`（**不是** `{ userId: "" , name: "" }`）：
        // 调用方靠它区分「未登录」与「已登录但名字读不到」。
        if (userId === null) return [variant, null];
        return [variant, { userId, name: profiles.currentAccountName || userId }];
      } catch {
        return [variant, null];
      }
    }),
  );
  return Object.fromEntries(pairs) as TraeVariantLogins;
}

/** 取某个区域的条目（找不到时返回 `undefined`，调用方自行回落占位）。 */
export function findRegionStatus(
  statuses: TraeVariantStatus[],
  region: TraeRegionId,
): TraeVariantStatus | undefined {
  return statuses.find((item) => item.variant === region);
}

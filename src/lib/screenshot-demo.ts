import type {
  AccountMeta, AccountStrategyMap, ApiKeyRecord, AppStatus, AutoRotateConfig, CatalogSnapshot,
  CheckinConfig, CheckinLog, CodeBuddyCliStatus, CodeBuddyCliSwitchResult, CodeBuddyCnIdeStatus, CreditExpiry,
  CreditOfficialUsageModel, CreditStatistics, GatewayConfig, GatewayLogEntry, GatewayStatus,
  GithubConfig, Region, RotateLog, RotateStatus, ScheduleConfig, TokenStatistics, TokenStatsGroup, TokenStatsSource,
  TokenStatsTotals, TravelConfig, TravelStatus,
} from "./types";
import type {
  TraeAccount,
  TraeApiKeyRecord,
  TraeCapabilities,
  TraeCheckinStatus,
  TraeCreditsOverview,
  TraeEnvStatus,
  TraeVariantsStatus,
  TraeGatewayConfigRaw,
  TraeGatewayLogEntry,
  TraeLogsResponse,
  TraeProfilesOverview,
  TraeSettings,
  TraeTokenStatistics,
} from "./trae-types";
import { demoModeEnabled } from "./demo-mode";
import { t } from "./i18n";
import type { TranslationKey } from "@/locales/zh";

export const screenshotDemoEnabled = demoModeEnabled;

/**
 * 演示夹具里的**自然语言**一律走 `t()` —— 演示站/截图站也支持中英切换，
 * 而 `npm run build:demo` 产出的 `dist-demo` 正是给人看的那一站。
 *
 * ⚠️ 两条纪律（演示数据是「看起来像真的」的样本，写错会误导截图核对）：
 *
 * 1. **键表是模块级常量 ⇒ 只能存键名，不能存文案。** 凡「一整张表在建表时定型」的地方
 *    （`creditPackageSeeds` / `tokenSessionSeeds` / `capabilityStubs` / 日志行 / 假账号名），
 *    都存 `TranslationKey`（或含键名的小结构），**在函数体/读取点现取** `t(key)`；
 *    直接把 `t()` 的结果写进表里，切语言后整张表不会更新（与 `PRODUCT_NAV` 同一个坑）。
 * 2. **刻意保持英文的字面量不要「顺手翻译」**：模型名（`glm-5.3`）、协议键
 *    （`apiProvider` / `apiBase`）、HTTP 端点（`/v1/chat/completions`）、状态字面量
 *    （`available` / `cooling` / `"限时免费"` 那种**数据值**除外）、路径、uid 都是**契约**，
 *    翻译了就不再是「与真机同形」的样本。
 */
const MODEL_NAMES = ["deepseek-v4-flash", "kimi-k3-1", "deepseek-v4-pro", "glm-5.2", "hy3"] as const;

interface ModelSeed {
  model: (typeof MODEL_NAMES)[number];
  requestCount: number;
  credit: number;
}

interface AccountUsageSeed {
  requestCount: number;
  models: ModelSeed[];
}

/**
 * 假账号的展示名与备注。
 *
 * 按 `id` 取键（**不是**数组下标）：`id` 才是稳定身份，将来插一条演示账号也不会
 * 让所有名字整体错位 —— 键与实体一一对应是这类映射的最低要求。
 */
const DEMO_ACCOUNT_KEYS: Record<string, { nickname: TranslationKey; remark?: TranslationKey }> = {
  "demo-account-a": { nickname: "shared.demo.account.a", remark: "shared.demo.account.aRemark" },
  "demo-account-b": { nickname: "shared.demo.account.b" },
  "demo-account-c": { nickname: "shared.demo.account.c" },
};
/**
 * 演示账号的**静态部分**（`nickname` / `remark` 不在表内）。
 *
 * 把随语言切换的两个字段**排除在表外**是刻意的：`screenshot-demo.ts` 的夹具表在模块
 * 加载时定型，若在此处写死 `t(...)`，切语言后整张表不会更新；写成「表 + 读取时补名」
 * 又会让 `AccountMeta` 类型缺字段（它要求 `nickname` 必填）。
 * ⇒ 用 `Omit` 明确表达「这张表暂时没有那两个字段」，由 `withDemoNames` 补齐。
 */
type DemoAccountSeed = Omit<AccountMeta, "nickname" | "remark"> & { id: keyof typeof DEMO_ACCOUNT_KEYS };

const accountSeeds: DemoAccountSeed[] = [
  { id: "demo-account-a", uid: "demo-user-001", email: "test-a@example.com", enterpriseName: "Demo Workspace", expiresAt: 0, refreshExpiresAt: 0, refreshedAt: 0, createdAt: 0, needsRelogin: false, needsReloginReason: null },
  { id: "demo-account-b", uid: "demo-user-002", email: "test-b@example.com", enterpriseName: "Demo Workspace", expiresAt: 0, refreshExpiresAt: 0, refreshedAt: 0, createdAt: 0, needsRelogin: false, needsReloginReason: null },
  { id: "demo-account-c", uid: "demo-user-003", email: "test-c@example.com", enterpriseName: "Demo Workspace", expiresAt: 0, refreshExpiresAt: 0, refreshedAt: 0, createdAt: 0, needsRelogin: false, needsReloginReason: null },
];

/**
 * 给账号补上随语言切换的展示字段（`nickname` / `remark`）。
 *
 * 调用点必须**在渲染期**（不是模块加载期）：`nickname` 直接进 React 树，
 * 用户切语言时它们跟着变，而不是停留在建表时的那一种语言。
 */
function withDemoNames(list: readonly DemoAccountSeed[]): AccountMeta[] {
  return list.map((account) => {
    const keys = DEMO_ACCOUNT_KEYS[account.id];
    return {
      ...account,
      nickname: t(keys.nickname),
      remark: keys.remark ? t(keys.remark) : null,
    };
  });
}

/**
 * 单条账号的展示名（`nickname → email → id`）。
 *
 * 给「手上只有 `accountSeeds[...]`（静态表，无 `nickname`）却要填 `accountName`」的
 * 调用点用 —— 与 `account.nickname ?? account.email ?? account.id` 同一条降级链，
 * 只是把第一环换成**现取**词表的昵称。
 */
function demoAccountName(account: DemoAccountSeed): string {
  return t(DEMO_ACCOUNT_KEYS[account.id].nickname) || account.email || account.id;
}

/** 演示模式中的临时 CLI 当前账号，仅存在于本次页面会话。 */
let demoActiveCliAccountId = accountSeeds[0].id;

// Counts and relative model roles follow anonymous aggregates from the sanitized local cache.
// No upstream request row or identifier is copied into this fixture.
const usageSeeds: AccountUsageSeed[] = [
  {
    requestCount: 2243,
    models: [
      { model: "deepseek-v4-flash", requestCount: 2133, credit: 1794.39 },
      { model: "kimi-k3-1", requestCount: 24, credit: 2497.16 },
      { model: "deepseek-v4-pro", requestCount: 23, credit: 3.63 },
      { model: "glm-5.2", requestCount: 1, credit: 33.63 },
      { model: "hy3", requestCount: 62, credit: 0 },
    ],
  },
  {
    requestCount: 679,
    models: [
      { model: "deepseek-v4-flash", requestCount: 659, credit: 1270.62 },
      { model: "hy3", requestCount: 20, credit: 0 },
    ],
  },
  {
    requestCount: 318,
    models: [
      { model: "deepseek-v4-flash", requestCount: 309, credit: 595.08 },
      { model: "hy3", requestCount: 9, credit: 0 },
    ],
  },
];

/** 积分包名称的键（三组账号用同一批名字，数值不同 —— 值留在 `creditPackageSeeds`）。 */
const PACK_FISSION: TranslationKey = "shared.demo.pack.fission";
const PACK_CREDITS: TranslationKey = "shared.demo.pack.credits";
const PACK_TRIAL: TranslationKey = "shared.demo.pack.trial";
const PACK_CHECKIN: TranslationKey = "shared.demo.pack.checkin";
const PACK_ACTIVITY: TranslationKey = "shared.demo.pack.activity";

/** `[包名键, 总量, 剩余, 距到期天数]` */
const creditPackageSeeds: [TranslationKey, number, number, number][][] = [
  [
    [PACK_FISSION, 5000, 3186.4, 36],
    [PACK_CREDITS, 2400, 1180.75, 18],
    [PACK_TRIAL, 800, 386.4, 5],
    [PACK_CHECKIN, 300, 196.25, 11],
    [PACK_ACTIVITY, 600, 428.6, 27],
  ],
  [
    [PACK_FISSION, 3600, 2468.2, 24],
    [PACK_CREDITS, 1800, 905.5, 42],
    [PACK_TRIAL, 500, 128.2, 7],
    [PACK_CHECKIN, 240, 174.35, 15],
    [PACK_ACTIVITY, 400, 286.8, 31],
  ],
  [
    [PACK_FISSION, 2400, 1680.4, 29],
    [PACK_CREDITS, 1200, 748.6, 55],
    [PACK_TRIAL, 360, 214.5, 14],
    [PACK_CHECKIN, 180, 96.75, 21],
    [PACK_ACTIVITY, 300, 207.9, 38],
  ],
];

function startOfToday(): Date {
  const date = new Date();
  date.setHours(0, 0, 0, 0);
  return date;
}

function localDate(daysAgo: number): string {
  const date = startOfToday();
  date.setDate(date.getDate() - daysAgo);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

function atLocalTime(daysAgo: number, hour: number, minute: number): number {
  const date = startOfToday();
  date.setDate(date.getDate() - daysAgo);
  date.setHours(hour, minute, 0, 0);
  return date.getTime();
}

function futureAt(daysAhead: number, hour = 23, minute = 59): number {
  const date = startOfToday();
  date.setDate(date.getDate() + daysAhead);
  date.setHours(hour, minute, 0, 0);
  return date.getTime();
}

function hydratedAccounts(): AccountMeta[] {
  return withDemoNames(accountSeeds).map((account, index) => ({
    ...account,
    expiresAt: futureAt(12 + index * 5, 18, 30),
    refreshExpiresAt: futureAt(40 + index * 7),
    refreshedAt: atLocalTime(0, 9, 12 + index * 7),
    createdAt: atLocalTime(45 + index * 19, 10, 0),
  }));
}

function creditExpiry(accountId: string): CreditExpiry {
  const index = Math.max(0, accountSeeds.findIndex((account) => account.id === accountId));
  const account = accountSeeds[index] ?? accountSeeds[0];
  const resources = creditPackageSeeds[index].map(([nameKey, total, remaining, expireDays], packageIndex) => ({
    packageCode: `demo-package-${index + 1}-${packageIndex + 1}`,
    packageName: t(nameKey),
    total,
    remaining,
    used: Number((total - remaining).toFixed(2)),
    status: 1,
    expireAt: futureAt(expireDays),
    expired: false,
    expiringSoon: expireDays <= 7,
  }));
  const totalCapacity = resources.reduce((sum, resource) => sum + resource.total, 0);
  const totalRemaining = resources.reduce((sum, resource) => sum + resource.remaining, 0);
  const expiringSoonRemaining = resources.filter((resource) => resource.expiringSoon).reduce((sum, resource) => sum + resource.remaining, 0);

  return {
    ok: true,
    accountId: account.id,
    accountName: demoAccountName(account),
    updatedAt: Date.now() - (index + 1) * 4 * 60 * 1000,
    totalCapacity,
    totalRemaining: Number(totalRemaining.toFixed(2)),
    expiringSoonRemaining: Number(expiringSoonRemaining.toFixed(2)),
    expiredRemaining: 0,
    soonestExpireAt: Math.min(...resources.map((resource) => resource.expireAt)),
    expiringSoon: expiringSoonRemaining > 0,
    expired: false,
    resources,
  };
}

function dailyWeight(accountIndex: number, dayIndex: number): number {
  const weekdayWave = [0.72, 1.08, 0.93, 1.22, 0.84, 1.16, 1.01][dayIndex % 7];
  const quiet = (dayIndex + accountIndex * 4) % 13 === 0 ? 0.16 : 1;
  return weekdayWave * quiet * (1 + accountIndex * 0.035);
}

function distributeModels(seed: AccountUsageSeed, accountIndex: number) {
  const weights = Array.from({ length: 30 }, (_, dayIndex) => dailyWeight(accountIndex, dayIndex));
  const weightTotal = weights.reduce((sum, weight) => sum + weight, 0);
  const countSeries = seed.models.map((model) => {
    const raw = weights.map((weight) => (model.requestCount * weight) / weightTotal);
    const values = raw.map(Math.floor);
    let remaining = model.requestCount - values.reduce((sum, value) => sum + value, 0);
    const byFraction = raw.map((value, index) => ({ index, fraction: value - Math.floor(value) })).sort((left, right) => right.fraction - left.fraction);
    for (let index = 0; index < remaining; index += 1) values[byFraction[index].index] += 1;
    return values;
  });
  const creditSeries = seed.models.map((model) => {
    const values = weights.map((weight) => Number(((model.credit * weight) / weightTotal).toFixed(2)));
    const drift = Number((model.credit - values.reduce((sum, value) => sum + value, 0)).toFixed(2));
    values[values.length - 1] = Number((values[values.length - 1] + drift).toFixed(2));
    return values;
  });
  return Array.from({ length: 30 }, (_, dayIndex) => {
    const models = seed.models.map((model, modelIndex) => ({
      model: model.model,
      requestCount: countSeries[modelIndex][dayIndex],
      credit: creditSeries[modelIndex][dayIndex],
    }));
    return {
      date: localDate(29 - dayIndex),
      usage: Number(models.reduce((sum, model) => sum + model.credit, 0).toFixed(2)),
      models,
    };
  });
}

function sumModels(rows: { models: CreditOfficialUsageModel[] }[]): CreditOfficialUsageModel[] {
  const totals = new Map<string, CreditOfficialUsageModel>();
  for (const row of rows) {
    for (const model of row.models) {
      const current = totals.get(model.model) ?? { model: model.model, requestCount: 0, credit: 0 };
      current.requestCount += model.requestCount;
      current.credit = Number((current.credit + model.credit).toFixed(2));
      totals.set(model.model, current);
    }
  }
  return [...totals.values()].sort((left, right) => right.credit - left.credit);
}

function visibleRequests(accountIndex: number) {
  const account = accountSeeds[accountIndex];
  const seed = usageSeeds[accountIndex];
  const hours = [16, 15, 17, 14, 1, 0, 3];
  const flashCredits = [0.13, 0.04, 0.2, 1, 0.08, 3.99, 0.45, 8.5, 24.56];
  const kimiCredits = [86.4, 103.2, 112.8, 128.4, 74.6];
  const proCredits = [0.04, 0.13, 0.2, 0.45];
  const weightedModels = seed.models.flatMap((model) =>
    Array.from({ length: Math.max(1, Math.round((model.requestCount / seed.requestCount) * 100)) }, () => model.model),
  );

  return Array.from({ length: 100 }, (_, rowIndex) => {
    const daysAgo = Math.floor(rowIndex / 8);
    const hour = hours[(rowIndex + accountIndex * 2) % hours.length];
    const minute = (rowIndex * 7 + accountIndex * 11) % 60;
    const ts = new Date(atLocalTime(daysAgo, hour, minute));
    const model = rowIndex < seed.models.length
      ? seed.models[rowIndex].model
      : weightedModels[rowIndex % weightedModels.length];
    const credit = model === "hy3"
      ? 0
      : model === "kimi-k3-1"
        ? kimiCredits[(rowIndex + accountIndex) % kimiCredits.length]
        : model === "glm-5.2"
          ? 33.63
          : model === "deepseek-v4-pro"
            ? proCredits[(rowIndex + accountIndex) % proCredits.length]
            : flashCredits[(rowIndex + accountIndex * 3) % flashCredits.length];
    return {
      accountId: account.id,
      accountName: demoAccountName(account),
      requestId: `demo-request-${String(accountIndex + 1).padStart(2, "0")}-${String(rowIndex + 1).padStart(4, "0")}`,
      credit,
      model,
      client: rowIndex % 50 === 0 ? "CodeBuddyIDE" : "CLI",
      requestTime: `${localDate(daysAgo)} ${String(ts.getHours()).padStart(2, "0")}:${String(ts.getMinutes()).padStart(2, "0")}:00`,
    };
  });
}

function buildStatistics(): CreditStatistics {
  const demoAccounts = hydratedAccounts();
  const accountDaily = usageSeeds.map((seed, index) => distributeModels(seed, index));
  const daily = accountDaily[0].map((_, dayIndex) => {
    const models = new Map<string, CreditOfficialUsageModel>();
    for (const rows of accountDaily) {
      for (const model of rows[dayIndex].models) {
        const current = models.get(model.model) ?? { model: model.model, requestCount: 0, credit: 0 };
        current.requestCount += model.requestCount;
        current.credit = Number((current.credit + model.credit).toFixed(2));
        models.set(model.model, current);
      }
    }
    const modelRows = [...models.values()];
    return { date: accountDaily[0][dayIndex].date, usage: Number(modelRows.reduce((sum, model) => sum + model.credit, 0).toFixed(2)), models: modelRows };
  });
  const sumRecent = (rows: { usage: number }[], count: number) => Number(rows.slice(-count).reduce((sum, row) => sum + row.usage, 0).toFixed(2));
  const monthPrefix = localDate(0).slice(0, 7);
  const sumMonth = (rows: { date: string; usage: number }[]) => Number(rows.filter((row) => row.date.startsWith(monthPrefix)).reduce((sum, row) => sum + row.usage, 0).toFixed(2));
  const generatedAt = Date.now() - 3 * 60 * 1000;
  const creditRows = accountSeeds.map((account) => creditExpiry(account.id));
  const officialAccounts = demoAccounts.map((account, index) => ({
    accountId: account.id,
    accountName: account.nickname ?? account.email ?? account.id,
    ok: true,
    requestCount: usageSeeds[index].requestCount,
    detailTruncated: true,
    usageToday: accountDaily[index][accountDaily[index].length - 1]?.usage ?? 0,
    usage7Days: sumRecent(accountDaily[index], 7),
    usageThisMonth: sumMonth(accountDaily[index]),
    reportedTotal: usageSeeds[index].requestCount,
    fetchedCount: usageSeeds[index].requestCount,
    models: sumModels(accountDaily[index]),
    daily: accountDaily[index],
  }));
  const usageToday = daily[daily.length - 1]?.usage ?? 0;
  const usage7Days = sumRecent(daily, 7);
  const usageThisMonth = sumMonth(daily);
  const totalRemaining = creditRows.reduce((sum, credit) => sum + (credit.totalRemaining ?? 0), 0);
  const totalCapacity = creditRows.reduce((sum, credit) => sum + (credit.totalCapacity ?? 0), 0);

  return {
    generatedAt,
    retentionDays: 90,
    coverageStartAt: atLocalTime(29, 0, 0),
    summary: { currentRemaining: Number(totalRemaining.toFixed(2)), currentCapacity: totalCapacity, usageToday, usage7Days, usageThisMonth, todayCheckedInAccounts: 3, todaySuccess: 2, todayAlready: 1, todayFailed: 0 },
    daily,
    accounts: demoAccounts.map((account, index) => ({
      accountId: account.id,
      accountName: account.nickname ?? account.email ?? account.id,
      isCurrent: index === 0,
      currentRemaining: creditRows[index].totalRemaining ?? null,
      totalCapacity: creditRows[index].totalCapacity ?? null,
      lastSnapshotAt: generatedAt - index * 120_000,
      usageToday: officialAccounts[index].usageToday ?? 0,
      usage7Days: officialAccounts[index].usage7Days ?? 0,
      usageThisMonth: officialAccounts[index].usageThisMonth ?? 0,
      checkedInToday: true,
      checkinStatusToday: index === 1 ? "already" : "success",
      lastCheckinAt: atLocalTime(0, 8, 6 + index * 9),
      lastCheckinResult: index === 1 ? "already" : "success",
      daily: accountDaily[index],
    })),
    events: demoAccounts.map((account, index) => ({ kind: "checkin" as const, ts: atLocalTime(0, 8, 6 + index * 9), date: localDate(0), accountId: account.id, accountName: account.nickname ?? account.email ?? account.id, result: index === 1 ? "already" : "success" })),
    officialUsage: {
      status: "complete",
      rangeStart: localDate(29),
      rangeEnd: localDate(0),
      collectedAt: generatedAt,
      summary: { usageToday, usage7Days, usageThisMonth },
      daily,
      accounts: officialAccounts,
      requests: accountSeeds.flatMap((_, index) => visibleRequests(index)),
      models: sumModels(daily),
      detailLimitPerAccount: 100,
      errors: [],
    },
  };
}

function checkinConfig(): CheckinConfig {
  return { enabled: true, keepalive_days: 7, lazy_refresh_hours: 12 };
}

function travelConfig(): TravelConfig {
  return { enabled: true };
}

function travelStatus(accountId: string): TravelStatus {
  const index = Math.max(0, accountSeeds.findIndex((account) => account.id === accountId));
  // 演示三种状态：旅行中 / 已结束 / 无 Buddy
  if (index % 3 === 0) return { label: "traveling", rewardCredit: 7, locationName: t("shared.demo.travel.cafe"), arriveAt: Math.floor(Date.now() / 1000) + 2 * 3600 + 40 * 60 };
  if (index % 3 === 1) return { label: "finished", rewardCredit: 20, locationName: t("shared.demo.travel.gym") };
  return { label: "no-buddy", rewardCredit: null, locationName: null };
}

function rotateConfig(): AutoRotateConfig {
  return { enabled: true, check_interval_minutes: 15, cooldown_minutes: 120, min_gap_hours: 24, min_urgency_hours: 72, active_guard_minutes: 30, min_remaining_credits: 50 };
}

function checkinLogs(): CheckinLog[] {
  return hydratedAccounts().flatMap((account, accountIndex) => [0, 1, 2].map((daysAgo) => ({ ts: atLocalTime(daysAgo, 8, 6 + accountIndex * 9), accountId: account.id, email: account.nickname ?? account.email ?? account.id, result: accountIndex === 1 && daysAgo === 0 ? "already" : "success" })));
}

function rotateLogs(): RotateLog[] {
  return [
    { ts: atLocalTime(0, 9, 30), action: "skipped", reason: t("shared.demo.rotate.reasonPinned"), from: { id: accountSeeds[0].id, name: t("shared.demo.account.a") }, to: null },
    { ts: atLocalTime(1, 16, 20), action: "switched", reason: t("shared.demo.rotate.reasonExpiring"), from: { id: accountSeeds[1].id, name: t("shared.demo.account.b") }, to: { id: accountSeeds[0].id, name: t("shared.demo.account.a") } },
  ];
}

function demoTokenTotals(input: number, output: number, cacheRead: number, cacheWrite: number, records: number): TokenStatsTotals {
  return { total: input + output + cacheWrite, input, output, cacheRead, cacheWrite, uncachedInput: Math.max(0, input - cacheRead), records, cacheHitRate: input > 0 ? cacheRead / input : null };
}

function demoTokenGroup(key: string, input: number, output: number, cacheRead: number, cacheWrite: number, records: number): TokenStatsGroup {
  return { key, ...demoTokenTotals(input, output, cacheRead, cacheWrite, records) };
}

function demoTokenSession(key: string, title: string, project: string, input: number, output: number, cacheRead: number, cacheWrite: number, records: number): TokenStatsGroup {
  const keyParts = key.split(" · ");
  return { ...demoTokenGroup(key, input, output, cacheRead, cacheWrite, records), title, project, sessionId: keyParts[keyParts.length - 1] };
}

/**
 * 演示会话标题（**存键不存文案**）。
 *
 * 结构：`[会话 key, 标题键, 项目名, 输入, 输出, 缓存读, 缓存写, 记录数]`。
 * 会话 key 与项目名保持英文 —— 它们模拟的是真实的仓库 / 会话标识。
 * 标题的 `t()` 在 `demoTokenSource` 内现取，故切语言时跟着变。
 */
const tokenSessionSeeds: [string, TranslationKey, string, number, number, number, number, number][] = [
  ["buddy-switch-rust · token-stats-dashboard", "shared.demo.session.tokenDashboard", "buddy-switch-rust", 18_700_000, 1_050_000, 16_100_000, 160_000, 72],
  ["my-code-teams · settings-agent-acp", "shared.demo.session.agentAcp", "my-code-teams", 13_200_000, 890_000, 11_300_000, 120_000, 55],
  ["LetterTotTown · character-audio", "shared.demo.session.characterAudio", "LetterTotTown", 8_600_000, 640_000, 7_200_000, 80_000, 38],
  ["buddy-switch-rust · account-card-redesign", "shared.demo.session.accountCard", "buddy-switch-rust", 6_300_000, 410_000, 5_400_000, 50_000, 29],
];

function demoTokenSource(source: TokenStatsSource["source"], scale: number): TokenStatsSource {
  const daily = Array.from({ length: 14 }, (_, index) => {
    const wave = [0.62, 0.86, 1.1, 0.72, 1.3, 0.94, 0.38][index % 7] * scale;
    return demoTokenGroup(localDate(13 - index), Math.round(7_600_000 * wave), Math.round(480_000 * wave), Math.round(6_650_000 * wave), Math.round(95_000 * wave), Math.round(24 * wave));
  });
  const summary = daily.reduce((sum, row) => demoTokenTotals(sum.input + row.input, sum.output + row.output, sum.cacheRead + row.cacheRead, sum.cacheWrite + row.cacheWrite, sum.records + row.records), demoTokenTotals(0, 0, 0, 0, 0));
  const hours = Array.from({ length: 7 * 24 }, (_, index) => {
    const day = Math.floor(index / 24); const hour = index % 24;
    const active = Math.max(0.02, Math.exp(-Math.pow(hour - (day >= 5 ? 22 : 15), 2) / 22));
    return demoTokenGroup(`${day}-${hour}`, Math.round(720_000 * active * scale), Math.round(41_000 * active * scale), Math.round(610_000 * active * scale), 0, Math.max(1, Math.round(6 * active * scale)));
  });
  const projects = [
    demoTokenGroup("buddy-switch-rust", 42_800_000 * scale, 2_400_000 * scale, 37_100_000 * scale, 420_000 * scale, Math.round(148 * scale)),
    demoTokenGroup("my-code-teams", 25_600_000 * scale, 1_650_000 * scale, 21_900_000 * scale, 260_000 * scale, Math.round(96 * scale)),
    demoTokenGroup("LetterTotTown", 11_900_000 * scale, 920_000 * scale, 9_700_000 * scale, 110_000 * scale, Math.round(51 * scale)),
  ];
  const models = [
    demoTokenGroup("deepseek-v4-flash", 56_400_000 * scale, 3_200_000 * scale, 49_100_000 * scale, 530_000 * scale, Math.round(210 * scale)),
    demoTokenGroup("kimi-k3-1", 17_300_000 * scale, 1_140_000 * scale, 14_200_000 * scale, 180_000 * scale, Math.round(61 * scale)),
    demoTokenGroup("glm-5.2", 6_600_000 * scale, 630_000 * scale, 5_400_000 * scale, 80_000 * scale, Math.round(24 * scale)),
  ];
  // 会话标题在**这里**取（`t(key)`），而不是写进模块级常量 —— 常量会在切语言时定格。
  const sessions = tokenSessionSeeds.map(([key, titleKey, project, input, output, cacheRead, cacheWrite, records]) =>
    demoTokenSession(key, t(titleKey), project, input * scale, output * scale, cacheRead * scale, cacheWrite * scale, Math.round(records * scale)),
  );
  const now = Date.now();
  return { source, summary, models, projects, sessions, daily, hours, filesScanned: source === "workbuddy" ? 63 : source === "codebuddy-ide" ? 17 : 41, parseErrors: 0, coverageStartAt: now - 13 * 86_400_000, coverageEndAt: now };
}

function demoTokenStatistics(days?: number): TokenStatistics {
  return { generatedAt: Date.now(), rangeDays: days ?? null, sources: [demoTokenSource("workbuddy", 1), demoTokenSource("codebuddy-cli", 0.58), demoTokenSource("codebuddy-ide", 0.36)] };
}

// ---------------------------------------------------------------------------
// API 网关（演示数据，只读）
// ---------------------------------------------------------------------------

function demoRegionStatus(region: Region): AppStatus {
  const demoAccounts = hydratedAccounts();
  const isGlobal = region === "global";
  const account = isGlobal ? demoAccounts[1] : demoAccounts[0];
  return {
    running: true,
    authFile: isGlobal
      ? "/demo/WorkBuddy AI/auth/workbuddy-desktop-ai.info"
      : "/demo/WorkBuddy/auth/workbuddy-desktop.info",
    current: { uid: account.uid, nickname: account.nickname, email: account.email },
    appPath: isGlobal ? "/demo/WorkBuddy AI.app" : "/demo/WorkBuddy.app",
    version: "2026.9.16",
    installed: true,
    regionMismatch: null,
    region,
  };
}

function demoGatewayConfig(): GatewayConfig {
  return {
    enabled: true,
    bind_addr: "127.0.0.1",
    port: 57891,
    allow_non_loopback: false,
    log_keep: 200,
    log_bodies: false,
    per_key_rate_limit: null,
  };
}

function demoGatewayStatus(): GatewayStatus {
  return {
    enabled: true,
    running: true,
    addr: "127.0.0.1",
    port: 57891,
    allowNonLoopback: false,
    baseUrl: "http://127.0.0.1:57891/v1",
    error: null,
  };
}

function demoApiKeys(): ApiKeyRecord[] {
  return [
    { id: "demo-key-1", name: "Cursor", region: "cn", prefix: "sk-wb-a1b2", createdAt: atLocalTime(3, 17, 3), revokedAt: null, revoked: false, lastUsedAt: atLocalTime(0, 17, 10) },
    { id: "demo-key-2", name: "Claude Code", region: "global", prefix: "sk-wb-c3d4", createdAt: atLocalTime(3, 17, 5), revokedAt: null, revoked: false, lastUsedAt: atLocalTime(0, 16, 41) },
    { id: "demo-key-3", name: "OpenWebUI", region: "cn", prefix: "sk-wb-e5f6", createdAt: atLocalTime(4, 10, 0), revokedAt: atLocalTime(1, 12, 30), revoked: true, lastUsedAt: atLocalTime(2, 9, 12) },
  ];
}

function demoCatalog(region: Region): CatalogSnapshot {
  const models = region === "global"
    ? ["GPT-5.6", "Claude-Sonnet-4.5", "Gemini-3-Pro", "GLM-5.3", "DeepSeek-V4-Pro"]
    : ["GLM-5.3", "GLM-5.2", "DeepSeek-V4-Pro", "Kimi-K3", "hy3"];
  return {
    region,
    source: region === "global" ? "cached" : "live",
    fetched_at: Date.now() - 3 * 60 * 1000,
    models: models.map((name, index) => ({
      id: name.toLowerCase(),
      name,
      context_window: 131072,
      max_tokens: 8192,
      supports_images: index % 3 === 0,
      credits: index % 4 === 0 ? t("shared.demo.catalog.limitedFree") : null,
      badges: index === 0 ? [t("shared.demo.catalog.promo")] : [],
      free: index % 4 === 0,
    })),
    note: region === "global" ? t("shared.demo.catalog.globalStale") : null,
  };
}

function demoStrategyMap(): AccountStrategyMap {
  const demoAccounts = withDemoNames(hydratedAccounts());
  return {
    cn: { region: "cn", strategy: { kind: "current" }, selected: demoAccounts[0] },
    global: { region: "global", strategy: { kind: "max_credits" }, selected: null, note: t("shared.demo.strategy.realtime") },
  };
}

function demoGatewayLogs(): GatewayLogEntry[] {
  return [
    { ts: atLocalTime(0, 17, 10), endpoint: "/v1/chat/completions", method: "POST", region: "cn", account: t("shared.demo.account.a"), model: "GLM-5.3", status: 200, latencyMs: 1200, promptTokens: 800, completionTokens: 434, stream: true },
    { ts: atLocalTime(0, 17, 9), endpoint: "/v1/messages", method: "POST", region: "global", account: t("shared.demo.account.b"), model: "GPT-5.6", status: 402, latencyMs: 800, promptTokens: null, completionTokens: null, stream: true },
    { ts: atLocalTime(0, 17, 7), endpoint: "/v1/chat/completions", method: "POST", region: "cn", account: t("shared.demo.account.a"), model: "DeepSeek-V4-Pro", status: 200, latencyMs: 2400, promptTokens: 2100, completionTokens: 900, stream: true },
    { ts: atLocalTime(0, 17, 2), endpoint: "/v1/messages", method: "POST", region: "global", account: t("shared.demo.account.b"), model: "Claude-Sonnet-4.5", status: 429, latencyMs: 300, promptTokens: null, completionTokens: null, stream: true },
  ];
}

// ---------------------------------------------------------------------------
// Trae 分区（演示数据，只读）
// ---------------------------------------------------------------------------
//
// 形状刻意与 `webui` 的 HTTP 响应**逐字段一致**（含 snake_case 的网关字段）：
// 前端页面在两种模式下走同一套归一化逻辑，形状不一致会让「演示站好看、真实站崩」
// 这类差异在打包后才暴露。数值与 `.qa-tmp/mock_api.py` 对齐，两种模式的截图可比。

/**
 * 演示账号的**用户名 / 分组名 / 套餐名 / 冷却原因**一律走 `t()`。
 *
 * ⚠️ `userId`（`7481920`）是**身份**，中英共用、永不翻译 —— 它是几个假数据块
 * （账号列表、签到结果、登录态快照、网关账号池）**互相指向**的键，翻译了就会
 * 让「名字与账号列表对不上」。同理 `cooldownType`（`SoftRate`/`SessionDead`）
 * 是协议值，只有 `cooldownReason` 是给人看的文案。
 */
function traeDemoAccounts(): TraeAccount[] {
  return [
    {
      userId: "7481920", name: t("shared.demo.trae.name.main"), groupId: null, jwt: "", jwtExpHours: 320.5,
      jwtExpTimestamp: atLocalTime(0, 9, 12), jwtStatus: "ok", checkedToday: true,
      credits: 120, remainingCredits: 120, creditsExpireAt: futureAt(26),
      creditPackages: [
        { packageCode: "pkg_work_month", packageName: t("shared.demo.trae.pack.month"), total: 100, remaining: 80, used: 20, expireAt: Math.floor(futureAt(26) / 1000), expired: false, expiringSoon: false },
        { packageCode: "pkg_work_bonus", packageName: t("shared.demo.trae.pack.checkinBonus"), total: 40, remaining: 40, used: 0, expireAt: Math.floor(futureAt(4) / 1000), expired: false, expiringSoon: true },
      ],
      deviceIdMasked: "a1b2…9f", cooldownType: null, cooldownUntil: null, cooldownReason: null,
      hasRefreshToken: true, jwtAutoRefresh: true,
      addedAt: "2026-09-10T02:11:00Z", updatedAt: "2026-09-17T01:54:00Z",
    },
    {
      userId: "7481999", name: t("shared.demo.trae.name.altA"), groupId: "g1", jwt: "", jwtExpHours: 6.2,
      jwtExpTimestamp: atLocalTime(0, 15, 20), jwtStatus: "warn", checkedToday: false,
      credits: 0, remainingCredits: 64.5, creditsExpireAt: futureAt(5),
      creditPackages: [
        { packageCode: "pkg_cn_plan", packageName: t("shared.demo.trae.pack.cnPlan"), total: 100, remaining: 64.5, used: 35.5, expireAt: Math.floor(futureAt(5) / 1000), expired: false, expiringSoon: true },
      ],
      deviceIdMasked: "c3d4…7e", cooldownType: "SoftRate",
      cooldownUntil: Math.floor(Date.now() / 1000) + 5400,
      cooldownReason: t("shared.demo.trae.cooldown.rateLimited"),
      hasRefreshToken: false, jwtAutoRefresh: false,
      addedAt: "2026-09-12T08:00:00Z", updatedAt: "2026-09-16T22:10:00Z",
    },
    {
      userId: "7482044", name: t("shared.demo.trae.name.altB"), groupId: "g1", jwt: "", jwtExpHours: -3,
      jwtExpTimestamp: atLocalTime(0, 5, 30), jwtStatus: "expired", checkedToday: false,
      credits: 8, remainingCredits: 8, creditsExpireAt: futureAt(3),
      creditPackages: [
        { packageCode: "pkg_trial", packageName: t("shared.demo.trae.pack.trial"), total: 8, remaining: 8, used: 0, expireAt: Math.floor(futureAt(3) / 1000), expired: false, expiringSoon: true },
      ],
      deviceIdMasked: "e5f6…1a", cooldownType: "SessionDead", cooldownUntil: 9_999_999_999,
      cooldownReason: t("shared.demo.trae.cooldown.sessionDead"),
      hasRefreshToken: true, jwtAutoRefresh: false,
      addedAt: "2026-09-14T09:30:00Z", updatedAt: "2026-09-17T00:40:00Z",
    },
  ];
}

/** 签到结果（演示）。`message` 是给人看的，`errorType` / `action` 是协议值。 */
function traeDemoCheckinResults() {
  return [
    { name: t("shared.demo.trae.name.main"), userId: "7481920", ok: true, code: 200, message: t("shared.demo.trae.checkin.ok"), action: "claim", credits: 120, delta: 20, errorType: null, cooldownUntil: null },
    { name: t("shared.demo.trae.name.altA"), userId: "7481999", ok: true, code: 200, message: t("shared.demo.trae.checkin.already"), action: "skip_already", credits: 64.5, delta: 0, errorType: null, cooldownUntil: null },
    { name: t("shared.demo.trae.name.altB"), userId: "7482044", ok: false, code: 401, message: t("shared.demo.trae.checkin.sessionDead"), action: "claim", credits: null, delta: 0, errorType: "SessionDead", cooldownUntil: 9_999_999_999 },
  ];
}

function demoTraeEnv(): TraeEnvStatus {
  return {
    installed: true, running: true, version: "1.107.1",
    // 演示数据刻意用「装在非系统盘」的真实形态：这正是自动探测要覆盖的场景，
    // 也让「同机多产品线」的能力在截图里可见。平台是 win32，路径就用 Windows 形态。
    path: "D:\\Programs\\TRAE SOLO CN\\TRAE SOLO CN.exe",
    dataDir: "C:\\Users\\demo\\AppData\\Roaming\\TRAE SOLO CN",
    dataDirExists: true, platform: "win32", configuredPath: null,
    // 变体字段必须与真实 HTTP 响应同形状：fixture 少一个键，演示站就看不到
    // 产品线标签，而这个差异只会在真机暴露。
    variant: "trae_work", variantLabel: t("trae.program.traeWork"),
  };
}

/**
 * 演示数据：**两条** Trae 产品线并排。
 *
 * 刻意让两条线的状态不同（一条运行中、一条未运行），这样截图/演示站上
 * 「并排两个图标各自独立」这件事才看得出来 —— 两条都同状态的话，
 * 分不清是「两条独立探测」还是「同一条画了两遍」。
 */
/**
 * 演示用的**区域 + 程序位**状态。
 *
 * 形状自 2026-09-21 起改为「按区域列条目、条目内含程序位」：区域才是账号体系的分界，
 * 程序位只决定客户端。演示数据刻意让两个区域状态不同（国内装齐两条程序、
 * 国际只装了 TraeWork），这样截图/演示站上能看出**区域与程序是两层**，
 * 而不是「同一条线画了两遍」。
 */
function demoTraeVariants(): TraeVariantsStatus {
  /** 程序位展示名走词表（`shared.demo.trae.program.*`）；`variant` 是回传后端的标识，保持英文。 */
  const program = (
    program: "trae_work" | "trae_code",
    labelKey: TranslationKey,
    nameAliasKey: TranslationKey,
    variant: "trae_work" | "trae_cn" | "global" | null,
    installed: boolean,
    running: boolean,
    path: string,
    dataDir: string,
  ) => ({
    program,
    label: t(labelKey),
    nameAlias: t(nameAliasKey),
    variant,
    installed,
    running,
    version: installed ? "1.107.1" : null,
    path: installed ? path : null,
    dataDir: installed ? dataDir : null,
    dataDirExists: installed,
  });

  return {
    platform: "win32",
    variants: [
      {
        variant: "cn",
        variantLabel: t("shared.region.version.cn"),
        consoleBase: "https://www.trae.cn",
        installed: true,
        running: true,
        version: "1.107.1",
        path: "D:\\Programs\\TRAE SOLO CN\\TRAE SOLO CN.exe",
        dataDir: "C:\\Users\\demo\\AppData\\Roaming\\TRAE SOLO CN",
        dataDirExists: true,
        programs: [
          program(
            "trae_work",
            "shared.demo.trae.program.work",
            "shared.demo.trae.program.workCn",
            "trae_work",
            true,
            true,
            "D:\\Programs\\TRAE SOLO CN\\TRAE SOLO CN.exe",
            "C:\\Users\\demo\\AppData\\Roaming\\TRAE SOLO CN",
          ),
          program(
            "trae_code",
            "shared.demo.trae.program.code",
            "shared.demo.trae.program.codeCn",
            "trae_cn",
            true,
            false,
            "D:\\Programs\\Trae CN\\Trae CN.exe",
            "C:\\Users\\demo\\AppData\\Roaming\\Trae CN",
          ),
        ],
      },
      {
        variant: "global",
        variantLabel: t("shared.region.version.global"),
        consoleBase: "https://www.trae.ai",
        installed: true,
        running: false,
        version: "1.107.1",
        path: "C:\\Users\\demo\\AppData\\Local\\Programs\\TRAE SOLO\\TRAE SOLO.exe",
        dataDir: "C:\\Users\\demo\\AppData\\Roaming\\TRAE SOLO",
        dataDirExists: true,
        programs: [
          program(
            "trae_work",
            "shared.demo.trae.program.workAi",
            "shared.demo.trae.program.workGlobal",
            "global",
            true,
            false,
            "C:\\Users\\demo\\AppData\\Local\\Programs\\TRAE SOLO\\TRAE SOLO.exe",
            "C:\\Users\\demo\\AppData\\Roaming\\TRAE SOLO",
          ),
          // 国际版 TraeCode 未安装也**未建模** ⇒ `variant: null`（按钮必须禁用，
          // 不能拿同区域另一个客户端的标识顶替）。
          program("trae_code", "shared.demo.trae.program.codeAi", "shared.demo.trae.program.codePending", null, false, false, "", ""),
        ],
      },
    ],
  };
}

/**
 * 「平台做不到的维度」（置灰卡）的**键名表**。
 *
 * 形状与 Rust `handlers::unsupported_note` 逐字一致：`supportedOn` 里的
 * `"WorkBuddy"` / `"—"` 是**产品标识与占位符**（不是文案），故保持原样；
 * 只有 `label` / `reason` 是给人看的，走词表。
 *
 * ⚠️ 用 `as const` 让 `label`/`reason` 保留字面量类型 —— `TranslationKey` 才能校验。
 */
const CAPABILITY_STUBS = {
  capabilities: [
    { capability: "auto_travel", label: "shared.demo.cap.travel", supportedOn: "WorkBuddy", reason: "shared.demo.cap.travelReason" },
    { capability: "codebuddy_cli", label: "shared.demo.cap.cli", supportedOn: "WorkBuddy", reason: "shared.demo.cap.cliReason" },
    { capability: "account_data_migration", label: "shared.demo.cap.migration", supportedOn: "WorkBuddy", reason: "shared.demo.cap.migrationReason" },
    { capability: "session_tree", label: "shared.demo.cap.sessionTree", supportedOn: "WorkBuddy", reason: "shared.demo.cap.sessionTreeReason" },
  ],
  credits: [
    { capability: "official_credit_by_model", label: "shared.demo.cap.officialByModel", supportedOn: "—", reason: "shared.demo.cap.officialByModelReason" },
  ],
  tokenStats: [
    { capability: "cache_metrics", label: "shared.demo.cap.cacheMetrics", supportedOn: "—", reason: "shared.demo.cap.cacheMetricsReason" },
    { capability: "project_dimension", label: "shared.demo.cap.projectDimension", supportedOn: "—", reason: "shared.demo.cap.projectDimensionReason" },
    { capability: "session_cost", label: "shared.demo.cap.sessionCost", supportedOn: "—", reason: "shared.demo.cap.sessionCostReason" },
  ],
} as const;

/** 把键名表化成实际文案。**每次读取都重算** —— 语言切换后才不会停在旧语言。 */
function localizedStubs(stubs: readonly { capability: string; label: TranslationKey; supportedOn: string; reason: TranslationKey }[]) {
  return stubs.map((stub) => ({ ...stub, label: t(stub.label), reason: t(stub.reason) }));
}

function demoTraeCapabilities(): TraeCapabilities {
  return {
    platform: "win32", processControl: true, clientDetection: true,
    userDataDir: "C:\\Users\\demo\\AppData\\Roaming\\TRAE SOLO CN",
    machineGuidReset: true, scheduledTask: true,
    // 产品级不支持项（WorkBuddy 有、Trae 无），形状与 `platform::Unsupported` 逐字一致。
    unsupported: localizedStubs(CAPABILITY_STUBS.capabilities),
  };
}

function demoTraeAccounts(): unknown {
  const list = traeDemoAccounts();
  return {
    accounts: list,
    groups: [{ id: "g1", name: t("shared.demo.trae.group.spare"), color: "#888", order: 0, count: 2 }],
    total: list.length, cooling: 1, ungrouped: 1,
  };
}

function demoTraeCheckinStatus(): TraeCheckinStatus {
  return {
    summary: {
      time: `${localDate(0)}T01:00:00+08:00`,
      results: traeDemoCheckinResults(),
      totalOk: 1, already: 1, failed: 1,
      warnings: [t("shared.demo.trae.warn.altBSessionDead")],
    },
    summaryIsToday: true,
    cooldowns: [
      { userId: "7481999", type: "SoftRate", until: Math.floor(Date.now() / 1000) + 5400, reason: t("shared.demo.trae.cooldown.reasonRateLimited"), permanent: false },
      { userId: "7482044", type: "SessionDead", until: 9_999_999_999, reason: t("shared.demo.trae.cooldown.reasonSessionDead"), permanent: true },
    ],
    cooldownCount: 2,
    logFile: "/demo/buddy-switch/trae/logs/checkin.log",
  };
}

function demoTraeCredits(): TraeCreditsOverview {
  const list = traeDemoAccounts();
  const daily = [188, 172.5, 180, 165.25, 158, 148.5, 140, 192.5].map((total, index, all) => ({
    date: localDate(all.length - 1 - index),
    total,
    earned: [0, 0, 12, 0, 0, 0, 0, 20][index],
    consumed: [0, 15.5, 4.5, 14.75, 7.25, 9.5, 8.5, 0][index],
  }));
  return {
    remaining: { "7481920": 120, "7481999": 64.5, "7482044": 8 },
    expireTimes: {
      "7481920": Math.floor(futureAt(26) / 1000),
      "7481999": Math.floor(futureAt(5) / 1000),
      "7482044": Math.floor(futureAt(3) / 1000),
    },
    // 逐包明细与账号卡共用同一份假数据（`traeDemoAccounts[..].creditPackages`）。
    packages: {
      "7481920": list[0].creditPackages ?? [],
      "7481999": list[1].creditPackages ?? [],
      "7482044": list[2].creditPackages ?? [],
    },
    updatedAt: `${localDate(0)}T09:12:00+08:00`,
    balances: list.map((account) => ({
      userId: account.userId,
      credits: account.credits ?? 0,
      date: localDate(0),
    })),
    records: [
      { date: localDate(0), userId: "7481920", credits: 120, delta: 20 },
      { date: localDate(0), userId: "7481999", credits: 64, delta: 0 },
    ],
    daily,
    todayEarned: 20,
    historyDays: 8,
    // 「官方积分消耗按模型」在 Trae 侧无数据源——形状与 `handlers::unsupported_note` 逐字一致。
    unsupported: localizedStubs(CAPABILITY_STUBS.credits),
  };
}

/**
 * 演示用的登录态快照总览（**按程序位分家**）。
 *
 * 快照是**客户端级**的（只能恢复到采集它的那个客户端），因此这里按**程序位**给数据：
 * 国内 TraeWork / 国内 TraeCode / 国际版 TraeWork 三者槽位与目录都不同 ——
 * 截图上必须能一眼看出「这是各自独立的快照」，而不是同一份数据被渲染了两次。
 *
 * 入参 `variant` 兼容**区域标识**（`cn` / `global`，页面传的就是它）与**程序位标识**
 * （`trae_work` / `trae_cn`）：前者落到该区域的**主程序**（TraeWork），
 * 与 Rust 侧 `TraeVariant::parse` 的映射保持一致。
 */
function demoTraeProfiles(args?: Record<string, unknown>): TraeProfilesOverview {
  const raw = typeof args?.variant === "string" ? args.variant : "cn";
  if (raw === "global") {
    return {
      profiles: [
        { slot: "7481920", sizeBytes: 3_180_000, fileCount: 9, lastModified: `${localDate(2)} 09:31`, sizeText: "3.0 MB" },
      ],
      currentAccount: "7481920",
      // 与 `traeAccounts` 里同 uid 的记录同名（`7481920` = 「主号」）：
      // 演示数据也必须**自洽**，否则截图会被当成「名字与账号列表对不上」的缺陷。
      currentAccountName: t("shared.demo.trae.name.main"),
      dataDir: "C:\\Users\\demo\\AppData\\Roaming\\TRAE SOLO",
      clientRunning: false,
      coreEntryCount: 9,
    };
  }
  const isCn = raw === "trae_cn";
  if (isCn) {
    return {
      profiles: [
        { slot: "9201733", sizeBytes: 2_610_000, fileCount: 9, lastModified: `${localDate(1)} 14:05`, sizeText: "2.5 MB" },
      ],
      currentAccount: "9201733",
      // 刻意给 `null`：这个 uid 不在 `traeAccounts` 里，正好演示
      // 「客户端登录着一个库里没有的账号」这一**正常状态**——界面回落到 uid，
      // 而不是把它渲染成「未知账号」（那会抹掉唯一可核对的线索）。
      currentAccountName: null,
      dataDir: "C:\\Users\\demo\\AppData\\Roaming\\Trae CN",
      clientRunning: false,
      coreEntryCount: 9,
    };
  }
  return {
    profiles: [
      { slot: "7481920", sizeBytes: 4_820_000, fileCount: 9, lastModified: `${localDate(0)} 09:12`, sizeText: "4.6 MB" },
      { slot: "7481999", sizeBytes: 3_140_000, fileCount: 9, lastModified: `${localDate(2)} 18:40`, sizeText: "3.0 MB" },
    ],
    currentAccount: "7481920",
    currentAccountName: t("shared.demo.trae.name.main"),
    dataDir: "C:\\Users\\demo\\AppData\\Roaming\\TRAE SOLO CN",
    clientRunning: true,
    coreEntryCount: 9,
  };
}

function demoTraeSettings(): TraeSettings {
  return {
    proxyPort: 8899, theme: "dark", launchMinimized: false, autoStartProxy: true, tray: true,
    language: "zh-CN", checkinSkipChecked: true, checkinSkipExpired: true, retry: 2,
    notify: "system", traePath: null, browserPath: null, logRetentionDays: 7,
    proxyDomains: "", apiPort: 7864, apiKey: "sk-trae-9f2c1a4b6d8e0f3a5b7c9d1e2f4a6b8c",
    apiDefaultModel: "deepseek-v4-flash",
  };
}

/**
 * Trae 网关配置。
 *
 * 返回 **snake_case 原始形状**（`TraeGatewayConfigRaw`）而不是前端的 camelCase 形状：
 * 演示模式必须与 HTTP 通道返回同一种形状，页面才会走同一套 `normalizeTraeGatewayConfig`。
 * 用 camelCase 的话演示站能跑、真实站要等归一化才生效，差异只在真机暴露。
 */
function demoTraeGatewayConfig(): TraeGatewayConfigRaw {
  return {
    enabled: true, bind_addr: "127.0.0.1", port: 7864, allow_non_loopback: false,
    log_keep: 200, log_bodies: false, max_body_mb: 8,
    default_model: "deepseek-v4-flash", max_rotate: 3,
  };
}

/** Trae 网关状态（**snake_case**，含账号池与诊断）。 */
function demoTraeGatewayStatus(): unknown {
  return {
    enabled: true, running: true, addr: "127.0.0.1:7864",
    base_url: "http://127.0.0.1:7864", bind_addr: "127.0.0.1", port: 7864,
    allow_non_loopback: false, version: "2026.9.17", total_requests: 137,
    last_error: null, api_key_prefix: "sk-trae-9f2c1a4b…7d31",
    pool: { total: 3, available: 1, cooling: 1, disabled: 1, expired: 0, zero_credits: 0, total_credits: 192.5 },
    accounts: [
      { uid: "7481920", name: t("shared.demo.trae.name.main"), status: "available", credits: 120, creditsExpireAt: Math.floor(futureAt(26) / 1000), cooling: false, cooldownUntil: null, cooldownReason: null, disabled: false, deviceIdMasked: "a1b2…9f0e" },
      { uid: "7481999", name: t("shared.demo.trae.name.altA"), status: "cooling", credits: 64.5, creditsExpireAt: Math.floor(futureAt(5) / 1000), cooling: true, cooldownUntil: Math.floor(Date.now() / 1000) + 5400, cooldownReason: t("shared.demo.trae.cooldown.reasonRateLimited"), disabled: false, deviceIdMasked: "c3d4…1122" },
      { uid: "7482044", name: t("shared.demo.trae.name.altB"), status: "disabled", credits: 8, creditsExpireAt: null, cooling: false, cooldownUntil: 9_999_999_999, cooldownReason: t("shared.demo.trae.cooldown.reasonSessionDead"), disabled: true, deviceIdMasked: "e5f6…3344" },
    ],
    diagnose: [
      t("shared.demo.trae.diagnose.main"),
      t("shared.demo.trae.diagnose.altA"),
      t("shared.demo.trae.diagnose.altB"),
    ],
    upstream: "https://trae-api-cn.mchost.guru",
  };
}

const TRAE_GATEWAY_MODEL_NAMES = [
  "doubao-seed-2.1-pro", "doubao-seed-2.1-turbo", "doubao-seed-2.0-code",
  "deepseek-v4-flash", "deepseek-v4-pro", "glm-5.2", "glm-5.3", "glm-5-turbo",
  "glm-5", "kimi-k2.7-code", "kimi-k3", "kimi-k2.6", "minimax-m3",
  "qwen-3.7-plus", "sagitta", "aquila",
];

function demoTraeGatewayModels(): unknown {
  return {
    object: "list",
    data: TRAE_GATEWAY_MODEL_NAMES.map((name) => ({
      id: name, object: "model", created: 1753600000, owned_by: "trae",
    })),
  };
}

/**
 * 演示用的 Trae 多 Key 列表（含**两个区域**各一把 + 一条历史归属）。
 *
 * 刻意给出不同 `variant`（`cn` / `global`；第三条保留改造前的 `trae_work`）与一条已吊销：
 * 截图要能看出「归属版本」列与「状态」列的差异，否则演示站会掩盖归属列的存在。
 * 第三条同时是**历史数据**的样本 —— 升级前建的 Key 存的是**产品线**标识，而那个产品线
 * 属于国内区域，因此它与第一条在界面上**必须都显示「国内版」**（走 `traeRegionLabelOf`）。
 * 若哪天有人把归属列改回按标识自身取名，这一行会显示成「TraeWork」，与第一行并列成
 * 一处**假差异** —— 那时这张演示图就是回归证据。
 * 明文 / hash 一律不出现（与真实 `list_response` 的脱敏白名单一致）。
 */
function demoTraeApiKeys(): { keys: TraeApiKeyRecord[] } {
  return {
    keys: [
      { id: "demo-trae-key-1", name: t("shared.demo.trae.key.cursorCn"), variant: "cn", prefix: "sk-trae-9f2c", createdAt: atLocalTime(3, 17, 3), revokedAt: null, revoked: false, lastUsedAt: atLocalTime(0, 17, 10) },
      { id: "demo-trae-key-2", name: t("shared.demo.trae.key.cherryGlobal"), variant: "global", prefix: "sk-trae-4b7e", createdAt: atLocalTime(2, 9, 40), revokedAt: null, revoked: false, lastUsedAt: atLocalTime(0, 16, 41) },
      { id: "demo-trae-key-3", name: t("shared.demo.trae.key.legacy"), variant: "trae_work", prefix: "sk-trae-0a1b…cdef", createdAt: atLocalTime(9, 8, 0), revokedAt: atLocalTime(1, 12, 30), revoked: true, lastUsedAt: atLocalTime(3, 10, 5) },
    ],
  };
}

function demoTraeGatewayLogs(): unknown {
  const now = Math.floor(Date.now() / 1000);
  const logs: TraeGatewayLogEntry[] = [
    { ts: (now - 60) * 1000, endpoint: "/v1/chat/completions", method: "POST", account: "7481920", model: "deepseek-v4-flash", status: 200, latencyMs: 1840, promptTokens: 1204, completionTokens: 386, stream: true, error: null },
    { ts: (now - 900) * 1000, endpoint: "/v1/chat/completions", method: "POST", account: "7481963", model: "glm-5.3", status: 429, latencyMs: 220, promptTokens: 0, completionTokens: 0, stream: true, error: t("shared.demo.trae.gatewayUpstreamError", { name: t("shared.demo.trae.name.altA"), status: 429 }) },
    { ts: (now - 3600) * 1000, endpoint: "/v1/chat/completions", method: "POST", account: "7481920", model: "glm-5.3", status: 200, latencyMs: 3120, promptTokens: 4021, completionTokens: 1188, stream: false, error: null },
  ];
  return { logs };
}

/** Trae Token 统计（源 = 网关请求日志）。 */
function demoTraeTokenStatistics(days?: number): TraeTokenStatistics {
  const totals = [0, 0, 0, 0, 1860, 4210, 3120, 6791];
  const inputs = [0, 0, 0, 0, 1500, 3400, 4021, 5225];
  const outputs = [0, 0, 0, 0, 360, 810, 1188, 1566];
  const daily = totals.map((total, index) => ({
    key: localDate(totals.length - 1 - index),
    total, input: inputs[index], output: outputs[index],
    records: total === 0 ? 0 : index,
    errors: index < 5 ? 0 : 1,
    streamRequests: index,
    avgLatencyMs: total === 0 ? 0 : 900 + index * 120,
    p95LatencyMs: total === 0 ? 0 : 1800 + index * 200,
  }));
  return {
    source: "trae-gateway", label: t("shared.demo.trae.tokenSource"),
    generatedAt: Date.now(), rangeDays: days ?? 30,
    logFile: "/demo/buddy-switch/trae/api_gateway_logs.json",
    summary: { total: 6791, input: 5225, output: 1566, records: 3, errors: 1, streamRequests: 2, avgLatencyMs: 1727, p95LatencyMs: 3120 },
    models: [
      { key: "deepseek-v4-flash", total: 1590, input: 1204, output: 386, records: 1, errors: 0, streamRequests: 1, avgLatencyMs: 1840, p95LatencyMs: 1840 },
      { key: "glm-5.3", total: 5209, input: 4021, output: 1188, records: 2, errors: 1, streamRequests: 1, avgLatencyMs: 1670, p95LatencyMs: 3120 },
    ],
    accounts: [
      { key: "7481920", name: t("shared.demo.trae.name.main"), shortId: "7481920", total: 6791, input: 5225, output: 1566, records: 2, errors: 0, streamRequests: 2, avgLatencyMs: 2480, p95LatencyMs: 3120 },
      { key: "7481963", name: t("shared.demo.trae.name.altA"), shortId: "7481963", total: 0, input: 0, output: 0, records: 1, errors: 1, streamRequests: 1, avgLatencyMs: 220, p95LatencyMs: 220 },
    ],
    daily,
    hours: Array.from({ length: 24 }, (_, hour) => ({
      key: String(hour).padStart(2, "0"),
      total: hour === 10 ? 6791 : 0,
      input: hour === 10 ? 5225 : 0,
      output: hour === 10 ? 1566 : 0,
      records: hour === 10 ? 3 : 0,
      errors: hour === 10 ? 1 : 0,
      streamRequests: hour === 10 ? 2 : 0,
      avgLatencyMs: hour === 10 ? 1727 : 0,
      p95LatencyMs: hour === 10 ? 3120 : 0,
    })),
    statuses: [{ key: "200", records: 2 }, { key: "429", records: 1 }],
    // 按天 × 模型的堆叠柱数据（两模型与下方两变体对应）。
    modelDaily: [
      { date: localDate(2), model: "deepseek-v4-flash", total: 1860, input: 1500, output: 360, records: 1 },
      { date: localDate(2), model: "glm-5.3", total: 4210, input: 3400, output: 810, records: 1 },
      { date: localDate(1), model: "deepseek-v4-flash", total: 3120, input: 4021, output: 1188, records: 1 },
      { date: localDate(0), model: "glm-5.3", total: 6791, input: 5225, output: 1566, records: 1 },
    ],
    // **区域**范围条各档计数（含「未标注」= 升级前的旧日志）。
    // demo 刻意同时给两个区域数据；国内那一档是两条程序位的**合计**。
    variantCounts: { cn: 3, global: 1, unlabeled: 1, all: 5 },
    // 平台做不到的维度（置灰卡）——形状与 `handlers::unsupported_note` 逐字一致。
    unsupported: localizedStubs(CAPABILITY_STUBS.tokenStats),
    filesScanned: 1, parseErrors: 0,
    coverageStartAt: Date.now() - 3600_000, coverageEndAt: Date.now(),
    note: t("shared.demo.trae.tokenNote"),
  };
}

/**
 * 运行日志（演示数据）。
 *
 * 文案逐字取自 `store::append_log` 的真实调用点，这样演示站看到的行
 * 与真实运行时的行是同一种形状，视觉核对才有意义。
 */
function demoTraeLogs(args?: Record<string, unknown>): TraeLogsResponse {
  const day0 = localDate(0);
  const day1 = localDate(1);
  const day2 = localDate(2);
  const stamp = (daysAgo: number, hour: number, minute: number) => {
    const date = new Date(atLocalTime(daysAgo, hour, minute));
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")} ${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}:${String(date.getSeconds()).padStart(2, "0")}`;
  };
  const all = [
    { kind: "checkin" as const, time: stamp(0, 8, 6), date: day0, message: t("shared.demo.trae.log.checkinDone", { ok: 1, already: 1, failed: 1, total: 3 }) },
    { kind: "app" as const, time: stamp(0, 8, 41), date: day0, message: t("shared.demo.trae.log.thawed", { uid: "7481999", credits: 64.5 }) },
    { kind: "switch" as const, time: stamp(0, 9, 12), date: day0, message: t("shared.demo.trae.log.savedLogin", { uid: "7481920", count: 9 }) },
    { kind: "checkin" as const, time: stamp(0, 9, 20), date: day0, message: t("shared.demo.trae.log.jwtRefreshed", { uid: "7481920", hours: 320.5 }) },
    { kind: "app" as const, time: stamp(0, 10, 3), date: day0, message: t("shared.demo.trae.log.deviceReset", { count: 6 }) },
    { kind: "checkin" as const, time: stamp(1, 8, 5), date: day1, message: t("shared.demo.trae.log.checkinDone", { ok: 3, already: 0, failed: 0, total: 3 }) },
    { kind: "switch" as const, time: stamp(1, 16, 20), date: day1, message: t("shared.demo.trae.log.savedLogin", { uid: "7481999", count: 9 }) },
    { kind: "app" as const, time: stamp(2, 22, 10), date: day2, message: t("shared.demo.trae.log.thawed", { uid: "7482044", credits: 8 }) },
  ];

  const kind = typeof args?.kind === "string" ? args.kind : "";
  const date = typeof args?.date === "string" ? args.date : "";
  const keyword = typeof args?.keyword === "string" ? args.keyword.toLowerCase() : "";
  const entries = all.filter((entry) => {
    if (kind && kind !== "all" && entry.kind !== kind) return false;
    if (date && entry.date !== date) return false;
    if (keyword && !entry.message.toLowerCase().includes(keyword)) return false;
    return true;
  });

  const dates = [...new Set(all.map((entry) => entry.date))].sort().reverse();
  return {
    entries,
    total: entries.length,
    limit: 500,
    dates,
    counts: {
      all: all.length,
      app: all.filter((entry) => entry.kind === "app").length,
      checkin: all.filter((entry) => entry.kind === "checkin").length,
      switch: all.filter((entry) => entry.kind === "switch").length,
    },
    sources: [
      { kind: "app", label: t("shared.demo.trae.log.app"), path: "/demo/buddy-switch/trae/logs/app.log", exists: true },
      { kind: "checkin", label: t("shared.demo.trae.log.checkin"), path: "/demo/buddy-switch/trae/logs/checkin.log", exists: true },
      { kind: "switch", label: t("shared.demo.trae.log.switch"), path: "/demo/buddy-switch/trae/logs/switcher.log", exists: true },
    ],
    logDir: "/demo/buddy-switch/trae/logs",
    note: t("shared.demo.trae.logNote"),
  };
}

/** Read-only demo response provider. It never reads or mutates real user data. */
export function screenshotDemoResponse(command: string, args?: Record<string, unknown>): unknown {
  const demoAccounts = hydratedAccounts();
  const activeIndex = Math.max(0, demoAccounts.findIndex((account) => account.id === demoActiveCliAccountId));
  const activeAccount = demoAccounts[activeIndex] ?? demoAccounts[0];
  const cliStatus: CodeBuddyCliStatus = { configured: true, settingsPresent: true, helperPresent: true, helperSupportsAccountIds: true, activeIndex, activeAccountId: activeAccount.id, activeAccountName: activeAccount.nickname, accountCount: demoAccounts.length, statePath: "/demo/codebuddy-cli-state.json" };
  const config = rotateConfig();
  const rotateStatus: RotateStatus = { config, cliConfigured: true, activeAccountId: demoAccounts[0].id, activeAccountName: demoAccounts[0].nickname, lastCheckAt: atLocalTime(0, 9, 30), lastSwitchAt: atLocalTime(1, 16, 20) };
  const githubConfig: GithubConfig = { owner: "zhangjia", repo: "buddy-switch", proxy: "" };
  switch (command) {
    case "get_status": return demoRegionStatus(args?.region === "global" ? "global" : "cn");
    case "get_accounts": return { accounts: demoAccounts };
    case "get_codebuddy_cli_status": return cliStatus;
    case "get_codebuddy_cn_ide_status": return {
      installed: true,
      running: false,
      dataDir: "/demo/CodeBuddy CN",
      dbPath: "/demo/CodeBuddy CN/User/globalStorage/state.vscdb",
      dbExists: true,
      appPath: "/demo/CodeBuddy CN.app",
      activeAccountId: demoAccounts[0].id,
      activeAccountName: demoAccounts[0].nickname,
      detectedFrom: "state.vscdb",
      statePath: "/demo/codebuddy-cn-ide-state.json",
    } satisfies CodeBuddyCnIdeStatus;
    case "switch_codebuddy_cli_account": {
      const target = demoAccounts.find((account) => account.id === args?.accountId);
      if (!target) throw new Error(t("shared.demo.error.accountMissing"));
      demoActiveCliAccountId = target.id;
      return { ok: true, configured: true, synced: true, verified: true, activeIndex: demoAccounts.indexOf(target), activeAccountId: target.id, message: t("shared.demo.switchDone") } satisfies CodeBuddyCliSwitchResult;
    }
    case "get_checkin_status": return { ok: true, todayCheckedIn: true };
    case "get_credit_expiry": return creditExpiry(String(args?.accountId ?? ""));
    case "get_credit_statistics": return buildStatistics();
    case "get_token_statistics": return demoTokenStatistics(typeof args?.days === "number" ? args.days : undefined);
    case "get_auto_checkin_config": return checkinConfig();
    case "get_checkin_logs": return { logs: checkinLogs() };
    case "get_travel_status": return travelStatus(String(args?.accountId ?? ""));
    case "get_auto_travel_config": return travelConfig();
    case "get_switch_config": return { copy_sessions_by_default: false, pin_current_account: true };
    case "get_schedule_config": return {
      checkin_hours: [9, 21],
      travel_hours: [9, 21],
      activity_hours: [10],
      keepalive_hours: [22],
      school_hours: [12],
      cat_hours: [1],
      trae_checkin_hours: [9, 21],
      checkin_enabled: true,
      travel_enabled: true,
      activity_enabled: true,
      keepalive_enabled: true,
      school_enabled: true,
      cat_enabled: true,
      // 与后端默认值一致（Trae 自动签到默认关闭）——演示数据也必须如实，
      // 否则截图会误导成「装上就是开着的」。
      trae_checkin_enabled: false,
      activity_report_count: 5,
    } satisfies ScheduleConfig;
    case "get_auto_rotate_config": return config;
    case "rotate_status": return rotateStatus;
    case "get_rotate_logs": return { logs: rotateLogs() };
    case "get_github_config": return githubConfig;
    case "check_update": return { ok: true, current: "2026.9.16", latest: "2026.9.17", latestTag: "v2026.9.17", hasUpdate: true, releaseName: t("shared.demo.updateTitle"), releaseUrl: "https://github.com/NextAgentX/trae-workbuddy-switch/releases/tag/v2026.9.17" };
    case "get_launch_at_login_enabled": return true;
    case "switch_progress": return { running: false, progress: null };
    case "get_gateway_config": return demoGatewayConfig();
    case "gateway_status": return demoGatewayStatus();
    case "list_api_keys": return { keys: demoApiKeys() };
    case "get_gateway_models": return demoCatalog(args?.region === "global" ? "global" : "cn");
    case "get_account_strategy": return demoStrategyMap();
    case "get_gateway_logs": return { logs: demoGatewayLogs() };
    // ---- Trae 分区（只读；写操作不进这里，由 DemoAction 统一拦截）----
    case "get_trae_env": return demoTraeEnv();
    case "get_trae_variants": return demoTraeVariants();
    case "get_trae_capabilities": return demoTraeCapabilities();
    case "get_trae_accounts": return demoTraeAccounts();
    case "get_trae_checkin_status": return demoTraeCheckinStatus();
    case "get_trae_credits": return demoTraeCredits();
    case "get_trae_profiles": return demoTraeProfiles(args);
    case "get_trae_settings": return demoTraeSettings();
    case "get_trae_token_statistics":
      return demoTraeTokenStatistics(typeof args?.days === "number" ? args.days : undefined);
    case "get_trae_logs": return demoTraeLogs(args);
    case "get_trae_gateway_config": return demoTraeGatewayConfig();
    case "trae_gateway_status": return demoTraeGatewayStatus();
    case "get_trae_gateway_models": return demoTraeGatewayModels();
    case "list_trae_api_keys": return demoTraeApiKeys();
    case "get_trae_gateway_logs": return demoTraeGatewayLogs();
    default: throw new Error(t("shared.demo.error.missingReadOnly", { command }));
  }
}

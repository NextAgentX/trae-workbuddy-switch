import { useCallback, useEffect, useMemo, useState, type ComponentProps } from "react";
import { Bar, BarChart, CartesianGrid, Rectangle, XAxis, YAxis } from "recharts";
import {
  CalendarDays,
  CalendarRange,
  Check,
  CircleAlert,
  CircleCheck,
  Loader2,
  Sparkles,
  RefreshCw,
  TrendingDown,
  Users,
  // XCircle, // 最近事件卡片隐藏后未使用
  type LucideIcon,
} from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { DemoAction } from "@/components/demo-action";
import { RegionBar } from "@/components/region-bar";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
} from "@/components/ui/card";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import * as api from "@/lib/api";
import { regionFilterLabel, regionLabel } from "@/lib/region";
import { cn } from "@/lib/utils";
import { getStackedSegmentVisualLayout } from "@/lib/stacked-bar-visuals";
import type {
  CreditExpiry,
  CreditOfficialUsage,
  CreditOfficialUsageAccount,
  CreditOfficialUsageModel,
  CreditOfficialUsageRequest,
  CreditResource,
  CreditStatsAccount,
  CreditStatsDailyPoint,
  // CreditStatsEvent, // 最近事件卡片隐藏后未使用
  CreditStatistics,
  Region,
  RegionFilter,
} from "@/lib/types";
import { useT } from "@/lib/i18n";
import type { TranslationKey } from "@/locales/zh";
import { useAccountsStore } from "@/stores/accounts";

type RangeKey = "30d" | "today" | "7d" | "month";

const CREDIT_REGION_STORAGE_KEY = "buddy-switch:credit-stats:region";

function isRegionFilter(value: unknown): value is RegionFilter {
  return value === "cn" || value === "global" || value === "all";
}

/** 统计范围默认「国内版」；各页独立记忆，不与账号管理页联动。 */
function readPreferredRegion(): RegionFilter {
  if (typeof window === "undefined") return "cn";
  try {
    const stored = window.localStorage.getItem(CREDIT_REGION_STORAGE_KEY);
    return isRegionFilter(stored) ? stored : "cn";
  } catch {
    return "cn";
  }
}

function persistPreferredRegion(region: RegionFilter): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(CREDIT_REGION_STORAGE_KEY, region);
  } catch {
    // localStorage 在受限 WebView/隐私模式下可能不可写，不影响本次会话内的切换。
  }
}

/**
 * 账号归属徽标（仅合并视图有意义）：后端为 `accounts[]` 注入 `region`。
 * 无归属信息时不渲染，避免用猜测误导用户。
 */
function AccountRegionBadge({ region, className }: { region?: Region; className?: string }) {
  if (region !== "cn" && region !== "global") return null;
  return (
    <Badge variant="outline" className={cn("shrink-0 text-[10px] font-normal text-muted-foreground", className)}>
      {regionLabel(region)}
    </Badge>
  );
}

const RANGE_OPTIONS: { key: RangeKey; label: TranslationKey }[] = [
  { key: "30d", label: "wbStats.token.range.30d" },
  { key: "today", label: "wbStats.token.range.today" },
  { key: "7d", label: "wbStats.token.range.7d" },
  { key: "month", label: "wbStats.token.range.month" },
];

function dateKey(date: Date): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function dateDaysAgo(days: number): string {
  const date = new Date();
  date.setHours(12, 0, 0, 0);
  date.setDate(date.getDate() - days);
  return dateKey(date);
}

function formatCredits(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return "—";
  return new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 }).format(value);
}

function formatDateTime(ts: number | null | undefined): string {
  if (ts === null || ts === undefined) return "—";
  return new Date(ts).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatDate(ts: number | null | undefined): string {
  if (ts === null || ts === undefined) return "—";
  return new Date(ts).toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
}

function formatChartDate(date: string): string {
  return date.slice(5).replace("-", "/");
}

function accountLabel(account: { accountName?: string | null; accountId: string }): string {
  return account.accountName || account.accountId;
}

function AccountFilterMenu({
  accounts,
  accountFilter,
  onAccountFilterChange,
  ariaLabel,
  allowAll = true,
}: {
  accounts: { accountId: string; accountName?: string | null; region?: Region }[];
  accountFilter: string | null;
  onAccountFilterChange: (accountId: string | null) => void;
  ariaLabel: string;
  /** false 时隐藏「所有账号」选项，仅允许选择具体账号 */
  allowAll?: boolean;
}) {
  const t = useT();
  const activeFilterAccount =
    accountFilter && accounts.some((account) => account.accountId === accountFilter)
      ? accounts.find((account) => account.accountId === accountFilter)
      : undefined;
  const effectiveFilter = activeFilterAccount?.accountId ?? null;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="h-8 max-w-[190px] gap-1.5 px-2.5 text-xs text-muted-foreground hover:text-foreground"
          aria-label={ariaLabel}
        >
          <Users className="size-3.5 shrink-0" />
          <span className="truncate">
            {activeFilterAccount
              ? accountLabel(activeFilterAccount)
              : allowAll
                ? t("wbStats.credit.allAccounts")
                : accounts[0]
                  ? accountLabel(accounts[0])
                  : t("wbStats.credit.noAccount")}
          </span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-80 w-56 overflow-y-auto">
        {allowAll && (
          <>
            <DropdownMenuItem onSelect={() => onAccountFilterChange(null)}>
              <Users className="size-3.5 shrink-0" />
              {t("wbStats.credit.allAccounts")}
              {!effectiveFilter && <Check className="ml-auto size-3.5 shrink-0" />}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
          </>
        )}
        {accounts.map((account) => (
          <DropdownMenuItem key={account.accountId} onSelect={() => onAccountFilterChange(account.accountId)}>
            <span className="min-w-0 flex-1 truncate">{accountLabel(account)}</span>
            <AccountRegionBadge region={account.region} className="ml-1.5" />
            {effectiveFilter === account.accountId && <Check className="ml-auto size-3.5 shrink-0" />}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function isOfficialUsageAvailable(officialUsage?: CreditOfficialUsage): boolean {
  return officialUsage?.status === "complete" || officialUsage?.status === "partial";
}

function officialAccountFor(
  officialUsage: CreditOfficialUsage | undefined,
  accountId: string,
): CreditOfficialUsageAccount | undefined {
  return officialUsage?.accounts.find((account) => account.accountId === accountId);
}

function chartPoints(daily: CreditStatsDailyPoint[], range: RangeKey) {
  const today = dateKey(new Date());
  const firstDate = range === "today" ? today : range === "7d" ? dateDaysAgo(6) : dateDaysAgo(29);
  return daily.filter((point) => {
    if (range === "month") {
      return point.date.startsWith(`${today.slice(0, 7)}-`);
    }
    return point.date >= firstDate && point.date <= today;
  });
}

function rangeUsage(
  summary: CreditStatistics["summary"] | CreditOfficialUsage["summary"],
  daily: CreditStatsDailyPoint[],
  range: RangeKey,
): number {
  switch (range) {
    case "today":
      return summary.usageToday;
    case "7d":
      return summary.usage7Days;
    case "month":
      return summary.usageThisMonth;
    case "30d":
      return daily
        .filter((point) => point.date >= dateDaysAgo(29) && point.date <= dateKey(new Date()))
        .reduce((sum, point) => sum + point.usage, 0);
  }
}

/* 最近事件卡片隐藏后 checkinLabel 一并停用。恢复时取消本注释。
function checkinLabel(result: string | null | undefined): string {
  switch (result) {
    case "success":
      return "签到成功";
    case "already":
      return "已签到";
    case "error":
      return "签到失败";
    default:
      return "暂无记录";
  }
}
*/

/* 仅账号积分明细表使用，表隐藏期间一并注释。
function checkinBadgeVariant(
  result: string | null | undefined,
): "success" | "warning" | "destructive" | "outline" {
  switch (result) {
    case "success":
    case "already":
      return "success";
    case "error":
      return "destructive";
    default:
      return "outline";
  }
}
*/

/** 资源包名取后端下发的名称；缺失时返回 null，由渲染处回落到词表。 */
function resourceName(resource: CreditResource): string | null {
  return resource.packageName || resource.packageCode || null;
}

function StatMetric({
  icon: Icon,
  label,
  value,
  divided = false,
}: {
  icon: LucideIcon;
  label: string;
  value: string;
  divided?: boolean;
}) {
  return (
    <div
      className={`flex min-w-0 flex-col items-center justify-center px-4 py-5 text-center sm:py-3 ${
        divided ? "sm:border-l sm:border-border/60" : ""
      }`}
    >
      <div className="flex max-w-full items-center justify-center gap-2 text-[13px] font-medium leading-5 text-muted-foreground">
        <Icon className="size-4 shrink-0 stroke-[1.75]" aria-hidden="true" />
        <span className="truncate">{label}</span>
      </div>
      <div className="mt-3 max-w-full truncate text-[26px] font-semibold leading-8 tracking-[-0.025em] text-foreground tabular-nums" style={{ fontFamily: '"Bricolage Grotesque Variable", "SF Pro Display", ui-sans-serif, sans-serif' }}>
        {value}
      </div>
    </div>
  );
}

/** 模型趋势共享数据色板；颜色由浅色/深色主题 token 提供。 */
const MODEL_COLORS = [
  "var(--data-series-emerald)",
  "var(--data-series-teal)",
  "var(--data-series-violet)",
  "var(--data-series-amber)",
  "var(--data-series-rose)",
  "var(--data-series-indigo)",
  "var(--data-series-sky)",
  "var(--data-series-lime)",
];
const MAX_MODELS = 5;
const OTHER_MODEL = "wbStats.credit.otherModel";

interface ModelChartPoint {
  date: string;
  total: number;
  [model: string]: number | string;
}

type CreditBarShapeProps = ComponentProps<typeof Rectangle> & {
  segmentKey: string;
  seriesKeys: string[];
  payload?: ModelChartPoint;
  value?: number | [number, number];
};

function CreditBarShape({
  segmentKey,
  seriesKeys,
  payload,
  x = 0,
  y = 0,
  width = 0,
  height = 0,
  value,
  fill,
  stroke,
  strokeWidth,
  ...rest
}: CreditBarShapeProps) {
  if (width <= 0 || height <= 0) return null;
  const segmentIndex = seriesKeys.indexOf(segmentKey);
  const stackStart = Array.isArray(value) ? Number(value[0]) : 0;
  const layout = payload
    ? getStackedSegmentVisualLayout({
        values: seriesKeys.map((key) => Number(payload[key] ?? 0)),
        segmentIndex,
        segmentHeight: height,
        segmentY: y,
        stackStart,
      })
    : null;
  return (
    <Rectangle
      {...rest}
      x={x}
      y={layout?.y ?? y}
      width={width}
      height={layout?.height ?? height}
      fill={fill}
      radius={layout?.isTop ? [6, 6, 0, 0] : 0}
      stroke={stroke ?? "var(--background)"}
      strokeWidth={strokeWidth ?? 2}
    />
  );
}

/** 从官方 daily（全量按模型聚合）构建层叠数据；模型按总消耗取前 N，其余并入「其他」。 */
function buildStackedChart(
  daily: CreditStatsDailyPoint[],
): { models: string[]; points: ModelChartPoint[] } {
  const modelTotals = new Map<string, number>();
  for (const point of daily) {
    for (const model of point.models ?? []) {
      modelTotals.set(model.model, (modelTotals.get(model.model) ?? 0) + model.credit);
    }
  }
  const topModels = [...modelTotals.entries()]
    .sort((left, right) => right[1] - left[1])
    .slice(0, MAX_MODELS)
    .map(([model]) => model);

  const points: ModelChartPoint[] = daily.map((point) => {
    const entry: ModelChartPoint = { date: point.date, total: point.usage };
    for (const model of point.models ?? []) {
      const key = topModels.includes(model.model) ? model.model : OTHER_MODEL;
      entry[key] = (typeof entry[key] === "number" ? entry[key] : 0) + model.credit;
    }
    return entry;
  });
  const models = [...topModels];
  if (points.some((point) => point[OTHER_MODEL] !== undefined)) {
    models.push(OTHER_MODEL);
  }
  return { models, points };
}

function TrendChart({
  stats,
  officialUsage,
}: {
  stats: CreditStatistics;
  officialUsage?: CreditOfficialUsage;
}) {
  const t = useT();
  /** null = 所有账号汇总；本卡片独立，不影响其他卡片 */
  const [accountFilter, setAccountFilter] = useState<string | null>(null);
  /** 本卡片独立的时间范围，不影响其他卡片 */
  const [range, setRange] = useState<RangeKey>("30d");
  const official = isOfficialUsageAvailable(officialUsage) ? officialUsage : undefined;
  const officialAvailable = Boolean(official);
  const filterAccounts = official ? official.accounts : stats.accounts;
  const activeFilterAccount =
    accountFilter && filterAccounts.some((account) => account.accountId === accountFilter)
      ? filterAccounts.find((account) => account.accountId === accountFilter)
      : undefined;
  const effectiveFilter = activeFilterAccount?.accountId ?? null;
  const officialAccount = effectiveFilter ? officialAccountFor(official, effectiveFilter) : undefined;
  const localAccount = effectiveFilter
    ? stats.accounts.find((account) => account.accountId === effectiveFilter)
    : undefined;
  // 选中账号时切到该账号的逐日数据与汇总；否则用全部账号的聚合
  const daily = official
    ? (officialAccount ? officialAccount.daily ?? [] : official.daily)
    : (localAccount ? localAccount.daily ?? [] : stats.daily);
  const summary = official
    ? (officialAccount
        ? {
            usageToday: officialAccount.usageToday ?? 0,
            usage7Days: officialAccount.usage7Days ?? 0,
            usageThisMonth: officialAccount.usageThisMonth ?? 0,
          }
        : official.summary)
    : (localAccount
        ? {
            usageToday: localAccount.usageToday,
            usage7Days: localAccount.usage7Days,
            usageThisMonth: localAccount.usageThisMonth,
          }
        : stats.summary);
  const basePoints = chartPoints(daily, range);
  const hasDataSource = officialAvailable || Boolean(stats.coverageStartAt);

  // 官方 daily 带全量模型聚合 → 层叠柱（按模型）；否则单层「本地观察」柱
  const hasModelDetail = (official?.daily ?? []).some((point) => (point.models?.length ?? 0) > 0);
  const stacked = official && hasModelDetail ? buildStackedChart(basePoints) : null;
  const chartData: ModelChartPoint[] = stacked
    ? stacked.points
    : basePoints.map((point) => ({ date: point.date, total: point.usage }));

  // 单层本地数据实际使用 `total` 字段；官方数据才按模型名称分层。
  const series = stacked ? stacked.models : ["total"];
  const chartConfig: ChartConfig = {};
  for (const model of series) {
    chartConfig[model] = {
      label: model === "total" ? t("wbStats.credit.totalUsage") : model === OTHER_MODEL ? t("wbStats.credit.otherModel") : model,
      ...(stacked
        ? { color: MODEL_COLORS[series.indexOf(model) % MODEL_COLORS.length] }
        : { color: "var(--data-series-emerald)" }),
    };
  }

  const hasObservedUsage = chartData.some((point) => point.total > 0);
  return (
    <section className="min-w-0 space-y-2.5" aria-labelledby="trend-chart-title">
      <div className="px-1">
        <h2 id="trend-chart-title" className="text-[13px] font-medium leading-5">
          {officialAvailable ? t("wbStats.credit.officialTitle") : t("wbStats.credit.localTitle")}
        </h2>
      </div>
      <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
        <CardHeader className="gap-0 px-4 pt-3 pb-0 sm:px-5">
          <div className="flex min-w-0 flex-wrap items-center justify-between gap-3">
            <CardDescription className="min-w-0 text-xs">
              {officialAvailable
                ? t("wbStats.credit.officialDesc", { start: official?.rangeStart ?? "", end: official?.rangeEnd ?? "" })
                : t("wbStats.credit.localDesc")}
            </CardDescription>
            <div className="flex max-w-full flex-wrap items-center gap-1.5">
              <AccountFilterMenu
                accounts={filterAccounts}
                accountFilter={effectiveFilter}
                onAccountFilterChange={setAccountFilter}
                ariaLabel={t("wbStats.credit.filterByTrend")}
              />
              <div className="flex max-w-full flex-wrap gap-1 rounded-lg bg-muted p-1" aria-label={t("wbStats.credit.trendRangeAria")}>
                {RANGE_OPTIONS.map((option) => (
                  <button
                    key={option.key}
                    type="button"
                    className={`cursor-pointer rounded-md px-2.5 py-1.5 text-xs transition-colors ${
                      range === option.key
                        ? "bg-background font-medium text-foreground shadow-sm"
                        : "text-muted-foreground hover:text-foreground"
                    }`}
                    onClick={() => setRange(option.key)}
                    aria-pressed={range === option.key}
                  >
                    {t(option.label)}
                  </button>
                ))}
              </div>
            </div>
          </div>
        </CardHeader>
        <CardContent className="min-w-0 px-4 pt-3 pb-4 sm:px-5">
        {!hasDataSource ? (
          <div className="rounded-lg border border-dashed px-4 py-10 text-center text-sm text-muted-foreground">
            {t("wbStats.credit.emptySnapshot")}
          </div>
        ) : chartData.length === 0 ? (
          <div className="rounded-lg border border-dashed px-4 py-10 text-center text-sm text-muted-foreground">
            {t("wbStats.credit.emptyObserved")}
          </div>
        ) : (
          <>
            <ChartContainer config={chartConfig} className="h-56 w-full">
              <BarChart data={chartData} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
                <CartesianGrid vertical={false} strokeDasharray="3 3" />
                <XAxis
                  dataKey="date"
                  tickLine={false}
                  axisLine={false}
                  tickMargin={8}
                  tickFormatter={(value) => formatChartDate(String(value))}
                />
                <YAxis tickLine={false} axisLine={false} width={42} tickFormatter={(value) => formatCredits(value)} />
                <ChartTooltip
                  cursor={{ fill: "var(--muted)", opacity: 0.4 }}
                  content={
                    <ChartTooltipContent
                      labelFormatter={(_, payload) => {
                        const item = Array.isArray(payload) ? payload[0] : payload;
                        return t("wbStats.credit.trendUsageLabel", { date: formatChartDate(String(item?.payload?.date ?? "")) });
                      }}
                    />
                  }
                />
                {series.map((model, index) => (
                  <Bar
                    key={model}
                    dataKey={model}
                    stackId="usage"
                    fill={stacked ? MODEL_COLORS[index % MODEL_COLORS.length] : "var(--color-total)"}
                    stroke="var(--background)"
                    strokeWidth={2}
                    maxBarSize={28}
                    shape={<CreditBarShape segmentKey={model} seriesKeys={series} />}
                    isAnimationActive={false}
                  />
                ))}
              </BarChart>
            </ChartContainer>
            {stacked && (
              <div className="mt-3 flex flex-wrap items-center justify-center gap-x-4 gap-y-1.5 text-xs text-muted-foreground">
                {stacked.models.map((model, index) => (
                  <span key={model} className="inline-flex items-center gap-1.5">
                    <span className="h-2 w-2 shrink-0 rounded-[2px]" style={{ backgroundColor: MODEL_COLORS[index % MODEL_COLORS.length] }} aria-hidden="true" />
                    {model === OTHER_MODEL ? t("wbStats.credit.otherModel") : model}
                  </span>
                ))}
              </div>
            )}
              <div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                <span>
                  {hasObservedUsage
                    ? t("wbStats.credit.summaryTotal", { amount: formatCredits(rangeUsage(summary, daily, range)) })
                    : officialAvailable
                      ? t("wbStats.credit.officialNoUsage")
                      : t("wbStats.credit.collectedFallback")}
                </span>
                <span>{officialAvailable ? t("wbStats.credit.dataUpdated", { time: formatDateTime(official?.collectedAt ?? stats.generatedAt) }) : t("wbStats.credit.coverage", { date: formatDate(stats.generatedAt) })}</span>
              </div>
              <p className="sr-only">
                {chartData.map((point) => t("wbStats.credit.srSummary", { date: point.date, amount: formatCredits(point.total) })).join(t("shared.punct.semicolon"))}
              </p>
          </>
        )}
        </CardContent>
      </Card>
    </section>
  );
}

/* 与下方「积分明细」重复，先隐藏。恢复时取消本注释，并恢复页面中的 <AccountTable />。
function AccountTable({
  stats,
  officialUsage,
  selectedId,
  onSelect,
}: {
  stats: CreditStatistics;
  officialUsage?: CreditOfficialUsage;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const official = isOfficialUsageAvailable(officialUsage) ? officialUsage : undefined;

  return (
    <section className="min-w-0 space-y-2.5" aria-labelledby="account-table-title">
      <div className="px-1">
        <h2 id="account-table-title" className="text-[13px] font-medium leading-5">账号积分明细</h2>
      </div>
      <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
        <CardHeader className="border-b px-4 py-3 sm:px-5">
          <CardDescription className="text-xs">
            账号 ID 是统计关联键，名称只用于展示；官方用量优先，点击一行查看积分明细和事件。
          </CardDescription>
        </CardHeader>
        {stats.accounts.length === 0 ? (
        <div className="px-4 py-10 text-center text-sm text-muted-foreground">暂无账号统计。</div>
      ) : (
        <div className="min-w-0 overflow-x-auto">
          <table className="w-full min-w-[760px] text-left text-xs">
            <thead className="bg-muted/45 text-muted-foreground">
              <tr>
                <th className="px-4 py-3 font-medium sm:px-5">账号</th>
                <th className="px-3 py-3 text-right font-medium">当前剩余</th>
                <th className="px-3 py-3 text-right font-medium">今日消耗</th>
                <th className="px-3 py-3 text-right font-medium">近 7 天</th>
                <th className="px-3 py-3 text-right font-medium">本月</th>
                <th className="px-4 py-3 text-right font-medium sm:px-5">今日签到</th>
              </tr>
            </thead>
            <tbody>
              {stats.accounts.map((account) => {
                const selected = account.accountId === selectedId;
                const officialAccount = officialAccountFor(official, account.accountId);
                const usageToday = official ? officialAccount?.usageToday : account.usageToday;
                const usage7Days = official ? officialAccount?.usage7Days : account.usage7Days;
                const usageThisMonth = official ? officialAccount?.usageThisMonth : account.usageThisMonth;
                return (
                  <tr
                    key={account.accountId}
                    className={`border-t border-border/60 transition-colors ${selected ? "bg-primary/[0.06]" : "hover:bg-muted/35"}`}
                  >
                    <td className="max-w-[240px] px-4 py-3 sm:px-5">
                      <button
                        type="button"
                        className="min-w-0 max-w-full text-left outline-none focus-visible:rounded-md focus-visible:ring-2 focus-visible:ring-ring"
                        onClick={() => onSelect(account.accountId)}
                      >
                        <span className="flex min-w-0 items-center gap-2">
                          <span className="min-w-0 truncate font-medium">{accountLabel(account)}</span>
                          {!account.isCurrent && (
                            <Badge variant="outline" className="shrink-0 px-1.5 py-0 text-[10px]">
                              历史
                            </Badge>
                          )}
                          {official && account.isCurrent && (
                            <Badge
                              variant={officialAccount?.ok ? "success" : "warning"}
                              className="shrink-0 px-1.5 py-0 text-[10px]"
                            >
                              {officialAccount?.ok ? "官方" : "不可用"}
                            </Badge>
                          )}
                        </span>
                        <span className="mt-0.5 block truncate text-[11px] text-muted-foreground">
                          {account.accountId}
                        </span>
                      </button>
                    </td>
                    <td className="px-3 py-3 text-right font-medium">
                      {formatCredits(account.currentRemaining)}
                    </td>
                    <td className="px-3 py-3 text-right">{formatCredits(usageToday)}</td>
                    <td className="px-3 py-3 text-right">{formatCredits(usage7Days)}</td>
                    <td className="px-3 py-3 text-right">{formatCredits(usageThisMonth)}</td>
                    <td className="px-4 py-3 text-right sm:px-5">
                      <Badge variant={checkinBadgeVariant(account.checkinStatusToday)}>
                        {checkinLabel(account.checkinStatusToday)}
                      </Badge>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      </Card>
    </section>
  );
}
*/

function ResourceBreakdown({ credit, loading }: { credit?: CreditExpiry; loading?: boolean }) {
  const t = useT();
  if (loading) {
    return (
      <div className="flex items-center gap-2 px-4 py-8 text-sm text-muted-foreground sm:px-5">
        <Loader2 className="size-4 animate-spin" />
        {t("wbStats.credit.loadingResources")}
      </div>
    );
  }
  if (!credit) {
    return <div className="px-4 py-8 text-center text-sm text-muted-foreground sm:px-5">{t("wbStats.credit.noCurrentResource")}</div>;
  }
  if (!credit.ok) {
    return (
      <div className="flex items-start gap-2 px-4 py-8 text-sm text-destructive sm:px-5">
        <CircleAlert className="mt-0.5 size-4 shrink-0" />
        <span>{credit.error || t("wbStats.credit.resourceQueryFail")}</span>
      </div>
    );
  }
  const resources = credit.resources ?? [];
  if (resources.length === 0) {
    return <div className="px-4 py-8 text-center text-sm text-muted-foreground sm:px-5">{t("wbStats.credit.noResources")}</div>;
  }

  return (
    <div className="divide-y divide-border/60">
      {resources.map((resource, index) => {
        const ratio = resource.total > 0 ? Math.min(100, Math.max(0, (resource.remaining / resource.total) * 100)) : 0;
        return (
          <div key={`${resource.packageCode || resource.packageName || "resource"}-${index}`} className="min-w-0 px-4 py-1.5 sm:px-5">
            <div className="flex min-w-0 items-center justify-between gap-2">
              <div className="min-w-0 truncate text-[13px] font-medium">
                {resourceName(resource) ?? t("wbStats.credit.unnamedResource")}
              </div>
              <div className="flex shrink-0 items-center gap-2.5">
                <span className="text-[11px] text-muted-foreground">
                  {resource.expired ? t("wbStats.credit.expired") : resource.expiringSoon ? t("wbStats.credit.expiringSoon") : t("wbStats.credit.expireAt", { date: formatDate(resource.expireAt) })}
                  {resource.used > 0 ? ` · ${t("wbStats.credit.used", { amount: formatCredits(resource.used) })}` : ""}
                </span>
                <span className="text-xs font-medium">{formatCredits(resource.remaining)} / {formatCredits(resource.total)}</span>
              </div>
            </div>
            <div className="mt-1 h-1 overflow-hidden rounded-full bg-muted" aria-hidden="true">
              <div className="h-full rounded-full bg-primary/75" style={{ width: `${ratio}%` }} />
            </div>
          </div>
        );
      })}
    </div>
  );
}

function ModelBreakdownRows({ models }: { models: CreditOfficialUsageModel[] }) {
  const t = useT();
  const totalCredit = models.reduce((sum, model) => sum + model.credit, 0);
  const totalRequests = models.reduce((sum, model) => sum + model.requestCount, 0);

  return (
    <div className="space-y-3">
      {models.slice(0, 8).map((model) => {
        const ratio = totalCredit > 0 ? model.credit / totalCredit : totalRequests > 0 ? model.requestCount / totalRequests : 0;
        const percent = ratio * 100;
        const label = model.model === "—" ? t("wbStats.credit.unknownModel") : model.model;
        return (
          <div key={model.model} className="min-w-0">
            <div className="flex min-w-0 items-center justify-between gap-3 text-xs">
              <span className="min-w-0 truncate font-medium" title={label}>
                {label}
              </span>
              <span className="shrink-0 text-muted-foreground">
                {t("wbStats.credit.modelCredit", { credit: formatCredits(model.credit), requests: formatCredits(model.requestCount) })}
                <span className="ml-1.5 font-medium text-foreground">
                  {percent < 0.05 ? "<0.1%" : `${percent.toFixed(1)}%`}
                </span>
              </span>
            </div>
            <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-muted" aria-hidden="true">
              <div className="h-full rounded-full bg-primary/75" style={{ width: `${Math.min(100, Math.max(0, ratio * 100))}%` }} />
            </div>
          </div>
        );
      })}
    </div>
  );
}

function ModelBreakdown({
  officialUsage,
}: {
  officialUsage: CreditOfficialUsage;
}) {
  const t = useT();
  /** null = 所有账号汇总；本卡片独立，不影响其他卡片 */
  const [accountFilter, setAccountFilter] = useState<string | null>(null);
  /** 本卡片独立的时间范围，不影响其他卡片 */
  const [range, setRange] = useState<RangeKey>("30d");
  const filterAccounts = officialUsage.accounts;
  const activeFilterAccount =
    accountFilter && filterAccounts.some((account) => account.accountId === accountFilter)
      ? filterAccounts.find((account) => account.accountId === accountFilter)
      : undefined;
  const effectiveFilter = activeFilterAccount?.accountId ?? null;
  // 按选中账号 + 时间范围，从逐日模型聚合求和（全量，不受明细条数上限影响）
  const basePoints = chartPoints(
    effectiveFilter ? activeFilterAccount?.daily ?? [] : officialUsage.daily,
    range,
  );
  const rangeModelMap = new Map<string, { requestCount: number; credit: number }>();
  for (const point of basePoints) {
    for (const item of point.models ?? []) {
      const entry = rangeModelMap.get(item.model) ?? { requestCount: 0, credit: 0 };
      entry.requestCount += item.requestCount;
      entry.credit += item.credit;
      rangeModelMap.set(item.model, entry);
    }
  }
  const models = [...rangeModelMap.entries()]
    .map(([model, value]) => ({ model, requestCount: value.requestCount, credit: value.credit }))
    .sort(
      (a, b) =>
        b.credit - a.credit ||
        b.requestCount - a.requestCount ||
        a.model.localeCompare(b.model),
    );
  const totalCredit = models.reduce((sum, model) => sum + model.credit, 0);
  const totalRequests = models.reduce((sum, model) => sum + model.requestCount, 0);

  return (
    <section className="min-w-0 space-y-2.5" aria-labelledby="model-breakdown-title">
      <div className="px-1">
        <h2 id="model-breakdown-title" className="text-[13px] font-medium leading-5">{t("wbStats.credit.byModelTitle")}</h2>
      </div>
      <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
        <CardHeader className="gap-0 px-4 pt-3 pb-0 sm:px-5">
          <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
            <Badge variant="outline" className="shrink-0">
              {t("wbStats.credit.modelsCount", { n: models.length })}
            </Badge>
            <div className="flex flex-wrap items-center justify-end gap-1.5">
              <AccountFilterMenu
                accounts={filterAccounts}
                accountFilter={effectiveFilter}
                onAccountFilterChange={setAccountFilter}
                ariaLabel={t("wbStats.credit.filterByModel")}
              />
              <div className="flex max-w-full flex-wrap gap-1 rounded-lg bg-muted p-1" aria-label={t("wbStats.credit.modelRangeAria")}>
                {RANGE_OPTIONS.map((option) => (
                  <button
                    key={option.key}
                    type="button"
                    className={`rounded-md px-2.5 py-1.5 text-xs transition-colors ${
                      range === option.key
                        ? "bg-background font-medium text-foreground shadow-sm"
                        : "text-muted-foreground hover:text-foreground"
                    }`}
                    onClick={() => setRange(option.key)}
                    aria-pressed={range === option.key}
                  >
                    {option.label}
                  </button>
                ))}
              </div>
            </div>
          </div>
        </CardHeader>
        {models.length === 0 ? (
        <CardContent className="px-4 py-8 text-center text-sm text-muted-foreground sm:px-5">
          {activeFilterAccount && !activeFilterAccount.ok ? t("wbStats.credit.accountUnavailable") : t("wbStats.credit.noModelDetail")}
        </CardContent>
      ) : (
        <CardContent className="px-4 pt-3 pb-4 sm:px-5">
          <div className="mb-4 flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
            <span>{t("wbStats.credit.totalRequests", { n: formatCredits(totalRequests) })}</span>
            <span className="font-medium text-foreground">{t("wbStats.credit.totalCredit", { amount: formatCredits(totalCredit) })}</span>
          </div>
          <ModelBreakdownRows models={models} />
          {models.length > 8 && <p className="mt-3 text-[11px] text-muted-foreground">{t("wbStats.credit.topModels")}</p>}
        </CardContent>
      )}
      </Card>
    </section>
  );
}

function OfficialRequestRow({
  request,
  showAccount,
}: {
  request: CreditOfficialUsageRequest;
  showAccount: boolean;
}) {
  return (
    <tr className="border-t border-border/60 align-top">
      <td className="whitespace-nowrap px-3 py-3 text-muted-foreground">{request.requestTime}</td>
      {showAccount && (
        <td className="max-w-[140px] truncate px-3 py-3" title={request.accountName}>
          {request.accountName}
        </td>
      )}
      <td className="whitespace-nowrap px-3 py-3 text-right font-medium text-primary">
        {formatCredits(request.credit)}
      </td>
      <td className="max-w-[180px] truncate px-3 py-3" title={request.model}>
        {request.model}
      </td>
      <td className="max-w-[120px] truncate px-3 py-3 text-muted-foreground" title={request.client}>
        {request.client}
      </td>
      <td className="max-w-[170px] truncate px-3 py-3 font-mono text-[10px] text-muted-foreground" title={request.requestId}>
        {request.requestId}
      </td>
    </tr>
  );
}

function OfficialUsageBreakdown({
  officialUsage,
  accountId,
}: {
  officialUsage?: CreditOfficialUsage;
  accountId: string | null;
}) {
  const t = useT();
  const officialAvailable = isOfficialUsageAvailable(officialUsage);
  const account = accountId ? officialAccountFor(officialUsage, accountId) : undefined;

  if (!officialAvailable || !officialUsage) {
    return (
      <div className="flex items-start gap-2 px-4 py-8 text-sm text-muted-foreground sm:px-5">
        <CircleAlert className="mt-0.5 size-4 shrink-0" />
        <span>{t("wbStats.credit.officialUnavailable")}</span>
      </div>
    );
  }

  if (accountId && !account) {
    return <div className="px-4 py-8 text-center text-sm text-muted-foreground sm:px-5">{t("wbStats.credit.noAccountRecords")}</div>;
  }

  if (account && !account.ok) {
    return (
      <div className="flex items-start gap-2 px-4 py-8 text-sm text-destructive sm:px-5">
        <CircleAlert className="mt-0.5 size-4 shrink-0" />
        <span>{account.error || t("wbStats.credit.accountQueryFail")}</span>
      </div>
    );
  }

  const requests = account
    ? officialUsage.requests.filter((request) => request.accountId === account.accountId)
    : officialUsage.requests;
  const totalRequests = account
    ? (account.reportedTotal ?? account.requestCount)
    : officialUsage.accounts.reduce((sum, item) => sum + (item.reportedTotal ?? item.requestCount), 0);
  const detailTruncated = account
    ? account.detailTruncated
    : officialUsage.accounts.some((item) => item.detailTruncated);
  const showAccount = !account;

  return (
    <div className="min-w-0">
      {detailTruncated && (
        <div className="flex items-start gap-2 border-b bg-amber-500/[0.06] px-4 py-2.5 text-xs text-amber-800 sm:px-5">
          <CircleAlert className="mt-0.5 size-3.5 shrink-0" />
          <span>
            {t("wbStats.credit.detailTruncated", { n: officialUsage.detailLimitPerAccount, total: formatCredits(totalRequests) })}
          </span>
        </div>
      )}
      {requests.length === 0 ? (
        <div className="px-4 py-8 text-center text-sm text-muted-foreground sm:px-5">
          {totalRequests > 0 ? t("wbStats.credit.formatFail") : t("wbStats.credit.noRequests")}
        </div>
      ) : (
        <div className="min-w-0 overflow-x-auto">
          <table className="w-full min-w-[700px] text-left text-[11px]">
            <thead className="sticky top-0 bg-muted/95 text-muted-foreground">
              <tr>
                <th className="px-3 py-2.5 font-medium">{t("wbStats.credit.colTime")}</th>
                {showAccount && <th className="px-3 py-2.5 font-medium">{t("wbStats.credit.colAccount")}</th>}
                <th className="px-3 py-2.5 text-right font-medium">{t("wbStats.credit.colUsage")}</th>
                <th className="px-3 py-2.5 font-medium">{t("wbStats.credit.colModel")}</th>
                <th className="px-3 py-2.5 font-medium">{t("wbStats.credit.colClient")}</th>
                <th className="px-3 py-2.5 font-medium">{t("wbStats.credit.colRequestId")}</th>
              </tr>
            </thead>
            <tbody>
              {requests.map((request) => (
                <OfficialRequestRow
                  key={`${request.requestId}-${request.requestTime}`}
                  request={request}
                  showAccount={showAccount}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

/* 最近事件卡片已隐藏，EventRow 一并停用。恢复时取消本注释。
function EventRow({ event }: { event: CreditStatsEvent }) {
  if (event.kind === "usage") {
    return (
      <div className="flex min-w-0 items-start gap-3 border-b border-border/60 py-3 last:border-b-0">
        <span className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full bg-primary/10 text-primary">
          <TrendingDown className="size-3.5" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1 text-xs">
            <span className="font-medium">观察到积分消耗</span>
            <span className="font-medium text-primary">-{formatCredits(event.amount)}</span>
          </div>
          <div className="mt-1 truncate text-[11px] text-muted-foreground">
            {event.accountName} · {formatDateTime(event.ts)}
          </div>
        </div>
      </div>
    );
  }

  const isError = event.result === "error";
  const isAlready = event.result === "already";
  return (
    <div className="flex min-w-0 items-start gap-3 border-b border-border/60 py-3 last:border-b-0">
      <span
        className={`mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full ${
          isError ? "bg-destructive/10 text-destructive" : isAlready ? "bg-amber-500/10 text-amber-700" : "bg-emerald-500/10 text-emerald-700"
        }`}
      >
        {isError ? <XCircle className="size-3.5" /> : <CircleCheck className="size-3.5" />}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1 text-xs">
          <span className="font-medium">{checkinLabel(event.result)}</span>
          <span className="text-muted-foreground">{formatDateTime(event.ts)}</span>
        </div>
        <div className="mt-1 truncate text-[11px] text-muted-foreground">
          {event.accountName}{event.error ? ` · ${event.error}` : ""}
        </div>
      </div>
    </div>
  );
}
*/

function ResourcesByAccount({
  accounts,
  creditMap,
  creditLoadingMap,
}: {
  accounts: CreditStatsAccount[];
  creditMap: Record<string, CreditExpiry>;
  creditLoadingMap: Record<string, boolean>;
}) {
  const t = useT();
  if (accounts.length === 0) {
    return <div className="px-4 py-8 text-center text-sm text-muted-foreground sm:px-5">{t("wbStats.credit.noAccountStats")}</div>;
  }
  if (accounts.length === 1) {
    const account = accounts[0];
    return <ResourceBreakdown credit={creditMap[account.accountId]} loading={creditLoadingMap[account.accountId]} />;
  }
  return (
    <div className="divide-y divide-border/60">
      {accounts.map((account) => (
        <div key={account.accountId} className="min-w-0">
          <div className="px-4 py-2.5 text-xs font-medium sm:px-5">{accountLabel(account)}</div>
          <ResourceBreakdown credit={creditMap[account.accountId]} loading={creditLoadingMap[account.accountId]} />
        </div>
      ))}
    </div>
  );
}

function SelectedAccountDetails({
  officialUsage,
  regionAccounts,
  creditMap,
  creditLoadingMap,
}: {
  officialUsage?: CreditOfficialUsage;
  /** 当前统计范围下的账号集合（cn / global 单版，或两版并集），已按归属标注。 */
  regionAccounts: CreditStatsAccount[];
  creditMap: Record<string, CreditExpiry>;
  creditLoadingMap: Record<string, boolean>;
}) {
  const t = useT();
  const [detailTab, setDetailTab] = useState<"credits" | "requests">("credits");
  const official = isOfficialUsageAvailable(officialUsage) ? officialUsage : undefined;
  // 账号选项集合随 region 变化；卡片内的时间范围与账号筛选控件本身不变。
  const optionAccounts: { accountId: string; accountName?: string | null; region?: Region }[] = official
    ? official.accounts
    : regionAccounts;
  const filterAccounts = optionAccounts;
  /** 本卡片仅允许选择单个账号，默认第一个账号 */
  const [accountFilter, setAccountFilter] = useState<string | null>(
    () => filterAccounts[0]?.accountId ?? null,
  );
  const activeFilterAccount =
    accountFilter && filterAccounts.some((account) => account.accountId === accountFilter)
      ? filterAccounts.find((account) => account.accountId === accountFilter)
      : undefined;
  // 筛选失效（如账号列表刷新变化）时回退到第一个账号
  const effectiveFilter = activeFilterAccount?.accountId ?? filterAccounts[0]?.accountId ?? null;
  const visibleAccounts = effectiveFilter
    ? regionAccounts.filter((account) => account.accountId === effectiveFilter)
    : regionAccounts;
  // 最近事件卡片已隐藏，events 不再使用。恢复时取消本注释。
  // const events = (effectiveFilter
  //   ? stats.events.filter((event) => event.accountId === effectiveFilter)
  //   : stats.events
  // ).slice(0, 50);
  const latestSnapshotAt = visibleAccounts.reduce<number | null>((latest, account) => {
    if (account.lastSnapshotAt == null) return latest;
    if (latest == null || account.lastSnapshotAt > latest) return account.lastSnapshotAt;
    return latest;
  }, null);

  useEffect(() => {
    setDetailTab("credits");
  }, [effectiveFilter]);

  return (
    <div className="flex min-w-0 flex-col gap-12">
      <section className="min-w-0 space-y-2.5" aria-labelledby="credit-detail-title">
        <div className="px-1">
          <h2 id="credit-detail-title" className="text-[13px] font-medium leading-5">{t("wbStats.credit.creditDetailTitle")}</h2>
        </div>
        <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
          <CardHeader className="gap-0 border-b px-4 pt-3 pb-3 sm:px-5">
            <div className="flex min-w-0 flex-wrap items-center justify-between gap-3">
              <CardDescription className="min-w-0 truncate text-xs">
                {latestSnapshotAt ? t("wbStats.credit.latestSnapshot", { time: formatDateTime(latestSnapshotAt) }) : t("wbStats.credit.noResourcePack")}
                {visibleAccounts[0]?.region && (
                  <span className="ml-1.5 text-muted-foreground/80">
                    · {regionLabel(visibleAccounts[0].region)}
                  </span>
                )}
              </CardDescription>
              <AccountFilterMenu
                accounts={filterAccounts}
                accountFilter={effectiveFilter}
                onAccountFilterChange={setAccountFilter}
                ariaLabel={t("wbStats.credit.filterByDetail")}
                allowAll={false}
              />
            </div>
            <div className="mt-3 flex max-w-full gap-1 rounded-lg bg-muted p-1" role="tablist" aria-label={t("wbStats.credit.detailTypeAria")}>
              {(
                [
                  ["credits", t("wbStats.credit.tabCredits")],
                  ["requests", t("wbStats.credit.tabRequests")],
                ] as const
              ).map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  role="tab"
                  aria-selected={detailTab === value}
                  className={`min-w-0 flex-1 rounded-md px-2.5 py-1.5 text-xs transition-colors ${
                    detailTab === value
                      ? "bg-background font-medium text-foreground shadow-sm"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                  onClick={() => setDetailTab(value)}
                >
                  {label}
                </button>
              ))}
            </div>
          </CardHeader>
          {detailTab === "credits" ? (
            <div className="min-w-0">
              <ResourcesByAccount accounts={visibleAccounts} creditMap={creditMap} creditLoadingMap={creditLoadingMap} />
            </div>
          ) : (
            <OfficialUsageBreakdown officialUsage={officialUsage} accountId={effectiveFilter} />
          )}
        </Card>
      </section>
      {/* 最近事件卡片已隐藏。恢复时取消本注释。
      <section className="min-w-0 space-y-2.5" aria-labelledby="account-events-title">
        <div className="px-1">
          <h2 id="account-events-title" className="text-[13px] font-medium leading-5">最近事件</h2>
        </div>
        <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
          <CardHeader className="border-b px-4 py-3 sm:px-5">
            <CardDescription className="text-xs">签到单独记录，不会计入官方请求用量。</CardDescription>
          </CardHeader>
          <CardContent className="max-h-[340px] min-w-0 overflow-y-auto px-4 py-1 sm:px-5">
            {events.length === 0 ? (
              <div className="py-8 text-center text-sm text-muted-foreground">
                {effectiveFilter ? "该账号暂无最近事件。" : "暂无最近事件。"}
              </div>
            ) : (
              events.map((event, index) => <EventRow key={`${event.kind}-${event.ts}-${index}`} event={event} />)
            )}
          </CardContent>
        </Card>
      </section>
      */}
    </div>
  );
}

/* 积分明细默认展示全部账号后不再单独使用。
function UnselectedRecentEvents({ events }: { events: CreditStatsEvent[] }) {
  return (
    <section className="min-w-0 space-y-2.5" aria-labelledby="all-events-title">
      <div className="px-1">
        <h2 id="all-events-title" className="text-[13px] font-medium leading-5">最近事件</h2>
      </div>
      <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
        <CardHeader className="border-b px-4 py-3 sm:px-5">
          <CardDescription className="text-xs">签到与积分观察分开记录，签到不会计入消耗。</CardDescription>
        </CardHeader>
        <CardContent className="max-h-[340px] min-w-0 overflow-y-auto px-4 py-1 sm:px-5">
          {events.slice(0, 50).map((event, index) => (
            <EventRow key={`${event.kind}-${event.ts}-${index}`} event={event} />
          ))}
        </CardContent>
      </Card>
    </section>
  );
}
*/

/** 当前会话内按统计范围各存一份统计数据；只有「刷新统计」才会重新采集。 */
let cachedStatistics: Partial<Record<RegionFilter, CreditStatistics>> = {};
let statisticsInflight: Partial<Record<RegionFilter, Promise<CreditStatistics>>> = {};

/** 进入统计页时距上次刷新超过此时长（ms）则自动触发一次刷新统计 */
const STATISTICS_AUTO_REFRESH_MS = 30 * 60 * 1000;

/** 最近一次「刷新统计」完成的时刻（会话级，0 = 从未刷新过） */
let lastStatisticsRefreshAt = 0;

function cachedFor(region: RegionFilter): CreditStatistics | undefined {
  return cachedStatistics[region];
}

function loadCachedStatistics(region: RegionFilter, refresh: boolean): Promise<CreditStatistics> {
  const cached = cachedFor(region);
  if (!refresh && cached) return Promise.resolve(cached);
  const inflight = statisticsInflight[region];
  if (!refresh && inflight) return inflight;
  const pending = api.getCreditStatistics(refresh, region).then((next) => {
    cachedStatistics[region] = next;
    return next;
  });
  statisticsInflight[region] = pending;
  return pending.finally(() => {
    if (statisticsInflight[region] === pending) delete statisticsInflight[region];
  });
}

export default function CreditStatsPage() {
  const {
    accounts,
    global,
    creditMap,
    creditLoadingMap,
    fetchAllRegions,
    refreshCredits,
  } = useAccountsStore();
  const [region, setRegion] = useState<RegionFilter>(readPreferredRegion);
  const [stats, setStats] = useState<CreditStatistics | null>(() => cachedFor(readPreferredRegion()) ?? null);
  const [loading, setLoading] = useState(() => !cachedFor(readPreferredRegion()));
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(
    async (target: RegionFilter, refresh = false) => {
      if (!refresh && cachedFor(target)) {
        setStats(cachedFor(target)!);
        setLoading(false);
        setError(null);
        return;
      }
      setLoading(true);
      setError(null);
      try {
        // 三态都同时刷新两边：refresh=true 时对 cn + global 两版账号分别刷新积分资源，
        // 统计请求再由后端对两版分别发起官方采集。
        await fetchAllRegions();
        const accountState = useAccountsStore.getState();
        const accountError = accountState.error ?? accountState.global.error;
        if (accountError) {
          throw new Error(accountError);
        }
        if (refresh) {
          const cnIds = accountState.accounts.map((account) => account.id);
          const globalIds = accountState.global.accounts.map((account) => account.id);
          await Promise.all([
            cnIds.length > 0 ? refreshCredits(cnIds, { region: "cn" }) : Promise.resolve(),
            globalIds.length > 0 ? refreshCredits(globalIds, { region: "global" }) : Promise.resolve(),
          ]);
        }
        setStats(await loadCachedStatistics(target, refresh));
        if (refresh) lastStatisticsRefreshAt = Date.now();
      } catch (cause) {
        setError(api.asError(cause));
      } finally {
        setLoading(false);
      }
    },
    [fetchAllRegions, refreshCredits],
  );

  // 区域切换：只读查询，不隐式触发 refresh=true。
  useEffect(() => {
    persistPreferredRegion(region);
    void load(region);
  }, [load, region]);

  useEffect(() => {
    // 已有会话缓存且距上次刷新超过 30 分钟时，进入页面自动刷新一次统计
    const autoRefresh =
      !api.isDemoMode() &&
      cachedFor(region) !== undefined &&
      Date.now() - lastStatisticsRefreshAt >= STATISTICS_AUTO_REFRESH_MS;
    if (autoRefresh) void load(region, true);
    // 仅进入页面时判定一次；后续 region 切换走上方 effect。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 账号选项集合随 region 变化：cn 只列 cn 账号、global 只列 global、all 列两版并集。
  // 取后端返回的 `accounts[]` 为基准 —— 该集合本身已按统计范围过滤，天然满足「不串味」；
  // 仅按 store 中该版本账号库的顺序重排，并补上归属标注（合并视图徽标用）。
  const regionAccounts = useMemo(() => {
    const order =
      region === "cn" ? accounts : region === "global" ? global.accounts : [...accounts, ...global.accounts];
    const rank = new Map<string, number>();
    for (const [index, account] of order.entries()) {
      const regionOf = account.region ?? (region === "global" ? "global" : "cn");
      rank.set(`${account.id}:${regionOf}`, index);
      if (!rank.has(account.id)) rank.set(account.id, index);
    }
    const accountsOfRegion = stats?.accounts ?? [];
    return [...accountsOfRegion]
      .map((account) => ({ ...account }))
      .sort((a, b) => {
        const rankA = rank.get(`${a.accountId}:${a.region}`) ?? rank.get(a.accountId);
        const rankB = rank.get(`${b.accountId}:${b.region}`) ?? rank.get(b.accountId);
        if (rankA === undefined && rankB === undefined) return 0;
        if (rankA === undefined) return 1;
        if (rankB === undefined) return -1;
        return rankA - rankB;
      });
  }, [accounts, global.accounts, region, stats]);

  /** 当前统计范围内的账号数：以后端过滤后的 `accounts[]` 为准。 */
  const scopedAccountCount = stats?.accounts.length ?? 0;

  const officialUsage = stats?.officialUsage;
  const official = isOfficialUsageAvailable(officialUsage) ? officialUsage : undefined;
  const t = useT();

  return (
    <div className="mx-auto w-full max-w-[1180px] min-w-0 px-4 py-6 sm:px-8 sm:py-9">
      <header className="mb-4 flex min-w-0 flex-wrap items-start justify-between gap-4 sm:mb-5">
        <div className="min-w-0">
          <h1 className="text-[28px] font-semibold tracking-tight">{t("wbStats.credit.pageTitle")}</h1>
          <p className="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">
            {t("wbStats.credit.updatedPrefix")} {stats ? formatDateTime(official?.collectedAt ?? stats.generatedAt) : "—"}
            {region === "all" && t("wbStats.credit.bothRegions")}
          </p>
        </div>
        <DemoAction>
          <Button
            className="shrink-0"
            variant="outline"
            size="sm"
            onClick={() => void load(region, true)}
            disabled={loading}
          >
            {loading ? <Loader2 className="animate-spin" /> : <RefreshCw />}
            {t("wbStats.credit.refresh")}
          </Button>
        </DemoAction>
      </header>

      {/* 层级 1：统计范围（决定数据源，全页共享） */}
      <div className="mb-9 border-b pb-4 sm:mb-11">
        <RegionBar
          value={region}
          onChange={setRegion}
          disabled={loading}
          ariaLabel={t("wbStats.credit.scopeAria")}
        />
      </div>

      {error && (
        <Alert variant="destructive" className="mb-5">
          <CircleAlert />
          <AlertTitle>{t("wbStats.credit.loadFailed")}</AlertTitle>
          <AlertDescription className="flex flex-wrap items-center gap-3">
            <span>{error}</span>
            <Button size="sm" variant="outline" onClick={() => void load(region)}>
              {t("wbStats.credit.retry")}
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {loading && !stats ? (
        <div className="flex items-center gap-2 py-20 text-sm text-muted-foreground">
          <Loader2 className="animate-spin" />
          {t("wbStats.credit.collecting", { scope: regionFilterLabel(region) })}
        </div>
      ) : stats ? (
        <div className="min-w-0 space-y-12">
          {scopedAccountCount === 0 && (
            <Alert>
              <CircleAlert />
              <AlertTitle>
                {region === "all"
                  ? t("wbStats.credit.noAccountsAll")
                  : t("wbStats.credit.noAccountsScoped", { scope: regionFilterLabel(region) })}
              </AlertTitle>
              <AlertDescription>
                {region === "all"
                  ? t("wbStats.credit.noAccountsDescAll")
                  : t("wbStats.credit.noAccountsDescScoped", { scope: regionFilterLabel(region) })}
              </AlertDescription>
            </Alert>
          )}

          {officialUsage && officialUsage.status !== "complete" && (
            <Alert variant="warning">
              <CircleAlert />
              <AlertTitle>
                {officialUsage.status === "partial"
                  ? t("wbStats.credit.partialSync")
                  : t("wbStats.credit.officialUnavailableTitle")}
              </AlertTitle>
              <AlertDescription>
                {officialUsage.status === "partial"
                  ? t("wbStats.credit.partialSynced", {
                      ok: officialUsage.accounts.filter((account) => account.ok).length,
                      total: officialUsage.accounts.length,
                    })
                  : t("wbStats.credit.localFallbackNote")}
                {officialUsage.errors.length > 0 && (
                  <span className="text-xs text-amber-900/75">
                    {officialUsage.errors.map((item) => `${item.accountName}${t("shared.punct.colon")}${item.error}`).join(t("shared.punct.semicolon"))}
                  </span>
                )}
              </AlertDescription>
            </Alert>
          )}

          <Card className="min-w-0 gap-0 overflow-hidden rounded-2xl bg-card/70 py-0 shadow-none" aria-label={t("wbStats.credit.overviewAria")}>
            <CardContent className="grid min-w-0 grid-cols-1 divide-y divide-border/60 p-0 sm:grid-cols-4 sm:divide-y-0 sm:py-5">
              <StatMetric
                icon={Sparkles}
                label={t("wbStats.credit.metricRemaining")}
                value={formatCredits(stats.summary.currentRemaining)}
              />
              <StatMetric
                icon={TrendingDown}
                label={t("wbStats.credit.metricToday")}
                value={formatCredits(official ? official.summary.usageToday : stats.summary.usageToday)}
                divided
              />
              <StatMetric
                icon={CalendarDays}
                label={t("wbStats.credit.metric7d")}
                value={formatCredits(official ? official.summary.usage7Days : stats.summary.usage7Days)}
                divided
              />
              <StatMetric
                icon={CalendarRange}
                label={t("wbStats.credit.metricMonth")}
                value={formatCredits(official ? official.summary.usageThisMonth : stats.summary.usageThisMonth)}
                divided
              />
            </CardContent>
          </Card>

          {!official && !stats.coverageStartAt && stats.events.some((event) => event.kind === "checkin") && (
            <Alert>
              <CircleCheck />
              <AlertTitle>{t("wbStats.credit.onlyCheckin")}</AlertTitle>
              <AlertDescription>{t("wbStats.credit.onlyCheckinDesc")}</AlertDescription>
            </Alert>
          )}

          <TrendChart
            stats={stats}
            officialUsage={officialUsage}
          />

          {official && (
            <ModelBreakdown
              officialUsage={official}
            />
          )}

          {/* 与下方「积分明细」重复，先隐藏。
          <AccountTable stats={stats} officialUsage={officialUsage} selectedId={selectedId} onSelect={setSelectedId} />
          */}

          <SelectedAccountDetails
            officialUsage={officialUsage}
            regionAccounts={regionAccounts}
            creditMap={creditMap}
            creditLoadingMap={creditLoadingMap}
          />
        </div>
      ) : (
        <div className="rounded-xl border border-dashed px-4 py-16 text-center text-sm text-muted-foreground">
          {t("wbStats.credit.noStats")}
        </div>
      )}
    </div>
  );
}

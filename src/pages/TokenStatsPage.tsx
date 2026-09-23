import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { ComponentProps, ReactNode } from "react";
import {
  CircleAlert,
  ArrowDownToLine,
  ArrowUpFromLine,
  Check,
  Gauge,
  Loader2,
  MessagesSquare,
  RefreshCw,
  SlidersHorizontal,
  type LucideIcon,
} from "lucide-react";
import {
  Bar,
  CartesianGrid,
  ComposedChart,
  Line,
  Rectangle,
  XAxis,
  YAxis,
} from "recharts";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader } from "@/components/ui/card";
import {
  ChartContainer,
  ChartTooltip,
  type ChartConfig,
} from "@/components/ui/chart";
import { DemoAction } from "@/components/demo-action";
import { RegionBar } from "@/components/region-bar";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Skeleton } from "@/components/ui/skeleton";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { useT } from "@/lib/i18n";
import type { TranslationKey } from "@/locales/zh";
import * as api from "@/lib/api";
import { regionFilterLabel } from "@/lib/region";
import { getStackedSegmentVisualLayout } from "@/lib/stacked-bar-visuals";
import type {
  RegionFilter,
  TokenStatistics,
  TokenStatsGroup,
  TokenStatsSource,
  TokenStatsTotals,
} from "@/lib/types";

type SourceKey = TokenStatsSource["source"];
type RangeKey = "30d" | "today" | "7d" | "month";
type OverviewRangeKey = "today" | "7d" | "30d" | "total";
type DistributionKey = "projects" | "models";

const TOKEN_SOURCE_STORAGE_KEY = "buddy-switch:token-stats:source";
const TOKEN_REGION_STORAGE_KEY = "buddy-switch:token-stats:region";
const RANKING_LIMIT = 10;

/** 各来源展示名（与 SourceKey 一一对应）。 */
const SOURCE_LABELS: Record<SourceKey, string> = {
  workbuddy: "WorkBuddy",
  "codebuddy-cli": "CodeBuddy CLI",
  "codebuddy-ide": "CodeBuddy IDE",
  "workbuddy-ai": "WorkBuddy AI",
};

/** 各来源的展示顺序（合并视图下按此顺序排列）。 */
const SOURCE_ORDER: SourceKey[] = ["workbuddy", "codebuddy-cli", "codebuddy-ide", "workbuddy-ai"];

function isSourceKey(value: unknown): value is SourceKey {
  return (
    value === "workbuddy" ||
    value === "codebuddy-cli" ||
    value === "codebuddy-ide" ||
    value === "workbuddy-ai"
  );
}

function readPreferredTokenSource(): SourceKey {
  if (typeof window === "undefined") return "workbuddy";
  try {
    const stored = window.localStorage.getItem(TOKEN_SOURCE_STORAGE_KEY);
    return isSourceKey(stored) ? stored : "workbuddy";
  } catch {
    return "workbuddy";
  }
}

function persistPreferredTokenSource(source: SourceKey): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(TOKEN_SOURCE_STORAGE_KEY, source);
  } catch {
    // localStorage 在受限 WebView/隐私模式下可能不可写，不影响页面切换。
  }
}

function isRegionFilter(value: unknown): value is RegionFilter {
  return value === "cn" || value === "global" || value === "all";
}

/** 统计范围默认「国内版」；各页独立记忆，不与账号管理页联动。 */
function readPreferredRegion(): RegionFilter {
  if (typeof window === "undefined") return "cn";
  try {
    const stored = window.localStorage.getItem(TOKEN_REGION_STORAGE_KEY);
    return isRegionFilter(stored) ? stored : "cn";
  } catch {
    return "cn";
  }
}

function persistPreferredRegion(region: RegionFilter): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(TOKEN_REGION_STORAGE_KEY, region);
  } catch {
    // 同上：不可写时仅丢失记忆，不影响本次会话内的切换。
  }
}

const RANGE_OPTIONS: { key: RangeKey; label: TranslationKey }[] = [
  { key: "30d", label: "wbStats.token.range.30d" },
  { key: "today", label: "wbStats.token.range.today" },
  { key: "7d", label: "wbStats.token.range.7d" },
  { key: "month", label: "wbStats.token.range.month" },
];

const OVERVIEW_RANGE_OPTIONS: { key: OverviewRangeKey; label: TranslationKey }[] = [
  { key: "today", label: "wbStats.token.range.today" },
  { key: "7d", label: "wbStats.token.range.7d" },
  { key: "30d", label: "wbStats.token.range.30d" },
  { key: "total", label: "wbStats.token.overviewTotal" },
];

const chartConfig = {
  cacheRead: { label: "wbStats.token.cacheRead", color: "var(--data-series-emerald)" },
  uncachedInput: { label: "wbStats.token.uncachedInput", color: "var(--data-series-teal)" },
  output: { label: "wbStats.token.output", color: "var(--data-series-violet)" },
  cacheWrite: { label: "wbStats.token.cacheWrite", color: "var(--data-series-amber)" },
  records: { label: "wbStats.token.records", color: "var(--data-series-indigo)" },
} satisfies ChartConfig;

const compactTokenFormatter = new Intl.NumberFormat("en-US", {
  notation: "compact",
  compactDisplay: "short",
  maximumFractionDigits: 1,
});
const exact = new Intl.NumberFormat("zh-CN");
const exactTokenFormatter = new Intl.NumberFormat("en-US");

function formatTokenCompact(value: number): string {
  return compactTokenFormatter
    .format(value)
    .replace(/([kmb])$/i, (unit) => unit.toUpperCase());
}

function formatTokenExact(value: number): string {
  return exactTokenFormatter.format(value);
}

/** 展示总量：input 已包含 cacheRead，因此不能再次加上 cacheRead。 */
const tokenTotal = (value: TokenStatsTotals) =>
  value.input + value.output + value.cacheWrite;

const percentage = (value: number, sum: number) =>
  sum > 0 ? `${((value / sum) * 100).toFixed(1)}%` : "—";

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

function formatDateTime(timestamp?: number | null): string {
  if (!timestamp) return "—";
  return new Date(timestamp).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatChartDate(date: string): string {
  return date.slice(5).replace("-", "/");
}

function formatHeatmapDate(date: Date): string {
  return date.toLocaleDateString("zh-CN", {
    month: "long",
    day: "numeric",
  });
}

function rangeLabel(range: RangeKey): TranslationKey {
  return RANGE_OPTIONS.find((option) => option.key === range)?.label ?? "wbStats.token.range.30d";
}

function rangePoints(daily: TokenStatsGroup[], range: RangeKey): TokenStatsGroup[] {
  const today = dateKey(new Date());
  const firstDate =
    range === "today" ? today : range === "7d" ? dateDaysAgo(6) : dateDaysAgo(29);

  return daily
    .filter((point) => {
      if (range === "month") {
        return point.key.startsWith(`${today.slice(0, 7)}-`);
      }
      return point.key >= firstDate && point.key <= today;
    })
    .sort((left, right) => left.key.localeCompare(right.key));
}

function rangeBounds(range: RangeKey): { start: string; end: string } {
  const today = dateKey(new Date());
  if (range === "today") return { start: today, end: today };
  if (range === "7d") return { start: dateDaysAgo(6), end: today };
  if (range === "30d") return { start: dateDaysAgo(29), end: today };
  const monthStart = new Date();
  monthStart.setHours(12, 0, 0, 0);
  monthStart.setDate(1);
  return { start: dateKey(monthStart), end: today };
}

function fillRangePoints(points: TokenStatsGroup[], range: RangeKey): TokenStatsGroup[] {
  if (points.length === 0) return [];
  const { start, end } = rangeBounds(range);
  const byDate = new Map(points.map((point) => [point.key, point]));
  const cursor = new Date(`${start}T12:00:00`);
  const endDate = new Date(`${end}T12:00:00`);
  const filled: TokenStatsGroup[] = [];
  while (cursor <= endDate) {
    const key = dateKey(cursor);
    const point = byDate.get(key);
    filled.push(
      point ?? {
        key,
        total: 0,
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
        uncachedInput: 0,
        records: 0,
        cacheHitRate: null,
      },
    );
    cursor.setDate(cursor.getDate() + 1);
  }
  return filled;
}

function rangeTotals(points: TokenStatsGroup[]): TokenStatsTotals {
  const totals = points.reduce(
    (sum, point) => ({
      total: sum.total + tokenTotal(point),
      input: sum.input + point.input,
      output: sum.output + point.output,
      cacheRead: sum.cacheRead + point.cacheRead,
      cacheWrite: sum.cacheWrite + point.cacheWrite,
      uncachedInput: sum.uncachedInput + point.uncachedInput,
      records: sum.records + point.records,
      cacheHitRate: null,
    }),
    {
      total: 0,
      input: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
      uncachedInput: 0,
      records: 0,
      cacheHitRate: null,
    } satisfies TokenStatsTotals,
  );

  return {
    ...totals,
    cacheHitRate: totals.input > 0 ? totals.cacheRead / totals.input : null,
  };
}

function overviewTotals(source: TokenStatsSource, range: OverviewRangeKey): TokenStatsTotals {
  if (range === "total") return source.summary;
  return rangeTotals(rangePoints(source.daily, range));
}

function SectionTitle({ id, children }: { id: string; children: ReactNode }) {
  return (
    <div className="px-1">
      <h2 id={id} className="text-[13px] font-medium leading-5">
        {children}
      </h2>
    </div>
  );
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
      <div
        className="mt-3 max-w-full truncate text-[26px] font-semibold leading-8 tracking-[-0.025em] text-foreground tabular-nums"
        style={{
          fontFamily:
            '"Bricolage Grotesque Variable", "SF Pro Display", ui-sans-serif, sans-serif',
        }}
      >
        {value}
      </div>
    </div>
  );
}

function CompactComposition({ value }: { value: TokenStatsTotals }) {
  const t = useT();
  const total = tokenTotal(value);
  const rows: { labelKey: TranslationKey; value: number; color: string }[] = [
    { labelKey: "wbStats.token.compCache", value: value.cacheRead, color: "bg-primary" },
    { labelKey: "wbStats.token.compNew", value: value.uncachedInput, color: "bg-sky-500" },
    { labelKey: "wbStats.token.compOutput", value: value.output, color: "bg-violet-500" },
    { labelKey: "wbStats.token.compWrite", value: value.cacheWrite, color: "bg-amber-500" },
  ];

  return (
    <div className="min-w-0 flex-1">
      <div className="min-w-0">
        <div
          className="flex h-2 w-full max-w-[360px] overflow-hidden rounded-full bg-muted"
          role="img"
          aria-label={rows
            .map((row) => `${t(row.labelKey)} ${percentage(row.value, total)}`)
            .join("，")}
        >
          {rows.map((row) => (
            <span
              key={row.labelKey}
              className={`h-full min-w-0 ${row.color}`}
              style={{ width: total > 0 ? `${(row.value / total) * 100}%` : "0%" }}
              title={`${t(row.labelKey)} ${formatTokenCompact(row.value)} · ${percentage(row.value, total)}`}
              aria-label={`${t(row.labelKey)} ${formatTokenExact(row.value)} Token，${percentage(row.value, total)}`}
            />
          ))}
        </div>
        <div className="mt-1 flex flex-wrap gap-x-2.5 gap-y-0.5 text-[10px] text-muted-foreground">
          {rows.map((row) => (
            <span key={row.labelKey} className="inline-flex items-center gap-1 whitespace-nowrap">
              <span className={`size-1.5 rounded-full ${row.color}`} aria-hidden="true" />
              {t(row.labelKey)} {percentage(row.value, total)}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}

function Overview({ source }: { source: TokenStatsSource }) {
  const t = useT();
  const [range, setRange] = useState<OverviewRangeKey>("today");
  const summary = useMemo(() => overviewTotals(source, range), [range, source]);
  const cacheRate = summary.cacheHitRate;

  return (
    <section className="min-w-0 space-y-2.5" aria-labelledby="token-overview-title">
      <SectionTitle id="token-overview-title">{t("wbStats.token.overviewTitle")}</SectionTitle>
      <Card
        className="min-w-0 gap-0 overflow-hidden rounded-2xl bg-card/70 py-0 shadow-none"
        aria-label={t("wbStats.token.overviewTitle")}
      >
        <CardHeader className="gap-0 px-4 pt-3 pb-0 sm:px-5">
          <div className="flex min-w-0 flex-wrap items-center justify-between gap-3">
            <CompactComposition value={summary} />
            <Tabs
              className="min-w-0 shrink-0 gap-0"
              value={range}
              onValueChange={(value) => setRange(value as OverviewRangeKey)}
            >
              <TabsList
                className="grid h-auto w-full grid-cols-2 sm:inline-flex sm:w-fit sm:flex-wrap"
                aria-label={t("wbStats.token.overviewRangeAria")}
              >
                {OVERVIEW_RANGE_OPTIONS.map((option) => (
                  <TabsTrigger key={option.key} value={option.key} className="px-2">
                    {t(option.label)}
                  </TabsTrigger>
                ))}
              </TabsList>
            </Tabs>
          </div>
        </CardHeader>
        <CardContent className="grid min-w-0 grid-cols-1 divide-y divide-border/60 p-0 sm:grid-cols-4 sm:divide-y-0 sm:py-5">
          <StatMetric icon={MessagesSquare} label={t("wbStats.token.metricTotal")} value={formatTokenCompact(tokenTotal(summary))} />
          <StatMetric icon={ArrowDownToLine} label={t("wbStats.token.metricInput")} value={formatTokenCompact(summary.input)} divided />
          <StatMetric
            icon={ArrowUpFromLine}
            label={t("wbStats.token.metricOutput")}
            value={formatTokenCompact(summary.output)}
            divided
          />
          <StatMetric
            icon={Gauge}
            label={t("wbStats.token.metricCacheRate")}
            value={cacheRate == null ? "—" : `${(cacheRate * 100).toFixed(1)}%`}
            divided
          />
        </CardContent>
      </Card>
    </section>
  );
}

type TrendChartPoint = TokenStatsGroup & { date: string };
type TokenSeriesKey = "cacheRead" | "uncachedInput" | "output" | "cacheWrite";
type TokenBarShapeProps = ComponentProps<typeof Rectangle> & {
  segmentKey: TokenSeriesKey;
  payload?: TrendChartPoint;
  value?: number | [number, number];
};

const TOKEN_SERIES: TokenSeriesKey[] = [
  "cacheRead",
  "uncachedInput",
  "output",
  "cacheWrite",
];

/**
 * 使用真实 Rectangle 形状绘制每个堆叠段：整柱约束分配小段的 5px 视觉保底，
 * 但不修改 data 值；每个日期实际顶部的非零段才使用顶部圆角。
 */
function TokenBarShape({
  segmentKey,
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
}: TokenBarShapeProps) {
  if (width <= 0 || height <= 0) return null;
  const segmentIndex = TOKEN_SERIES.indexOf(segmentKey);
  const stackStart = Array.isArray(value) ? Number(value[0]) : 0;
  const layout = payload
    ? getStackedSegmentVisualLayout({
        values: TOKEN_SERIES.map((key) => payload[key]),
        segmentIndex,
        segmentHeight: height,
        segmentY: y,
        stackStart,
      })
    : null;
  if (!layout) {
    return (
      <Rectangle
        {...rest}
        x={x}
        y={y}
        width={width}
        height={height}
        fill={fill}
        radius={0}
        stroke={stroke ?? "var(--background)"}
        strokeWidth={strokeWidth ?? 1}
      />
    );
  }

  return (
    <Rectangle
      {...rest}
      x={x}
      y={layout.y}
      width={width}
      height={layout.height}
      fill={fill}
      radius={layout.isTop ? [6, 6, 0, 0] : 0}
      stroke={stroke ?? "var(--background)"}
      strokeWidth={strokeWidth ?? 1}
    />
  );
}

function TrendLegend() {
  const t = useT();
  const items: { key: string; labelKey: TranslationKey; color: string; kind: string }[] = [
    { key: "cacheRead", labelKey: "wbStats.token.cacheRead", color: "var(--data-series-emerald)", kind: "area" },
    { key: "uncachedInput", labelKey: "wbStats.token.uncachedInput", color: "var(--data-series-teal)", kind: "area" },
    { key: "output", labelKey: "wbStats.token.output", color: "var(--data-series-violet)", kind: "area" },
    { key: "cacheWrite", labelKey: "wbStats.token.cacheWrite", color: "var(--data-series-amber)", kind: "area" },
    { key: "records", labelKey: "wbStats.token.records", color: "var(--data-series-indigo)", kind: "line" },
  ];

  return (
    <div
      className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1.5 text-xs text-muted-foreground"
      aria-label={t("wbStats.token.legendAria")}
    >
      {items.map((item) => (
        <span key={item.key} className="inline-flex items-center gap-1.5 whitespace-nowrap">
          {item.kind === "line" ? (
            <span
              className="relative inline-flex h-2 w-4 shrink-0 items-center"
              aria-hidden="true"
            >
              <span
                className="absolute inset-x-0 top-1/2 border-t-2 border-dashed"
                style={{ borderColor: item.color }}
              />
              <span
                className="relative z-10 mx-auto size-1.5 rounded-full border border-background"
                style={{ backgroundColor: item.color }}
              />
            </span>
          ) : (
            <span
              className="size-2.5 shrink-0 rounded-[3px]"
              style={{ backgroundColor: item.color }}
              aria-hidden="true"
            />
          )}
          {t(item.labelKey)}
        </span>
      ))}
    </div>
  );
}

function TrendTooltipContent({
  active,
  payload,
}: {
  active?: boolean;
  payload?: Array<{ payload?: TrendChartPoint }>;
}) {
  const t = useT();
  if (!active || !payload?.length) return null;
  const point = payload[0]?.payload;
  if (!point) return null;
  const rows: { key: string; labelKey: TranslationKey; value: number; color: string }[] = [
    { key: "cacheRead", labelKey: "wbStats.token.cacheRead", value: point.cacheRead, color: "var(--data-series-emerald)" },
    {
      key: "uncachedInput",
      labelKey: "wbStats.token.uncachedInput",
      value: point.uncachedInput,
      color: "var(--data-series-teal)",
    },
    { key: "output", labelKey: "wbStats.token.output", value: point.output, color: "var(--data-series-violet)" },
    { key: "cacheWrite", labelKey: "wbStats.token.cacheWrite", value: point.cacheWrite, color: "var(--data-series-amber)" },
    { key: "records", labelKey: "wbStats.token.records", value: point.records, color: "var(--data-series-indigo)" },
  ];
  const total = tokenTotal(point);

  return (
    <div className="grid min-w-[13rem] gap-2 rounded-lg border border-border/50 bg-background px-3 py-2.5 text-xs shadow-xl">
      <div className="font-medium text-foreground">{formatChartDate(point.date)}</div>
      <div className="flex items-center justify-between border-b border-border/60 pb-1.5">
        <span className="text-muted-foreground">{t("wbStats.token.totalLabel")}</span>
        <span
          className="whitespace-nowrap font-mono font-semibold tabular-nums text-foreground"
          title={`${formatTokenExact(total)} Token`}
          aria-label={`${formatTokenExact(total)} Token`}
        >
          {formatTokenCompact(total)} Token
        </span>
      </div>
      <div className="grid gap-1.5">
        {rows.map((row) => (
          <div key={row.key} className="flex items-center gap-2">
            <span
              className={`shrink-0 ${row.key === "records" ? "h-0 w-3 border-t-2 border-dashed" : "size-2.5 rounded-[3px]"}`}
              style={
                row.key === "records"
                  ? { borderColor: row.color }
                  : { backgroundColor: row.color }
              }
              aria-hidden="true"
            />
            <span className="flex-1 text-muted-foreground">{t(row.labelKey)}</span>
            <span
              className="whitespace-nowrap font-mono font-medium tabular-nums text-foreground"
              title={row.key === "records" ? undefined : `${formatTokenExact(row.value)} Token`}
              aria-label={row.key === "records" ? undefined : `${formatTokenExact(row.value)} Token`}
            >
              {row.key === "records" ? exact.format(row.value) : formatTokenCompact(row.value)} {row.key === "records" ? t("wbStats.token.unitCalls") : "Token"}
              {row.key !== "records" ? (
                <span className="ml-1 font-sans text-[11px] font-normal text-muted-foreground">
                  ({percentage(row.value, total)})
                </span>
              ) : null}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function TrendChart({ source }: { source: TokenStatsSource }) {
  const t = useT();
  const [range, setRange] = useState<RangeKey>("30d");
  const [modelFilter, setModelFilter] = useState("all");
  const modelOptions = useMemo(
    () => (source.dailyByModel ? source.models.map((model) => model.key).filter(Boolean) : []),
    [source.dailyByModel, source.models],
  );
  useEffect(() => {
    if (modelFilter !== "all" && !modelOptions.includes(modelFilter)) setModelFilter("all");
  }, [modelFilter, modelOptions]);
  const dailySeries = modelFilter === "all"
    ? source.daily
    : source.dailyByModel?.[modelFilter] ?? [];
  const points = useMemo(
    () => fillRangePoints(rangePoints(dailySeries, range), range),
    [range, dailySeries],
  );
  const totals = useMemo(() => rangeTotals(points), [points]);
  const chartData: TrendChartPoint[] = points.map((point) => ({
    ...point,
    date: point.key,
  }));

  return (
    <section className="min-w-0 space-y-2.5" aria-labelledby="token-trend-title">
      <SectionTitle id="token-trend-title">{t("wbStats.token.trendTitle")}</SectionTitle>
      <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
        <CardHeader className="gap-0 px-4 pt-3 pb-0 sm:px-5">
          <div className="flex min-w-0 flex-wrap items-center justify-between gap-3">
            <CardDescription className="min-w-0 text-xs">
              {t("wbStats.token.trendDesc")}
            </CardDescription>
            <div className="flex max-w-full flex-wrap items-center gap-2">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-8 max-w-[190px] gap-1.5 px-2.5 text-xs text-muted-foreground hover:text-foreground"
                    aria-label={t("wbStats.token.filterByModel")}
                  >
                    <SlidersHorizontal className="size-3.5 shrink-0" />
                    <span className="truncate">{modelFilter === "all" ? t("wbStats.token.allModels") : modelFilter}</span>
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="max-h-80 w-56 overflow-y-auto">
                  <DropdownMenuItem onSelect={() => setModelFilter("all")}>
                    <SlidersHorizontal className="size-3.5 shrink-0" />
                    {t("wbStats.token.allModels")}
                    {modelFilter === "all" && <Check className="ml-auto size-3.5 shrink-0" />}
                  </DropdownMenuItem>
                  {modelOptions.length > 0 && <DropdownMenuSeparator />}
                  {modelOptions.map((model) => (
                    <DropdownMenuItem key={model} onSelect={() => setModelFilter(model)}>
                      <span className="min-w-0 flex-1 truncate">{model}</span>
                      {modelFilter === model && <Check className="ml-auto size-3.5 shrink-0" />}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuContent>
              </DropdownMenu>
              <div className="flex max-w-full flex-wrap gap-1 rounded-lg bg-muted p-1" aria-label={t("wbStats.token.trendRangeAria")}>
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
          {chartData.length === 0 ? (
            <div className="rounded-lg border border-dashed px-4 py-10 text-center text-sm text-muted-foreground">
              {t("wbStats.token.emptyRange")}
            </div>
          ) : (
            <>
              <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
                <TrendLegend />
              </div>
              <div className="mb-1 flex items-center justify-end px-1 text-[11px] font-medium text-muted-foreground">
                <span className="font-normal">{t("wbStats.token.axisNote")}</span>
              </div>
              <ChartContainer config={chartConfig} className="h-64 w-full sm:h-72">
                <ComposedChart
                  data={chartData}
                  margin={{ top: 8, right: 8, left: 0, bottom: 0 }}
                  barCategoryGap="18%"
                >
                  <CartesianGrid vertical={false} strokeDasharray="3 3" />
                  <XAxis
                    dataKey="date"
                    tickLine={false}
                    axisLine={false}
                    tickMargin={8}
                    minTickGap={24}
                    tickFormatter={(value) => formatChartDate(String(value))}
                  />
                  <YAxis
                    yAxisId="tokens"
                    tickLine={false}
                    axisLine={false}
                    width={48}
                    tickFormatter={(value) => formatTokenCompact(Number(value))}
                  />
                  <YAxis
                    yAxisId="calls"
                    orientation="right"
                    tickLine={false}
                    axisLine={false}
                    width={46}
                    allowDecimals={false}
                    tickFormatter={(value) => exact.format(Number(value))}
                  />
                  <ChartTooltip
                    cursor={{ fill: "var(--muted)", opacity: 0.4 }}
                    content={<TrendTooltipContent />}
                  />
                  {TOKEN_SERIES.map((key) => (
                    <Bar
                      key={key}
                      yAxisId="tokens"
                      dataKey={key}
                      stackId="token"
                      fill={`var(--color-${key})`}
                      stroke="var(--background)"
                      strokeWidth={2}
                      maxBarSize={28}
                      shape={<TokenBarShape segmentKey={key} />}
                      isAnimationActive={false}
                    >
                    </Bar>
                  ))}
                  <Line
                    yAxisId="calls"
                    type="monotone"
                    dataKey="records"
                    stroke="var(--color-records)"
                    strokeWidth={2}
                    strokeDasharray="7 4"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    dot={false}
                    activeDot={{ r: 4, fill: "var(--color-records)", stroke: "var(--background)", strokeWidth: 2 }}
                    isAnimationActive={false}
                  />
                </ComposedChart>
              </ChartContainer>
              <div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                <span>
                  {t(rangeLabel(range))} {t("wbStats.token.trendSummary", { total: formatTokenCompact(tokenTotal(totals)), calls: exact.format(totals.records) })}
                </span>
                <span>{t("wbStats.token.coverage", { date: formatDateTime(source.coverageEndAt) })}</span>
              </div>
              <p className="sr-only">
                {chartData
                  .map(
                    (point) =>
                      t("wbStats.token.srSummary", { date: point.date, total: formatTokenCompact(tokenTotal(point)), calls: exact.format(point.records) }),
                  )
                  .join("；")}
              </p>
            </>
          )}
        </CardContent>
      </Card>
    </section>
  );
}

const HEATMAP_LEVEL_CLASS = [
  "bg-muted/70",
  "bg-primary/20",
  "bg-primary/40",
  "bg-primary/65",
  "bg-primary",
] as const;

function Heatmap({ groups }: { groups: TokenStatsGroup[] }) {
  const t = useT();
  const scrollerRef = useRef<HTMLDivElement>(null);
  const valueByDate = new Map(groups.map((group) => [group.key, tokenTotal(group)]));
  const recordByDate = new Map(groups.map((group) => [group.key, group.records]));
  const today = new Date();
  today.setHours(12, 0, 0, 0);
  const todayKey = dateKey(today);
  const start = new Date(today);
  start.setDate(start.getDate() - start.getDay() - 52 * 7);

  const weeks = Array.from({ length: 53 }, (_, weekIndex) =>
    Array.from({ length: 7 }, (_, dayIndex) => {
      const date = new Date(start);
      date.setDate(start.getDate() + weekIndex * 7 + dayIndex);
      const key = dateKey(date);
      return {
        date,
        key,
        value: valueByDate.get(key) ?? 0,
        records: recordByDate.get(key) ?? 0,
        future: key > todayKey,
      };
    }),
  );
  const max = Math.max(
    1,
    ...weeks.flatMap((week) => week.filter((day) => !day.future).map((day) => day.value)),
  );
  const monthLabels = weeks.map((week, weekIndex) => {
    const firstOfMonth = week.find((day) => day.date.getDate() === 1);
    let labelDate: Date | null = null;
    if (firstOfMonth && firstOfMonth.key <= todayKey) {
      labelDate = firstOfMonth.date;
    } else if (weekIndex === 0) {
      labelDate = week[0].date;
    }
    if (!labelDate || dateKey(labelDate) > todayKey) return null;
    return labelDate.toLocaleDateString("zh-CN", {
      month: "short",
    });
  });
  const activeDays = weeks
    .flat()
    .filter((day) => !day.future && day.value > 0).length;

  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    if (!scroller) return;
    scroller.scrollLeft = scroller.scrollWidth - scroller.clientWidth;
  }, [groups]);

  return (
    <section className="min-w-0 space-y-2.5" aria-labelledby="token-heatmap-title">
      <SectionTitle id="token-heatmap-title">{t("wbStats.token.heatmapTitle")}</SectionTitle>
      <Card className="min-w-0 gap-0 rounded-xl py-0 shadow-none">
        <CardHeader className="px-4 pt-4 pb-0 sm:px-5">
          <div className="flex items-center justify-between gap-3">
            <CardDescription className="text-xs">{t("wbStats.token.heatmapDesc")}</CardDescription>
            <span className="shrink-0 text-xs font-medium text-foreground">{t("wbStats.token.heatmapDaily")}</span>
          </div>
        </CardHeader>
        <CardContent className="min-w-0 px-4 pt-5 pb-5 sm:px-5">
          <div ref={scrollerRef} className="overflow-x-auto pb-1">
            <div
              className="min-w-[760px]"
              role="img"
              aria-label={t("wbStats.token.heatmapAria", { activeDays })}
            >
              <div
                className="grid gap-1"
                style={{ gridTemplateColumns: "repeat(53, minmax(10px, 1fr))" }}
                aria-hidden="true"
              >
                {weeks.flatMap((week, weekIndex) =>
                  week.map((day, dayIndex) => {
                    const level = day.value
                      ? Math.max(1, Math.ceil(Math.sqrt(day.value / max) * 4))
                      : 0;
                    const cell = (
                      <span
                        key={day.key}
                        className={`aspect-square min-w-0 rounded-[3px] ${
                          day.future ? "opacity-0" : HEATMAP_LEVEL_CLASS[level]
                        }`}
                        style={{ gridColumn: weekIndex + 1, gridRow: dayIndex + 1 }}
                        aria-label={t("wbStats.token.heatmapCell", { date: formatHeatmapDate(day.date), count: formatTokenExact(day.value) })}
                      />
                    );

                    if (day.future) return cell;

                    return (
                      <Tooltip key={day.key} disableHoverableContent>
                        <TooltipTrigger asChild>{cell}</TooltipTrigger>
                        <TooltipContent
                          side="top"
                          sideOffset={7}
                          className="pointer-events-none rounded-lg bg-foreground px-2.5 py-1.5 text-xs leading-4 text-background shadow-md"
                        >
                          {t("wbStats.token.heatmapTip", { date: formatHeatmapDate(day.date), count: formatTokenCompact(day.value) })}
                          {day.records > 0 ? ` · ${t("wbStats.token.heatmapCalls", { calls: exact.format(day.records) })}` : ""}
                        </TooltipContent>
                      </Tooltip>
                    );
                  }),
                )}
              </div>
              <div
                className="mt-3 grid gap-1 text-[11px] text-muted-foreground"
                style={{ gridTemplateColumns: "repeat(53, minmax(10px, 1fr))" }}
                aria-hidden="true"
              >
                {monthLabels.map((label, index) => (
                  <span key={`${index}-${label ?? "empty"}`} className="whitespace-nowrap">
                    {label}
                  </span>
                ))}
              </div>
            </div>
          </div>
        </CardContent>
      </Card>
    </section>
  );
}

function Ranking({
  groups,
  denominator,
  description,
  controls,
}: {
  groups: TokenStatsGroup[];
  denominator: number;
  description: string;
  controls?: ReactNode;
}) {
  const t = useT();
  const rows = groups.slice(0, RANKING_LIMIT);

  return (
    <Card className="min-w-0 gap-0 rounded-xl py-0 shadow-none">
      <CardHeader className="gap-0 px-4 pt-3 pb-0 sm:px-5">
        <div className="flex min-w-0 flex-wrap items-center justify-between gap-3">
          <CardDescription className="min-w-0 text-xs">{description}</CardDescription>
          {controls}
        </div>
      </CardHeader>
      <CardContent className="space-y-3 px-4 pt-3 pb-5 sm:px-5">
        {rows.map((row, index) => {
          const amount = tokenTotal(row);
          const share = percentage(amount, denominator);
          return (
            <div key={row.key}>
              <div className="mb-1.5 flex min-w-0 items-center gap-3 text-xs">
                <span className="w-5 shrink-0 font-mono text-muted-foreground">
                  {String(index + 1).padStart(2, "0")}
                </span>
                <span className="min-w-0 flex-1 truncate font-medium" title={row.key}>
                  {row.key}
                </span>
                <span
                  className="shrink-0 tabular-nums"
                  title={`${formatTokenExact(amount)} Token`}
                  aria-label={`${formatTokenExact(amount)} Token`}
                >
                  {formatTokenCompact(amount)}
                </span>
                <span className="w-12 shrink-0 text-right text-muted-foreground tabular-nums">
                  {share}
                </span>
              </div>
              <div className="ml-8 h-1.5 overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full rounded-full bg-primary"
                  style={{ width: share === "—" ? "0%" : share }}
                />
              </div>
            </div>
          );
        })}
        {rows.length === 0 && (
          <div className="rounded-lg border border-dashed px-4 py-10 text-center text-sm text-muted-foreground">
            {t("wbStats.token.emptyRank")}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function SessionRanking({ groups, denominator }: { groups: TokenStatsGroup[]; denominator: number }) {
  const t = useT();
  const rows = groups.slice(0, RANKING_LIMIT);

  return (
    <section className="min-w-0 space-y-2.5" aria-labelledby="token-sessions-title">
      <SectionTitle id="token-sessions-title">{t("wbStats.token.sessionsTitle")}</SectionTitle>
      <Card className="min-w-0 gap-0 rounded-xl py-0 shadow-none">
        <CardHeader className="px-4 pt-3 pb-0 sm:px-5">
          <CardDescription className="text-xs">{t("wbStats.token.sessionsDesc")}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3 px-4 pt-3 pb-5 sm:px-5">
          {rows.map((row, index) => {
            const amount = tokenTotal(row);
            const share = percentage(amount, denominator);
            const label = row.title?.trim() || t("wbStats.token.unnamedSession");
            const detail = [row.project, row.title ? undefined : row.sessionId]
              .filter(Boolean)
              .join(" · ");
            return (
              <div key={row.key}>
                <div className="mb-1.5 flex min-w-0 items-start gap-3 text-xs">
                  <span className="mt-0.5 w-5 shrink-0 font-mono text-muted-foreground">
                    {String(index + 1).padStart(2, "0")}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium" title={label} aria-label={label}>
                      {label}
                    </div>
                    {detail && (
                      <div className="mt-0.5 truncate text-[11px] text-muted-foreground" title={detail}>
                        {detail}
                      </div>
                    )}
                  </div>
                  <span
                    className="mt-0.5 shrink-0 tabular-nums"
                    title={`${formatTokenExact(amount)} Token`}
                    aria-label={`${formatTokenExact(amount)} Token`}
                  >
                    {formatTokenCompact(amount)}
                  </span>
                  <span className="mt-0.5 w-12 shrink-0 text-right text-muted-foreground tabular-nums">
                    {share}
                  </span>
                </div>
                <div className="ml-8 h-1.5 overflow-hidden rounded-full bg-muted">
                  <div
                    className="h-full rounded-full bg-primary"
                    style={{ width: share === "—" ? "0%" : share }}
                  />
                </div>
              </div>
            );
          })}
          {rows.length === 0 && (
            <div className="rounded-lg border border-dashed px-4 py-10 text-center text-sm text-muted-foreground">
              {t("wbStats.token.emptyRank")}
            </div>
          )}
        </CardContent>
      </Card>
    </section>
  );
}

function Distribution({ source }: { source: TokenStatsSource }) {
  const t = useT();
  const [distribution, setDistribution] = useState<DistributionKey>("projects");
  const groups = source[distribution];

  return (
    <section className="min-w-0 space-y-2.5" aria-labelledby="token-distribution-title">
      <SectionTitle id="token-distribution-title">{t("wbStats.token.distTitle")}</SectionTitle>
      <Ranking
        groups={groups}
        denominator={tokenTotal(source.summary)}
        description={
          distribution === "projects"
            ? t("wbStats.token.rankProjectsDesc")
            : t("wbStats.token.rankModelsDesc")
        }
        controls={
          <div className="flex rounded-lg bg-muted p-1" role="group" aria-label={t("wbStats.token.distAria")}>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className={`h-7 px-2.5 text-xs ${
                distribution === "projects"
                  ? "bg-background font-medium text-foreground shadow-sm hover:bg-background"
                  : "text-muted-foreground"
              }`}
              aria-pressed={distribution === "projects"}
              onClick={() => setDistribution("projects")}
            >
              {t("wbStats.token.byProject")}
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className={`h-7 px-2.5 text-xs ${
                distribution === "models"
                  ? "bg-background font-medium text-foreground shadow-sm hover:bg-background"
                  : "text-muted-foreground"
              }`}
              aria-pressed={distribution === "models"}
              onClick={() => setDistribution("models")}
            >
              {t("wbStats.token.byModel")}
            </Button>
          </div>
        }
      />
    </section>
  );
}

function Dashboard({ source }: { source: TokenStatsSource }) {
  const t = useT();
  const denominator = tokenTotal(source.summary);

  if (source.summary.records === 0) {
    return (
      <div className="rounded-xl border border-dashed px-4 py-16 text-center text-sm text-muted-foreground">
        <div>
          {source.filesScanned > 0
            ? t("wbStats.token.scannedFiles", { count: exact.format(source.filesScanned) })
            : t("wbStats.token.noLogs")}
        </div>
        {source.parseErrors > 0 && (
          <div className="mt-2 text-xs text-amber-600">
            {t("wbStats.token.skippedRecords", { count: exact.format(source.parseErrors) })}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="min-w-0 space-y-12">
      <Overview source={source} />
      <TrendChart source={source} />
      <Heatmap groups={source.daily} />
      <Distribution source={source} />
      <SessionRanking groups={source.sessions} denominator={denominator} />
      {source.parseErrors > 0 && (
        <p className="flex items-center gap-1.5 px-1 text-xs text-amber-600">
          <CircleAlert className="size-3.5" aria-hidden="true" />
          {t("wbStats.token.skippedRecords", { count: exact.format(source.parseErrors) })}
        </p>
      )}
    </div>
  );
}

function TokenStatsLoadingSkeleton() {
  const t = useT();
  return (
    <div
      className="min-w-0 space-y-12"
      role="status"
      aria-label={t("wbStats.token.scanning")}
    >
      <span className="sr-only">{t("wbStats.token.scanning")}</span>
      <p className="flex items-center gap-2 text-sm text-muted-foreground" aria-hidden="true">
        <span className="size-1.5 rounded-full bg-primary/70" />
        {t("wbStats.token.scanning")}
      </p>

      <section className="min-w-0 space-y-2.5" aria-hidden="true">
        <Skeleton className="h-4 w-20" />
        <Card className="min-w-0 gap-0 overflow-hidden rounded-2xl bg-card/70 py-0 shadow-none">
          <CardContent className="grid min-w-0 grid-cols-1 divide-y divide-border/60 p-0 sm:grid-cols-4 sm:divide-y-0 sm:py-5">
            {Array.from({ length: 4 }, (_, index) => (
              <div
                key={index}
                className={`flex min-w-0 flex-col items-center justify-center px-4 py-5 sm:py-3 ${
                  index > 0 ? "sm:border-l sm:border-border/60" : ""
                }`}
              >
                <Skeleton className="h-4 w-24" />
                <Skeleton className="mt-3 h-8 w-28" />
              </div>
            ))}
          </CardContent>
        </Card>
      </section>

      <section className="min-w-0 space-y-2.5" aria-hidden="true">
        <Skeleton className="h-4 w-36" />
        <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
          <CardHeader className="gap-0 px-4 pt-3 pb-0 sm:px-5">
            <div className="flex min-w-0 flex-wrap items-center justify-between gap-3">
              <Skeleton className="h-4 w-64 max-w-full" />
              <Skeleton className="h-8 w-40 rounded-lg" />
            </div>
          </CardHeader>
          <CardContent className="min-w-0 px-4 pt-3 pb-4 sm:px-5">
            <div className="mb-3 flex flex-wrap items-center gap-4">
              {Array.from({ length: 5 }, (_, index) => (
                <Skeleton key={index} className="h-4 w-16" />
              ))}
            </div>
            <Skeleton className="h-64 w-full rounded-lg sm:h-72" />
          </CardContent>
        </Card>
      </section>

      <section className="min-w-0 space-y-2.5" aria-hidden="true">
        <Skeleton className="h-4 w-24" />
        <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
          <CardHeader className="px-4 pt-4 pb-0 sm:px-5">
            <Skeleton className="h-4 w-56 max-w-full" />
          </CardHeader>
          <CardContent className="px-4 pt-5 pb-5 sm:px-5">
            <Skeleton className="h-44 w-full rounded-lg" />
          </CardContent>
        </Card>
      </section>

      <section className="min-w-0 space-y-2.5" aria-hidden="true">
        <Skeleton className="h-4 w-24" />
        <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
          <CardHeader className="gap-0 px-4 pt-3 pb-0 sm:px-5">
            <div className="flex items-center justify-between gap-3">
              <Skeleton className="h-4 w-56 max-w-full" />
              <Skeleton className="h-8 w-32 rounded-lg" />
            </div>
          </CardHeader>
          <CardContent className="space-y-3 px-4 pt-3 pb-5 sm:px-5">
            {Array.from({ length: RANKING_LIMIT }, (_, index) => (
              <div key={index} className="space-y-1.5">
                <div className="flex items-center gap-3">
                  <Skeleton className="h-4 w-5" />
                  <Skeleton className="h-4 flex-1" />
                  <Skeleton className="h-4 w-16" />
                </div>
                <Skeleton className="ml-8 h-1.5 w-[70%] rounded-full" />
              </div>
            ))}
          </CardContent>
        </Card>
      </section>

      <section className="min-w-0 space-y-2.5" aria-hidden="true">
        <Skeleton className="h-4 w-36" />
        <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
          <CardHeader className="px-4 pt-3 pb-0 sm:px-5">
            <Skeleton className="h-4 w-56 max-w-full" />
          </CardHeader>
          <CardContent className="space-y-3 px-4 pt-3 pb-5 sm:px-5">
            {Array.from({ length: RANKING_LIMIT }, (_, index) => (
              <div key={index} className="space-y-1.5">
                <div className="flex items-start gap-3">
                  <Skeleton className="mt-0.5 h-4 w-5" />
                  <Skeleton className="h-8 flex-1" />
                  <Skeleton className="h-4 w-16" />
                </div>
                <Skeleton className="ml-8 h-1.5 w-[70%] rounded-full" />
              </div>
            ))}
          </CardContent>
        </Card>
      </section>
    </div>
  );
}

export default function TokenStatsPage() {
  const [stats, setStats] = useState<TokenStatistics | null>(null);
  const [region, setRegion] = useState<RegionFilter>(readPreferredRegion);
  const [active, setActive] = useState<SourceKey>(readPreferredTokenSource);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const t = useT();

  // region 是页面级数据源：切换即重新取数；旧结果保留到新结果返回，避免闪白。
  useEffect(() => {
    let disposed = false;
    setLoading(true);
    setError(null);
    api
      .getTokenStatistics(undefined, region)
      .then((result) => {
        if (!disposed) setStats(result);
      })
      .catch((cause) => {
        if (!disposed) setError(api.asError(cause));
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [region, reload]);

  useEffect(() => {
    persistPreferredRegion(region);
  }, [region]);

  // 当前 region 下可见的来源（按固定展示顺序）。后端已按范围返回对应 sources，
  // 这里再兜底，保证旧后端 / 异常响应下也不会超出该范围的来源集合。
  const visibleSources = useMemo(() => {
    const present = new Set((stats?.sources ?? []).map((item) => item.source));
    const allowed =
      region === "global"
        ? SOURCE_ORDER.filter((key) => key === "workbuddy-ai")
        : region === "cn"
          ? SOURCE_ORDER.filter((key) => key !== "workbuddy-ai")
          : SOURCE_ORDER;
    return allowed.filter((key) => present.has(key));
  }, [region, stats]);

  // 选中的来源在新 region 下不可见时自动回退到该 region 的首个合法来源，避免空白页。
  useEffect(() => {
    if (visibleSources.length === 0) return;
    const next = visibleSources.includes(active)
      ? active
      : visibleSources.includes("workbuddy")
        ? "workbuddy"
        : visibleSources[0];
    if (next !== active) setActive(next);
  }, [active, visibleSources]);

  const source = stats?.sources.find((item) => item.source === active);

  return (
    <div className="mx-auto w-full max-w-[1180px] min-w-0 px-4 py-6 sm:px-8 sm:py-9">
      <header className="mb-4 flex min-w-0 flex-wrap items-start justify-between gap-4 sm:mb-5">
        <div className="min-w-0">
          {loading && !stats ? (
            <div aria-hidden="true">
              <Skeleton className="h-8 w-40" />
              <Skeleton className="mt-2 h-5 w-64 max-w-full" />
            </div>
          ) : (
            <>
              <h1 className="text-[28px] font-semibold tracking-tight">{t("wbStats.token.pageTitle")}</h1>
              <p className="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">
                {t("wbStats.token.updatedPrefix")} {stats ? formatDateTime(stats.generatedAt) : "—"}
                {region === "all" && t("wbStats.token.bothRegions")}
              </p>
            </>
          )}
        </div>
        <div className="flex max-w-full flex-wrap items-center justify-end gap-2">
          <DemoAction>
            <Button
              className="shrink-0"
              variant="outline"
              size="sm"
              onClick={() => setReload((value) => value + 1)}
              disabled={loading}
            >
              {loading ? <Loader2 className="animate-spin" /> : <RefreshCw />}
              {t("wbStats.token.refresh")}
            </Button>
          </DemoAction>
        </div>
      </header>

      {/* 层级 1：统计范围（决定数据源，全页共享） */}
      <div className="mb-9 border-b pb-4 sm:mb-11">
        <RegionBar
          value={region}
          onChange={setRegion}
          disabled={loading}
          ariaLabel={t("wbStats.token.scopeAria")}
        />
      </div>

      {error && (
        <Alert variant="destructive" className="mb-5">
          <CircleAlert />
          <AlertTitle>{t("wbStats.token.loadFailed")}</AlertTitle>
          <AlertDescription className="flex flex-wrap items-center gap-3">
            <span>{error}</span>
            <Button size="sm" variant="outline" onClick={() => setReload((value) => value + 1)}>
              {t("wbStats.token.retry")}
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {loading && !stats ? (
        <TokenStatsLoadingSkeleton />
      ) : stats ? (
        <div className="min-w-0 space-y-5">
          {/* 层级 2：来源切片（作用于当前范围已选定的数据） */}
          {visibleSources.length > 0 && (
            <Tabs
              className="min-w-0 gap-0"
              value={active}
              onValueChange={(value) => {
                if (!isSourceKey(value)) return;
                if (!visibleSources.includes(value)) return;
                setActive(value);
                persistPreferredTokenSource(value);
              }}
            >
              <TabsList className="h-auto max-w-full flex-wrap" aria-label={t("wbStats.token.sourceAria")}>
                {visibleSources.map((key) => (
                  <TabsTrigger key={key} className="max-w-full whitespace-normal" value={key}>
                    {SOURCE_LABELS[key]}
                  </TabsTrigger>
                ))}
              </TabsList>
            </Tabs>
          )}
          {source ? (
            <Dashboard source={source} />
          ) : (
            <div className="rounded-xl border border-dashed px-4 py-16 text-center text-sm text-muted-foreground">
              {t("wbStats.token.noSource", { scope: regionFilterLabel(region) })}
            </div>
          )}
        </div>
      ) : (
        !error && (
          <div className="rounded-xl border border-dashed px-4 py-16 text-center text-sm text-muted-foreground">
            {t("wbStats.token.noStats")}
          </div>
        )
      )}
    </div>
  );
}

import { useCallback, useEffect, useState } from "react";
import {
  AlertTriangle,
  Copy,
  Download,
  Eraser,
  ExternalLink,
  FileText,
  HardDriveDownload,
  HardDriveUpload,
  Info,
  Loader2,
  RefreshCw,
  RotateCcw,
  Save,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { toast } from "sonner";

import { DemoAction } from "@/components/demo-action";
import { HoursEditor } from "@/components/schedule-hours-editor";
import { TraeVariantSwitch } from "@/components/trae-variant-switch";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CardContent } from "@/components/ui/card";
import { SettingsFieldRow, SettingsGroup, SettingsRow } from "@/components/settings-primitives";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import * as api from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import {
  loadScheduleConfig,
  saveSchedulePatch,
  SCHEDULE_CONFIG_KEY,
} from "@/lib/schedule-config";
import { normalizeTraeGatewayLogs } from "@/lib/trae-gateway";
import { isAutoDetected, traeProductLabel } from "@/lib/trae-client";
import { useTraeVariant } from "@/lib/use-trae-variant";
import {
  findRegionStatus,
  loadTraeVariantStatuses,
  TRAE_VARIANT_FALLBACK,
  TRAE_VARIANTS_KEY,
} from "@/lib/trae-variant-status";
import type { ScheduleConfig } from "@/lib/types";
import type {
  TraeCapabilities,
  TraeDeviceResetReport,
  TraeEnvStatus,
  TraeGatewayLogEntry,
  TraeLogKind,
  TraeLogsResponse,
  TraeProfileInfo,
  TraeProfilesOverview,
  TraeSettings,
  TraeVariantId,
  TraeVariantStatus,
} from "@/lib/trae-types";
import { cn } from "@/lib/utils";
import { useCachedResource } from "@/lib/use-cached-resource";
import { useT } from "@/lib/i18n";
import type { TranslationKey } from "@/locales/zh";

// ---------------------------------------------------------------------------
// 板块骨架：与 WorkBuddy 的 SettingsPage 共用同一实现
// （`SettingsGroup` / `SettingsRow` / `SettingsFieldRow` 从共享模块导入）
// ---------------------------------------------------------------------------

/** 能力徽章。 */
function CapabilityBadge({ labelKey, supported }: { labelKey: TranslationKey; supported: boolean }) {
  const t = useT();
  return (
    <Badge variant={supported ? "secondary" : "outline"} className={cn(!supported && "text-muted-foreground")}>
      {t(labelKey)}
      {supported ? "" : t("trae.page.settings.capabilityUnsupported")}
    </Badge>
  );
}

// ---------------------------------------------------------------------------
// 运行日志（原「系统日志」页的运行日志 Tab）
// ---------------------------------------------------------------------------

const KIND_OPTIONS: { value: string; labelKey: TranslationKey }[] = [
  { value: "all", labelKey: "trae.page.settings.kindAll" },
  { value: "app", labelKey: "trae.page.settings.kindApp" },
  { value: "checkin", labelKey: "trae.page.settings.kindCheckin" },
  { value: "switch", labelKey: "trae.page.settings.kindSwitch" },
];

const KIND_TONE: Record<TraeLogKind, "default" | "success" | "warning"> = {
  app: "default",
  checkin: "success",
  switch: "warning",
};

const KIND_LABEL: Record<TraeLogKind, TranslationKey> = {
  app: "trae.page.settings.kindApp",
  checkin: "trae.page.settings.kindCheckin",
  switch: "trae.page.settings.kindSwitch",
};

/**
 * 运行日志分组。
 *
 * 原先是一个独立的「系统日志」导航项；既然侧栏收敛到与 WorkBuddy 同构的五项，
 * 日志作为「排障用的设置类信息」下沉到设置页，能力本身完整保留
 * （类型/日期/关键字筛选、自动刷新、复制、导出 CSV）。
 */
function RuntimeLogsSection() {
  const t = useT();
  // 运行日志按产品线分家（`checkin` / `switch` 两条来源各读各的，`app` 刻意共用）。
  // 变体取自侧栏分区，不由探测推导 —— 探测回答「本机哪条线最近活跃」，
  // 不回答「用户此刻想管哪条线」。
  const [variant] = useTraeVariant();
  const [kind, setKind] = useState<string>("all");
  const [date, setDate] = useState<string>("all");
  const [keywordDraft, setKeywordDraft] = useState("");
  const [keyword, setKeyword] = useState("");
  const [autoRefresh, setAutoRefresh] = useState(true);

  /**
   * 键里带上四个筛选条件（含变体）：任何一个变了都是**另一份**结果，
   * 漏在键外就会出现「换了日期、列表还是旧的」。
   */
  const load = useCallback(
    () =>
      api.getTraeLogs({
        kind,
        date: date === "all" ? undefined : date,
        keyword: keyword || undefined,
        variant,
      }),
    [kind, date, keyword, variant],
  );

  const {
    data,
    loading,
    error,
    refresh,
  } = useCachedResource<TraeLogsResponse>(
    `trae:logs:${variant}:${kind}:${date}:${keyword}`,
    load,
  );

  // 自动刷新只在「没有未提交的输入」时跑：否则用户正在输入关键字，
  // 每次刷新都会把列表换成旧条件的结果，看起来像在抖。
  const dirty = keywordDraft !== keyword;
  useEffect(() => {
    if (!autoRefresh || dirty) return;
    const timer = setInterval(() => void refresh(), 2000);
    return () => clearInterval(timer);
  }, [autoRefresh, dirty, refresh]);

  const entries = data?.entries ?? [];
  const counts = data?.counts;

  /** 未知 kind（后端新增的类型）仍回落到原始值，与改造前的表现一致。 */
  const kindText = (kind: TraeLogKind): string => {
    const key: TranslationKey | undefined = KIND_LABEL[kind];
    return key ? t(key) : kind;
  };

  const copyAll = async () => {
    if (entries.length === 0) return;
    await copyText(entries.map((entry) => `[${entry.time}] [${entry.kind}] ${entry.message}`).join("\n"));
  };

  const exportCsv = () => {
    if (entries.length === 0) return;
    const header = `${t("trae.page.settings.csvHeader")}\n`;
    const body = entries
      .map((entry) => `${entry.time}\t${kindText(entry.kind)}\t${entry.message}`)
      .join("\n");
    // BOM 前缀：Excel 不带它会按本地代码页解读，中文全乱。
    const blob = new Blob([`\ufeff${header}${body}`], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `trae-logs-${new Date().toISOString().slice(0, 10)}.csv`;
    anchor.click();
    URL.revokeObjectURL(url);
  };

  return (
    <SettingsGroup id="trae-settings-logs" title={t("trae.page.settings.logsGroup")}>
      <CardContent className="space-y-0 p-0">
      <div className="p-4 sm:p-5">
        <div className="flex flex-wrap items-center gap-2">
          <Select value={kind} onValueChange={setKind}>
            <SelectTrigger size="sm" className="w-28">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {KIND_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {t(option.labelKey)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select value={date} onValueChange={setDate}>
            <SelectTrigger size="sm" className="w-40">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t("trae.page.settings.allDates")}</SelectItem>
              {(data?.dates ?? []).map((day) => (
                <SelectItem key={day} value={day}>
                  {day}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <div className="relative min-w-[180px] flex-1">
            <Search className="absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={keywordDraft}
              onChange={(event) => setKeywordDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") setKeyword(keywordDraft.trim());
              }}
              placeholder={t("trae.page.settings.searchPlaceholder")}
              className="h-8 pl-8"
            />
          </div>

          <Button variant="outline" size="sm" onClick={() => setKeyword(keywordDraft.trim())}>
            <Search />
            {t("trae.page.settings.search")}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setKeywordDraft("");
              setKeyword("");
              setKind("all");
              setDate("all");
            }}
          >
            <X />
            {t("trae.page.settings.logsReset")}
          </Button>
        </div>

        <div className="mt-3 flex flex-wrap items-center justify-between gap-3 border-t border-border/60 pt-3">
          <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
            <span className="flex items-center gap-1.5">
              <Switch
                checked={autoRefresh}
                onCheckedChange={setAutoRefresh}
                aria-label={t("trae.page.settings.autoRefreshAria")}
              />
              {t("trae.page.settings.autoRefresh")}
            </span>
            {counts && (
              <span className="flex flex-wrap items-center gap-1.5">
                <span>{t("trae.page.settings.totalCount", { count: counts.all })}</span>
                <Badge variant="secondary">
                  {t("trae.page.settings.kindCount", {
                    label: t("trae.page.settings.kindApp"),
                    count: counts.app,
                  })}
                </Badge>
                <Badge variant="success">
                  {t("trae.page.settings.kindCount", {
                    label: t("trae.page.settings.kindCheckin"),
                    count: counts.checkin,
                  })}
                </Badge>
                <Badge variant="warning">
                  {t("trae.page.settings.kindCount", {
                    label: t("trae.page.settings.kindSwitch"),
                    count: counts.switch,
                  })}
                </Badge>
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" disabled={entries.length === 0} onClick={() => void copyAll()}>
              <Copy />
              {t("trae.page.settings.copy")}
            </Button>
            <Button variant="ghost" size="sm" disabled={entries.length === 0} onClick={exportCsv}>
              <Download />
              {t("trae.page.settings.exportCsv")}
            </Button>
            <Button variant="outline" size="sm" disabled={loading} onClick={() => void refresh()}>
              {loading ? <Loader2 className="animate-spin" /> : <RefreshCw />}
              {t("trae.page.settings.refresh")}
            </Button>
          </div>
        </div>
      </div>

      {error && (
        <div className="px-4 pb-4 sm:px-5">
          <Alert variant="destructive">
            <AlertTriangle />
            <AlertTitle>{t("trae.page.settings.logsLoadFailed")}</AlertTitle>
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        </div>
      )}

      <div className="border-t border-border/60">
        <div className="flex items-center justify-between border-b border-border/60 px-4 py-2.5 sm:px-5">
          <span className="flex items-center gap-2 text-[13px] font-medium">
            <FileText className="size-4 text-muted-foreground" />
            {t("trae.page.settings.detailLabel")}
          </span>
          <span className="text-xs text-muted-foreground">
            {data
              ? t("trae.page.settings.showing", { shown: entries.length, total: data.total })
              : t("trae.page.settings.loading")}
          </span>
        </div>

        <div className="max-h-[420px] overflow-auto">
          {loading && !data ? (
            <div className="space-y-2 p-4">
              {Array.from({ length: 6 }, (_, index) => (
                <Skeleton key={index} className="h-5 w-full" />
              ))}
            </div>
          ) : entries.length === 0 ? (
            <div className="flex flex-col items-center gap-2 px-4 py-12 text-center">
              <Trash2 className="size-7 text-muted-foreground/50" />
              <p className="text-sm font-medium">{t("trae.page.settings.logsEmptyTitle")}</p>
              <p className="max-w-md text-xs text-muted-foreground">
                {t("trae.page.settings.logsEmptyBody")}
              </p>
            </div>
          ) : (
            <ul className="divide-y divide-border/60">
              {entries.map((entry, index) => (
                <li
                  key={`${entry.time}-${index}`}
                  className="flex items-start gap-3 px-4 py-2 font-mono text-xs hover:bg-muted/40 sm:px-5"
                >
                  <span className="shrink-0 text-muted-foreground">
                    {entry.time || t("trae.page.settings.noTime")}
                  </span>
                  <Badge variant={KIND_TONE[entry.kind] ?? "secondary"} className="shrink-0 font-sans">
                    {kindText(entry.kind)}
                  </Badge>
                  <span className="break-all leading-5">{entry.message}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>

      {/* 边界说明与文件状态：不写清楚，用户会把「日志是空的」当成 bug。 */}
      <div className="border-t border-border/60 px-4 py-3 sm:px-5">
        <p className="text-xs leading-5 text-muted-foreground">
          {data?.note ?? t("trae.page.settings.noteFallback")}
        </p>
        <ul className="mt-2 space-y-1">
          {(data?.sources ?? []).map((source) => (
            <li key={source.kind} className="flex flex-wrap items-center gap-2">
              <Badge variant={source.exists ? "success" : "outline"}>
                {source.exists ? t("trae.page.settings.sourceExists") : t("trae.page.settings.sourceMissing")}
              </Badge>
              <span className="break-all font-mono text-[11px] text-muted-foreground">{source.path}</span>
            </li>
          ))}
        </ul>
        {data?.logDir && (
          <p className="mt-2 break-all font-mono text-[11px] text-muted-foreground">
            {t("trae.page.settings.logDir", { dir: data.logDir })}
          </p>
        )}
      </div>
      </CardContent>
    </SettingsGroup>
  );
}

// ---------------------------------------------------------------------------
// 网关请求日志（原「系统日志」页的网关 Tab）
// ---------------------------------------------------------------------------

const STATUS_OPTIONS: { value: string; labelKey: TranslationKey }[] = [
  { value: "all", labelKey: "trae.page.settings.statusAll" },
  { value: "ok", labelKey: "trae.page.settings.statusOk" },
  { value: "client", labelKey: "trae.page.settings.statusClient" },
  { value: "server", labelKey: "trae.page.settings.statusServer" },
];

function formatClock(ts: number): string {
  if (!ts) return "—";
  const date = new Date(ts);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

function formatTokens(value: number | null | undefined): string {
  if (value === null || value === undefined) return "—";
  if (value >= 1000) return `${(value / 1000).toFixed(1)}K`;
  return String(value);
}

function statusTone(status: number): string {
  if (status >= 200 && status < 300) return "text-emerald-600 dark:text-emerald-400";
  if (status >= 400) return "text-destructive";
  return "text-muted-foreground";
}

/**
 * 网关请求日志分组。
 *
 * 与「Token 统计」页同源（同一份 `api_gateway_logs.json`），但用途不同：
 * 统计页回答「用了多少」，这里回答「刚刚那次请求发生了什么」。
 * 提供状态码筛选、关键字搜索、逐条详情抽屉与清空。
 */
function GatewayLogsSection() {
  const t = useT();
  const [statusFilter, setStatusFilter] = useState<string>("all");
  const [keyword, setKeyword] = useState("");
  const [detail, setDetail] = useState<TraeGatewayLogEntry | null>(null);
  const [clearing, setClearing] = useState(false);
  /**
   * 「清空日志」失败的文案。
   *
   * 读侧的错误归快照缓存所有（`error` 只读），而清空是**动作**、它的失败没有快照
   * 可挂，因此单独一个本地状态。两者共用同一个提示位，与改造前的表现一致。
   */
  const [actionError, setActionError] = useState<string | null>(null);

  /** 日志已在加载器里归一化：消费方拿到的一定是数组，不必各自兜底。 */
  const load = useCallback(async () => normalizeTraeGatewayLogs(await api.getTraeGatewayLogs()), []);

  const {
    data,
    loading,
    error,
    refresh,
    patch,
  } = useCachedResource<TraeGatewayLogEntry[]>("trae:gateway-logs", load);
  const logs = data ?? [];
  const failure = error ?? actionError;

  const clear = async () => {
    setClearing(true);
    try {
      await api.clearTraeGatewayLogs();
      setActionError(null);
      patch(() => []);
      toast.success(t("trae.page.settings.gwCleared"));
    } catch (e) {
      setActionError(api.asError(e));
    } finally {
      setClearing(false);
    }
  };

  const visible = logs
    .filter((entry) => {
      if (statusFilter === "ok") return entry.status >= 200 && entry.status < 300;
      if (statusFilter === "client") return entry.status >= 400 && entry.status < 500;
      if (statusFilter === "server") return entry.status >= 500;
      return true;
    })
    .filter((entry) => {
      const needle = keyword.trim().toLowerCase();
      if (!needle) return true;
      return [entry.account, entry.model, entry.endpoint, entry.error]
        .filter((value): value is string => typeof value === "string")
        .some((value) => value.toLowerCase().includes(needle));
    })
    // 最新在前：排查问题看的是「刚刚那次」，而不是最早那次。
    .sort((left, right) => right.ts - left.ts);

  const totalTokens = visible.reduce(
    (sum, entry) => sum + (entry.promptTokens ?? 0) + (entry.completionTokens ?? 0),
    0,
  );

  return (
    <SettingsGroup id="trae-settings-gateway-logs" title={t("trae.page.settings.gatewayGroup")}>
      <CardContent className="space-y-0 p-0">
      <div className="p-4 sm:p-5">
        <div className="flex flex-wrap items-center gap-2">
          <Select value={statusFilter} onValueChange={setStatusFilter}>
            <SelectTrigger size="sm" className="w-40">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {STATUS_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {t(option.labelKey)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <div className="relative min-w-[180px] flex-1">
            <Search className="absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={keyword}
              onChange={(event) => setKeyword(event.target.value)}
              placeholder={t("trae.page.settings.gwSearchPlaceholder")}
              className="h-8 pl-8"
            />
          </div>
          <Button variant="outline" size="sm" disabled={loading} onClick={() => void refresh()}>
            {loading ? <Loader2 className="animate-spin" /> : <RefreshCw />}
            {t("trae.page.settings.refresh")}
          </Button>
          <DemoAction>
            <Button
              variant="ghost"
              size="sm"
              disabled={clearing || logs.length === 0}
              onClick={() => void clear()}
            >
              {clearing ? <Loader2 className="animate-spin" /> : <Eraser />}
              {t("trae.page.settings.gwClear")}
            </Button>
          </DemoAction>
        </div>
        <p className="mt-3 border-t border-border/60 pt-3 text-xs text-muted-foreground">
          {t("trae.page.settings.gwSummary", {
            count: visible.length,
            filtered:
              visible.length !== logs.length
                ? t("trae.page.settings.gwFiltered", { total: logs.length })
                : "",
            tokens:
              totalTokens > 0 ? t("trae.page.settings.gwTokens", { tokens: formatTokens(totalTokens) }) : "",
          })}
        </p>
      </div>

      {failure && (
        <div className="px-4 pb-4 sm:px-5">
          <Alert variant="destructive">
            <AlertTriangle />
            <AlertTitle>{t("trae.page.settings.gwLoadFailed")}</AlertTitle>
            <AlertDescription>{failure}</AlertDescription>
          </Alert>
        </div>
      )}

      <div className="max-h-[420px] overflow-auto border-t border-border/60">
        {loading && logs.length === 0 ? (
          <div className="space-y-2 p-4">
            {Array.from({ length: 5 }, (_, index) => (
              <Skeleton key={index} className="h-6 w-full" />
            ))}
          </div>
        ) : visible.length === 0 ? (
          <div className="flex flex-col items-center gap-2 px-4 py-12 text-center">
            <Trash2 className="size-7 text-muted-foreground/50" />
            <p className="text-sm font-medium">{t("trae.page.settings.gwEmptyTitle")}</p>
            <p className="max-w-md text-xs text-muted-foreground">
              {t("trae.page.settings.gwEmptyBody")}
            </p>
          </div>
        ) : (
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-muted/60 text-xs text-muted-foreground backdrop-blur">
              <tr>
                <th className="px-4 py-2 text-left font-medium">{t("trae.page.settings.colTime")}</th>
                <th className="px-3 py-2 text-left font-medium">{t("trae.page.settings.colAccount")}</th>
                <th className="px-3 py-2 text-left font-medium">{t("trae.page.settings.colModel")}</th>
                <th className="px-3 py-2 text-left font-medium">{t("trae.page.settings.colStatus")}</th>
                <th className="px-3 py-2 text-right font-medium">{t("trae.page.settings.colLatency")}</th>
                <th className="px-3 py-2 text-right font-medium">{t("trae.page.settings.colToken")}</th>
              </tr>
            </thead>
            <tbody>
              {visible.map((entry, index) => (
                <tr
                  key={`${entry.ts}-${index}`}
                  className="cursor-pointer border-t border-border/60 hover:bg-muted/40"
                  onClick={() => setDetail(entry)}
                >
                  <td className="whitespace-nowrap px-4 py-2 font-mono text-xs text-muted-foreground">
                    {formatClock(entry.ts)}
                  </td>
                  <td className="px-3 py-2 text-xs">{entry.account ?? "—"}</td>
                  <td className="px-3 py-2 text-xs">{entry.model ?? "—"}</td>
                  <td className={cn("px-3 py-2 font-mono text-xs", statusTone(entry.status))}>
                    {entry.status || "—"}
                  </td>
                  <td className="px-3 py-2 text-right text-xs tabular-nums text-muted-foreground">
                    {entry.latencyMs ? `${entry.latencyMs} ms` : "—"}
                  </td>
                  <td className="px-3 py-2 text-right text-xs tabular-nums text-muted-foreground">
                    {entry.promptTokens === null && entry.completionTokens === null
                      ? "—"
                      : `${formatTokens(entry.promptTokens)} / ${formatTokens(entry.completionTokens)}`}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      <div className="border-t border-border/60 px-4 py-2.5 text-xs text-muted-foreground sm:px-5">
        {t("trae.page.settings.gwFooter")}
      </div>

      <Dialog open={detail !== null} onOpenChange={(open) => !open && setDetail(null)}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t("trae.page.settings.gwDetailTitle")}</DialogTitle>
            <DialogDescription>{t("trae.page.settings.gwDetailDesc")}</DialogDescription>
          </DialogHeader>
          <pre className="max-h-[50vh] overflow-auto whitespace-pre-wrap break-all rounded-lg bg-muted/50 p-3 font-mono text-xs">
            {detail ? JSON.stringify(detail, null, 2) : ""}
          </pre>
          <DialogFooter>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                detail && void copyText(JSON.stringify(detail, null, 2), t("trae.page.settings.gwDetailCopied"))
              }
            >
              <Copy />
              {t("trae.page.settings.copy")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      </CardContent>
    </SettingsGroup>
  );
}

// ---------------------------------------------------------------------------
// 登录态快照（原「登录态快照」页）
// ---------------------------------------------------------------------------

/**
 * 登录态快照分组。
 *
 * 快照保存的是 Trae 客户端 userData 下 9 类核心文件，用于把账号的完整登录态
 * 在「当前」与「目标」之间搬运。原先是一个独立导航项，现下沉到设置页。
 *
 * **恢复是高级操作**：它会直接覆盖客户端当前登录态，因此这里保留行内风险标注
 * 与二次确认 Dialog；常规路径应使用「账号管理」页的「切换」（切换会先自动保存快照）。
 */
function ProfilesSection({
  data,
  loading,
  busy,
  variant,
  onReload,
  onRun,
}: {
  data: TraeProfilesOverview | null;
  loading: boolean;
  busy: string | null;
  /** 当前产品线：快照按变体分家，备份/恢复/删除都必须带上它。 */
  variant: TraeVariantId;
  onReload: () => void;
  onRun: (key: string, label: string, action: () => Promise<unknown>) => Promise<void>;
}) {
  const t = useT();
  const [backupSlot, setBackupSlot] = useState("");
  const [pendingDelete, setPendingDelete] = useState<TraeProfileInfo | null>(null);
  const [pendingRestore, setPendingRestore] = useState<TraeProfileInfo | null>(null);

  const profiles = data?.profiles ?? [];

  return (
    <SettingsGroup id="trae-settings-profiles" title={t("trae.page.settings.profilesGroup")}>
      <CardContent className="space-y-0 p-0">
      <SettingsRow className="flex-wrap gap-y-1">
        <div className="flex flex-wrap items-center gap-x-6 gap-y-2 text-sm">
          <span className="text-muted-foreground">
            {t("trae.page.settings.currentAccount")}
            {/* 主文本给**展示名**（账号库里的 `name`），uid 退到次要位置：
                本页的槽位行（`profile.slot`）本身就是 uid，用户要对照的是它，
                所以 uid 不能丢，但也不该占据「谁」这个位置。 */}
            <span className="ml-2 font-medium text-foreground">
              {data?.currentAccountName ?? data?.currentAccount ?? t("trae.page.settings.unknown")}
            </span>
            {data?.currentAccountName && data?.currentAccount && (
              <span className="ml-1.5 text-xs text-muted-foreground/70">{data.currentAccount}</span>
            )}
          </span>
          <span className="flex items-center gap-2">
            {t("trae.page.settings.clientLabel")}
            <span
              className={cn(
                "inline-flex items-center gap-1.5 font-medium",
                data?.clientRunning ? "text-emerald-600" : "text-muted-foreground",
              )}
            >
              <span
                className={cn(
                  "size-2 rounded-full",
                  data?.clientRunning ? "bg-emerald-500" : "bg-muted-foreground/50",
                )}
              />
              {data?.clientRunning ? t("trae.page.settings.running") : t("trae.page.settings.notRunning")}
            </span>
          </span>
          <span className="text-muted-foreground">
            {t("trae.page.settings.snapshotCount", { count: profiles.length })}
          </span>
        </div>
        <Button variant="ghost" size="sm" onClick={onReload} disabled={loading}>
          <RefreshCw className={cn(loading && "animate-spin")} />
          {t("trae.page.settings.refresh")}
        </Button>
      </SettingsRow>

      {data?.dataDir && (
        <div className="border-b border-border/50 px-4 py-2.5 text-xs text-muted-foreground sm:px-5">
          {t("trae.page.settings.dataDirLabel")}
          <code className="font-mono">{data.dataDir}</code>
        </div>
      )}

      <SettingsFieldRow
        label={t("trae.page.settings.backupLabel")}
        description={t("trae.page.settings.backupDesc", { count: data?.coreEntryCount ?? 9 })}
        htmlFor="trae-backup-slot"
      >
        <div className="flex w-full flex-wrap items-center justify-end gap-2 sm:w-auto">
          <Input
            id="trae-backup-slot"
            className="h-8 sm:w-56"
            value={backupSlot}
            onChange={(event) => setBackupSlot(event.target.value)}
            placeholder={t("trae.page.settings.slotPlaceholder")}
          />
          <DemoAction>
            <Button
              size="sm"
              variant="outline"
              disabled={!backupSlot.trim() || busy === "backup"}
              onClick={() =>
                void onRun("backup", t("trae.page.settings.backup"), () =>
                  api.traeBackupProfile(backupSlot.trim(), variant),
                ).then(() => setBackupSlot(""))
              }
            >
              {busy === "backup" ? <Loader2 className="animate-spin" /> : <HardDriveUpload />}
              {t("trae.page.settings.backup")}
            </Button>
          </DemoAction>
        </div>
      </SettingsFieldRow>

      {loading && !data ? (
        <div className="space-y-2 p-4 sm:p-5">
          <Skeleton className="h-14 w-full" />
          <Skeleton className="h-14 w-full" />
        </div>
      ) : profiles.length === 0 ? (
        <p className="px-4 py-8 text-center text-sm text-muted-foreground sm:px-5">
          {t("trae.page.settings.profilesEmpty")}
        </p>
      ) : (
        <div className="divide-y divide-border/50">
          {profiles.map((profile) => (
            <div key={profile.slot} className="flex flex-wrap items-center justify-between gap-3 px-4 py-3 sm:px-5">
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="truncate text-[13px] font-medium">{profile.slot}</span>
                  {profile.slot === "last" && (
                    <Badge variant="outline">{t("trae.page.settings.fallbackSlot")}</Badge>
                  )}
                  {profile.slot === data?.currentAccount && (
                    <Badge variant="secondary">{t("trae.page.settings.currentAccount")}</Badge>
                  )}
                </div>
                <div className="mt-1 flex flex-wrap items-center gap-x-5 gap-y-1 text-xs text-muted-foreground">
                  <span>{profile.sizeText}</span>
                  <span>{t("trae.page.settings.fileCount", { count: profile.fileCount })}</span>
                  <span>{t("trae.page.settings.updatedAt", { time: profile.lastModified })}</span>
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-1.5">
                <DemoAction>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={busy === `restore-${profile.slot}`}
                    onClick={() => setPendingRestore(profile)}
                  >
                    {busy === `restore-${profile.slot}` ? (
                      <Loader2 className="animate-spin" />
                    ) : (
                      <HardDriveDownload />
                    )}
                    {t("trae.page.settings.restore")}
                  </Button>
                </DemoAction>
                <DemoAction>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-destructive hover:text-destructive"
                    disabled={busy === `delete-${profile.slot}`}
                    onClick={() => setPendingDelete(profile)}
                  >
                    <Trash2 />
                    {t("trae.page.settings.delete")}
                  </Button>
                </DemoAction>
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="border-t border-border/60 px-4 py-3 text-xs leading-5 text-muted-foreground sm:px-5">
        {t("trae.page.settings.profilesFooter")}
      </div>

      {/* 恢复确认：明确告知会覆盖当前登录态 */}
      <Dialog open={pendingRestore !== null} onOpenChange={(open) => !open && setPendingRestore(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("trae.page.settings.restoreTitle")}</DialogTitle>
            <DialogDescription>
              {t("trae.page.settings.restoreBodyLead", { slot: pendingRestore?.slot ?? "" })}
              <strong className="font-medium text-foreground">
                {t("trae.page.settings.restoreBodyStrong")}
              </strong>
              {t("trae.page.settings.restoreBodyTail")}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setPendingRestore(null)}>
              {t("trae.page.settings.cancel")}
            </Button>
            <Button
              onClick={() => {
                const target = pendingRestore;
                if (!target) return;
                setPendingRestore(null);
                void onRun(`restore-${target.slot}`, t("trae.page.settings.restore"), () =>
                  api.traeRestoreProfile(target.slot, variant),
                );
              }}
            >
              {t("trae.page.settings.restoreConfirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 删除确认 */}
      <Dialog open={pendingDelete !== null} onOpenChange={(open) => !open && setPendingDelete(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("trae.page.settings.deleteTitle")}</DialogTitle>
            <DialogDescription>
              {t("trae.page.settings.deleteBody", {
                slot: pendingDelete?.slot ?? "",
                count: pendingDelete?.fileCount ?? 0,
                size: pendingDelete?.sizeText ?? "",
              })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setPendingDelete(null)}>
              {t("trae.page.settings.cancel")}
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                const target = pendingDelete;
                if (!target) return;
                setPendingDelete(null);
                void onRun(`delete-${target.slot}`, t("trae.page.settings.delete"), () =>
                  api.traeDeleteProfile(target.slot, variant),
                );
              }}
            >
              {t("trae.page.settings.delete")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      </CardContent>
    </SettingsGroup>
  );
}

// ---------------------------------------------------------------------------
// 页面
// ---------------------------------------------------------------------------

/**
 * 设置页除「设置项」之外的只读快照。
 *
 * 设置项本身不在这里：它与账号页的「跳过今日已签到」开关是同一份
 * `get_trae_settings`，因此单独一把键 `trae:settings`、两个页面共用。
 */
interface SettingsPageSnapshot {
  env: TraeEnvStatus;
  capabilities: TraeCapabilities;
  profiles: TraeProfilesOverview;
}

/**
 * 「设置」页（Trae 分区）。
 *
 * 板块顺序与 WorkBuddy 设置页同构（外观 → 客户端 → 端口与网络 → 策略 → 高级）。
 * 原先独立的「登录态快照」与「系统日志」两个导航项作为高级板块下沉到这里，
 * 能力完整保留——侧栏收敛的是入口数量，不是功能范围。
 */
export default function TraeSettingsPage() {
  const t = useT();
  // 快照按产品线分家（`paths::profiles_dir_for`），故这里也必须带上变体。
  const [variant] = useTraeVariant();
  const [saving, setSaving] = useState(false);
  /**
   * 排程（自动签到）的保存中标志，与上面的 `saving` **刻意分开**：
   * 两者写的是两份不同的文件（`trae_settings.json` / `schedule_config.json`），
   * 共用一个标志会让底部「保存」按钮在保存排程时转圈，把操作归因到错的地方。
   */
  const [scheduleSaving, setScheduleSaving] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [resetOpen, setResetOpen] = useState(false);
  const [resetBusy, setResetBusy] = useState(false);
  const [resetReport, setResetReport] = useState<TraeDeviceResetReport | null>(null);

  /**
   * 设置页除「设置项」之外的只读快照。
   *
   * 设置项**不在这里**：它与账号页的「跳过今日已签到」开关是同一份
   * `get_trae_settings`，因此单独一把键 `trae:settings`、两个页面共用 ——
   * 一处改完，另一处下次挂载拿到的就是新值。
   */
  const loadSnapshot = useCallback(async (): Promise<SettingsPageSnapshot> => {
    const [envData, capabilityData, profileData] = await Promise.all([
      api.getTraeEnv(),
      api.getTraeCapabilities(),
      api.getTraeProfiles(variant),
    ]);
    return { env: envData, capabilities: capabilityData, profiles: profileData };
  }, [variant]);

  const {
    data: pageSnapshot,
    loading,
    error,
    refresh: load,
  } = useCachedResource<SettingsPageSnapshot>(`trae:settings-page:${variant}`, loadSnapshot);

  /** 设置项本身：与账号页共用 `trae:settings`，写入一律走 `patchSettings`（乐观更新）。 */
  const { data: settings, patch: patchSettings } = useCachedResource<TraeSettings>(
    "trae:settings",
    api.getTraeSettings,
  );

  const env = pageSnapshot?.env ?? null;
  const capabilities = pageSnapshot?.capabilities ?? null;
  const profiles = pageSnapshot?.profiles ?? null;

  /**
   * 「关于」外链要指向**当前区域的官方站点**，域由后端给（`consoleBase`），
   * 前端不另立常量 —— 漏改的症状不是报错，而是国际版用户被静默导到国内站。
   *
   * 键与侧栏那颗运行状态圆点共用（`TRAE_VARIANTS_KEY`）⇒ 这里是**缓存命中**，
   * 不会为本页多发一次探测；首帧（尚未取到）先用兜底快照，
   * 免得外链先消失再出现。
   */
  const { data: variantStatuses } = useCachedResource<TraeVariantStatus[]>(
    TRAE_VARIANTS_KEY,
    loadTraeVariantStatuses,
  );
  const consoleBase =
    findRegionStatus(variantStatuses ?? TRAE_VARIANT_FALLBACK, variant)?.consoleBase ?? null;

  /**
   * 自动签到的**排程**（到点触发 / 启动补跑 / 小时表）。
   *
   * ⚠️ 它不在 `get_trae_settings` 里：排程是**全局单份**（`schedule_config.json`），
   * 按**任务**分家而不是按产品/区域；WorkBuddy 的六类任务读写的是同一份文件。
   * 键与 Trae 账号页的工具栏开关共用（`SCHEDULE_CONFIG_KEY`）⇒ 一处改完另一处
   * 下次挂载即拿到新值，不会出现「设置页开着、账号页显示关着」。
   */
  const { data: schedule, patch: patchSchedule } = useCachedResource<ScheduleConfig>(
    SCHEDULE_CONFIG_KEY,
    loadScheduleConfig,
  );

  /**
   * 保存排程：乐观落本地 → 整份提交 → 回读权威值；失败回滚并提示。
   *
   * 提交必须带**当前整份**配置（见 `saveSchedulePatch`）：后端按整份解析，
   * 只发一个字段会让其余字段被默认值覆盖 —— 那是静默的配置丢失。
   */
  async function saveSchedule(next: Partial<ScheduleConfig>) {
    if (!schedule || scheduleSaving) return;
    const previous = schedule;
    setScheduleSaving(true);
    patchSchedule((prev) => ({ ...prev, ...next }));
    try {
      const saved = await saveSchedulePatch(previous, next);
      patchSchedule(() => saved);
    } catch (e) {
      patchSchedule(() => previous);
      toast.error(t("trae.page.settings.autoCheckinSaveFailed"), { description: api.asError(e) });
    } finally {
      setScheduleSaving(false);
    }
  }

  async function patch(next: Partial<TraeSettings>) {
    if (!settings) return;
    setSaving(true);
    // 乐观更新：设置项是单值开关/输入，本地先落再回读，避免每次拖动开关都等一轮往返。
    patchSettings((prev) => ({ ...prev, ...next }));
    try {
      const saved = await api.saveTraeSettings(next);
      patchSettings(() => saved);
    } catch (e) {
      toast.error(t("trae.page.settings.saveFailed"), { description: api.asError(e) });
      await load();
    } finally {
      setSaving(false);
    }
  }

  /** 快照动作：加忙标记、提示、随后刷新设置页的全部聚合数据。 */
  async function runProfileAction(key: string, label: string, action: () => Promise<unknown>) {
    setBusy(key);
    try {
      await action();
      toast.success(t("trae.page.settings.actionDone", { label }));
      await load();
    } catch (e) {
      toast.error(t("trae.page.settings.actionFailed", { label }), { description: api.asError(e) });
    } finally {
      setBusy(null);
    }
  }

  async function runResetDevice() {
    setResetBusy(true);
    try {
      const report = await api.traeResetDevice(variant);
      setResetReport(report);
      toast.success(t("trae.page.settings.resetDone"), {
        description: t("trae.page.settings.resetLayers", {
          done: report.resetCount,
          total: report.totalLayers,
        }),
      });
      await load();
    } catch (e) {
      toast.error(t("trae.page.settings.resetFailed"), { description: api.asError(e) });
    } finally {
      setResetBusy(false);
    }
  }

  if (loading && !settings) {
    return (
      <div className="mx-auto min-w-0 w-full max-w-3xl space-y-3 px-4 py-6 sm:px-6 sm:py-8">
        <Skeleton className="h-10 w-64" />
        <Skeleton className="h-40 w-full" />
        <Skeleton className="h-40 w-full" />
      </div>
    );
  }

  // 探测到的是哪条产品线 / 是否为自动探测。产品名由路径推断，推断不出就省略标签。
  const productLabel = traeProductLabel(env);
  const autoDetected = isAutoDetected(env);

  return (
    <div className="mx-auto min-w-0 w-full max-w-3xl px-4 py-6 sm:px-6 sm:py-8">
      <header className="mb-10 sm:mb-12">
        <div className="flex flex-wrap items-start justify-between gap-x-6 gap-y-3">
          <div className="min-w-0">
            <h1 className="text-2xl font-semibold tracking-tight">{t("trae.page.settings.title")}</h1>
            <p className="mt-2 text-sm leading-6 text-muted-foreground">
              {t("trae.page.settings.subtitle")}
            </p>
          </div>
          {/* 产品线切换器：设置项本身按产品线分家，切到这里改的就是对应那条线的配置。 */}
          <TraeVariantSwitch className="shrink-0" />
        </div>
      </header>

      {error && (
        <Alert variant="destructive" className="mb-6">
          <AlertTriangle />
          <AlertTitle>{t("trae.page.settings.loadFailed")}</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      <div className="min-w-0 space-y-12">
        {/* ---- 客户端 ---- */}
        <SettingsGroup id="trae-settings-client" title={t("trae.page.settings.clientGroup")}>
          <CardContent className="space-y-0 p-0">
          <SettingsRow className="flex-wrap gap-y-2">
            <div className="flex flex-wrap items-center gap-x-6 gap-y-2 text-sm">
              <span className="flex items-center gap-2">
                {t("trae.page.settings.installLabel")}
                <span className={cn("font-medium", env?.installed ? "text-emerald-600" : "text-muted-foreground")}>
                  {env?.installed
                    ? env.version
                      ? t("trae.page.settings.detectedVersion", { version: env.version })
                      : t("trae.page.settings.detected")
                    : t("trae.page.settings.notDetected")}
                </span>
                {env?.installed && productLabel && <Badge variant="secondary">{productLabel}</Badge>}
              </span>
              <span className="flex items-center gap-2">
                {t("trae.page.settings.runLabel")}
                <span className={cn("font-medium", env?.running ? "text-emerald-600" : "text-muted-foreground")}>
                  {env?.running ? t("trae.page.settings.running") : t("trae.page.settings.notRunning")}
                </span>
              </span>
              <Badge variant="outline">
                {t("trae.page.settings.platform", {
                  name: capabilities?.platform ?? env?.platform ?? t("trae.page.settings.unknown"),
                })}
              </Badge>
            </div>
          </SettingsRow>

          {/* 探测结果：同时装了两个 Trae 时，用户靠这块确认切换器管的是哪一个 */}
          {autoDetected && (env?.path || env?.dataDir) && (
            <div className="border-b border-border/50 px-4 py-3 sm:px-5">
              <div className="space-y-1.5 rounded-md border bg-muted/40 px-3 py-2.5">
                <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1 text-xs">
                  <span className="shrink-0 text-muted-foreground">
                    {t("trae.page.settings.autoDetectPath")}
                  </span>
                  <code className="break-all font-mono text-foreground">{env?.path ?? "—"}</code>
                </div>
                <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1 text-xs">
                  <span className="shrink-0 text-muted-foreground">
                    {t("trae.page.settings.autoDetectDataDir")}
                  </span>
                  <code className="break-all font-mono text-foreground">{env?.dataDir ?? "—"}</code>
                  {env?.dataDir && !env.dataDirExists && (
                    <Badge variant="destructive">{t("trae.page.settings.dirMissing")}</Badge>
                  )}
                </div>
                <p className="text-xs leading-5 text-muted-foreground">
                  {t("trae.page.settings.multiLead")}
                  <code className="font-mono">TRAE SOLO CN</code>
                  {t("trae.page.settings.multiSep")}
                  <code className="font-mono">Trae CN</code>
                  {t("trae.page.settings.multiTail")}
                </p>
              </div>
            </div>
          )}

          <SettingsFieldRow
            label={t("trae.page.settings.pathLabel")}
            description={t("trae.page.settings.pathDesc")}
            htmlFor="trae-path"
          >
            <div className="flex w-full flex-wrap items-center justify-end gap-2 sm:w-auto">
              <Input
                id="trae-path"
                className="h-8 sm:w-80"
                value={settings?.traePath ?? ""}
                onChange={(event) => patchSettings((prev) => ({ ...prev, traePath: event.target.value }))}
                placeholder={env?.path ?? t("trae.page.settings.pathPlaceholder")}
              />
              <Button
                size="sm"
                variant="outline"
                disabled={saving}
                onClick={() => void patch({ traePath: settings?.traePath?.trim() ? settings.traePath.trim() : null })}
              >
                {saving ? <Loader2 className="animate-spin" /> : <Save />}
                {t("trae.page.settings.save")}
              </Button>
            </div>
          </SettingsFieldRow>
          </CardContent>
        </SettingsGroup>

        {/* ---- 端口与网络 ---- */}
        <SettingsGroup id="trae-settings-network" title={t("trae.page.settings.networkGroup")}>
          <CardContent className="space-y-0 p-0">
          <SettingsFieldRow
            label={t("trae.page.settings.proxyPortLabel")}
            description={t("trae.page.settings.proxyPortDesc")}
            htmlFor="trae-proxy-port"
          >
            <Input
              id="trae-proxy-port"
              className="h-8 w-full sm:w-40"
              inputMode="numeric"
              value={settings?.proxyPort ?? ""}
              onChange={(event) =>
                patchSettings((prev) => ({ ...prev, proxyPort: Number(event.target.value) || 0 }))
              }
              onBlur={() => void patch({ proxyPort: settings?.proxyPort ?? 8899 })}
            />
          </SettingsFieldRow>
          <SettingsFieldRow
            label={t("trae.page.settings.apiPortLabel")}
            description={t("trae.page.settings.apiPortDesc")}
            htmlFor="trae-api-port"
          >
            <Input
              id="trae-api-port"
              className="h-8 w-full sm:w-40"
              inputMode="numeric"
              value={settings?.apiPort ?? ""}
              onChange={(event) =>
                patchSettings((prev) => ({ ...prev, apiPort: Number(event.target.value) || 0 }))
              }
              onBlur={() => void patch({ apiPort: settings?.apiPort ?? 7864 })}
            />
          </SettingsFieldRow>
          <SettingsFieldRow
            label={t("trae.page.settings.proxyDomainsLabel")}
            description={t("trae.page.settings.proxyDomainsDesc")}
            htmlFor="trae-domains"
          >
            <Input
              id="trae-domains"
              className="h-8 w-full font-mono text-xs sm:w-80"
              value={settings?.proxyDomains ?? ""}
              onChange={(event) =>
                patchSettings((prev) => ({ ...prev, proxyDomains: event.target.value }))
              }
              onBlur={() => void patch({ proxyDomains: settings?.proxyDomains ?? "" })}
            />
          </SettingsFieldRow>
          </CardContent>
        </SettingsGroup>

        {/* ---- 自动签到（排程）---- */}
        <SettingsGroup id="trae-settings-auto-checkin" title={t("trae.page.settings.autoCheckinGroup")}>
          <CardContent className="space-y-0 p-0">
          <SettingsFieldRow
            label={t("trae.page.settings.autoCheckinEnable")}
            description={t("trae.page.settings.autoCheckinDesc")}
          >
            <Switch
              checked={schedule?.trae_checkin_enabled ?? false}
              disabled={!schedule || scheduleSaving}
              onCheckedChange={(checked) => void saveSchedule({ trae_checkin_enabled: checked })}
              aria-label={t("trae.page.settings.autoCheckinEnable")}
            />
          </SettingsFieldRow>
          <SettingsFieldRow
            label={t("trae.page.settings.checkinHoursLabel")}
            description={t("trae.page.settings.checkinHoursDesc")}
            htmlFor="trae-checkin-hour"
          >
            <HoursEditor
              id="trae-checkin-hour"
              hours={schedule?.trae_checkin_hours ?? []}
              disabled={!schedule || scheduleSaving}
              onChange={(hours) => void saveSchedule({ trae_checkin_hours: hours })}
            />
          </SettingsFieldRow>
          </CardContent>
        </SettingsGroup>

        {/* ---- 签到策略 ---- */}
        <SettingsGroup id="trae-settings-checkin" title={t("trae.page.settings.checkinGroup")}>
          <CardContent className="space-y-0 p-0">
          <SettingsFieldRow
            label={t("trae.page.settings.skipCheckedLabel")}
            description={t("trae.page.settings.skipCheckedDesc")}
          >
            <Switch
              checked={settings?.checkinSkipChecked ?? true}
              disabled={saving}
              onCheckedChange={(checked) => void patch({ checkinSkipChecked: checked })}
              aria-label={t("trae.page.settings.skipCheckedLabel")}
            />
          </SettingsFieldRow>
          <SettingsFieldRow
            label={t("trae.page.settings.skipExpiredLabel")}
            description={t("trae.page.settings.skipExpiredDesc")}
          >
            <Switch
              checked={settings?.checkinSkipExpired ?? true}
              disabled={saving}
              onCheckedChange={(checked) => void patch({ checkinSkipExpired: checked })}
              aria-label={t("trae.page.settings.skipExpiredLabel")}
            />
          </SettingsFieldRow>
          <SettingsFieldRow
            label={t("trae.page.settings.retryLabel")}
            description={t("trae.page.settings.retryDesc")}
            htmlFor="trae-retry"
          >
            <Input
              id="trae-retry"
              className="h-8 w-full sm:w-24"
              inputMode="numeric"
              value={settings?.retry ?? 1}
              onChange={(event) =>
                patchSettings((prev) => ({ ...prev, retry: Number(event.target.value) || 0 }))
              }
              onBlur={() => void patch({ retry: settings?.retry ?? 1 })}
            />
          </SettingsFieldRow>
          <SettingsFieldRow
            label={t("trae.page.settings.logRetentionLabel")}
            description={t("trae.page.settings.logRetentionDesc")}
            htmlFor="trae-log-retention"
          >
            <Input
              id="trae-log-retention"
              className="h-8 w-full sm:w-24"
              inputMode="numeric"
              value={settings?.logRetentionDays ?? 30}
              onChange={(event) =>
                patchSettings((prev) => ({ ...prev, logRetentionDays: Number(event.target.value) || 0 }))
              }
              onBlur={() => void patch({ logRetentionDays: settings?.logRetentionDays ?? 30 })}
            />
          </SettingsFieldRow>
          </CardContent>
        </SettingsGroup>

        {/* ---- 登录态快照 ---- */}
        <ProfilesSection
          data={profiles}
          loading={loading}
          busy={busy}
          variant={variant}
          onReload={() => void load()}
          onRun={runProfileAction}
        />

        {/* ---- 设备标识 ---- */}
        <SettingsGroup id="trae-settings-device" title={t("trae.page.settings.deviceGroup")}>
          <CardContent className="space-y-0 p-0">
          <SettingsFieldRow
            label={t("trae.page.settings.resetLabel")}
            description={t("trae.page.settings.resetDesc")}
            operational
          >
            <Button
              variant="outline"
              size="sm"
              disabled={resetBusy || !capabilities?.clientDetection}
              onClick={() => setResetOpen(true)}
            >
              {resetBusy ? <Loader2 className="animate-spin" /> : <RotateCcw />}
              {t("trae.page.settings.reset")}
            </Button>
          </SettingsFieldRow>
          {resetReport && (
            <div className="space-y-1.5 px-4 py-3 sm:px-5">
              <div className="text-xs text-muted-foreground">
                {t("trae.page.settings.resetResultPrefix")}
                {t("trae.page.settings.resetLayers", {
                  done: resetReport.resetCount,
                  total: resetReport.totalLayers,
                })}
              </div>
              {resetReport.steps.map((step) => (
                <div key={step.layer} className="flex items-center gap-2 text-xs">
                  <Badge variant={step.status === "ok" ? "success" : "outline"}>
                    {step.status === "ok"
                      ? t("trae.page.settings.stepOk")
                      : step.status === "unsupported"
                        ? t("trae.page.settings.stepUnsupported")
                        : t("trae.page.settings.stepSkipped")}
                  </Badge>
                  <span className="text-muted-foreground">
                    {step.label}
                    {step.reason ? ` — ${step.reason}` : ""}
                  </span>
                </div>
              ))}
            </div>
          )}
          </CardContent>
        </SettingsGroup>

        {/* ---- 平台能力 ---- */}
        <SettingsGroup id="trae-settings-capabilities" title={t("trae.page.settings.capabilitiesGroup")}>
          <CardContent className="space-y-0 p-0">
          <div className="px-4 py-3.5 sm:px-5">
            <div className="flex flex-wrap gap-2">
              <CapabilityBadge
                labelKey="trae.page.settings.capClientDetection"
                supported={capabilities?.clientDetection ?? false}
              />
              <CapabilityBadge
                labelKey="trae.page.settings.capProcessControl"
                supported={capabilities?.processControl ?? false}
              />
              <CapabilityBadge
                labelKey="trae.page.settings.capScheduledTask"
                supported={capabilities?.scheduledTask ?? false}
              />
              <CapabilityBadge
                labelKey="trae.page.settings.capMachineGuid"
                supported={capabilities?.machineGuidReset ?? false}
              />
            </div>
            <p className="mt-3 flex items-start gap-1.5 text-xs leading-5 text-muted-foreground">
              <Info className="mt-0.5 size-3.5 shrink-0" />
              {t("trae.page.settings.capabilitiesNote")}
            </p>
          </div>
          {capabilities && capabilities.unsupported.length > 0 && (
            <div className="space-y-1.5 border-t border-border/50 px-4 py-3 sm:px-5">
              {capabilities.unsupported.map((item) => (
                <div key={item.capability} className="text-xs text-muted-foreground">
                  {t("trae.page.settings.capabilityRow", {
                    label: item.label,
                    on: item.supportedOn,
                    reason: item.reason,
                  })}
                </div>
              ))}
            </div>
          )}
          </CardContent>
        </SettingsGroup>

        {/* ---- 运行日志 ---- */}
        <RuntimeLogsSection />

        {/* ---- 网关请求日志 ---- */}
        <GatewayLogsSection />

        {/* ---- 关于 ---- */}
        <SettingsGroup id="trae-settings-about" title={t("trae.page.settings.aboutGroup")}>
          <CardContent className="space-y-0 p-0">
          <SettingsRow className="flex-wrap gap-y-2">
            <div className="min-w-0">
              <div className="text-[13px]">{t("trae.page.settings.aboutTitle")}</div>
              <p className="mt-0.5 text-xs leading-4 text-muted-foreground/75">
                {t("trae.page.settings.aboutDesc")}
              </p>
            </div>
            {consoleBase ? (
              <Button variant="ghost" size="sm" asChild>
                <a href={consoleBase} target="_blank" rel="noreferrer">
                  <ExternalLink />
                  {consoleBase.replace(/^https?:\/\//, "")}
                </a>
              </Button>
            ) : (
              // 域未知时**不给死链**：宁可少一枚按钮，也不把用户导到可能错的站。
              <span className="text-xs text-muted-foreground/75">
                {t("trae.page.settings.aboutUnknownSite")}
              </span>
            )}
          </SettingsRow>
          </CardContent>
        </SettingsGroup>
      </div>

      {/* 重置确认 */}
      <Dialog open={resetOpen} onOpenChange={setResetOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("trae.page.settings.resetLabel")}</DialogTitle>
            <DialogDescription>{t("trae.page.settings.resetDialogDesc")}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setResetOpen(false)}>
              {t("trae.page.settings.cancel")}
            </Button>
            <Button
              onClick={() => {
                setResetOpen(false);
                void runResetDevice();
              }}
            >
              {t("trae.page.settings.resetConfirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

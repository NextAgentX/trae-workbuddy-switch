import { useCallback, useEffect, useMemo, useState } from "react";
import { AlertTriangle, Copy, FolderOpen, Loader2, Power, RefreshCw, ShieldAlert } from "lucide-react";
import { toast } from "sonner";

import { DemoAction } from "@/components/demo-action";
import { TraeVariantSwitch } from "@/components/trae-variant-switch";
import { TraeAccountPoolCard } from "@/components/gateway/trae-account-pool-card";
import { TraeApiKeyTable } from "@/components/gateway/trae-api-key-table";
import { TraeIntegrationGuide } from "@/components/gateway/trae-integration-guide";
import { TraeModelList } from "@/components/gateway/trae-model-list";
import { TraeRequestLog } from "@/components/gateway/trae-request-log";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
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
import { useT } from "@/lib/i18n";
import { copyText } from "@/lib/clipboard";
import {
  DEFAULT_TRAE_GATEWAY_CONFIG,
  normalizeTraeGatewayConfig,
  normalizeTraeGatewayLogs,
  normalizeTraeGatewayStatus,
  toTraeGatewayConfigRaw,
} from "@/lib/trae-gateway";
import type { TraeGatewayConfig, TraeGatewayLogEntry, TraeGatewayModel, TraeGatewayStatus } from "@/lib/trae-types";
import { cn } from "@/lib/utils";
import { useCachedResource } from "@/lib/use-cached-resource";
import { useTraeVariant } from "@/lib/use-trae-variant";

const LOOPBACK = "127.0.0.1";
const LAN = "0.0.0.0";

/**
 * 本页快照：网关配置 / 监听状态 / 模型清单 / 请求日志。
 *
 * 四者一起缓存：它们在同一次 `Promise.all` 里取、也一起被保存与清空操作改写，
 * 拆开缓存只会让「配置已保存、状态还是旧的」这种中间态有缝可钻。
 *
 * `status` 允许为 `null`（尚未取到），`config` 用默认值兜底 —— 快照还没回来时
 * 表单也还没渲染，因此默认值只服务于类型完整，不会先显示假端口再跳一下。
 */
interface ApiServiceSnapshot {
  config: TraeGatewayConfig;
  status: TraeGatewayStatus | null;
  models: TraeGatewayModel[];
  logs: TraeGatewayLogEntry[];
}

/**
 * 「API 服务」页（Trae 分区）。
 *
 * 与 WorkBuddy 的「API 服务」页是**两套东西**：Trae 网关的上游是
 * `trae-api-cn.mchost.guru`，凭据是 `Cloud-IDE-JWT`，响应是私有 SOLO SSE，
 * 因此它的独立监听端口（默认 7864）与 WorkBuddy 网关（57891）必须分开——
 * 两个网关都占 `/v1/chat/completions`，合并到同一端口会直接冲突。
 *
 * ## 本轮结构：容器 + 五个 Trae 专用组件
 *
 * Key 管理已由后端升级为**多 Key 库（含归属产品线）**（`list/create/revoke/delete_trae_api_key`），
 * 因此单 Key 卡与其「重新生成」入口（R6）一并删除，改由 `TraeApiKeyTable` 承担
 * （「归属产品线」列取代 WorkBuddy 的「归属版本」列）。接入指引 / 账号池 / 模型清单 /
 * 请求日志分别下沉为 `TraeIntegrationGuide` / `TraeAccountPoolCard` / `TraeModelList` /
 * `TraeRequestLog`——**页面不再保留任何内联副本**。
 *
 * **骨架与 WorkBuddy 刻意同构**（容器宽度、页头字号、卡片节奏、工具条排布），
 * 但下列控件因 Trae 无对应能力而**不出现**：`Region` Tabs / 「按版本」双条目 /
 * `AccountStrategyCard`（Trae 无 `accountStrategy`）——分别以单条 Base URL、
 * 账号池卡替代。模型区**不加刷新按钮**（模型名是客户端常量，刷新永不改变结果）。
 */
export default function TraeApiServicePage() {
  const t = useT();
  /** 当前产品线：决定读哪个账号池（`trae_gateway_status(variant)`）与归属列默认值。 */
  const [variant] = useTraeVariant();
  const [saving, setSaving] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [riskOpen, setRiskOpen] = useState(false);
  const [openingDir, setOpeningDir] = useState(false);

  const loadSnapshot = useCallback(async (): Promise<ApiServiceSnapshot> => {
    const [configRaw, statusRaw, modelsRaw, logsRaw] = await Promise.all([
      api.getTraeGatewayConfig(),
      api.getTraeGatewayStatus(variant),
      api.getTraeGatewayModels(),
      api.getTraeGatewayLogs(),
    ]);
    return {
      config: normalizeTraeGatewayConfig(configRaw),
      status: normalizeTraeGatewayStatus(statusRaw),
      models: readModels(modelsRaw),
      logs: normalizeTraeGatewayLogs(logsRaw),
    };
  }, [variant]);

  /**
   * 快照缓存：切到 WorkBuddy 再切回来时不再闪骨架。变更操作（保存配置、清空日志）
   * 走 `patch` 就地改写快照，而不是写组件 state —— 值归缓存所有，两处写迟早分歧。
   */
  const {
    data: snapshot,
    loading,
    error,
    refresh: loadAll,
    patch,
  } = useCachedResource<ApiServiceSnapshot>(`trae:api-service:${variant}`, loadSnapshot);

  const config = snapshot?.config ?? DEFAULT_TRAE_GATEWAY_CONFIG;
  const status = snapshot?.status ?? null;
  const models = snapshot?.models ?? [];
  const logs = snapshot?.logs ?? [];

  /**
   * 端口输入框的草稿值。
   *
   * 与 `config.port` 同步的时机是「`config.port` 本身变了」：加载完成、保存成功后
   * 都会变，而用户打字期间 `config.port` 不动，所以输入不会被打断。
   * 初值直接取当前配置，因此**带着缓存重挂载时不会先显示默认端口再跳一下**。
   */
  const [portDraft, setPortDraft] = useState(() => String(config.port));
  useEffect(() => {
    setPortDraft(String(config.port));
  }, [config.port]);

  /** 就地改写快照（保留其余字段）。 */
  const patchSnapshot = useCallback(
    (next: Partial<ApiServiceSnapshot>) => {
      patch((current) => ({ ...current, ...next }));
    },
    [patch],
  );

  async function persist(next: Partial<TraeGatewayConfig>) {
    const merged = { ...config, ...next };
    setSaving(true);
    try {
      await api.saveTraeGatewayConfig(toTraeGatewayConfigRaw(merged));
      patchSnapshot({ config: merged });
      // 保存会启动/重启监听，状态要重读（仍按当前产品线）。
      const statusRaw = await api.getTraeGatewayStatus(variant);
      patchSnapshot({ status: normalizeTraeGatewayStatus(statusRaw) });
      toast.success(
        merged.enabled
          ? t("trae.stats.api.toast.savedStarted")
          : t("trae.stats.api.toast.savedDisabled"),
      );
    } catch (e) {
      toast.error(t("trae.stats.api.toast.saveFailed"), { description: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  function onBindAddrChange(next: string) {
    if (next === config.bindAddr) return;
    if (next === LOOPBACK) {
      void persist({ bindAddr: LOOPBACK, allowNonLoopback: false });
      return;
    }
    setRiskOpen(true);
  }

  function confirmLan() {
    setRiskOpen(false);
    void persist({ bindAddr: LAN, allowNonLoopback: true });
  }

  function commitPort() {
    const parsed = Number.parseInt(portDraft, 10);
    if (!Number.isFinite(parsed) || parsed < 1 || parsed > 65535) {
      toast.error(t("trae.stats.api.toast.portInvalid"), {
        description: t("trae.stats.api.toast.portRange"),
      });
      setPortDraft(String(config.port));
      return;
    }
    if (parsed === config.port) return;
    void persist({ port: parsed });
  }

  async function onClearLogs() {
    setClearing(true);
    try {
      await api.clearTraeGatewayLogs();
      patchSnapshot({ logs: [] });
      toast.success(t("trae.stats.api.toast.logsCleared"));
    } catch (e) {
      toast.error(t("trae.stats.api.toast.clearFailed"), { description: api.asError(e) });
    } finally {
      setClearing(false);
    }
  }

  /**
   * 打开 Trae 数据目录。
   *
   * 非 Windows 上后端返回**结构化 `Unsupported`**（不是假成功）：此时如实把
   * 「当前平台不支持」提示给用户，而不是弹一句「已打开」却什么都没发生。
   */
  async function onOpenDataDir() {
    setOpeningDir(true);
    try {
      const result = (await api.openTraeDataDir(variant)) as {
        ok?: boolean;
        path?: string;
        capability?: string;
        reason?: string;
      };
      if (result?.capability) {
        toast.message(t("trae.stats.api.toast.unsupported"), {
          description: result.reason ?? t("trae.stats.api.toast.unsupportedDesc"),
        });
        return;
      }
      toast.success(t("trae.stats.api.toast.dirOpened"), { description: result?.path });
    } catch (e) {
      toast.error(t("trae.stats.api.toast.dirOpenFailed"), { description: api.asError(e) });
    } finally {
      setOpeningDir(false);
    }
  }

  const running = status?.running ?? false;
  const baseUrl = useMemo(() => {
    const host = config.bindAddr === LAN ? LOOPBACK : config.bindAddr;
    return `http://${host}:${config.port}/v1`;
  }, [config.bindAddr, config.port]);

  const keyPrefix = status?.apiKeyPrefix || "";

  if (loading && !status) {
    return (
      <div className="mx-auto w-full max-w-[1180px] px-6 py-8 sm:px-8 sm:py-9">
        <Skeleton className="h-9 w-48" />
        <Skeleton className="mt-4 h-32 w-full" />
        <Skeleton className="mt-6 h-64 w-full" />
      </div>
    );
  }

  return (
    <div className="mx-auto w-full max-w-[1180px] px-6 py-8 sm:px-8 sm:py-9">
      <header className="mb-6 flex flex-wrap items-start justify-between gap-x-6 gap-y-3">
        <div className="min-w-0">
          <h1 className="text-[28px] font-semibold tracking-tight">{t("trae.stats.api.title")}</h1>
          <p className="mt-2 text-sm leading-6 text-muted-foreground">
            {t("trae.stats.api.subtitle")}
          </p>
        </div>
        {/* 产品线切换器：Trae 分区的每个页面都可切，位置固定在页头右侧。 */}
        <TraeVariantSwitch className="shrink-0" />
      </header>

      {error && (
        <Alert variant="destructive" className="mb-5">
          <AlertTriangle />
          <AlertTitle>{t("trae.stats.api.loadFailed")}</AlertTitle>
          <AlertDescription className="flex flex-col gap-3">
            <span>{error}</span>
            <div>
              <Button variant="outline" size="sm" onClick={() => void loadAll()}>
                <RefreshCw />
                {t("trae.stats.api.retry")}
              </Button>
            </div>
          </AlertDescription>
        </Alert>
      )}

      {/* ---- 网关 ---- */}
      <Card className="mb-6 gap-0 py-0">
        <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Power className="size-4 text-muted-foreground" />
              {t("trae.stats.api.gateway.enable")}
            </div>
            <p className="mt-1 text-xs text-muted-foreground">
              {t("trae.stats.api.gateway.desc", {
                upstream: status?.upstream || "trae-api-cn.mchost.guru",
              })}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {saving && <Loader2 className="size-3.5 animate-spin text-muted-foreground" />}
            <DemoAction>
              <Switch
                checked={config.enabled}
                disabled={saving}
                onCheckedChange={(checked) => void persist({ enabled: checked })}
                aria-label={t("trae.stats.api.gateway.enable")}
              />
            </DemoAction>
            <DemoAction>
              <Button variant="outline" size="sm" onClick={() => void loadAll()}>
                <RefreshCw />
                {t("trae.stats.api.refresh")}
              </Button>
            </DemoAction>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-x-6 gap-y-3 px-5 py-4">
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">{t("trae.stats.api.gateway.bindAddr")}</span>
            <Select value={config.bindAddr} onValueChange={onBindAddrChange} disabled={saving}>
              <SelectTrigger size="sm" className="w-52" aria-label={t("trae.stats.api.gateway.bindAddr")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={LOOPBACK}>{t("trae.stats.api.gateway.bindLoopback")}</SelectItem>
                <SelectItem value={LAN}>{t("trae.stats.api.gateway.bindLan")}</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">{t("trae.stats.api.gateway.port")}</span>
            <DemoAction>
              <Input
                className="h-8 w-28"
                inputMode="numeric"
                value={portDraft}
                onChange={(event) => setPortDraft(event.target.value.replace(/[^\d]/g, ""))}
                onBlur={commitPort}
                onKeyDown={(event) => {
                  if (event.key === "Enter") commitPort();
                }}
                aria-label={t("trae.stats.api.gateway.port")}
              />
            </DemoAction>
          </div>
          <span className="flex items-center gap-1.5 text-xs">
            {t("trae.stats.api.gateway.statusLabel")}
            <span
              className={cn(
                "inline-flex items-center gap-1.5 font-medium",
                running ? "text-emerald-600 dark:text-emerald-400" : "text-muted-foreground",
              )}
            >
              <span className={cn("size-2 rounded-full", running ? "bg-emerald-500" : "bg-muted-foreground/50")} />
              {running
                ? t("trae.stats.api.gateway.running")
                : t("trae.stats.api.gateway.stopped")}
            </span>
          </span>
          <DemoAction>
            <Button
              variant="outline"
              size="sm"
              className="ml-auto"
              disabled={openingDir}
              onClick={() => void onOpenDataDir()}
            >
              {openingDir ? <Loader2 className="animate-spin" /> : <FolderOpen />}
              {t("trae.stats.api.gateway.openDataDir")}
            </Button>
          </DemoAction>
        </div>

        <div className="flex flex-wrap items-center gap-2 border-t border-border/60 px-5 py-3 text-xs text-muted-foreground">
          {t("trae.stats.api.gateway.noteLead")}
          <code className="font-mono">/v1/chat/completions</code>
          {t("trae.stats.api.gateway.noteTail")}
          {config.bindAddr === LAN && t("trae.stats.api.gateway.lanWarning")}
        </div>

        {status?.lastError && (
          <div className="border-t border-border/60 px-5 py-3">
            <p className="text-xs text-muted-foreground">{t("trae.stats.api.gateway.lastError")}</p>
            <p className="mt-1 break-words text-xs text-destructive">{status.lastError}</p>
          </div>
        )}
      </Card>

      {/* ---- 接入地址 ---- */}
      {/* WorkBuddy 此处是「按版本」逐 region 一行；Trae 无 region，故只有一条。 */}
      <Card className="mb-6 gap-0 py-0">
        <div className="border-b border-border/60 px-5 py-3">
          <span className="text-sm font-semibold">{t("trae.stats.api.endpoint.title")}</span>
        </div>
        <div className="px-5 py-4">
          <div className="flex flex-wrap items-center gap-3">
            <span className="text-xs text-muted-foreground">{t("trae.stats.api.endpoint.baseUrl")}</span>
            <code className="min-w-0 break-all rounded-md border border-border bg-muted/40 px-2 py-1 font-mono text-xs">
              {baseUrl}
            </code>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void copyText(baseUrl, t("trae.stats.api.endpoint.copied"))}
            >
              <Copy />
              {t("trae.stats.api.copy")}
            </Button>
          </div>
          <div className="mt-1.5 flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
            <span>{t("trae.stats.api.endpoint.keyPrefix")}</span>
            <code className="font-mono">
              {keyPrefix ? `${keyPrefix}…` : t("trae.stats.api.endpoint.noKey")}
            </code>
          </div>
        </div>
      </Card>

      {/* ---- API Key（多 Key + 归属产品线） ---- */}
      {/* 替代 WorkBuddy 的单 Key 卡：这里支持多把 Key，每把绑定一条产品线。 */}
      <TraeApiKeyTable
        className="mb-6"
        defaultVariant={variant}
        onChanged={() => void loadAll()}
      />

      {/* ---- 账号池 ---- */}
      <TraeAccountPoolCard status={status} className="mb-6" />

      {/* ---- 模型清单（无刷新按钮：模型名是客户端常量） ---- */}
      <TraeModelList models={models} defaultModel={config.defaultModel} className="mb-6" />

      {/* ---- 接入指引（单条 Base URL，无 region Tabs） ---- */}
      <TraeIntegrationGuide
        baseUrl={baseUrl}
        model={config.defaultModel}
        keyPrefix={keyPrefix}
        className="mb-6"
      />

      {/* ---- 请求日志 ---- */}
      <TraeRequestLog logs={logs} clearing={clearing} onClear={() => void onClearLogs()} />

      {/* ---- 局域网风险确认 ---- */}
      <Dialog open={riskOpen} onOpenChange={setRiskOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <ShieldAlert className="size-4 text-amber-500" />
              {t("trae.stats.api.lanDialog.title")}
            </DialogTitle>
            <DialogDescription>{t("trae.stats.api.lanDialog.desc")}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRiskOpen(false)}>
              {t("trae.stats.api.lanDialog.cancel")}
            </Button>
            <Button onClick={confirmLan}>
              <Power />
              {t("trae.stats.api.lanDialog.confirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** `get_trae_gateway_models` 返回 OpenAI `/v1/models` 形状，这里只取 `data`。 */
function readModels(raw: unknown): TraeGatewayModel[] {
  const record = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const list = Array.isArray(record.data) ? record.data : [];
  return list
    .map((item) => {
      if (!item || typeof item !== "object") return null;
      const entry = item as Record<string, unknown>;
      const id = typeof entry.id === "string" ? entry.id : null;
      if (!id) return null;
      return {
        id,
        object: typeof entry.object === "string" ? entry.object : "model",
        created: typeof entry.created === "number" ? entry.created : 0,
        owned_by: typeof entry.owned_by === "string" ? entry.owned_by : "trae",
      };
    })
    .filter((item): item is TraeGatewayModel => item !== null);
}

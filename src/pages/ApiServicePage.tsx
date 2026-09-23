import { useEffect, useState } from "react";
import { AlertTriangle, Copy, Loader2, Power } from "lucide-react";
import { toast } from "sonner";

import { ApiKeyTable } from "@/components/gateway/api-key-table";
import { AccountStrategyCard } from "@/components/gateway/account-strategy-card";
import { IntegrationGuide } from "@/components/gateway/integration-guide";
import { ModelList } from "@/components/gateway/model-list";
import { RequestLog } from "@/components/gateway/request-log";
import { DemoAction } from "@/components/demo-action";
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
import { Switch } from "@/components/ui/switch";
import * as api from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { useT } from "@/lib/i18n";
import { resolveGatewayBaseUrl, resolveGatewayRunning } from "@/lib/gateway";
import { REGIONS, regionDescriptor } from "@/lib/region";
import { cn } from "@/lib/utils";
import type { ApiKeyRecord, GatewayConfig, Region } from "@/lib/types";
import { useGatewayStore } from "@/stores/gateway";

const LOOPBACK = "127.0.0.1";
const LAN = "0.0.0.0";

/** 无启用中的 Key 时返回 null，由渲染处给出占位提示文案。 */
function representativeKey(keys: ApiKeyRecord[], region: Region): string | null {
  const active = keys.find((key) => key.region === region && !key.revoked);
  return active ? `${active.prefix}…` : null;
}

/** 「API 服务」页：网关开关、监听、Base URL、Key、模型、策略、接入指引、请求日志（P0-11）。 */
export default function ApiServicePage() {
  const t = useT();
  const config = useGatewayStore((s) => s.config);
  const status = useGatewayStore((s) => s.status);
  const keys = useGatewayStore((s) => s.keys);
  const loading = useGatewayStore((s) => s.loading);
  const error = useGatewayStore((s) => s.error);
  const loadAll = useGatewayStore((s) => s.loadAll);
  const saveConfig = useGatewayStore((s) => s.saveConfig);

  const [portDraft, setPortDraft] = useState(String(config.port));
  const [saving, setSaving] = useState(false);
  const [riskOpen, setRiskOpen] = useState(false);

  useEffect(() => {
    void loadAll();
  }, [loadAll]);

  useEffect(() => {
    setPortDraft(String(config.port));
  }, [config.port]);

  async function persist(next: Partial<GatewayConfig>) {
    setSaving(true);
    try {
      await saveConfig({ ...config, ...next });
    } catch (e) {
      toast.error(t("wbStats.gateway.saveFail"), { description: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  function onBindAddrChange(next: string) {
    if (next === config.bind_addr) return;
    if (next === LOOPBACK) {
      void persist({ bind_addr: LOOPBACK, allow_non_loopback: false });
      return;
    }
    // 非回环监听需要风险确认（Q2 / U5）。
    setRiskOpen(true);
  }

  function confirmLan() {
    setRiskOpen(false);
    void persist({ bind_addr: LAN, allow_non_loopback: true });
  }

  function commitPort() {
    const parsed = Number.parseInt(portDraft, 10);
    if (!Number.isFinite(parsed) || parsed < 1 || parsed > 65535) {
      toast.error(t("wbStats.gateway.portRange"));
      setPortDraft(String(config.port));
      return;
    }
    if (parsed === config.port) return;
    void persist({ port: parsed });
  }

  const baseUrl = resolveGatewayBaseUrl(status, config.bind_addr, config.port);
  const running = resolveGatewayRunning(status);

  return (
    <div className="mx-auto w-full max-w-[1180px] px-6 py-8 sm:px-8 sm:py-9">
      <header className="mb-6">
        <h1 className="text-[28px] font-semibold tracking-tight">{t("wbStats.gateway.title")}</h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          {t("wbStats.gateway.desc")}
        </p>
      </header>

      {error && (
        <Alert variant="destructive" className="mb-4">
          <AlertTriangle />
          <AlertTitle>{t("wbStats.gateway.opFail")}</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {/* 网关 */}
      <Card className="mb-6 gap-0 py-0">
        <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Power className="size-4 text-muted-foreground" />
              {t("wbStats.gateway.enableGateway")}
            </div>
            <p className="mt-1 text-xs text-muted-foreground">{t("wbStats.gateway.enableDesc")}</p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {saving && <Loader2 className="size-3.5 animate-spin text-muted-foreground" />}
            <DemoAction>
              <Switch
                checked={config.enabled}
                disabled={saving}
                onCheckedChange={(enabled) => void persist({ enabled })}
                aria-label={t("wbStats.gateway.enableAria")}
              />
            </DemoAction>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-x-6 gap-y-3 px-5 py-4">
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">{t("wbStats.gateway.bindAddr")}</span>
            <Select value={config.bind_addr} onValueChange={onBindAddrChange} disabled={saving}>
              <SelectTrigger size="sm" className="w-52" aria-label={t("wbStats.gateway.bindAddrAria")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={LOOPBACK}>{t("wbStats.gateway.loopback")}</SelectItem>
                <SelectItem value={LAN}>{t("wbStats.gateway.lan")}</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">{t("wbStats.gateway.port")}</span>
            <DemoAction>
              <Input
                className="h-8 w-28"
                inputMode="numeric"
                value={portDraft}
                onChange={(event) => setPortDraft(event.target.value)}
                onBlur={commitPort}
                onKeyDown={(event) => {
                  if (event.key === "Enter") commitPort();
                }}
                aria-label={t("wbStats.gateway.portAria")}
              />
            </DemoAction>
          </div>
          <span className="flex items-center gap-1.5 text-xs">
            {t("wbStats.gateway.status")}
            <span className={cn("inline-flex items-center gap-1.5 font-medium", running ? "text-emerald-600" : "text-muted-foreground")}>
              <span className={cn("size-2 rounded-full", running ? "bg-emerald-500" : "bg-muted-foreground/50")} />
              {running ? t("wbStats.gateway.running") : t("wbStats.gateway.stopped")}
            </span>
          </span>
        </div>
      </Card>

      {/* 接入地址（按版本） */}
      <Card className="mb-6 gap-0 py-0">
        <div className="border-b border-border/60 px-5 py-3">
          <span className="text-sm font-semibold">{t("wbStats.gateway.addrByVersion")}</span>
        </div>
        <div className="divide-y divide-border/60">
          {REGIONS.map((region) => (
            <div key={region} className="px-5 py-4">
              <div className="text-sm font-medium">{regionDescriptor(region).gatewayLabel}</div>
              <div className="mt-2 flex flex-wrap items-center gap-3">
                <span className="text-xs text-muted-foreground">Base URL</span>
                <code className="rounded-md border border-border bg-muted/40 px-2 py-1 font-mono text-xs">{baseUrl}</code>
                <Button variant="ghost" size="sm" onClick={() => void copyText(baseUrl, t("wbStats.gateway.baseUrlCopied"))}>
                  <Copy />
                  {t("wbStats.gateway.copy")}
                </Button>
              </div>
              <div className="mt-1.5 flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                <span>API Key</span>
                <code className="font-mono">
                  {representativeKey(keys, region) ?? t("wbStats.gateway.keyPlaceholder")}
                </code>
              </div>
            </div>
          ))}
        </div>
      </Card>

      <ApiKeyTable className="mb-6" />
      <AccountStrategyCard className="mb-6" />
      <ModelList className="mb-6" />
      <IntegrationGuide baseUrl={baseUrl} className="mb-6" />
      <RequestLog />

      {/* 非回环监听风险确认 */}
      <Dialog open={riskOpen} onOpenChange={setRiskOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("wbStats.gateway.allowLanTitle")}</DialogTitle>
            <DialogDescription>
              {t("wbStats.gateway.allowLanDesc")}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRiskOpen(false)}>
              {t("wbStats.gateway.cancel")}
            </Button>
            <Button variant="destructive" onClick={confirmLan}>
              {t("wbStats.gateway.confirmOpen")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {loading && (
        <div className="mt-6 flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="animate-spin" />
          {t("wbStats.gateway.loadingData")}
        </div>
      )}
    </div>
  );
}

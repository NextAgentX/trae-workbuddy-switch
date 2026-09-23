import { useEffect, useState } from "react";
import {
  CircleCheck,
  FolderOpen,
  Loader2,
  Play,
  RefreshCw,
  Save,
} from "lucide-react";
import { toast } from "sonner";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { CardContent } from "@/components/ui/card";
import { SettingsFieldRow, SettingsGroup, SettingsRow } from "@/components/settings-primitives";
// 小时表编辑器与 Trae 设置页**共用同一份实现**（校验规则属于规则，不属于版式）。
import { HoursEditor } from "@/components/schedule-hours-editor";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import * as api from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { useT } from "@/lib/i18n";
import { regionDescriptor } from "@/lib/region";
import type { TranslationKey } from "@/locales/zh";
import type {
  AutoRotateConfig,
  CheckinConfig,
  CheckinLog,
  GatewayConfig,
  Region,
  RotateLog,
  RotateStatus,
  ScheduleConfig,
  ScheduleRunResult,
  SwitchConfig,
} from "@/lib/types";
import { DemoAction } from "@/components/demo-action";
import { useAccountsStore } from "@/stores/accounts";
import { useGatewayStore } from "@/stores/gateway";

/**
 * 设置页的三个行原语来自 `@/components/settings-primitives`，
 * 与 Trae 设置页共用同一实现（原先两份是逐字重复，已发生过漂移）。
 * 这里重新导出只是为了让既有单测/引用路径继续可用。
 */
export { SettingsFieldRow, SettingsGroup, SettingsRow };

function formatTime(ts: number): string {
  try {
    return new Date(ts).toLocaleString("zh-CN", {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return String(ts);
  }
}

function logLabel(result: string): { textKey: TranslationKey; tone: "success" | "warning" | "error" } {
  switch (result) {
    case "success":
      return { textKey: "wbSettings.checkin.resultSuccess", tone: "success" };
    case "already":
      return { textKey: "wbSettings.checkin.resultAlready", tone: "warning" };
    default:
      return { textKey: "wbSettings.checkin.resultFailed", tone: "error" };
  }
}

/** 自动签到配置 + 一键签到 + 日志。 */
function AutoCheckinCard() {
  const t = useT();
  const [cfg, setCfg] = useState<CheckinConfig | null>(null);
  const [logs, setLogs] = useState<CheckinLog[]>([]);
  const [saving, setSaving] = useState(false);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  useEffect(() => {
    void load();
  }, []);

  async function load() {
    try {
      const [c, l] = await Promise.all([api.getAutoCheckinConfig(), api.getCheckinLogs()]);
      setCfg(c);
      setLogs(l.logs);
    } catch (e) {
      setMsg({ type: "err", text: api.asError(e) });
    }
  }

  async function save() {
    if (!cfg) return;
    setSaving(true);
    setMsg(null);
    try {
      const saved = await api.saveAutoCheckinConfig(cfg);
      setCfg(saved);
      setMsg({ type: "ok", text: t("wbSettings.common.saved") });
    } catch (e) {
      setMsg({ type: "err", text: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  async function checkinAllNow() {
    setBusy(true);
    setMsg(null);
    try {
      const res = await api.checkinAll();
      if (res.status === "skipped" && res.reason === "already_running") {
        setMsg({ type: "err", text: t("wbSettings.checkin.alreadyRunning") });
        return;
      }
      const ok = res.accounts.filter((a) => a.result === "success").length;
      const already = res.accounts.filter((a) => a.result === "already").length;
      const err = res.accounts.filter((a) => a.result === "error").length;
      const detail = res.accounts
        .filter((a) => a.result === "error")
        .map((a) => `${a.email}（${a.error}）`)
        .join("；");
      setMsg({
        type: err > 0 ? "err" : "ok",
        text: detail
          ? t("wbSettings.checkin.summaryWithDetail", { ok, already, err, detail })
          : t("wbSettings.checkin.summary", { ok, already, err }),
      });
      void load();
    } catch (e) {
      setMsg({ type: "err", text: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  function setNum(key: keyof CheckinConfig, value: string) {
    if (!cfg) return;
    setCfg({ ...cfg, [key]: Number(value) });
  }

  return (
    <SettingsGroup
      id="settings-auto-checkin"
      title={t("wbSettings.checkin.groupTitle")}
    >
      <CardContent className="space-y-0 p-0">
        {cfg ? (
          <>
            <SettingsFieldRow
              label={t("wbSettings.checkin.enableLabel")}
              description={t("wbSettings.checkin.enableDesc")}
              htmlFor="ac-enabled"
              operational
            >
              <Switch
                id="ac-enabled"
                checked={cfg.enabled}
                onCheckedChange={(v) => setCfg({ ...cfg, enabled: v })}
              />
            </SettingsFieldRow>

            <SettingsFieldRow
              label={t("wbSettings.checkin.keepaliveLabel")}
              description={t("wbSettings.checkin.keepaliveDesc")}
              htmlFor="ac-keep"
              operational
            >
              <Input
                id="ac-keep"
                className="w-full sm:w-48"
                type="number"
                min={0}
                max={90}
                value={cfg.keepalive_days}
                onChange={(e) => setNum("keepalive_days", e.target.value)}
              />
            </SettingsFieldRow>
            <SettingsFieldRow
              label={t("wbSettings.checkin.lazyLabel")}
              description={t("wbSettings.unit.hours")}
              htmlFor="ac-lazy"
              operational
            >
              <Input
                id="ac-lazy"
                className="w-full sm:w-48"
                type="number"
                min={1}
                max={72}
                value={cfg.lazy_refresh_hours}
                onChange={(e) => setNum("lazy_refresh_hours", e.target.value)}
              />
            </SettingsFieldRow>

            <div className="flex flex-wrap gap-2 border-b-0 border-border/60 px-4 py-3 sm:px-5">
              <DemoAction><Button size="sm" onClick={save} disabled={saving}>
                {saving ? <Loader2 className="animate-spin" /> : <Save />}{t("wbSettings.common.saveConfig")}
              </Button></DemoAction>
              <DemoAction><Button size="sm" variant="outline" onClick={checkinAllNow} disabled={busy}>
                {busy ? <Loader2 className="animate-spin" /> : <CircleCheck />}{t("wbSettings.checkin.checkinAllBtn")}
              </Button></DemoAction>
            </div>
          </>
        ) : (
          <p className="px-4 py-3 text-sm text-muted-foreground sm:px-5">{t("wbSettings.common.loading")}</p>
        )}

        {msg && (
          <Alert
            variant={msg.type === "err" ? "destructive" : "default"}
            className="!w-auto mx-4 my-4 sm:mx-5"
          >
            <AlertDescription>{msg.text}</AlertDescription>
          </Alert>
        )}

        <div className="px-4 py-3 sm:px-5">
          <p className="mb-2 text-[13px] font-medium">{t("wbSettings.checkin.logTitle")}</p>
          {logs.length === 0 ? (
            <p className="py-3 text-center text-sm text-muted-foreground">{t("wbSettings.checkin.logEmpty")}</p>
          ) : (
            <div className="max-h-64 overflow-y-auto pr-1">
              {[...logs].reverse().map((l, i) => {
                const tone = logLabel(l.result);
                return (
                  <div
                    key={i}
                    className="flex items-center justify-between border-b border-border/60 py-2 text-xs last:border-b-0"
                  >
                    <div className="min-w-0 flex-1 truncate">
                      <span className="font-medium">{l.email}</span>
                      {l.error && <span className="text-destructive">（{l.error}）</span>}
                    </div>
                    <div className="ml-2 flex shrink-0 items-center gap-2">
                      <span
                        className={
                          tone.tone === "error"
                            ? "text-destructive"
                            : tone.tone === "warning"
                              ? "text-amber-600"
                              : "text-emerald-600"
                        }
                      >
                        {t(tone.textKey)}
                      </span>
                      <span className="text-muted-foreground">{formatTime(l.ts)}</span>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </CardContent>
    </SettingsGroup>
  );
}

/** 自动轮换配置（CodeBuddy CLI）+ 手动检查 + 日志。 */
function AutoRotateCard() {
  const t = useT();
  const [cfg, setCfg] = useState<AutoRotateConfig | null>(null);
  const [status, setStatus] = useState<RotateStatus | null>(null);
  const [logs, setLogs] = useState<RotateLog[]>([]);
  const [saving, setSaving] = useState(false);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  useEffect(() => {
    void load();
  }, []);

  async function load() {
    try {
      const [c, s, l] = await Promise.all([
        api.getAutoRotateConfig(),
        api.getRotateStatus(),
        api.getRotateLogs(),
      ]);
      setCfg(c);
      setStatus(s);
      setLogs(l.logs);
    } catch (e) {
      setMsg({ type: "err", text: api.asError(e) });
    }
  }

  async function save() {
    if (!cfg) return;
    setSaving(true);
    setMsg(null);
    try {
      const saved = await api.saveAutoRotateConfig(cfg);
      setCfg(saved);
      setMsg({ type: "ok", text: t("wbSettings.common.saved") });
    } catch (e) {
      setMsg({ type: "err", text: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  async function runNow() {
    setBusy(true);
    setMsg(null);
    try {
      const res = await api.runRotate();
      setMsg({
        type: res.status === "error" ? "err" : "ok",
        text:
          res.status === "switched"
            ? t("wbSettings.rotate.switchedTo", { name: res.to ?? t("wbSettings.rotate.targetAccount") })
            : res.status === "disabled"
              ? t("wbSettings.rotate.disabledMsg")
              : (res.reason ?? t("wbSettings.rotate.checkDone", { status: res.status })),
      });
      void load();
    } catch (e) {
      setMsg({ type: "err", text: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  function setNum(key: keyof AutoRotateConfig, value: string) {
    if (!cfg) return;
    setCfg({ ...cfg, [key]: Number(value) });
  }

  function actionLabel(action: string): { text: string; tone: "success" | "warning" | "error" } {
    switch (action) {
      case "switched":
        return { text: t("wbSettings.rotate.actionSwitched"), tone: "success" };
      case "skipped":
        return { text: t("wbSettings.rotate.actionSkipped"), tone: "warning" };
      case "disabled":
        return { text: t("wbSettings.rotate.actionDisabled"), tone: "warning" };
      case "error":
        return { text: t("wbSettings.rotate.actionError"), tone: "error" };
      default:
        return { text: action, tone: "warning" };
    }
  }

  return (
    <SettingsGroup
      id="settings-auto-rotate"
      title={t("wbSettings.rotate.groupTitle")}
    >
      <CardContent className="space-y-0 p-0">
        {status && (
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-border/60 bg-muted/25 px-4 py-3 text-xs text-muted-foreground sm:px-5">
            <span>
              {t("wbSettings.rotate.currentAccountLabel")}
              <b className="text-foreground">{status.activeAccountName ?? t("wbSettings.rotate.notConfigured")}</b>
            </span>
            {status.lastCheckAt && (
              <span>{t("wbSettings.rotate.lastCheck", { time: formatTime(status.lastCheckAt) })}</span>
            )}
            {status.lastSwitchAt && (
              <span>{t("wbSettings.rotate.lastSwitch", { time: formatTime(status.lastSwitchAt) })}</span>
            )}
            {!status.cliConfigured && (
              <span className="text-destructive">{t("wbSettings.rotate.cliNotInstalled")}</span>
            )}
          </div>
        )}

        {cfg ? (
          <>
            <SettingsFieldRow
              label={t("wbSettings.rotate.enableLabel")}
              description={t("wbSettings.rotate.enableDesc")}
              htmlFor="ar-enabled"
              operational
            >
              <Switch
                id="ar-enabled"
                checked={cfg.enabled}
                onCheckedChange={(v) => setCfg({ ...cfg, enabled: v })}
              />
            </SettingsFieldRow>

            <SettingsFieldRow
              label={t("wbSettings.rotate.intervalLabel")}
              description={t("wbSettings.unit.minutes")}
              htmlFor="ar-interval"
              operational
            >
              <Input
                id="ar-interval"
                className="w-full sm:w-48"
                type="number"
                min={1}
                max={1440}
                value={cfg.check_interval_minutes}
                onChange={(e) => setNum("check_interval_minutes", e.target.value)}
              />
            </SettingsFieldRow>
            <SettingsFieldRow
              label={t("wbSettings.rotate.cooldownLabel")}
              description={t("wbSettings.unit.minutes")}
              htmlFor="ar-cooldown"
              operational
            >
              <Input
                id="ar-cooldown"
                className="w-full sm:w-48"
                type="number"
                min={1}
                max={1440}
                value={cfg.cooldown_minutes}
                onChange={(e) => setNum("cooldown_minutes", e.target.value)}
              />
            </SettingsFieldRow>
            <SettingsFieldRow
              label={t("wbSettings.rotate.gapLabel")}
              description={t("wbSettings.unit.hours")}
              htmlFor="ar-gap"
              operational
            >
              <Input
                id="ar-gap"
                className="w-full sm:w-48"
                type="number"
                min={0}
                max={720}
                value={cfg.min_gap_hours}
                onChange={(e) => setNum("min_gap_hours", e.target.value)}
              />
            </SettingsFieldRow>
            <SettingsFieldRow
              label={t("wbSettings.rotate.urgencyLabel")}
              description={t("wbSettings.unit.hours")}
              htmlFor="ar-urgency"
              operational
            >
              <Input
                id="ar-urgency"
                className="w-full sm:w-48"
                type="number"
                min={0}
                max={720}
                value={cfg.min_urgency_hours}
                onChange={(e) => setNum("min_urgency_hours", e.target.value)}
              />
            </SettingsFieldRow>
            <SettingsFieldRow
              label={t("wbSettings.rotate.guardLabel")}
              description={t("wbSettings.unit.minutes")}
              htmlFor="ar-guard"
              operational
            >
              <Input
                id="ar-guard"
                className="w-full sm:w-48"
                type="number"
                min={0}
                max={1440}
                value={cfg.active_guard_minutes}
                onChange={(e) => setNum("active_guard_minutes", e.target.value)}
              />
            </SettingsFieldRow>
            <SettingsFieldRow
              label={t("wbSettings.rotate.minCreditsLabel")}
              description={t("wbSettings.rotate.minCreditsDesc")}
              htmlFor="ar-min"
              operational
            >
              <Input
                id="ar-min"
                className="w-full sm:w-48"
                type="number"
                min={0}
                value={cfg.min_remaining_credits}
                onChange={(e) => setNum("min_remaining_credits", e.target.value)}
              />
            </SettingsFieldRow>
            <p className="border-b border-border/60 px-4 py-3 text-[13px] leading-5 text-muted-foreground sm:px-5">
              {t("wbSettings.rotate.timingHint")}
            </p>

            <div className="flex flex-wrap gap-2 border-b-0 border-border/60 px-4 py-3 sm:px-5">
              <DemoAction><Button size="sm" onClick={save} disabled={saving}>
                {saving ? <Loader2 className="animate-spin" /> : <Save />}{t("wbSettings.common.saveConfig")}
              </Button></DemoAction>
              <DemoAction><Button size="sm" variant="outline" onClick={runNow} disabled={busy}>
                {busy ? <Loader2 className="animate-spin" /> : <RefreshCw />}{t("wbSettings.rotate.runNowBtn")}
              </Button></DemoAction>
            </div>
          </>
        ) : (
          <p className="px-4 py-3 text-sm text-muted-foreground sm:px-5">{t("wbSettings.common.loading")}</p>
        )}

        {msg && (
          <Alert
            variant={msg.type === "err" ? "destructive" : "default"}
            className="!w-auto mx-4 my-4 sm:mx-5"
          >
            <AlertDescription>{msg.text}</AlertDescription>
          </Alert>
        )}

        <div className="px-4 py-3 sm:px-5">
          <p className="mb-2 text-[13px] font-medium">{t("wbSettings.rotate.logTitle")}</p>
          {logs.length === 0 ? (
            <p className="py-3 text-center text-sm text-muted-foreground">{t("wbSettings.rotate.logEmpty")}</p>
          ) : (
            <div className="max-h-64 overflow-y-auto pr-1">
              {logs.map((l, i) => {
                const tone = actionLabel(l.action);
                return (
                  <div
                    key={i}
                    className="flex items-center justify-between border-b border-border/60 py-2 text-xs last:border-b-0"
                  >
                    <div className="min-w-0 flex-1 truncate">
                      {l.action === "switched" && l.from && l.to && (
                        <span className="font-medium">
                          {l.from.name ?? l.from.id} → {l.to.name ?? l.to.id}
                        </span>
                      )}
                      {l.reason && <span className="text-muted-foreground">（{l.reason}）</span>}
                    </div>
                    <div className="ml-2 flex shrink-0 items-center gap-2">
                      <span
                        className={
                          tone.tone === "error"
                            ? "text-destructive"
                            : tone.tone === "success"
                              ? "text-emerald-600"
                              : "text-amber-600"
                        }
                      >
                        {tone.text}
                      </span>
                      <span className="text-muted-foreground">{formatTime(l.ts)}</span>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </CardContent>
    </SettingsGroup>
  );
}

/** 权限检测卡片：确认本 App 是否有权写入 WorkBuddy 认证文件。 */
function PermissionCheckCard() {
  const t = useT();
  const authFile = useAuthFile();
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<null | { ok: boolean; text: string }>(null);

  async function runCheck() {
    setChecking(true);
    setResult(null);
    try {
      const res = await api.checkAuthPermission();
      setResult({
        ok: res.ok,
        text: res.ok
          ? res.message ?? t("wbSettings.permission.okMsg")
          : `${res.error}（${res.dir ?? ""}）`,
      });
    } catch (e) {
      setResult({ ok: false, text: api.asError(e) });
    } finally {
      setChecking(false);
    }
  }

  return (
    <SettingsGroup
      id="settings-permission"
      title={t("wbSettings.permission.groupTitle")}
    >
      <CardContent className="space-y-0 p-0">
        <div className="break-all border-b border-border/60 bg-muted/25 px-4 py-3 font-mono text-[11px] leading-5 text-muted-foreground sm:px-5">
          {authFile || t("wbSettings.permission.authFileMissing")}
        </div>
        <div className="flex flex-wrap gap-2 border-b-0 border-border/60 px-4 py-3 sm:px-5">
          <DemoAction><Button size="sm" onClick={runCheck} disabled={checking}>
            {checking ? t("wbSettings.permission.checking") : t("wbSettings.permission.checkBtn")}
          </Button></DemoAction>
          <DemoAction><Button
            size="sm"
            variant="outline"
            onClick={() => void api.openPermissionSettings("all_files")}
          >
            {t("wbSettings.permission.openFullDisk")}
          </Button></DemoAction>
          <DemoAction><Button
            size="sm"
            variant="outline"
            onClick={() => void api.openPermissionSettings("app_management")}
          >
            {t("wbSettings.permission.openAppManagement")}
          </Button></DemoAction>
          <DemoAction><Button size="sm" variant="outline" onClick={() => void api.revealAppInFinder()}>
            {t("wbSettings.permission.revealInFinder")}
          </Button></DemoAction>
        </div>

        {result && (
          <Alert variant={result.ok ? "default" : "destructive"} className="!w-auto mx-4 my-4 sm:mx-5">
            <AlertDescription>{result.text}</AlertDescription>
          </Alert>
        )}
        {result && !result.ok && (
          <div className="mx-4 mb-4 border-l-2 border-destructive/50 bg-muted/30 px-3 py-2.5 text-xs text-muted-foreground sm:mx-5">
            <p className="mb-1 font-medium text-foreground">{t("wbSettings.permission.guideTitle")}</p>
            <ol className="list-decimal space-y-1 pl-4">
              <li>{t("wbSettings.permission.guideStep1")}</li>
              <li>{t("wbSettings.permission.guideStep2")}</li>
              <li>
                {t("wbSettings.permission.guideStep3Pre")} <b>BuddySwitch.app</b>{" "}
                {t("wbSettings.permission.guideStep3Mid")} <b>{t("wbSettings.permission.guideStep3Drag")}</b>
                {t("wbSettings.permission.guideStep3Post")}
              </li>
              <li>{t("wbSettings.permission.guideStep4")}</li>
            </ol>
          </div>
        )}
      </CardContent>
    </SettingsGroup>
  );
}

function useAuthFile(): string | undefined {
  return useAccountsStore((s) => s.status?.authFile);
}

/**
 * 账号切换：切换账号与账号列表展示的偏好。
 *
 * 两项都写 `~/.buddy-switch/switch_config.json`（**全局单份**）：
 * 它们是「我怎么用这个工具」的偏好，而随版本分家的是账号库本身。
 */
function SwitchBehaviorCard() {
  const t = useT();
  const [config, setConfig] = useState<SwitchConfig | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void api
      .getSwitchConfig()
      .then((value) => {
        if (!cancelled) setConfig(value);
      })
      .catch((e) => {
        if (!cancelled) toast.error(t("wbSettings.switch.loadFailed"), { description: api.asError(e) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function patch(next: Partial<SwitchConfig>) {
    if (!config || saving) return;
    const previous = config;
    const merged = { ...config, ...next };
    // 乐观更新：开关必须立刻跟手，否则用户会以为没点上而连点。
    setConfig(merged);
    setSaving(true);
    try {
      setConfig(await api.saveSwitchConfig(merged));
    } catch (e) {
      // 失败时退回改动前的值，界面不停留在「看起来已保存」的状态。
      setConfig(previous);
      toast.error(t("wbSettings.common.saveFailed"), { description: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  return (
    <SettingsGroup id="settings-switch" title={t("wbSettings.switch.groupTitle")}>
      <CardContent className="space-y-0 p-0">
        <SettingsFieldRow
          label={t("wbSettings.switch.copySessionsLabel")}
          description={t("wbSettings.switch.copySessionsDesc")}
          htmlFor="switch-copy-sessions"
          operational
        >
          <Switch
            id="switch-copy-sessions"
            checked={config?.copy_sessions_by_default ?? false}
            disabled={saving || config === null}
            onCheckedChange={(value) => void patch({ copy_sessions_by_default: value })}
            aria-label={t("wbSettings.switch.copySessionsLabel")}
          />
        </SettingsFieldRow>

        <SettingsFieldRow
          className="border-b-0"
          label={t("wbSettings.switch.pinCurrentLabel")}
          description={t("wbSettings.switch.pinCurrentDesc")}
          htmlFor="switch-pin-current"
          operational
        >
          <Switch
            id="switch-pin-current"
            checked={config?.pin_current_account ?? false}
            disabled={saving || config === null}
            onCheckedChange={(value) => void patch({ pin_current_account: value })}
            aria-label={t("wbSettings.switch.pinCurrentLabel")}
          />
        </SettingsFieldRow>
      </CardContent>
    </SettingsGroup>
  );
}

/** 版本与账号库：两版认证文件路径（只读展示 + 复制）、国际版 UA 版本，及打开账号库目录。 */
function VersionAccountsCard() {
  const t = useT();
  const cnStatus = useAccountsStore((s) => s.status);
  const globalStatus = useAccountsStore((s) => s.global.status);
  const fetchAllRegions = useAccountsStore((s) => s.fetchAllRegions);

  useEffect(() => {
    void fetchAllRegions();
  }, [fetchAllRegions]);

  const rows: { region: Region; authFile: string | undefined }[] = [
    { region: "cn", authFile: cnStatus?.authFile },
    { region: "global", authFile: globalStatus?.authFile },
  ];

  async function onOpen(region: Region) {
    try {
      await api.openAccountsDir(region);
    } catch (e) {
      toast.error(t("wbSettings.version.openDirFailed"), { description: api.asError(e) });
    }
  }

  return (
    <SettingsGroup id="settings-regions" title={t("wbSettings.version.groupTitle")}>
      <CardContent className="space-y-0 p-0">
        {rows.map(({ region, authFile }) => {
          const descriptor = regionDescriptor(region);
          return (
            <SettingsFieldRow
              key={region}
              label={t("wbSettings.version.authFileLabel", { version: descriptor.versionLabel })}
              description={t("wbSettings.version.authFileDesc", { env: descriptor.authEnv })}
            >
              <div className="flex w-full min-w-0 items-center gap-2 sm:w-auto">
                <code
                  className="min-w-0 flex-1 truncate rounded-md border border-border bg-muted/40 px-2 py-1 font-mono text-[11px] text-muted-foreground sm:max-w-72"
                  title={authFile || ""}
                >
                  {authFile || "—"}
                </code>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={!authFile}
                  onClick={() => void copyText(authFile || "", t("wbSettings.version.copiedToast"))}
                >
                  {t("wbSettings.version.copyBtn")}
                </Button>
              </div>
            </SettingsFieldRow>
          );
        })}

        <SettingsFieldRow
          label={t("wbSettings.version.uaLabel")}
          description={t("wbSettings.version.uaDesc")}
        >
          <code className="rounded-md border border-border bg-muted/40 px-2 py-1 font-mono text-[11px] text-muted-foreground">
            WorkBuddyAI/{globalStatus?.version || "?"}
          </code>
        </SettingsFieldRow>

        <SettingsFieldRow className="border-b-0" label={t("wbSettings.version.openDirLabel")}>
          <div className="flex flex-wrap gap-2">
            <DemoAction>
              <Button variant="outline" size="sm" onClick={() => void onOpen("cn")}>
                <FolderOpen />
                {t("wbSettings.version.cn")}
              </Button>
            </DemoAction>
            <DemoAction>
              <Button variant="outline" size="sm" onClick={() => void onOpen("global")}>
                <FolderOpen />
                {t("wbSettings.version.global")}
              </Button>
            </DemoAction>
          </div>
        </SettingsFieldRow>
      </CardContent>
    </SettingsGroup>
  );
}

/** API 网关：默认监听地址 / 端口 / 日志保留条数 / 正文记录 / 独立端口模式。 */
function GatewaySettingsCard() {
  const t = useT();
  const config = useGatewayStore((s) => s.config);
  const loadConfig = useGatewayStore((s) => s.loadConfig);
  const saveConfig = useGatewayStore((s) => s.saveConfig);
  const [portDraft, setPortDraft] = useState(String(config.port));
  const [keepDraft, setKeepDraft] = useState(String(config.log_keep));
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void loadConfig();
  }, [loadConfig]);

  useEffect(() => setPortDraft(String(config.port)), [config.port]);
  useEffect(() => setKeepDraft(String(config.log_keep)), [config.log_keep]);

  async function persist(next: Partial<GatewayConfig>) {
    setSaving(true);
    try {
      await saveConfig({ ...config, ...next });
    } catch (e) {
      toast.error(t("wbSettings.common.saveFailed"), { description: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  function commitPort() {
    const parsed = Number.parseInt(portDraft, 10);
    if (!Number.isFinite(parsed) || parsed < 1 || parsed > 65535) {
      toast.error(t("wbSettings.gateway.invalidPort"));
      setPortDraft(String(config.port));
      return;
    }
    if (parsed !== config.port) void persist({ port: parsed });
  }

  function commitKeep() {
    const parsed = Number.parseInt(keepDraft, 10);
    if (!Number.isFinite(parsed) || parsed < 0 || parsed > 10000) {
      toast.error(t("wbSettings.gateway.invalidKeep"));
      setKeepDraft(String(config.log_keep));
      return;
    }
    if (parsed !== config.log_keep) void persist({ log_keep: parsed });
  }

  return (
    <SettingsGroup id="settings-gateway" title={t("wbSettings.gateway.groupTitle")}>
      <CardContent className="space-y-0 p-0">
        <SettingsFieldRow
          label={t("wbSettings.gateway.bindLabel")}
          description={t("wbSettings.gateway.bindDesc")}
          htmlFor="gw-addr"
          operational
        >
          <Select
            value={config.bind_addr}
            onValueChange={(value) => void persist({ bind_addr: value, allow_non_loopback: value !== "127.0.0.1" })}
            disabled={saving}
          >
            <SelectTrigger
              id="gw-addr"
              size="sm"
              className="w-full sm:w-44"
              aria-label={t("wbSettings.gateway.bindLabel")}
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="127.0.0.1">{t("wbSettings.gateway.bindLoopback")}</SelectItem>
              <SelectItem value="0.0.0.0">{t("wbSettings.gateway.bindLan")}</SelectItem>
            </SelectContent>
          </Select>
        </SettingsFieldRow>

        <SettingsFieldRow
          label={t("wbSettings.gateway.portLabel")}
          description={t("wbSettings.gateway.portDesc")}
          htmlFor="gw-port"
          operational
        >
          <Input
            id="gw-port"
            className="w-full sm:w-44"
            inputMode="numeric"
            value={portDraft}
            onChange={(event) => setPortDraft(event.target.value)}
            onBlur={commitPort}
          />
        </SettingsFieldRow>

        <SettingsFieldRow
          label={t("wbSettings.gateway.keepLabel")}
          description={t("wbSettings.gateway.keepDesc")}
          htmlFor="gw-keep"
          operational
        >
          <Input
            id="gw-keep"
            className="w-full sm:w-44"
            inputMode="numeric"
            value={keepDraft}
            onChange={(event) => setKeepDraft(event.target.value)}
            onBlur={commitKeep}
          />
        </SettingsFieldRow>

        <SettingsFieldRow
          className="border-b-0"
          label={t("wbSettings.gateway.bodiesLabel")}
          description={t("wbSettings.gateway.bodiesDesc")}
          htmlFor="gw-bodies"
          operational
        >
          <Switch
            id="gw-bodies"
            checked={config.log_bodies}
            disabled={saving}
            onCheckedChange={(checked) => void persist({ log_bodies: checked })}
            aria-label={t("wbSettings.gateway.bodiesLabel")}
          />
        </SettingsFieldRow>
      </CardContent>
    </SettingsGroup>
  );
}

// ---------------------------------------------------------------------------
// 定时任务排程（六类任务：各自独立开关 + 独立小时表）
// ---------------------------------------------------------------------------

type ScheduleHoursField =
  | "checkin_hours"
  | "travel_hours"
  | "activity_hours"
  | "keepalive_hours"
  | "school_hours"
  | "cat_hours";

type ScheduleEnabledField =
  | "checkin_enabled"
  | "travel_enabled"
  | "activity_enabled"
  | "keepalive_enabled"
  | "school_enabled"
  | "cat_enabled";

interface ScheduleTaskDef {
  key: string;
  /** 任务名文案键（模块级常量只存键，渲染处再 `t(labelKey)`）。 */
  labelKey: TranslationKey;
  descKey: TranslationKey;
  hoursField: ScheduleHoursField;
  enabledField: ScheduleEnabledField;
}

/**
 * WorkBuddy 分区**六类**定时任务的展示定义（顺序与后端 `ScheduleTask::all()` 的前六类一致）。
 *
 * ⚠️ 后端 `all()` 还有**第七类** `trae_checkin`（Trae 分区的自动签到），它**刻意不在此表**：
 * 本页是 WorkBuddy 分区的设置页，把另一条产品线的开关摆进来会让人误以为它属于本产品。
 * Trae 的那份入口在 Trae 设置页，两者读写**同一份** `schedule_config.json`
 * （排程是全局单份，与 region / 产品无关）。
 */
const SCHEDULE_TASKS: ScheduleTaskDef[] = [
  { key: "checkin", labelKey: "wbSettings.schedule.task.checkin.label", descKey: "wbSettings.schedule.task.checkin.desc", hoursField: "checkin_hours", enabledField: "checkin_enabled" },
  { key: "travel", labelKey: "wbSettings.schedule.task.travel.label", descKey: "wbSettings.schedule.task.travel.desc", hoursField: "travel_hours", enabledField: "travel_enabled" },
  { key: "activity", labelKey: "wbSettings.schedule.task.activity.label", descKey: "wbSettings.schedule.task.activity.desc", hoursField: "activity_hours", enabledField: "activity_enabled" },
  { key: "keepalive", labelKey: "wbSettings.schedule.task.keepalive.label", descKey: "wbSettings.schedule.task.keepalive.desc", hoursField: "keepalive_hours", enabledField: "keepalive_enabled" },
  { key: "school", labelKey: "wbSettings.schedule.task.school.label", descKey: "wbSettings.schedule.task.school.desc", hoursField: "school_hours", enabledField: "school_enabled" },
  { key: "cat", labelKey: "wbSettings.schedule.task.cat.label", descKey: "wbSettings.schedule.task.cat.desc", hoursField: "cat_hours", enabledField: "cat_enabled" },
];

/**
 * 把「立即执行」的返回压成一句可读摘要。
 *
 * 活跃地图单独处理：它要回答的正是「官网连登到底点亮了没」，故回报上报条数与连登天数
 * （`reported` 与 `streakDays` 由后端逐账号返回）。
 */
function summarizeScheduleRun(
  task: ScheduleTaskDef,
  res: ScheduleRunResult,
): { key: TranslationKey; params?: Record<string, string | number> } {
  if (task.key !== "activity") return { key: "wbSettings.schedule.summaryExecuted" };
  let reported = 0;
  let streak: number | null = null;
  for (const region of res.regions ?? []) {
    const accounts = Array.isArray(region.accounts)
      ? (region.accounts as Record<string, unknown>[])
      : [];
    for (const account of accounts) {
      reported += Number(account.reported ?? 0);
      const days = Number(account.streakDays);
      if (Number.isFinite(days) && days > 0) {
        streak = streak === null ? days : Math.max(streak, days);
      }
    }
  }
  return streak === null
    ? { key: "wbSettings.schedule.summaryNoStreak", params: { reported } }
    : { key: "wbSettings.schedule.summaryStreak", params: { reported, streak } };
}

/** 定时任务排程配置：六类任务各自独立开关与小时表 + 活跃上报次数。 */
function ScheduleCard() {
  const t = useT();
  const [cfg, setCfg] = useState<ScheduleConfig | null>(null);
  const [saving, setSaving] = useState(false);
  /** 正在立即执行的任务 key（null 表示空闲）；一次只跑一类，避免并发打到同一批账号。 */
  const [running, setRunning] = useState<string | null>(null);
  const [msg, setMsg] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api
      .getScheduleConfig()
      .then((value) => {
        if (!cancelled) setCfg(value);
      })
      .catch((e) => {
        if (!cancelled) setMsg({ type: "err", text: api.asError(e) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  function patch(task: ScheduleTaskDef, hours: number[]) {
    if (!cfg) return;
    setCfg({ ...cfg, [task.hoursField]: hours });
  }

  function toggle(task: ScheduleTaskDef, enabled: boolean) {
    if (!cfg) return;
    setCfg({ ...cfg, [task.enabledField]: enabled });
  }

  /** 前端基本范围校验：小时必须为 0-23 的整数；已启用任务至少需一个小时点。 */
  function validate(config: ScheduleConfig): string | null {
    for (const task of SCHEDULE_TASKS) {
      const hours = config[task.hoursField];
      const bad = hours.find((hour) => !Number.isInteger(hour) || hour < 0 || hour > 23);
      if (bad !== undefined) {
        return t("wbSettings.schedule.invalidHour", { label: t(task.labelKey), hour: bad });
      }
      if (config[task.enabledField] && hours.length === 0) {
        return t("wbSettings.schedule.emptyHours", { label: t(task.labelKey) });
      }
    }
    if (!Number.isInteger(config.activity_report_count) || config.activity_report_count < 1) {
      return t("wbSettings.schedule.invalidReportCount");
    }
    return null;
  }

  /** 立即执行某一类任务：不等排程到点，用于保存配置后当场自证是否生效。 */
  async function runNow(task: ScheduleTaskDef) {
    setRunning(task.key);
    setMsg(null);
    try {
      const res = await api.runScheduleTask(task.key);
      const summary = summarizeScheduleRun(task, res);
      setMsg({
        type: "ok",
        text: t("wbSettings.schedule.runDone", {
          label: t(task.labelKey),
          summary: t(summary.key, summary.params),
        }),
      });
    } catch (e) {
      setMsg({ type: "err", text: api.asError(e) });
    } finally {
      setRunning(null);
    }
  }

  async function save() {
    if (!cfg) return;
    const invalid = validate(cfg);
    if (invalid) {
      setMsg({ type: "err", text: invalid });
      return;
    }
    setSaving(true);
    setMsg(null);
    try {
      const saved = await api.saveScheduleConfig(cfg);
      setCfg(saved);
      setMsg({ type: "ok", text: t("wbSettings.schedule.savedMsg") });
    } catch (e) {
      // 后端 400 的 error 文案会指明具体是哪个 `*_enabled` 开关，原样展示以保留诊断价值。
      setMsg({ type: "err", text: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  return (
    <SettingsGroup id="settings-schedule" title={t("wbSettings.schedule.groupTitle")}>
      <CardContent className="space-y-0 p-0">
        <p className="border-b border-border/60 bg-muted/25 px-4 py-3 text-xs leading-5 text-muted-foreground sm:px-5">
          {t("wbSettings.schedule.intro")}
        </p>

        {cfg ? (
          <>
            {SCHEDULE_TASKS.map((task) => (
              <SettingsRow
                key={task.key}
                className="flex-col items-stretch gap-3 sm:flex-row sm:items-start"
              >
                <div className="flex min-w-0 flex-1 items-start gap-3">
                  <Switch
                    id={`schedule-${task.key}-enabled`}
                    checked={cfg[task.enabledField]}
                    onCheckedChange={(v) => toggle(task, v)}
                    aria-label={t("wbSettings.schedule.enableAria", { label: t(task.labelKey) })}
                  />
                  <div className="min-w-0">
                    <Label htmlFor={`schedule-${task.key}-enabled`} className="text-[13px] leading-4">
                      {t(task.labelKey)}
                    </Label>
                    <p className="mt-0.5 text-xs leading-4 text-muted-foreground/75">{t(task.descKey)}</p>
                    <DemoAction>
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        className="mt-2 h-7 px-2 text-xs"
                        disabled={running !== null}
                        onClick={() => runNow(task)}
                      >
                        {running === task.key ? <Loader2 className="animate-spin" /> : <Play />}
                        {t("wbSettings.schedule.runNowBtn")}
                      </Button>
                    </DemoAction>
                  </div>
                </div>
                <HoursEditor
                  id={`schedule-${task.key}-hour`}
                  hours={cfg[task.hoursField]}
                  onChange={(hours) => patch(task, hours)}
                />
              </SettingsRow>
            ))}

            <SettingsFieldRow
              label={t("wbSettings.schedule.activityCountLabel")}
              description={t("wbSettings.schedule.activityCountDesc")}
              htmlFor="schedule-activity-count"
            >
              <Input
                id="schedule-activity-count"
                className="w-full sm:w-48"
                type="number"
                min={1}
                value={cfg.activity_report_count}
                onChange={(e) => setCfg({ ...cfg, activity_report_count: Number(e.target.value) })}
              />
            </SettingsFieldRow>

            <div className="flex flex-wrap gap-2 border-b-0 border-border/60 px-4 py-3 sm:px-5">
              <DemoAction>
                <Button size="sm" onClick={save} disabled={saving}>
                  {saving ? <Loader2 className="animate-spin" /> : <Save />}{t("wbSettings.schedule.saveBtn")}
                </Button>
              </DemoAction>
            </div>
          </>
        ) : (
          <p className="px-4 py-3 text-sm text-muted-foreground sm:px-5">{t("wbSettings.common.loading")}</p>
        )}

        {msg && (
          <Alert
            variant={msg.type === "err" ? "destructive" : "default"}
            className="!w-auto mx-4 my-4 sm:mx-5"
          >
            <AlertDescription>{msg.text}</AlertDescription>
          </Alert>
        )}
      </CardContent>
    </SettingsGroup>
  );
}

/**
 * 设置页：**产品级**设置。
 *
 * 只放依赖本产品上下文的块（版本与账号库、权限检测、网关、签到、排程、CLI 轮换）。
 *
 * 应用级的三块（外观 / 开机自启 / 自动更新）已抽到 `@/components/app-settings`，
 * 入口固定在侧栏底部、**版本号上方** —— 那是两个产品分区唯一共用的位置。
 */
export default function SettingsPage() {
  const t = useT();
  return (
    <div className="mx-auto min-w-0 w-full max-w-3xl px-4 py-6 sm:px-6 sm:py-8">
      <header className="mb-10 sm:mb-12">
        <h1 className="text-2xl font-semibold tracking-tight">{t("wbSettings.page.title")}</h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          {t("wbSettings.page.subtitle")}
        </p>
      </header>

      <div className="min-w-0 space-y-12">
        <VersionAccountsCard />
        <SwitchBehaviorCard />
        <PermissionCheckCard />
        <GatewaySettingsCard />
        <AutoCheckinCard />
        <ScheduleCard />
        <AutoRotateCard />
      </div>
    </div>
  );
}

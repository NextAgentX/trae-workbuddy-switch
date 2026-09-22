import { useEffect, useState } from "react";
import {
  CircleCheck,
  FolderOpen,
  Loader2,
  Play,
  Plus,
  RefreshCw,
  Save,
  X,
} from "lucide-react";
import { toast } from "sonner";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CardContent } from "@/components/ui/card";
import { SettingsFieldRow, SettingsGroup, SettingsRow } from "@/components/settings-primitives";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import * as api from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { regionDescriptor } from "@/lib/region";
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

function logLabel(result: string): { text: string; tone: "success" | "warning" | "error" } {
  switch (result) {
    case "success":
      return { text: "签到成功", tone: "success" };
    case "already":
      return { text: "已签到", tone: "warning" };
    default:
      return { text: "失败", tone: "error" };
  }
}

/** 自动签到配置 + 一键签到 + 日志。 */
function AutoCheckinCard() {
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
      setMsg({ type: "ok", text: "配置已保存" });
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
        setMsg({ type: "err", text: "签到任务正在进行，请稍后再试" });
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
        text: `签到完成：成功 ${ok}，已签 ${already}，失败 ${err}${detail ? `。${detail}` : ""}`,
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
      title="自动签到"
    >
      <CardContent className="space-y-0 p-0">
        {cfg ? (
          <>
            <SettingsFieldRow
              label="启用自动签到"
              description="启动时立即核验服务端状态，未签到账号会自动补签"
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
              label="保活阈值"
              description="天；0 表示每天无条件刷新"
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
            <SettingsFieldRow label="惰性刷新" description="小时" htmlFor="ac-lazy" operational>
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
                {saving ? <Loader2 className="animate-spin" /> : <Save />}保存配置
              </Button></DemoAction>
              <DemoAction><Button size="sm" variant="outline" onClick={checkinAllNow} disabled={busy}>
                {busy ? <Loader2 className="animate-spin" /> : <CircleCheck />}全部立即签到
              </Button></DemoAction>
            </div>
          </>
        ) : (
          <p className="px-4 py-3 text-sm text-muted-foreground sm:px-5">加载配置中…</p>
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
          <p className="mb-2 text-[13px] font-medium">签到日志（最近 30 天）</p>
          {logs.length === 0 ? (
            <p className="py-3 text-center text-sm text-muted-foreground">暂无签到记录</p>
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

/** 自动轮换配置（CodeBuddy CLI）+ 手动检查 + 日志。 */
function AutoRotateCard() {
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
      setMsg({ type: "ok", text: "配置已保存" });
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
            ? `已切换到 ${res.to ?? "目标账号"}`
            : res.status === "disabled"
              ? "自动轮换未启用（请在下方开启后重试）"
              : (res.reason ?? `检查完成：${res.status}`),
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
        return { text: "已切换", tone: "success" };
      case "skipped":
        return { text: "未切换", tone: "warning" };
      case "disabled":
        return { text: "未启用", tone: "warning" };
      case "error":
        return { text: "出错", tone: "error" };
      default:
        return { text: action, tone: "warning" };
    }
  }

  return (
    <SettingsGroup
      id="settings-auto-rotate"
      title="CodeBuddy CLI 自动轮换"
    >
      <CardContent className="space-y-0 p-0">
        {status && (
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-border/60 bg-muted/25 px-4 py-3 text-xs text-muted-foreground sm:px-5">
            <span>
              当前 CLI 账号：
              <b className="text-foreground">{status.activeAccountName ?? "未配置"}</b>
            </span>
            {status.lastCheckAt && <span>上次检查 {formatTime(status.lastCheckAt)}</span>}
            {status.lastSwitchAt && <span>上次切换 {formatTime(status.lastSwitchAt)}</span>}
            {!status.cliConfigured && (
              <span className="text-destructive">未接入 CodeBuddy CLI（请先到账号页安装 helper）</span>
            )}
          </div>
        )}

        {cfg ? (
          <>
            <SettingsFieldRow
              label="启用自动轮换"
              description="开启后按下方间隔自动检查并切换 CodeBuddy CLI 账号"
              htmlFor="ar-enabled"
              operational
            >
              <Switch
                id="ar-enabled"
                checked={cfg.enabled}
                onCheckedChange={(v) => setCfg({ ...cfg, enabled: v })}
              />
            </SettingsFieldRow>

            <SettingsFieldRow label="检查间隔" description="分钟" htmlFor="ar-interval" operational>
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
            <SettingsFieldRow label="切换冷却" description="分钟" htmlFor="ar-cooldown" operational>
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
            <SettingsFieldRow label="到期差异阈值" description="小时" htmlFor="ar-gap" operational>
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
            <SettingsFieldRow label="到期紧迫阈值" description="小时" htmlFor="ar-urgency" operational>
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
            <SettingsFieldRow label="活跃保护" description="分钟" htmlFor="ar-guard" operational>
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
            <SettingsFieldRow label="最小剩余积分" description="低于此值时不切换" htmlFor="ar-min" operational>
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
              切换时机：目标账号剩余到期时间少于「紧迫阈值」且比当前账号早超过「差异阈值」，且最近「活跃保护」分钟内 CLI 无对话、目标剩余积分不低于「最小剩余积分」。
            </p>

            <div className="flex flex-wrap gap-2 border-b-0 border-border/60 px-4 py-3 sm:px-5">
              <DemoAction><Button size="sm" onClick={save} disabled={saving}>
                {saving ? <Loader2 className="animate-spin" /> : <Save />}保存配置
              </Button></DemoAction>
              <DemoAction><Button size="sm" variant="outline" onClick={runNow} disabled={busy}>
                {busy ? <Loader2 className="animate-spin" /> : <RefreshCw />}立即检查一次
              </Button></DemoAction>
            </div>
          </>
        ) : (
          <p className="px-4 py-3 text-sm text-muted-foreground sm:px-5">加载配置中…</p>
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
          <p className="mb-2 text-[13px] font-medium">轮换日志（最近 200 条）</p>
          {logs.length === 0 ? (
            <p className="py-3 text-center text-sm text-muted-foreground">暂无轮换记录</p>
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
          ? res.message ?? "认证目录可写，权限正常"
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
      title="权限检测"
    >
      <CardContent className="space-y-0 p-0">
        <div className="break-all border-b border-border/60 bg-muted/25 px-4 py-3 font-mono text-[11px] leading-5 text-muted-foreground sm:px-5">
          {authFile || "认证文件路径未获取"}
        </div>
        <div className="flex flex-wrap gap-2 border-b-0 border-border/60 px-4 py-3 sm:px-5">
          <DemoAction><Button size="sm" onClick={runCheck} disabled={checking}>
            {checking ? "检测中…" : "检测权限"}
          </Button></DemoAction>
          <DemoAction><Button
            size="sm"
            variant="outline"
            onClick={() => void api.openPermissionSettings("all_files")}
          >
            打开完全磁盘访问
          </Button></DemoAction>
          <DemoAction><Button
            size="sm"
            variant="outline"
            onClick={() => void api.openPermissionSettings("app_management")}
          >
            打开 App 管理
          </Button></DemoAction>
          <DemoAction><Button size="sm" variant="outline" onClick={() => void api.revealAppInFinder()}>
            在 Finder 中显示
          </Button></DemoAction>
        </div>

        {result && (
          <Alert variant={result.ok ? "default" : "destructive"} className="!w-auto mx-4 my-4 sm:mx-5">
            <AlertDescription>{result.text}</AlertDescription>
          </Alert>
        )}
        {result && !result.ok && (
          <div className="mx-4 mb-4 border-l-2 border-destructive/50 bg-muted/30 px-3 py-2.5 text-xs text-muted-foreground sm:mx-5">
            <p className="mb-1 font-medium text-foreground">如何授权（拖拽方式）：</p>
            <ol className="list-decimal space-y-1 pl-4">
              <li>点上方「打开完全磁盘访问」</li>
              <li>再点「在 Finder 中显示」打开 BuddySwitch 所在位置</li>
              <li>
                把 <b>BuddySwitch.app</b> 从 Finder <b>直接拖进</b>完全磁盘访问的列表区域
                （即使没有提示框，拖入即生效），然后打开它的开关
              </li>
              <li>回到本页点「检测权限」，或直接重试切换</li>
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
        if (!cancelled) toast.error("账号切换配置加载失败", { description: api.asError(e) });
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
      toast.error("保存失败", { description: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  return (
    <SettingsGroup id="settings-switch" title="账号切换">
      <CardContent className="space-y-0 p-0">
        <SettingsFieldRow
          label="切换账号时默认复制会话"
          description="打开后，切换账号时会默认勾选「复制会话」并全选当前账号的会话；仍可在弹窗里逐条取消。"
          htmlFor="switch-copy-sessions"
          operational
        >
          <Switch
            id="switch-copy-sessions"
            checked={config?.copy_sessions_by_default ?? false}
            disabled={saving || config === null}
            onCheckedChange={(value) => void patch({ copy_sessions_by_default: value })}
            aria-label="切换账号时默认复制会话"
          />
        </SettingsFieldRow>

        <SettingsFieldRow
          className="border-b-0"
          label="把当前账号置顶"
          description="账号列表里把当前登录账号排到第一位；其余账号仍按积分优先级排序，「建议优先」标记不受影响。"
          htmlFor="switch-pin-current"
          operational
        >
          <Switch
            id="switch-pin-current"
            checked={config?.pin_current_account ?? false}
            disabled={saving || config === null}
            onCheckedChange={(value) => void patch({ pin_current_account: value })}
            aria-label="把当前账号置顶"
          />
        </SettingsFieldRow>
      </CardContent>
    </SettingsGroup>
  );
}

/** 版本与账号库：两版认证文件路径（只读展示 + 复制）、国际版 UA 版本，及打开账号库目录。 */
function VersionAccountsCard() {
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
      toast.error("打开目录失败", { description: api.asError(e) });
    }
  }

  return (
    <SettingsGroup id="settings-regions" title="版本与账号库">
      <CardContent className="space-y-0 p-0">
        {rows.map(({ region, authFile }) => {
          const descriptor = regionDescriptor(region);
          return (
            <SettingsFieldRow
              key={region}
              label={`${descriptor.versionLabel}认证文件`}
              description={`环境变量 ${descriptor.authEnv} 可覆盖`}
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
                  onClick={() => void copyText(authFile || "", "路径已复制")}
                >
                  复制
                </Button>
              </div>
            </SettingsFieldRow>
          );
        })}

        <SettingsFieldRow
          label="国际版 UA 版本"
          description="目录请求使用 WorkBuddyAI/<版本>（无空格）"
        >
          <code className="rounded-md border border-border bg-muted/40 px-2 py-1 font-mono text-[11px] text-muted-foreground">
            WorkBuddyAI/{globalStatus?.version || "?"}
          </code>
        </SettingsFieldRow>

        <SettingsFieldRow className="border-b-0" label="打开账号库所在目录">
          <div className="flex flex-wrap gap-2">
            <DemoAction>
              <Button variant="outline" size="sm" onClick={() => void onOpen("cn")}>
                <FolderOpen />
                国内版
              </Button>
            </DemoAction>
            <DemoAction>
              <Button variant="outline" size="sm" onClick={() => void onOpen("global")}>
                <FolderOpen />
                国际版
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
      toast.error("保存失败", { description: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  function commitPort() {
    const parsed = Number.parseInt(portDraft, 10);
    if (!Number.isFinite(parsed) || parsed < 1 || parsed > 65535) {
      toast.error("端口需为 1-65535 之间的整数");
      setPortDraft(String(config.port));
      return;
    }
    if (parsed !== config.port) void persist({ port: parsed });
  }

  function commitKeep() {
    const parsed = Number.parseInt(keepDraft, 10);
    if (!Number.isFinite(parsed) || parsed < 0 || parsed > 10000) {
      toast.error("保留条数需为 0-10000 之间的整数");
      setKeepDraft(String(config.log_keep));
      return;
    }
    if (parsed !== config.log_keep) void persist({ log_keep: parsed });
  }

  return (
    <SettingsGroup id="settings-gateway" title="API 网关">
      <CardContent className="space-y-0 p-0">
        <SettingsFieldRow
          label="网关默认监听地址"
          description="默认仅监听回环地址；0.0.0.0 表示允许局域网访问"
          htmlFor="gw-addr"
          operational
        >
          <Select
            value={config.bind_addr}
            onValueChange={(value) => void persist({ bind_addr: value, allow_non_loopback: value !== "127.0.0.1" })}
            disabled={saving}
          >
            <SelectTrigger id="gw-addr" size="sm" className="w-full sm:w-44" aria-label="网关默认监听地址">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="127.0.0.1">127.0.0.1（仅本机）</SelectItem>
              <SelectItem value="0.0.0.0">0.0.0.0（局域网）</SelectItem>
            </SelectContent>
          </Select>
        </SettingsFieldRow>

        <SettingsFieldRow label="网关默认端口" description="默认 57891，与 webui 端口区分" htmlFor="gw-port" operational>
          <Input
            id="gw-port"
            className="w-full sm:w-44"
            inputMode="numeric"
            value={portDraft}
            onChange={(event) => setPortDraft(event.target.value)}
            onBlur={commitPort}
          />
        </SettingsFieldRow>

        <SettingsFieldRow label="请求日志保留条数" description="仅记录元数据" htmlFor="gw-keep" operational>
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
          label="记录请求正文"
          description="含隐私风险；默认关闭，仅记录时间/模型/状态码等元数据"
          htmlFor="gw-bodies"
          operational
        >
          <Switch
            id="gw-bodies"
            checked={config.log_bodies}
            disabled={saving}
            onCheckedChange={(checked) => void persist({ log_bodies: checked })}
            aria-label="记录请求正文"
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
  label: string;
  description: string;
  hoursField: ScheduleHoursField;
  enabledField: ScheduleEnabledField;
}

/** 六类定时任务的展示定义（顺序与后端 `ScheduleTask::all()` 一致）。 */
const SCHEDULE_TASKS: ScheduleTaskDef[] = [
  { key: "checkin", label: "签到", description: "自动签到各账号", hoursField: "checkin_hours", enabledField: "checkin_enabled" },
  { key: "travel", label: "猫猫旅行", description: "派猫猫出门旅行并领取奖励", hoursField: "travel_hours", enabledField: "travel_enabled" },
  { key: "activity", label: "活跃地图", description: "活跃地图上报，点亮连登", hoursField: "activity_hours", enabledField: "activity_enabled" },
  { key: "keepalive", label: "token 保活", description: "刷新登录态，避免 token 过期", hoursField: "keepalive_hours", enabledField: "keepalive_enabled" },
  { key: "school", label: "开学季", description: "开学季任务", hoursField: "school_hours", enabledField: "school_enabled" },
  { key: "cat", label: "夜猫子", description: "夜猫子（猫猫领取）任务", hoursField: "cat_hours", enabledField: "cat_enabled" },
];

/**
 * 单个任务的小时列表编辑器：以标签展示当前小时点，可逐个删除或新增。
 *
 * 输入侧即做 0-23 整数校验（非法时不入列并给出可读提示），仅允许合法的整点小时。
 */
function HoursEditor({
  id,
  hours,
  disabled,
  onChange,
}: {
  id: string;
  hours: number[];
  disabled?: boolean;
  onChange: (hours: number[]) => void;
}) {
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  function add() {
    const raw = draft.trim();
    const value = Number(raw);
    if (raw === "" || !Number.isInteger(value) || value < 0 || value > 23) {
      setError("小时必须是 0-23 之间的整数");
      return;
    }
    if (hours.includes(value)) {
      setError(`${value} 点已在列表中`);
      return;
    }
    setError(null);
    setDraft("");
    onChange([...hours, value].sort((a, b) => a - b));
  }

  return (
    <div className="flex w-full flex-col items-end gap-2 sm:w-auto">
      <div className="flex w-full flex-wrap items-center justify-end gap-1.5">
        {hours.length === 0 ? (
          <span className="text-xs text-muted-foreground">未设置小时点</span>
        ) : (
          hours.map((hour) => (
            <Badge key={hour} variant="secondary" className="gap-1 pr-1 font-mono">
              {String(hour).padStart(2, "0")}:00
              <button
                type="button"
                aria-label={`移除 ${hour} 点`}
                className="rounded-full p-0.5 text-muted-foreground transition-colors hover:bg-foreground/10 hover:text-foreground disabled:pointer-events-none disabled:opacity-50"
                disabled={disabled}
                onClick={() => onChange(hours.filter((h) => h !== hour))}
              >
                <X className="size-3" />
              </button>
            </Badge>
          ))
        )}
      </div>
      <div className="flex w-full items-center justify-end gap-2">
        <Input
          id={id}
          className="w-full sm:w-24"
          type="number"
          min={0}
          max={23}
          inputMode="numeric"
          placeholder="0-23"
          value={draft}
          disabled={disabled}
          onChange={(e) => {
            setDraft(e.target.value);
            if (error) setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
        />
        <Button type="button" size="sm" variant="outline" onClick={add} disabled={disabled}>
          <Plus />
          添加
        </Button>
      </div>
      {error && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}

/**
 * 把「立即执行」的返回压成一句可读摘要。
 *
 * 活跃地图单独处理：它要回答的正是「官网连登到底点亮了没」，故回报上报条数与连登天数
 * （`reported` 与 `streakDays` 由后端逐账号返回）。
 */
function summarizeScheduleRun(task: ScheduleTaskDef, res: ScheduleRunResult): string {
  if (task.key !== "activity") return "已执行";
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
    ? `已上报 ${reported} 条（未读到连登天数）`
    : `已上报 ${reported} 条，当前连登 ${streak} 天`;
}

/** 定时任务排程配置：六类任务各自独立开关与小时表 + 活跃上报次数。 */
function ScheduleCard() {
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
        return `「${task.label}」包含非法小时 ${bad}，小时必须是 0-23 的整数`;
      }
      if (config[task.enabledField] && hours.length === 0) {
        return `「${task.label}」已启用，请至少设置一个小时点，或关闭该任务`;
      }
    }
    if (!Number.isInteger(config.activity_report_count) || config.activity_report_count < 1) {
      return "活跃上报次数必须是大于 0 的整数";
    }
    return null;
  }

  /** 立即执行某一类任务：不等排程到点，用于保存配置后当场自证是否生效。 */
  async function runNow(task: ScheduleTaskDef) {
    setRunning(task.key);
    setMsg(null);
    try {
      const res = await api.runScheduleTask(task.key);
      setMsg({ type: "ok", text: `「${task.label}」${summarizeScheduleRun(task, res)}` });
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
      setMsg({ type: "ok", text: "排程配置已保存" });
    } catch (e) {
      // 后端 400 的 error 文案会指明具体是哪个 `*_enabled` 开关，原样展示以保留诊断价值。
      setMsg({ type: "err", text: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  return (
    <SettingsGroup id="settings-schedule" title="定时任务排程">
      <CardContent className="space-y-0 p-0">
        <p className="border-b border-border/60 bg-muted/25 px-4 py-3 text-xs leading-5 text-muted-foreground sm:px-5">
          六类任务各自独立开关与小时表，到点由调度器触发。小时使用 24 小时制本地时间，可配置多个小时点。
          改完可用「立即执行」当场跑一轮验证，无需等到下一个整点。
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
                    aria-label={`启用${task.label}`}
                  />
                  <div className="min-w-0">
                    <Label htmlFor={`schedule-${task.key}-enabled`} className="text-[13px] leading-4">
                      {task.label}
                    </Label>
                    <p className="mt-0.5 text-xs leading-4 text-muted-foreground/75">{task.description}</p>
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
                        立即执行
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
              label="活跃上报次数"
              description="每个账号每天上报的对话次数"
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
                  {saving ? <Loader2 className="animate-spin" /> : <Save />}保存排程
                </Button>
              </DemoAction>
            </div>
          </>
        ) : (
          <p className="px-4 py-3 text-sm text-muted-foreground sm:px-5">加载配置中…</p>
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
  return (
    <div className="mx-auto min-w-0 w-full max-w-3xl px-4 py-6 sm:px-6 sm:py-8">
      <header className="mb-10 sm:mb-12">
        <h1 className="text-2xl font-semibold tracking-tight">设置</h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          自动签到、权限检测与网关配置。外观、开机自启与自动更新属应用级设置，见侧栏底部「通用设置」。
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

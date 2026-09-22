import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import {
  AlertTriangle,
  Columns3,
  Download,
  FileDown,
  FileUp,
  Loader2,
  QrCode,
  RefreshCw,
  Rows3,
  Terminal,
} from "lucide-react";

import { AccountCard } from "@/components/account-card";
import { DemoAction } from "@/components/demo-action";
import { CodeBuddyCnIdeMark, CodeBuddyMark, WorkBuddyMark } from "@/components/product-marks";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ExportAccountsDialog } from "@/components/export-accounts-dialog";
import { ImportAccountsDialog } from "@/components/import-accounts-dialog";
import { OAuthLoginDialog } from "@/components/oauth-login-dialog";
import { SwitchAccountDialog } from "@/components/switch-account-dialog";
import * as api from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { REGIONS, regionDescriptor } from "@/lib/region";
import type {
  AccountMeta,
  AppStatus,
  CheckinConfig,
  CodeBuddyCliStatus,
  CodeBuddyCnIdeStatus,
  CreditExpiry,
  Region,
  SwitchConfig,
  TravelConfig,
  TravelStatus,
} from "@/lib/types";
import { cn } from "@/lib/utils";
import { useCompactMode } from "@/lib/use-compact-mode";
import { useAccountsStore } from "@/stores/accounts";

function expiringSoonAmount(credit?: CreditExpiry): number {
  return credit?.ok ? credit.expiringSoonRemaining ?? 0 : 0;
}

function hasExpiringSoonCredits(credit?: CreditExpiry): boolean {
  return credit?.ok === true && expiringSoonAmount(credit) > 0;
}

function soonestRelevantExpiry(credit?: CreditExpiry): number {
  const soonestExpiringCredit = (credit?.resources ?? [])
    .filter((resource) => resource.remaining > 0 && resource.expiringSoon && resource.expireAt != null)
    .map((resource) => resource.expireAt as number)
    .reduce((soonest, expireAt) => Math.min(soonest, expireAt), Number.POSITIVE_INFINITY);
  return Number.isFinite(soonestExpiringCredit)
    ? soonestExpiringCredit
    : credit?.soonestExpireAt ?? Number.POSITIVE_INFINITY;
}

function creditPriorityRank(credit?: CreditExpiry): number {
  if (!credit?.ok) return 3;
  if (hasExpiringSoonCredits(credit)) return 0;
  if (credit.expired) return 1;
  return 2;
}

function isWorkbuddyCurrent(account: AccountMeta, current: AppStatus["current"] | undefined): boolean {
  if (!current) return false;
  return Boolean(
    (current.uid && (account.uid === current.uid || account.id === current.uid)) ||
      (current.email && account.email === current.email),
  );
}

/** 并行查询今日签到；失败的账号不写入，由调用方保留原值。 */
async function fetchTodayCheckinMap(
  accountIds: string[],
  region: Region,
  isStale?: () => boolean,
): Promise<Record<string, boolean>> {
  const entries = await Promise.all(
    accountIds.map(async (id) => {
      try {
        const res = await api.getCheckinStatus(id, region);
        if (isStale?.() || !res.ok) return null;
        return [id, res.todayCheckedIn] as const;
      } catch {
        return null;
      }
    }),
  );
  const next: Record<string, boolean> = {};
  for (const entry of entries) {
    if (entry) next[entry[0]] = entry[1];
  }
  return next;
}

/** 并行查询各账号今日旅行状态；失败的账号不写入，由调用方保留原值。 */
async function fetchTravelMap(
  accountIds: string[],
  region: Region,
  isStale?: () => boolean,
): Promise<Record<string, TravelStatus>> {
  const entries = await Promise.all(
    accountIds.map(async (id) => {
      try {
        const res = await api.getTravelStatus(id, region);
        if (isStale?.()) return null;
        return [id, res] as const;
      } catch {
        return null;
      }
    }),
  );
  const next: Record<string, TravelStatus> = {};
  for (const entry of entries) {
    if (entry) next[entry[0]] = entry[1];
  }
  return next;
}

// ---------------------------------------------------------------------------
// 版本状态（Tab 徽标 / 空态判定）
// ---------------------------------------------------------------------------

type RegionPresence = "logged-in" | "installed" | "absent";

/**
 * 判定某版本的展示状态：
 * - logged-in：该 region 有当前登录账号（status.current 非空）。
 * - installed：客户端已安装（后端 status.installed === true）或账号库非空但未登录。
 * - absent：未检测到该版本（后端 status.installed === false，或状态与账号库均为空）。
 */
function regionPresence(status: AppStatus | null, accounts: AccountMeta[]): RegionPresence {
  if (status?.current) return "logged-in";
  if (status?.installed === false) return "absent";
  if (status?.installed === true || accounts.length > 0) return "installed";
  return "absent";
}

function presenceText(presence: RegionPresence, status: AppStatus | null): string {
  if (presence === "logged-in") {
    const name = status?.current?.nickname || status?.current?.email || status?.current?.uid || "未知账号";
    return `已登录: ${name}`;
  }
  if (presence === "installed") return "未登录";
  return "未检测到";
}

function RegionTab({ region, active }: { region: Region; active: boolean }) {
  const status = useAccountsStore((s) => (region === "cn" ? s.status : s.global.status));
  const accounts = useAccountsStore((s) => (region === "cn" ? s.accounts : s.global.accounts));
  const descriptor = regionDescriptor(region);
  const presence = regionPresence(status, accounts);

  return (
    <TabsTrigger
      value={region}
      className="h-auto flex-col items-start gap-0.5 rounded-lg px-4 py-2 text-left"
    >
      <span className={cn("flex items-center gap-1.5 text-[13px] font-medium", active ? "text-foreground" : "text-muted-foreground")}>
        <span
          className={cn(
            "inline-block size-2 rounded-full",
            presence === "logged-in"
              ? "bg-primary"
              : presence === "installed"
                ? "bg-muted-foreground/40"
                : "border border-muted-foreground/50",
          )}
        />
        {descriptor.versionLabel}
        <span className="text-muted-foreground/70">{descriptor.displayName}</span>
      </span>
      <span className="pl-3.5 text-[11px] font-normal text-muted-foreground">{presenceText(presence, status)}</span>
    </TabsTrigger>
  );
}

export default function AccountsPage() {
  const [activeRegion, setActiveRegion] = useState<Region>("cn");
  const fetchAllRegions = useAccountsStore((s) => s.fetchAllRegions);

  useEffect(() => {
    void fetchAllRegions();
  }, [fetchAllRegions]);

  return (
    <div className="mx-auto w-full max-w-[1180px] px-6 py-8 sm:px-8 sm:py-9">
      <header className="mb-6">
        <h1 className="text-[28px] font-semibold tracking-tight">账号管理</h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          分别管理国内版与国际版 WorkBuddy 账号、积分和签到状态，两版账号库互相隔离。
        </p>
      </header>

      <Tabs value={activeRegion} onValueChange={(value) => setActiveRegion(value as Region)}>
        <TabsList className="mb-6 h-auto gap-1 p-1">
          {REGIONS.map((region) => (
            <RegionTab key={region} region={region} active={activeRegion === region} />
          ))}
        </TabsList>
        <TabsContent value="cn">
          <RegionPanel region="cn" />
        </TabsContent>
        <TabsContent value="global">
          <RegionPanel region="global" />
        </TabsContent>
      </Tabs>
    </div>
  );
}

/** 单个版本的完整账号管理区块（账号卡片、签到开关、积分、刷新等）。 */
function RegionPanel({ region }: { region: Region }) {
  const accounts = useAccountsStore((s) => (region === "cn" ? s.accounts : s.global.accounts));
  const status = useAccountsStore((s) => (region === "cn" ? s.status : s.global.status));
  const loading = useAccountsStore((s) => (region === "cn" ? s.loading : s.global.loading));
  const error = useAccountsStore((s) => (region === "cn" ? s.error : s.global.error));
  const creditMap = useAccountsStore((s) => (region === "cn" ? s.creditMap : s.global.creditMap));
  const creditLoadingMap = useAccountsStore((s) => (region === "cn" ? s.creditLoadingMap : s.global.creditLoadingMap));
  const creditUpdatedAtMap = useAccountsStore((s) => (region === "cn" ? s.creditUpdatedAtMap : s.global.creditUpdatedAtMap));
  const refreshingCredits = useAccountsStore((s) => (region === "cn" ? s.refreshingCredits : s.global.refreshingCredits));
  const reconcileAccounts = useAccountsStore((s) => s.reconcileAccounts);
  const refreshRegionStatus = useAccountsStore((s) => s.refreshRegionStatus);
  const deleteAccountStore = useAccountsStore((s) => s.deleteAccount);
  const ensureCredits = useAccountsStore((s) => s.ensureCredits);
  const refreshCredits = useAccountsStore((s) => s.refreshCredits);
  const importLocalStore = useAccountsStore((s) => s.importLocal);

  const descriptor = regionDescriptor(region);

  const [oauthOpen, setOauthOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [switchAccount, setSwitchAccount] = useState<AccountMeta | null>(null);
  const [importing, setImporting] = useState(false);
  const [autoCheckinConfig, setAutoCheckinConfig] = useState<CheckinConfig | null>(null);
  const [autoCheckinSaving, setAutoCheckinSaving] = useState(false);
  /** 账号 id -> 今日是否已签到（undefined=查询中/未知） */
  const [checkinMap, setCheckinMap] = useState<Record<string, boolean>>({});
  const [autoTravelConfig, setAutoTravelConfig] = useState<TravelConfig | null>(null);
  const [autoTravelSaving, setAutoTravelSaving] = useState(false);
  /**
   * 账号切换 / 账号列表展示配置（全局单份，不随 region 分家）。
   *
   * 未加载完时保持 `null`：两处消费方都按「拿不到就不用这项偏好」处理 ——
   * `pin_current_account` 视为关（不改排序）、`copy_sessions_by_default` 视为关
   * （不改变切换语义）。宁可退化成改造前的行为，也不要凭猜测展示。
   */
  const [switchConfig, setSwitchConfig] = useState<SwitchConfig | null>(null);
  /** 账号 id -> 今日旅行状态（undefined=查询中/未知） */
  const [travelMap, setTravelMap] = useState<Record<string, TravelStatus>>({});
  const [codebuddyCli, setCodebuddyCli] = useState<CodeBuddyCliStatus | null>(null);
  const [codebuddyCliSwitchingId, setCodebuddyCliSwitchingId] = useState<string | null>(null);
  const [codebuddyCnIde, setCodebuddyCnIde] = useState<CodeBuddyCnIdeStatus | null>(null);
  const [codebuddyCnIdeSwitchingId, setCodebuddyCnIdeSwitchingId] = useState<string | null>(null);
  const [installingCodebuddyCli, setInstallingCodebuddyCli] = useState(false);
  /** 刷新按钮触发的批量签到进行中 */
  const [checkinAllRunning, setCheckinAllRunning] = useState(false);
  /** 接入/升级 CLI helper 确认框 */
  const [installConfirmOpen, setInstallConfirmOpen] = useState(false);
  /** 删除账号确认目标（null=关闭） */
  const [deleteTarget, setDeleteTarget] = useState<AccountMeta | null>(null);
  /** 区域不匹配详情展开 */
  const [mismatchDetailOpen, setMismatchDetailOpen] = useState(false);
  /** 紧凑模式：卡片更小、同屏更多列；默认开启，偏好由 `useCompactMode` 统一持久化 */
  const [compact, toggleCompact] = useCompactMode();

  useEffect(() => {
    let cancelled = false;
    void api
      .getAutoCheckinConfig()
      .then((config) => {
        if (!cancelled) setAutoCheckinConfig(config);
      })
      .catch((e) => {
        if (!cancelled) {
          toast.error("自动签到配置加载失败", { description: api.asError(e) });
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    void api
      .getAutoTravelConfig()
      .then((config) => {
        if (!cancelled) setAutoTravelConfig(config);
      })
      .catch((e) => {
        if (!cancelled) {
          toast.error("自动旅行配置加载失败", { description: api.asError(e) });
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    void api
      .getSwitchConfig()
      .then((config) => {
        if (!cancelled) setSwitchConfig(config);
      })
      .catch(() => {
        // 静默按默认值走：这两项只是偏好（排序 / 默认勾选），读不到不影响核心功能，
        // 而每次挂载都弹一次错误提示的代价远大于收益。
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function refreshCodebuddyCliStatus() {
    try {
      setCodebuddyCli(await api.getCodebuddyCliStatus());
    } catch {
      setCodebuddyCli(null);
    }
  }

  async function refreshCodebuddyCnIdeStatus() {
    try {
      setCodebuddyCnIde(await api.getCodebuddyCnIdeStatus());
    } catch {
      setCodebuddyCnIde(null);
    }
  }

  useEffect(() => {
    let cancelled = false;
    void refreshCodebuddyCliStatus();
    void (async () => {
      if (!api.isDemoMode()) {
        try {
          await api.detectCodebuddyCnIdeAccount();
        } catch {
          /* 未登录或钥匙串拒绝时静默，下面仍拉安装/运行状态 */
        }
      }
      if (!cancelled) await refreshCodebuddyCnIdeStatus();
    })();
    return () => {
      cancelled = true;
    };
  }, [accounts.length]);

  /** 首次进入该版本且无账号时自动导入本机账号（本会话每版本只尝试一次，无本机账号时静默） */
  const autoImportTried = useRef(false);
  useEffect(() => {
    if (autoImportTried.current || loading || accounts.length > 0) return;
    autoImportTried.current = true;
    void importLocalStore(region)
      .then(() => void reconcileAccounts(region))
      .catch(() => {
        /* 本机无 WorkBuddy 登录态时静默，不打扰用户 */
      });
  }, [accounts.length, loading, importLocalStore, reconcileAccounts, region]);

  // 切到该版本时立刻重查一次状态：`status.current` 决定卡片上的「当前账号」标记，
  // 若沿用上一次的快照，切换账号（尤其是国际版）后标记会停在旧账号上。
  useEffect(() => {
    void refreshRegionStatus(region);
  }, [refreshRegionStatus, region]);

  // 账号列表变化后并行查询各账号今日签到状态
  useEffect(() => {
    if (!accounts.length) return;
    let cancelled = false;
    void fetchTodayCheckinMap(
      accounts.map((account) => account.id),
      region,
      () => cancelled,
    ).then((next) => {
      if (!cancelled && Object.keys(next).length > 0) {
        setCheckinMap((prev) => ({ ...prev, ...next }));
      }
    });
    return () => {
      cancelled = true;
    };
  }, [accounts, region]);

  async function loadTravelMap(accountIds: string[], isStale?: () => boolean) {
    const next = await fetchTravelMap(accountIds, region, isStale);
    if (!isStale?.() && Object.keys(next).length > 0) {
      setTravelMap((prev) => ({ ...prev, ...next }));
    }
  }

  // 账号列表变化后并行查询旅行状态；后台领取后每 60 秒再拉一次，避免卡片停在「旅行中」。
  useEffect(() => {
    if (!accounts.length) return;
    let cancelled = false;
    const ids = accounts.map((account) => account.id);
    void loadTravelMap(ids, () => cancelled);
    const timer = window.setInterval(() => {
      void loadTravelMap(ids, () => cancelled);
    }, 60_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accounts, region]);

  // 只给尚未缓存的账号拉积分；切回首页不重复请求。点「刷新积分」才强制更新。
  useEffect(() => {
    if (!accounts.length) return;
    void ensureCredits(accounts.map((account) => account.id), region);
  }, [accounts, ensureCredits, region]);

  async function onRecheck() {
    await Promise.all([refreshRegionStatus(region), reconcileAccounts(region)]);
  }

  async function onImport() {
    setImporting(true);
    try {
      const acc = await importLocalStore(region);
      toast.success("账号已导入", { description: acc.nickname || acc.email || acc.id });
    } catch (e) {
      toast.error("导入失败", { description: api.asError(e) });
    } finally {
      setImporting(false);
    }
  }

  async function onAutoCheckinChange(enabled: boolean) {
    if (!autoCheckinConfig || autoCheckinSaving) return;
    const previous = autoCheckinConfig;
    const next = { ...previous, enabled };
    setAutoCheckinConfig(next);
    setAutoCheckinSaving(true);
    try {
      setAutoCheckinConfig(await api.saveAutoCheckinConfig(next));
    } catch (e) {
      setAutoCheckinConfig(previous);
      toast.error("自动签到设置保存失败", { description: api.asError(e) });
    } finally {
      setAutoCheckinSaving(false);
    }
  }

  async function onAutoTravelChange(enabled: boolean) {
    if (!autoTravelConfig || autoTravelSaving) return;
    const previous = autoTravelConfig;
    const next = { ...previous, enabled };
    setAutoTravelConfig(next);
    setAutoTravelSaving(true);
    try {
      setAutoTravelConfig(await api.saveAutoTravelConfig(next));
      if (enabled) {
        toast.success("自动旅行已开启", { description: "正在按官方状态派发或领取" });
        window.setTimeout(() => {
          void loadTravelMap(accounts.map((account) => account.id));
        }, 2500);
      }
    } catch (e) {
      setAutoTravelConfig(previous);
      toast.error("自动旅行设置保存失败", { description: api.asError(e) });
    } finally {
      setAutoTravelSaving(false);
    }
  }

  /** 导出完成提示（含安全提醒）。 */
  function onExported(count: number) {
    const text = `已导出 ${count} 个账号。文件含登录 token，等同密码，请勿上传网盘或发送给他人。`;
    toast.success("导出成功", { description: text });
  }

  /** 导入完成提示：计数 + token 可能过期提醒，并刷新列表。 */
  function onImported(result: { imported: number; skipped: number; overwritten: number }) {
    void reconcileAccounts(region);
    const overwriteText = result.overwritten > 0 ? `（覆盖 ${result.overwritten} 个）` : "";
    const text = `已导入 ${result.imported} 个${overwriteText}，跳过 ${result.skipped} 个。token 可能已过期，切换后可能需要重新登录。`;
    toast.success("导入成功", { description: text });
  }

  function onDelete(a: AccountMeta) {
    // 桌面 App（Tauri WebView）不支持 window.confirm，改用 Dialog 确认
    setDeleteTarget(a);
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    const a = deleteTarget;
    setDeleteTarget(null);
    try {
      await deleteAccountStore(a.id, region);
      toast.success("账号已删除");
    } catch (e) {
      toast.error("删除失败", { description: api.asError(e) });
    }
  }

  async function onCheckin(a: AccountMeta) {
    try {
      const res = await api.checkin(a.id, region);
      const label =
        res.result === "success"
          ? "签到成功"
          : res.result === "already"
            ? "今天已签到"
            : "签到失败";
      const description = `${a.nickname || a.email || a.id}${res.error ? `：${res.error}` : ""}`;
      if (res.result === "error") toast.error(label, { description });
      else toast.success(label, { description });
      // 刷新该账号的今日签到状态
      try {
        const st = await api.getCheckinStatus(a.id, region);
        if (st.ok) setCheckinMap((prev) => ({ ...prev, [a.id]: st.todayCheckedIn }));
      } catch {
        /* ignore */
      }
      void reconcileAccounts(region);
      // 签到成功/已签到会带来积分变动，force 刷新该账号积分
      if (res.result !== "error") void refreshCredits([a.id], { region });
    } catch (e) {
      toast.error("签到失败", { description: api.asError(e) });
    }
  }

  async function onRefresh(a: AccountMeta) {
    try {
      const res = await api.refreshAccountToken(a.id, region);
      const label = a.nickname || a.email || a.id;
      if (res.needsRelogin) {
        toast.error("Token 刷新失败", { description: `${label}：需重新登录${res.needsReloginReason ? `（${res.needsReloginReason}）` : ""}` });
      } else {
        toast.success("Token 已刷新", { description: label });
      }
      void reconcileAccounts(region);
    } catch (e) {
      toast.error("Token 刷新失败", { description: api.asError(e) });
    }
  }

  /** 刷新按钮：先跑一轮批量签到并重查今日签到状态，再强制刷新全部积分。 */
  async function onRefreshCredits() {
    if (!accounts.length || refreshingCredits || checkinAllRunning) return;
    setCheckinAllRunning(true);
    try {
      try {
        const res = await api.checkinAll(region);
        const entries = res.accounts ?? [];
        const success = entries.filter((e) => e.result === "success").length;
        const already = entries.filter((e) => e.result === "already").length;
        const failed = entries.filter((e) => e.result === "error").length;
        const parts: string[] = [];
        if (success > 0) parts.push(`${success} 个签到成功`);
        if (already > 0) parts.push(`${already} 个已签到`);
        if (failed > 0) parts.push(`${failed} 个失败`);
        const summary = parts.length > 0 ? parts.join("，") : "无账号需要签到";
        if (entries.length > 0 && failed === entries.length) {
          toast.error("签到失败", { description: summary });
        } else {
          toast.success("签到完成", { description: summary });
        }
        // 批量签到后重查全部账号的今日签到状态，无需切换页面即反映最新结果
        const next = await fetchTodayCheckinMap(accounts.map((account) => account.id), region);
        if (Object.keys(next).length > 0) {
          setCheckinMap((prev) => ({ ...prev, ...next }));
        }
      } catch (e) {
        toast.error("批量签到失败", { description: api.asError(e) });
      }
      await refreshCredits(accounts.map((account) => account.id), { region });
      await loadTravelMap(accounts.map((account) => account.id));
      toast.success("积分到期情况已刷新");
    } finally {
      setCheckinAllRunning(false);
    }
  }

  async function onSwitchCodebuddyCli(account: AccountMeta) {
    if (codebuddyCliSwitchingId !== null) return;
    setCodebuddyCliSwitchingId(account.id);
    const toastId = toast.loading("正在切换 CodeBuddy CLI…", {
      description: `正在将默认账号设为 ${account.nickname || account.email || account.id}`,
    });
    try {
      const result = await api.switchCodebuddyCliAccount(account.id);
      await refreshCodebuddyCliStatus();
      toast.success("CodeBuddy CLI 默认账号已更新", {
        id: toastId,
        description: `${account.nickname || account.email || account.id}：${result.message || "配置已更新"}`,
      });
    } catch (error) {
      toast.error("CodeBuddy CLI 切换失败", {
        id: toastId,
        description: api.asError(error),
      });
    } finally {
      setCodebuddyCliSwitchingId(null);
    }
  }

  async function onSwitchCodebuddyCnIde(account: AccountMeta) {
    if (codebuddyCnIdeSwitchingId !== null) return;
    setCodebuddyCnIdeSwitchingId(account.id);
    const toastId = toast.loading("正在切换 CodeBuddy IDE…", {
      description: "将注入凭证并重启 CodeBuddy IDE",
    });
    try {
      const result = await api.switchCodebuddyCnIdeAccount(account.id, true);
      await refreshCodebuddyCnIdeStatus();
      toast.success("CodeBuddy IDE 已切换", {
        id: toastId,
        description: result.message || result.account,
      });
    } catch (error) {
      toast.error("CodeBuddy IDE 切换失败", {
        id: toastId,
        description: api.asError(error),
      });
    } finally {
      setCodebuddyCnIdeSwitchingId(null);
    }
  }

  async function onInstallCodebuddyCli() {
    // 桌面 App（Tauri WebView）不支持 window.confirm，改用 Dialog 确认
    setInstallConfirmOpen(true);
  }

  async function confirmInstallCodebuddyCli() {
    setInstallConfirmOpen(false);
    setInstallingCodebuddyCli(true);
    try {
      const result = await api.installCodebuddyCliHelper();
      toast.success("CodeBuddy CLI 接入已更新", { description: result.message });
      await refreshCodebuddyCliStatus();
    } catch (error) {
      toast.error("CodeBuddy CLI 接入失败", { description: api.asError(error) });
    } finally {
      setInstallingCodebuddyCli(false);
    }
  }

  const current = status?.current;

  /**
   * 保存账号备注。
   *
   * 成功后**重新拉取账号列表**而不是就地改 state：备注是唯一由用户手改的字段，
   * 让后端回传的脱敏 meta 成为唯一真相源，可以避免两边说法不一致
   * （后端会把全空白备注归一成「没有备注」，前端就地改就会留下一个空串）。
   *
   * 返回布尔值而不是抛异常：失败时卡片要保持编辑态，让用户改完重试，
   * 而不是把已输入的文字丢掉。
   */
  async function onSaveRemark(account: AccountMeta, remark: string): Promise<boolean> {
    try {
      await api.setAccountRemark(account.id, remark, region);
      await reconcileAccounts(region);
      toast.success(remark.trim() ? "备注已保存" : "备注已清除");
      return true;
    } catch (e) {
      toast.error("备注保存失败", { description: api.asError(e) });
      return false;
    }
  }

  const creditOrderingReady =
    accounts.length > 0 &&
    accounts.every((account) => Boolean(creditMap[account.id]) && !creditLoadingMap[account.id]);

  /**
   * 列表顺序：**积分优先级为基准**（快过期 / 建议优先的靠前）。
   *
   * `pin_current_account` 打开时，把当前登录账号整体提到第一位。这是**显式覆盖**
   * 而不是往比较函数里插一条分支：置顶与积分排序是同一根轴的两端
   * （「该切到谁」vs「正在用谁」），插进比较函数会让两者互相打架，
   * 结果取决于哪条规则先命中 —— 那种「有时候置顶、有时候不置顶」最难排查。
   * 覆盖之后其余账号的相对顺序完全不动，⭐「建议优先」徽章也照旧渲染，
   * 因此打开这项设置**不会让用户丢掉原有信息**，只是改变了第一条。
   */
  const orderedAccounts = (() => {
    const base = creditOrderingReady
      ? accounts
          .map((account, index) => ({ account, index }))
          .sort((left, right) => {
            const leftCredit = creditMap[left.account.id];
            const rightCredit = creditMap[right.account.id];
            const rankDifference = creditPriorityRank(leftCredit) - creditPriorityRank(rightCredit);
            if (rankDifference !== 0) return rankDifference;

            const leftExpiry = soonestRelevantExpiry(leftCredit);
            const rightExpiry = soonestRelevantExpiry(rightCredit);
            if (leftExpiry !== rightExpiry) return leftExpiry - rightExpiry;

            const amountDifference = expiringSoonAmount(rightCredit) - expiringSoonAmount(leftCredit);
            if (amountDifference !== 0) return amountDifference;
            return left.index - right.index;
          })
          .map(({ account }) => account)
      : accounts;

    if (!switchConfig?.pin_current_account) return base;
    const pinned = base.find((account) => isWorkbuddyCurrent(account, current));
    if (!pinned || base[0]?.id === pinned.id) return base;
    return [pinned, ...base.filter((account) => account.id !== pinned.id)];
  })();
  const priorityAccountId = creditOrderingReady
    ? orderedAccounts.find((account) => hasExpiringSoonCredits(creditMap[account.id]))?.id
    : undefined;
  const cliCurrentAccountId = codebuddyCli?.activeAccountId;
  const workbuddyCurrentName = current
    ? current.nickname || current.email || current.uid || "未知账号"
    : "未登录";
  const codebuddyCurrentName = codebuddyCli?.configured
    ? codebuddyCli.activeAccountName || "未检测到"
    : "尚未接入";
  const cnIdeCurrentAccountId = codebuddyCnIde?.activeAccountId;
  const cnIdeCurrentName = codebuddyCnIde?.installed
    ? codebuddyCnIde.activeAccountName || "未检测到"
    : "未安装";
  const codebuddyUsesSettingsEnv = codebuddyCli?.authMode === "settings-env";

  const presence = regionPresence(status, accounts);
  const showEmpty = accounts.length === 0;
  const mismatch = status?.regionMismatch ?? null;
  const expectedAuthFile = status?.authFile || descriptor.authFilename;

  return (
    <>
      {/* 产品状态徽标 */}
      <div className="mb-6 flex justify-end gap-4">
        <div className="flex items-center gap-2.5">
          <span className="group relative inline-flex cursor-default">
            <span
              className={
                status?.running
                  ? "inline-flex rounded-[22%] bg-primary p-[2px] shadow-sm shadow-primary/40"
                  : "inline-flex rounded-[22%] bg-muted-foreground/30 p-[2px]"
              }
            >
              <WorkBuddyMark size={28} />
            </span>
            <span className="pointer-events-none absolute right-0 top-full z-50 mt-2 hidden whitespace-nowrap rounded-md bg-popover px-2.5 py-1.5 text-xs text-popover-foreground shadow-lg ring-1 ring-black/5 group-hover:block">
              {descriptor.displayName}：{status?.running ? "运行中" : "未运行"} · 当前账号：{workbuddyCurrentName}
            </span>
          </span>
          <span className="group relative inline-flex cursor-default">
            <span
              className={
                codebuddyCnIde?.installed
                  ? "inline-flex rounded-[22%] bg-primary p-[2px] shadow-sm shadow-primary/40"
                  : "inline-flex rounded-[22%] bg-muted-foreground/30 p-[2px]"
              }
            >
              <CodeBuddyCnIdeMark size={28} />
            </span>
            <span className="pointer-events-none absolute right-0 top-full z-50 mt-2 hidden whitespace-nowrap rounded-md bg-popover px-2.5 py-1.5 text-xs text-popover-foreground shadow-lg ring-1 ring-black/5 group-hover:block">
              CodeBuddy IDE：{codebuddyCnIde?.installed ? (codebuddyCnIde.running ? "运行中" : "已接入") : "未接入"} · 当前账号：{cnIdeCurrentName}
            </span>
          </span>
          <span className="group relative inline-flex cursor-default">
            <span
              className={
                codebuddyCli?.configured
                  ? "inline-flex rounded-[22%] bg-primary p-[2px] shadow-sm shadow-primary/40"
                  : "inline-flex rounded-[22%] bg-muted-foreground/30 p-[2px]"
              }
            >
              <CodeBuddyMark size={28} />
            </span>
            <span className="pointer-events-none absolute right-0 top-full z-50 mt-2 hidden whitespace-nowrap rounded-md bg-popover px-2.5 py-1.5 text-xs text-popover-foreground shadow-lg ring-1 ring-black/5 group-hover:block">
              CodeBuddy CLI：{codebuddyCli?.migrationRequired ? "需升级" : codebuddyCli?.configured ? "已接入" : "未接入"} · 当前账号：{codebuddyCurrentName}
            </span>
          </span>
        </div>
      </div>

      {/* 区域不匹配（安全红线） */}
      {mismatch && (
        <Alert variant="warning" className="mb-4">
          <AlertTriangle />
          <AlertTitle>检测到凭据版本不匹配</AlertTitle>
          <AlertDescription>
            <p>
              检测到 {regionDescriptor(mismatch.actualRegion ?? "global").displayName} 的登录凭据出现在
              {descriptor.versionLabel}的认证文件位置（domain: {mismatch.actualDomain}）。已拒绝使用该凭据。
            </p>
            <p className="mt-1">
              请把 {mismatch.envVar || descriptor.authEnv} 指向{descriptor.versionLabel}登录态，或移除该文件。
            </p>
            <Button className="mt-2" size="sm" variant="outline" onClick={() => setMismatchDetailOpen(true)}>
              查看详情
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {/* 空态（未安装 / 未登录） */}
      {loading && accounts.length === 0 ? (
        <div className="flex items-center gap-2 py-16 text-sm text-muted-foreground">
          <Loader2 className="animate-spin" />
          加载账号…
        </div>
      ) : showEmpty ? (
        <EmptyRegionCard
          region={region}
          installed={presence !== "absent"}
          expectedAuthFile={expectedAuthFile}
          onRecheck={() => void onRecheck()}
          onImport={() => void onImport()}
          importing={importing}
        />
      ) : (
        <>
          <div className="relative mb-6 overflow-visible rounded-2xl border border-border bg-muted/30 px-5 py-5 shadow-[0_6px_20px_rgba(15,23,42,.025)]">
            <div className="pointer-events-none absolute inset-0 overflow-hidden rounded-2xl">
              <div className="absolute -right-12 -top-20 size-44 rounded-full border-[28px] border-slate-400/[0.035]" />
            </div>
            <div className="relative flex flex-wrap items-center gap-x-5 gap-y-4">
              <div className="min-w-[190px] flex-1">
                <h2 className="text-sm font-semibold text-foreground">添加与迁移账号</h2>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">快速接入新账号，或从已有环境恢复</p>
              </div>
              <div className="flex flex-wrap items-center gap-2.5">
                <DemoAction>
                  <Button
                    className="h-10 bg-primary px-4 text-primary-foreground shadow-sm hover:bg-primary/90"
                    onClick={() => setOauthOpen(true)}
                  >
                    <QrCode />OAuth 扫码添加
                  </Button>
                </DemoAction>
                <DemoAction>
                  <Button className="h-10 px-4" onClick={onImport} disabled={importing} variant="outline">
                    {importing ? <Loader2 className="animate-spin" /> : <Download />}导入本机账号
                  </Button>
                </DemoAction>
              </div>
              <div className="flex items-center gap-1">
                <DemoAction>
                  <Button variant="ghost" size="sm" className="h-9 px-2.5" onClick={() => setImportOpen(true)} title="从备份文件导入账号">
                    <FileUp />导入备份
                  </Button>
                </DemoAction>
                <DemoAction>
                  <Button variant="ghost" size="sm" className="h-9 px-2.5" onClick={() => setExportOpen(true)} disabled={accounts.length === 0} title="导出账号备份">
                    <FileDown />导出
                  </Button>
                </DemoAction>
              </div>
            </div>
          </div>

          {error && (
            <Alert variant="destructive" className="mb-4">
              <AlertTitle>加载失败</AlertTitle>
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}

          {codebuddyCli &&
            (!codebuddyCli.configured ||
              (!codebuddyUsesSettingsEnv && !codebuddyCli.helperSupportsAccountIds) ||
              codebuddyCli.migrationRequired ||
              codebuddyCli.syncPending) && (
              <Alert className="mb-4">
                <Terminal />
                <AlertTitle>CodeBuddy CLI 接入</AlertTitle>
                <AlertDescription>
                  <p>
                    {codebuddyUsesSettingsEnv
                      ? codebuddyCli.environmentOverride
                        ? "检测到进程环境变量 CODEBUDDY_AUTH_TOKEN。它会覆盖 settings.json；请先从 Windows 用户或系统环境变量中删除它，再重启本应用与 CodeBuddy CLI。"
                        : codebuddyCli.syncPending
                          ? "Windows CLI 认证配置与当前账号 Token 已脱节。点击更新认证后写入最新 Token；当前运行会话不会切换，请由 ACP 重新加载会话或重启 CLI 后生效。"
                          : codebuddyCli.migrationRequired
                            ? "检测到旧版 Windows helper 配置。接入后会改用 settings.json 的 env.CODEBUDDY_AUTH_TOKEN，不再执行 helper。"
                            : "Windows 使用 CodeBuddy settings.json 中的认证 Token；切换或保活刷新后会自动更新。当前运行会话不会切换，请由 ACP 重新加载会话或重启 CLI 后生效。"
                      : codebuddyCli.migrationRequired
                        ? "检测到旧版 helper，请先升级；升级前不会将 CLI 切换显示为已验证。"
                        : codebuddyCli.configured
                          ? "当前 helper 仍按旧索引读取账号；升级后将按账号 ID 独立切换，账号增删也不会错位。"
                          : "WorkBuddy 账号与积分功能可正常使用；如需从这里切换 CodeBuddy CLI 账号，点击下方按钮一键接入。"}
                  </p>
                  <DemoAction>
                    <Button
                      className="mt-2"
                      size="sm"
                      variant="outline"
                      onClick={() => void onInstallCodebuddyCli()}
                      disabled={installingCodebuddyCli}
                    >
                      {installingCodebuddyCli && <Loader2 className="animate-spin" />}
                      {codebuddyUsesSettingsEnv
                        ? codebuddyCli.configured ? "更新 CLI 认证" : "接入 CLI"
                        : codebuddyCli.configured || codebuddyCli.migrationRequired ? "升级 CLI helper" : "接入 CLI"}
                    </Button>
                  </DemoAction>
                </AlertDescription>
              </Alert>
            )}

          <section className="mt-7 min-w-0" aria-labelledby={`accounts-list-title-${region}`}>
            <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <h2 id={`accounts-list-title-${region}`} className="text-base font-semibold tracking-tight">
                  账号
                </h2>
                <Badge
                  variant="secondary"
                  className="h-6 min-w-6 rounded-full border-0 px-1.5 text-[11px] tabular-nums text-muted-foreground shadow-none"
                  aria-label={`${accounts.length} 个账号`}
                >
                  {accounts.length}
                </Badge>
              </div>
              <TooltipProvider delayDuration={400}>
                <div className="ml-auto flex items-center gap-1">
                  <div className="mr-1 flex items-center gap-2.5">
                    <label htmlFor={`accounts-auto-checkin-${region}`} className="cursor-pointer text-xs font-medium text-muted-foreground">
                      自动签到
                    </label>
                    <DemoAction>
                      <Switch
                        id={`accounts-auto-checkin-${region}`}
                        checked={autoCheckinConfig?.enabled ?? false}
                        disabled={!autoCheckinConfig || autoCheckinSaving}
                        onCheckedChange={(enabled) => void onAutoCheckinChange(enabled)}
                        aria-label="自动签到"
                      />
                    </DemoAction>
                    {autoCheckinSaving && <Loader2 className="size-3.5 animate-spin text-muted-foreground" aria-label="正在保存自动签到设置" />}
                  </div>
                  <div className="mr-1 flex items-center gap-2.5">
                    <label htmlFor={`accounts-auto-travel-${region}`} className="cursor-pointer text-xs font-medium text-muted-foreground">
                      自动旅行
                    </label>
                    <DemoAction>
                      <Switch
                        id={`accounts-auto-travel-${region}`}
                        checked={autoTravelConfig?.enabled ?? false}
                        disabled={!autoTravelConfig || autoTravelSaving}
                        onCheckedChange={(enabled) => void onAutoTravelChange(enabled)}
                        aria-label="自动旅行"
                      />
                    </DemoAction>
                    {autoTravelSaving && <Loader2 className="size-3.5 animate-spin text-muted-foreground" aria-label="正在保存自动旅行设置" />}
                  </div>
                  <Separator orientation="vertical" className="mx-2 h-5" />
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        className={cn("size-9 rounded-lg", compact && "bg-accent text-accent-foreground")}
                        onClick={toggleCompact}
                        aria-label={compact ? "切换为宽松模式" : "切换为紧凑模式"}
                      >
                        {compact ? <Rows3 /> : <Columns3 />}
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent side="top">{compact ? "切换为宽松模式" : "切换为紧凑模式"}</TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span>
                        <DemoAction>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="size-9 rounded-lg"
                            disabled={refreshingCredits || checkinAllRunning || accounts.length === 0}
                            onClick={() => void onRefreshCredits()}
                            aria-label="签到并刷新全部账号积分"
                          >
                            <RefreshCw className={refreshingCredits || checkinAllRunning ? "animate-spin" : undefined} />
                          </Button>
                        </DemoAction>
                      </span>
                    </TooltipTrigger>
                    <TooltipContent side="top">{api.isDemoMode() ? "演示模式下不可操作" : "签到并刷新全部账号积分"}</TooltipContent>
                  </Tooltip>
                </div>
              </TooltipProvider>
            </div>
            <div className={cn("grid min-w-0 items-start gap-5", compact ? "grid-cols-[repeat(auto-fit,minmax(min(100%,300px),1fr))]" : "grid-cols-[repeat(auto-fit,minmax(min(100%,340px),1fr))]")}>
                {orderedAccounts.map((a) => (
                  <AccountCard
                    key={a.id}
                    account={a}
                    compact={compact}
                    onDelete={onDelete}
                    onSwitch={setSwitchAccount}
                    onSaveRemark={onSaveRemark}
                    onCheckin={onCheckin}
                    onRefresh={onRefresh}
                    todayCheckedIn={checkinMap[a.id]}
                    travelStatus={travelMap[a.id]}
                    credit={creditMap[a.id]}
                    creditLoading={creditLoadingMap[a.id]}
                    creditUpdatedAt={creditUpdatedAtMap[a.id]}
                    creditPriority={a.id === priorityAccountId}
                    workbuddyActive={isWorkbuddyCurrent(a, current)}
                    codebuddyCliConfigured={codebuddyCli?.configured && !codebuddyCli.migrationRequired && !codebuddyCli.syncPending}
                    codebuddyCliActive={a.id === cliCurrentAccountId}
                    codebuddyCliBusy={codebuddyCliSwitchingId !== null}
                    onSwitchCodebuddyCli={onSwitchCodebuddyCli}
                    codebuddyCliLoading={codebuddyCliSwitchingId === a.id}
                    codebuddyCnIdeAvailable={Boolean(codebuddyCnIde?.installed)}
                    codebuddyCnIdeActive={a.id === cnIdeCurrentAccountId}
                    codebuddyCnIdeBusy={codebuddyCnIdeSwitchingId !== null}
                    codebuddyCnIdeLoading={codebuddyCnIdeSwitchingId === a.id}
                    onSwitchCodebuddyCnIde={onSwitchCodebuddyCnIde}
                    featuresDisabled={false}
                  />
                ))}
              </div>
          </section>
        </>
      )}

      <OAuthLoginDialog open={oauthOpen} onOpenChange={setOauthOpen} region={region} />
      <ExportAccountsDialog
        open={exportOpen}
        onOpenChange={setExportOpen}
        accounts={accounts}
        onExported={onExported}
        region={region}
      />
      <ImportAccountsDialog
        open={importOpen}
        onOpenChange={setImportOpen}
        onImported={onImported}
        region={region}
      />
      <SwitchAccountDialog
        open={switchAccount !== null}
        onOpenChange={(o) => {
          if (!o) setSwitchAccount(null);
        }}
        account={switchAccount}
        region={region}
        copySessionsByDefault={Boolean(switchConfig?.copy_sessions_by_default)}
        onDone={() => {
          // 切换完成后必须同时刷新账号库与状态：账号库决定卡片内容，
          // `status.current` 决定「当前账号」标记；只刷账号库会让标记留在旧账号上。
          void reconcileAccounts(region);
          void refreshRegionStatus(region);
          void refreshCodebuddyCliStatus();
          void refreshCodebuddyCnIdeStatus();
        }}
      />

      {/* 接入/升级 CLI 认证确认（桌面 App 不支持 window.confirm） */}
      <Dialog open={installConfirmOpen} onOpenChange={setInstallConfirmOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {codebuddyUsesSettingsEnv
                ? "更新 CodeBuddy CLI 认证"
                : codebuddyCli?.configured || codebuddyCli?.migrationRequired
                  ? "升级 CodeBuddy CLI helper"
                  : "接入 CodeBuddy CLI"}
            </DialogTitle>
            <DialogDescription>
              {codebuddyUsesSettingsEnv ? (
                <>
                  将把当前账号的认证 Token 写入
                  <code className="mx-1 rounded bg-muted px-1">~/.codebuddy/settings.json</code>
                  的 <code className="mx-1 rounded bg-muted px-1">env.CODEBUDDY_AUTH_TOKEN</code>。
                  其他配置会保留；更新只影响后续加载的会话，当前运行会话不会切换。是否继续？
                </>
              ) : (
                <>
                  {codebuddyCli?.configured || codebuddyCli?.migrationRequired ? "升级" : "接入"}会自动写入
                  <code className="mx-1 rounded bg-muted px-1">~/.codebuddy-rotate/helper.cjs</code>
                  并更新
                  <code className="mx-1 rounded bg-muted px-1">~/.codebuddy/settings.json</code>
                  的 apiKeyHelper 配置，是否继续？
                </>
              )}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setInstallConfirmOpen(false)}>
              取消
            </Button>
            <Button onClick={() => void confirmInstallCodebuddyCli()}>继续</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 删除账号确认 */}
      <Dialog open={deleteTarget !== null} onOpenChange={(o) => !o && setDeleteTarget(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>删除账号</DialogTitle>
            <DialogDescription>
              确定删除账号「{deleteTarget?.nickname || deleteTarget?.email || deleteTarget?.id}」？
              此操作不可撤销。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteTarget(null)}>
              取消
            </Button>
            <Button variant="destructive" onClick={() => void confirmDelete()}>
              删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 区域不匹配详情 */}
      <Dialog open={mismatchDetailOpen} onOpenChange={setMismatchDetailOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>凭据版本不匹配</DialogTitle>
            <DialogDescription>
              为保证安全，该凭据不会被使用，也不会向任何上游端点发送。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-1.5 text-sm">
            <div className="flex justify-between gap-4">
              <span className="text-muted-foreground">实际 domain</span>
              <code className="font-mono">{mismatch?.actualDomain || "—"}</code>
            </div>
            <div className="flex justify-between gap-4">
              <span className="text-muted-foreground">期望认证文件</span>
              <code className="font-mono">{mismatch?.expectedFile || descriptor.authFilename}</code>
            </div>
            <div className="flex justify-between gap-4">
              <span className="text-muted-foreground">环境变量</span>
              <code className="font-mono">{mismatch?.envVar || descriptor.authEnv}</code>
            </div>
          </div>
          <DialogFooter>
            <Button onClick={() => setMismatchDetailOpen(false)}>关闭</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

/** 该版本未安装 / 未登录时的空态卡片（PRD §3.3.1）。 */
function EmptyRegionCard({
  region,
  installed,
  expectedAuthFile,
  onRecheck,
  onImport,
  importing,
}: {
  region: Region;
  installed: boolean;
  expectedAuthFile: string;
  onRecheck: () => void;
  onImport: () => void;
  importing: boolean;
}) {
  const descriptor = regionDescriptor(region);
  return (
    <Card className="gap-0 py-0">
      <div className="flex items-start gap-3 px-5 py-5">
        <AlertTriangle className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-medium">未检测到 WorkBuddy {descriptor.versionLabel}</h2>

          <div className="mt-3 text-sm text-muted-foreground">
            <p className="font-medium text-foreground/80">可能原因：</p>
            <ul className="mt-1 list-disc space-y-1 pl-5">
              <li>未安装{descriptor.versionLabel} WorkBuddy 桌面客户端</li>
              <li>已安装但从未登录过（未生成认证文件）</li>
              {installed && <li>客户端已安装，但当前无可用登录态</li>}
            </ul>
          </div>

          <div className="mt-4">
            <p className="text-sm font-medium text-foreground/80">期望认证文件：</p>
            <div className="mt-1.5 flex flex-wrap items-center gap-2">
              <code className="min-w-0 break-all rounded-md border border-border bg-muted/40 px-2 py-1 font-mono text-[11px] text-muted-foreground">
                {expectedAuthFile}
              </code>
              <Button variant="ghost" size="sm" onClick={() => void copyText(expectedAuthFile, "路径已复制")}>
                复制路径
              </Button>
            </div>
          </div>

          <div className="mt-4 flex flex-wrap gap-2">
            <Button size="sm" variant="outline" onClick={onRecheck}>
              <RefreshCw />
              重新检测
            </Button>
            <Button size="sm" onClick={onImport} disabled={importing}>
              {importing ? <Loader2 className="animate-spin" /> : <Download />}
              从本机导入
            </Button>
          </div>
        </div>
      </div>
    </Card>
  );
}
